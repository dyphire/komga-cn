#![allow(clippy::result_large_err)]

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use komga_application::discovery::{
    SeriesAlternateTitleRecord, SeriesEventEmitter, SeriesMetadataLinkRecord, SeriesMetadataPatch,
    SeriesMetadataUpdateError, SeriesMetadataUpdateResult, SeriesMetadataWriter,
    SeriesReadingDirection, resolve_persisted_series_id,
};
use komga_application::runtime_sse::{RuntimeSseEvent, RuntimeSseEventSource};
use komga_domain::discovery::SeriesStatus;
use serde_json::{Value, json};

use super::detail_utils::internal_error_response;
use super::series_persistence::{
    load_persisted_series_collections, load_persisted_series_detail, load_persisted_series_resource,
};
use super::{series_collections_payload, series_detail_payload};
use crate::discovery_auth::context::{DetailContentContext, DetailResourceContext};
use crate::helpers::{detail_access_denial_response, to_domain_query_context};
use crate::identity_access::auth::{Admin, Authenticated};
use crate::state::DiscoveryState;

pub(crate) async fn series_detail(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    let app = &app;

    let resolved_series_id =
        resolve_persisted_series_id(app.series_id_resolver.as_ref(), &series_id).await;

    let Some(resource) = (match load_persisted_series_resource(app, &resolved_series_id).await {
        Ok(resource) => resource,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id.clone()),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating,
            sharing_labels: resource.sharing_labels.clone(),
        }),
    };

    let detail_query_context = match app
        .discovery_auth
        .resolve_detail_query_context_with_persistence(&app.identity, &headers, &detail_context)
        .await
    {
        Ok(context) => context,
        Err(denial) => return detail_access_denial_response(denial),
    };
    let is_admin = detail_query_context.is_admin;
    let Some(series) = (match load_persisted_series_detail(
        app,
        &resolved_series_id,
        detail_query_context.user_id.as_deref(),
    )
    .await
    {
        Ok(series) => series,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    Json(series_detail_payload(&series, is_admin)).into_response()
}

pub(crate) async fn series_collections(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    let app = &app;

    let context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(&app.identity, &headers, None)
        .await
    {
        Ok(Some(context)) => context,
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let Some(resource) = (match load_persisted_series_resource(app, &series_id).await {
        Ok(resource) => resource,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating,
            sharing_labels: resource.sharing_labels,
        }),
    };

    match app
        .discovery_auth
        .resolve_detail_query_context_with_persistence(&app.identity, &headers, &detail_context)
        .await
    {
        Ok(_) => match load_persisted_series_collections(app, &series_id).await {
            Ok(collections) => {
                let domain_context = to_domain_query_context(context);
                let visible_collections = match app
                    .persisted_sets
                    .visible_collections(&domain_context, collections)
                    .await
                {
                    Ok(collections) => collections,
                    Err(error) => return internal_error_response(error),
                };

                Json(series_collections_payload(&visible_collections)).into_response()
            }
            Err(error) => internal_error_response(error),
        },
        Err(denial) => detail_access_denial_response(denial),
    }
}

pub(crate) async fn series_metadata_update(
    State(app): State<DiscoveryState>,
    _: Admin,
    Path(series_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let app = &app;

    let body = match body.as_object() {
        Some(body) => body,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "series metadata update payload must be a JSON object" })),
            )
                .into_response();
        }
    };

    let patch = match parse_series_metadata_patch(body) {
        Ok(patch) => patch,
        Err(response) => return response,
    };

    let event_emitter = RuntimeSeriesEventEmitter {
        runtime_events: app.runtime_events.as_ref(),
    };
    let writer = SeriesMetadataWriter::new(app.series_metadata.as_ref(), &event_emitter);
    match writer.update_series(&series_id, patch).await {
        Ok(SeriesMetadataUpdateResult::Updated) => StatusCode::NO_CONTENT.into_response(),
        Ok(SeriesMetadataUpdateResult::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => series_metadata_update_error_response(error),
    }
}

fn series_metadata_update_error_response(error: SeriesMetadataUpdateError) -> Response {
    match error {
        SeriesMetadataUpdateError::Validation(error) => {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": error }))).into_response()
        }
        SeriesMetadataUpdateError::Persistence(error) => internal_error_response(error),
    }
}

struct RuntimeSeriesEventEmitter<'a> {
    runtime_events: &'a dyn RuntimeSseEventSource,
}

impl SeriesEventEmitter for RuntimeSeriesEventEmitter<'_> {
    fn emit_series_changed(&self, series_id: &str, library_id: &str) {
        self.runtime_events
            .register(RuntimeSseEvent::SeriesChanged {
                series_id: series_id.to_string(),
                library_id: library_id.to_string(),
            });
    }
}

fn parse_series_metadata_patch(
    body: &serde_json::Map<String, Value>,
) -> Result<SeriesMetadataPatch, Response> {
    Ok(SeriesMetadataPatch {
        status: optional_series_status_field(body, "status")?,
        status_lock: optional_bool_field(body, "statusLock")?,
        title: optional_string_field(body, "title")?,
        title_lock: optional_bool_field(body, "titleLock")?,
        title_sort: optional_string_field(body, "titleSort")?,
        title_sort_lock: optional_bool_field(body, "titleSortLock")?,
        summary: optional_string_field(body, "summary")?,
        summary_lock: optional_bool_field(body, "summaryLock")?,
        reading_direction: optional_reading_direction_field(body, "readingDirection")?,
        reading_direction_lock: optional_bool_field(body, "readingDirectionLock")?,
        publisher: optional_string_field(body, "publisher")?,
        publisher_lock: optional_bool_field(body, "publisherLock")?,
        age_rating: optional_nullable_u32_field(body, "ageRating")?,
        age_rating_lock: optional_bool_field(body, "ageRatingLock")?,
        language: optional_string_field(body, "language")?,
        language_lock: optional_bool_field(body, "languageLock")?,
        genres: optional_string_list_field(body, "genres")?,
        genres_lock: optional_bool_field(body, "genresLock")?,
        tags: optional_string_list_field(body, "tags")?,
        tags_lock: optional_bool_field(body, "tagsLock")?,
        total_book_count: optional_nullable_u32_field(body, "totalBookCount")?,
        total_book_count_lock: optional_bool_field(body, "totalBookCountLock")?,
        sharing_labels: optional_string_list_field(body, "sharingLabels")?,
        sharing_labels_lock: optional_bool_field(body, "sharingLabelsLock")?,
        links: optional_links_field(body, "links")?,
        links_lock: optional_bool_field(body, "linksLock")?,
        alternate_titles: optional_alternate_titles_field(body, "alternateTitles")?,
        alternate_titles_lock: optional_bool_field(body, "alternateTitlesLock")?,
    })
}

fn optional_reading_direction_field(
    body: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Option<SeriesReadingDirection>>, Response> {
    match optional_nullable_string_field(body, key)? {
        Some(Some(value)) => SeriesReadingDirection::parse(&value)
            .map(Some)
            .map(Some)
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": format!("{key} has an invalid value") })),
                )
                    .into_response()
            }),
        Some(None) => Ok(Some(None)),
        None => Ok(None),
    }
}

fn optional_series_status_field(
    body: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<SeriesStatus>, Response> {
    match optional_string_field(body, key)? {
        Some(value) => SeriesStatus::parse(&value).map(Some).ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("{key} has an invalid value") })),
            )
                .into_response()
        }),
        None => Ok(None),
    }
}

fn optional_bool_field(
    body: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<bool>, Response> {
    match body.get(key) {
        Some(value) if value.is_null() => Ok(None),
        Some(value) => value
            .as_bool()
            .map(Some)
            .ok_or_else(|| bad_request_response(&format!("{key} must be a boolean or null"))),
        None => Ok(None),
    }
}

fn optional_string_field(
    body: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, Response> {
    match body.get(key) {
        Some(value) if value.is_null() => Ok(None),
        Some(value) => value
            .as_str()
            .map(|value| Some(value.to_string()))
            .ok_or_else(|| bad_request_response(&format!("{key} must be a string or null"))),
        None => Ok(None),
    }
}

fn optional_nullable_string_field(
    body: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Option<String>>, Response> {
    match body.get(key) {
        Some(value) if value.is_null() => Ok(Some(None)),
        Some(value) => value
            .as_str()
            .map(|value| Some(Some(value.to_string())))
            .ok_or_else(|| bad_request_response(&format!("{key} must be a string or null"))),
        None => Ok(None),
    }
}

fn optional_nullable_u32_field(
    body: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Option<u32>>, Response> {
    let Some(value) = body.get(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(Some(None));
    }

    let Some(value) = value.as_i64() else {
        return Err(bad_request_response(&format!(
            "{key} must be an integer or null"
        )));
    };
    if !(0..=i64::from(u32::MAX)).contains(&value) {
        return Err(bad_request_response(&format!(
            "{key} must be between 0 and {}",
            u32::MAX,
        )));
    }

    Ok(Some(Some(value as u32)))
}

fn optional_string_list_field(
    body: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Vec<String>>, Response> {
    let Some(value) = body.get(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(Some(Vec::new()));
    }

    let Some(values) = value.as_array() else {
        return Err(bad_request_response(&format!(
            "{key} must be an array or null"
        )));
    };
    let values = values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| bad_request_response(&format!("{key} entries must be strings")))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Some(values))
}

fn optional_links_field(
    body: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Vec<SeriesMetadataLinkRecord>>, Response> {
    let Some(value) = body.get(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(Some(Vec::new()));
    }

    let Some(values) = value.as_array() else {
        return Err(bad_request_response(&format!(
            "{key} must be an array or null"
        )));
    };

    let values = values
        .iter()
        .map(|value| {
            let Some(object) = value.as_object() else {
                return Err(bad_request_response("links entries must be objects"));
            };
            let Some(label) = object.get("label").and_then(Value::as_str) else {
                return Err(bad_request_response("links.label must be a string"));
            };
            let Some(url) = object.get("url").and_then(Value::as_str) else {
                return Err(bad_request_response("links.url must be a string"));
            };
            Ok(SeriesMetadataLinkRecord {
                label: label.to_string(),
                url: url.to_string(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Some(values))
}

fn optional_alternate_titles_field(
    body: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Vec<SeriesAlternateTitleRecord>>, Response> {
    let Some(value) = body.get(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(Some(Vec::new()));
    }

    let Some(values) = body.get(key).and_then(Value::as_array) else {
        return Err(bad_request_response(&format!(
            "{key} must be an array or null"
        )));
    };

    let values = values
        .iter()
        .map(|value| {
            let Some(object) = value.as_object() else {
                return Err(bad_request_response(
                    "alternateTitles entries must be objects",
                ));
            };
            let Some(label) = object.get("label").and_then(Value::as_str) else {
                return Err(bad_request_response(
                    "alternateTitles.label must be a string",
                ));
            };
            let Some(title) = object.get("title").and_then(Value::as_str) else {
                return Err(bad_request_response(
                    "alternateTitles.title must be a string",
                ));
            };
            Ok(SeriesAlternateTitleRecord {
                label: label.to_string(),
                title: title.to_string(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Some(values))
}

fn bad_request_response(message: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))).into_response()
}

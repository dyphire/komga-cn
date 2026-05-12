#![allow(clippy::result_large_err)]

use super::series_persistence::ExistingSeriesMetadata;
use super::*;
use crate::identity_access::auth::{Admin, Authenticated};
use crate::state::DiscoveryState;
use axum::extract::State;
use language_tags::LanguageTag;
use reqwest::Url;

pub async fn series_detail(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    let app = &app;

    let resolved_series_id = resolve_series_id_for_persisted(app, &series_id).await;

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
        .resolve_detail_query_context_with_persistence(
            &*app.identity.service,
            &headers,
            &detail_context,
        )
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

pub async fn series_collections(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    let app = &app;

    let Some(context) = app
        .discovery_auth
        .resolve_query_context_with_persistence(&*app.identity.service, &headers, None)
        .await
    else {
        return StatusCode::UNAUTHORIZED.into_response();
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
        .resolve_detail_query_context_with_persistence(
            &*app.identity.service,
            &headers,
            &detail_context,
        )
        .await
    {
        Ok(_) => match load_persisted_series_collections(app, &series_id).await {
            Ok(mut collections) => {
                for collection in &mut collections {
                    let mut visible_series_ids = Vec::with_capacity(collection.series_ids.len());
                    for related_series_id in &collection.series_ids {
                        match series_visible_to_context(app, &context, related_series_id, None)
                            .await
                        {
                            Ok(true) => visible_series_ids.push(related_series_id.clone()),
                            Ok(false) => {}
                            Err(error) => return internal_error_response(error),
                        }
                    }

                    if visible_series_ids.len() != collection.series_ids.len() {
                        collection.filtered = true;
                    }
                    collection.series_ids = visible_series_ids;
                }

                collections.retain(|collection| !collection.series_ids.is_empty());
                Json(series_collections_payload(&collections)).into_response()
            }
            Err(error) => internal_error_response(error),
        },
        Err(denial) => detail_access_denial_response(denial),
    }
}

pub async fn series_metadata_update(
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

    if let Err(response) = validate_series_metadata_patch(body) {
        return response;
    }

    let existing = match load_existing_series_metadata(app, &series_id).await {
        Ok(Some(existing)) => existing,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    };

    let update = merge_series_metadata_patch(body, &existing);

    match persist_series_metadata_update(app, &series_id, update).await {
        Ok(true) => {
            if let Err(error) =
                sync_series_search_documents_after_metadata_update(app, &series_id).await
            {
                return internal_error_response(error);
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

/// Merges a validated JSON patch onto existing series metadata, producing the
/// full update record to persist.
pub(super) fn merge_series_metadata_patch(
    body: &serde_json::Map<String, Value>,
    existing: &ExistingSeriesMetadata,
) -> SeriesMetadataUpdateRecord {
    let merge_str = |key: &str, fallback: &str| -> String {
        body.get(key)
            .and_then(Value::as_str)
            .unwrap_or(fallback)
            .to_string()
    };
    let merge_bool = |key: &str, fallback: bool| -> bool {
        body.get(key).and_then(Value::as_bool).unwrap_or(fallback)
    };
    let merge_nullable_str = |key: &str, fallback: &Option<String>| -> Option<String> {
        if body.contains_key(key) {
            body.get(key).and_then(Value::as_str).map(str::to_string)
        } else {
            fallback.clone()
        }
    };
    let merge_nullable_u32 = |key: &str, fallback: Option<u32>| -> Option<u32> {
        if body.contains_key(key) {
            body.get(key)
                .and_then(Value::as_u64)
                .and_then(|v| u32::try_from(v).ok())
        } else {
            fallback
        }
    };

    SeriesMetadataUpdateRecord {
        status: merge_str("status", &existing.status),
        status_lock: merge_bool("statusLock", existing.status_lock),
        title: merge_str("title", &existing.title),
        title_lock: merge_bool("titleLock", existing.title_lock),
        title_sort: merge_str("titleSort", &existing.title_sort),
        title_sort_lock: merge_bool("titleSortLock", existing.title_sort_lock),
        summary: merge_str("summary", &existing.summary),
        summary_lock: merge_bool("summaryLock", existing.summary_lock),
        reading_direction: merge_nullable_str("readingDirection", &existing.reading_direction),
        reading_direction_lock: merge_bool("readingDirectionLock", existing.reading_direction_lock),
        publisher: merge_str("publisher", &existing.publisher),
        publisher_lock: merge_bool("publisherLock", existing.publisher_lock),
        age_rating: merge_nullable_u32("ageRating", existing.age_rating),
        age_rating_lock: merge_bool("ageRatingLock", existing.age_rating_lock),
        language: merge_str("language", &existing.language),
        language_lock: merge_bool("languageLock", existing.language_lock),
        genres: merge_string_list_field(body, "genres", &existing.genres),
        genres_lock: merge_bool("genresLock", existing.genres_lock),
        tags: merge_string_list_field(body, "tags", &existing.tags),
        tags_lock: merge_bool("tagsLock", existing.tags_lock),
        total_book_count: merge_nullable_u32("totalBookCount", existing.total_book_count),
        total_book_count_lock: merge_bool("totalBookCountLock", existing.total_book_count_lock),
        sharing_labels: merge_string_list_field(body, "sharingLabels", &existing.sharing_labels),
        sharing_labels_lock: merge_bool("sharingLabelsLock", existing.sharing_labels_lock),
        links: merge_links_field(body, "links", &existing.links),
        links_lock: merge_bool("linksLock", existing.links_lock),
        alternate_titles: merge_alternate_titles_field(
            body,
            "alternateTitles",
            &existing.alternate_titles,
        ),
        alternate_titles_lock: merge_bool("alternateTitlesLock", existing.alternate_titles_lock),
    }
}

fn validate_series_metadata_patch(body: &serde_json::Map<String, Value>) -> Result<(), Response> {
    validate_optional_enum(body, "status", &["ENDED", "ONGOING", "ABANDONED", "HIATUS"])?;
    validate_optional_bool(body, "statusLock")?;
    validate_optional_non_blank_string(body, "title")?;
    validate_optional_bool(body, "titleLock")?;
    validate_optional_non_blank_string(body, "titleSort")?;
    validate_optional_bool(body, "titleSortLock")?;
    validate_optional_string(body, "summary")?;
    validate_optional_bool(body, "summaryLock")?;
    validate_optional_enum(
        body,
        "readingDirection",
        &["LEFT_TO_RIGHT", "RIGHT_TO_LEFT", "VERTICAL", "WEBTOON"],
    )?;
    validate_optional_bool(body, "readingDirectionLock")?;
    validate_optional_string(body, "publisher")?;
    validate_optional_bool(body, "publisherLock")?;
    validate_optional_non_negative_u32(body, "ageRating")?;
    validate_optional_bool(body, "ageRatingLock")?;
    validate_optional_language(body, "language")?;
    validate_optional_bool(body, "languageLock")?;
    validate_optional_string_array(body, "genres")?;
    validate_optional_bool(body, "genresLock")?;
    validate_optional_string_array(body, "tags")?;
    validate_optional_bool(body, "tagsLock")?;
    validate_optional_positive_i32_range(body, "totalBookCount")?;
    validate_optional_bool(body, "totalBookCountLock")?;
    validate_optional_string_array(body, "sharingLabels")?;
    validate_optional_bool(body, "sharingLabelsLock")?;
    validate_links_array(body, "links")?;
    validate_optional_bool(body, "linksLock")?;
    validate_alternate_titles_array(body, "alternateTitles")?;
    validate_optional_bool(body, "alternateTitlesLock")?;
    Ok(())
}

fn merge_string_list_field(
    body: &serde_json::Map<String, Value>,
    key: &str,
    existing: &[String],
) -> Vec<String> {
    if !body.contains_key(key) {
        return existing.to_vec();
    }

    body.get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn validate_optional_bool(
    body: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<(), Response> {
    if body.contains_key(key)
        && !body
            .get(key)
            .is_some_and(|value| value.is_null() || value.is_boolean())
    {
        return Err(bad_request_response(&format!(
            "{key} must be a boolean or null"
        )));
    }
    Ok(())
}

fn validate_optional_string(
    body: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<(), Response> {
    if body.contains_key(key)
        && !body
            .get(key)
            .is_some_and(|value| value.is_null() || value.is_string())
    {
        return Err(bad_request_response(&format!(
            "{key} must be a string or null"
        )));
    }
    Ok(())
}

fn validate_optional_non_blank_string(
    body: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<(), Response> {
    validate_optional_string(body, key)?;
    if let Some(value) = body.get(key).and_then(Value::as_str)
        && value.trim().is_empty()
    {
        return Err(bad_request_response(&format!("{key} must not be blank")));
    }
    Ok(())
}

fn validate_optional_language(
    body: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<(), Response> {
    validate_optional_string(body, key)?;
    if let Some(value) = body.get(key).and_then(Value::as_str)
        && !value.trim().is_empty()
        && LanguageTag::parse(value).is_err()
    {
        return Err(bad_request_response(&format!(
            "{key} must be blank or a valid BCP47 language tag"
        )));
    }
    Ok(())
}

fn validate_optional_enum(
    body: &serde_json::Map<String, Value>,
    key: &str,
    allowed: &[&str],
) -> Result<(), Response> {
    validate_optional_string(body, key)?;
    if let Some(value) = body.get(key).and_then(Value::as_str)
        && !allowed.iter().any(|candidate| candidate == &value)
    {
        return Err(bad_request_response(&format!("{key} has an invalid value")));
    }
    Ok(())
}

fn validate_optional_non_negative_u32(
    body: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<(), Response> {
    if !body.contains_key(key) || body.get(key).is_some_and(Value::is_null) {
        return Ok(());
    }
    let Some(value) = body.get(key).and_then(Value::as_i64) else {
        return Err(bad_request_response(&format!(
            "{key} must be an integer or null"
        )));
    };
    if !(0..=i64::from(i32::MAX)).contains(&value) {
        return Err(bad_request_response(&format!(
            "{key} must be between 0 and {}",
            i32::MAX
        )));
    }
    Ok(())
}

fn validate_optional_positive_i32_range(
    body: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<(), Response> {
    if !body.contains_key(key) || body.get(key).is_some_and(Value::is_null) {
        return Ok(());
    }
    let Some(value) = body.get(key).and_then(Value::as_i64) else {
        return Err(bad_request_response(&format!(
            "{key} must be a positive integer or null"
        )));
    };
    if !(1..=i64::from(i32::MAX)).contains(&value) {
        return Err(bad_request_response(&format!(
            "{key} must be a positive integer"
        )));
    }
    Ok(())
}

fn validate_optional_string_array(
    body: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<(), Response> {
    if !body.contains_key(key) || body.get(key).is_some_and(Value::is_null) {
        return Ok(());
    }
    let Some(values) = body.get(key).and_then(Value::as_array) else {
        return Err(bad_request_response(&format!(
            "{key} must be an array or null"
        )));
    };
    if values.iter().any(|value| value.as_str().is_none()) {
        return Err(bad_request_response(&format!(
            "{key} entries must be strings"
        )));
    }
    Ok(())
}

fn validate_links_array(body: &serde_json::Map<String, Value>, key: &str) -> Result<(), Response> {
    if !body.contains_key(key) || body.get(key).is_some_and(Value::is_null) {
        return Ok(());
    }
    let Some(values) = body.get(key).and_then(Value::as_array) else {
        return Err(bad_request_response(&format!(
            "{key} must be an array or null"
        )));
    };

    for value in values {
        let Some(object) = value.as_object() else {
            return Err(bad_request_response("links entries must be objects"));
        };
        let Some(label) = object.get("label").and_then(Value::as_str) else {
            return Err(bad_request_response("links.label must be a string"));
        };
        if label.trim().is_empty() {
            return Err(bad_request_response("links.label must not be blank"));
        }
        let Some(url) = object.get("url").and_then(Value::as_str) else {
            return Err(bad_request_response("links.url must be a string"));
        };
        if Url::parse(url).is_err() {
            return Err(bad_request_response("links.url must be a valid URL"));
        }
    }

    Ok(())
}

fn validate_alternate_titles_array(
    body: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<(), Response> {
    if !body.contains_key(key) || body.get(key).is_some_and(Value::is_null) {
        return Ok(());
    }
    let Some(values) = body.get(key).and_then(Value::as_array) else {
        return Err(bad_request_response(&format!(
            "{key} must be an array or null"
        )));
    };

    for value in values {
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
        if label.trim().is_empty() {
            return Err(bad_request_response(
                "alternateTitles.label must not be blank",
            ));
        }
        let Some(title) = object.get("title").and_then(Value::as_str) else {
            return Err(bad_request_response(
                "alternateTitles.title must be a string",
            ));
        };
        if title.trim().is_empty() {
            return Err(bad_request_response(
                "alternateTitles.title must not be blank",
            ));
        }
    }

    Ok(())
}

fn bad_request_response(message: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))).into_response()
}

fn merge_links_field(
    body: &serde_json::Map<String, Value>,
    key: &str,
    existing: &[SeriesMetadataLinkRecord],
) -> Vec<SeriesMetadataLinkRecord> {
    if !body.contains_key(key) {
        return existing.to_vec();
    }

    body.get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_object)
                .filter_map(|value| {
                    Some(SeriesMetadataLinkRecord {
                        label: value.get("label")?.as_str()?.to_string(),
                        url: value.get("url")?.as_str()?.to_string(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn merge_alternate_titles_field(
    body: &serde_json::Map<String, Value>,
    key: &str,
    existing: &[SeriesAlternateTitleRecord],
) -> Vec<SeriesAlternateTitleRecord> {
    if !body.contains_key(key) {
        return existing.to_vec();
    }

    body.get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_object)
                .filter_map(|value| {
                    Some(SeriesAlternateTitleRecord {
                        label: value.get("label")?.as_str()?.to_string(),
                        title: value.get("title")?.as_str()?.to_string(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

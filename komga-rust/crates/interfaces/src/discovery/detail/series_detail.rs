#![allow(clippy::result_large_err)]

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

    let status = body
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or(existing.status.as_str())
        .to_string();
    let status_lock = body
        .get("statusLock")
        .and_then(Value::as_bool)
        .unwrap_or(existing.status_lock);
    let title = body
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(existing.title.as_str())
        .to_string();
    let title_lock = body
        .get("titleLock")
        .and_then(Value::as_bool)
        .unwrap_or(existing.title_lock);
    let title_sort = body
        .get("titleSort")
        .and_then(Value::as_str)
        .unwrap_or(existing.title_sort.as_str())
        .to_string();
    let title_sort_lock = body
        .get("titleSortLock")
        .and_then(Value::as_bool)
        .unwrap_or(existing.title_sort_lock);
    let summary = body
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or(existing.summary.as_str())
        .to_string();
    let summary_lock = body
        .get("summaryLock")
        .and_then(Value::as_bool)
        .unwrap_or(existing.summary_lock);
    let language = body
        .get("language")
        .and_then(Value::as_str)
        .unwrap_or(existing.language.as_str())
        .to_string();
    let language_lock = body
        .get("languageLock")
        .and_then(Value::as_bool)
        .unwrap_or(existing.language_lock);
    let publisher = body
        .get("publisher")
        .and_then(Value::as_str)
        .unwrap_or(existing.publisher.as_str())
        .to_string();
    let publisher_lock = body
        .get("publisherLock")
        .and_then(Value::as_bool)
        .unwrap_or(existing.publisher_lock);
    let reading_direction = if body.contains_key("readingDirection") {
        body.get("readingDirection")
            .and_then(Value::as_str)
            .map(str::to_string)
    } else {
        existing.reading_direction.clone()
    };
    let reading_direction_lock = body
        .get("readingDirectionLock")
        .and_then(Value::as_bool)
        .unwrap_or(existing.reading_direction_lock);
    let age_rating = if body.contains_key("ageRating") {
        body.get("ageRating")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
    } else {
        existing.age_rating
    };
    let age_rating_lock = body
        .get("ageRatingLock")
        .and_then(Value::as_bool)
        .unwrap_or(existing.age_rating_lock);
    let genres = merge_string_list_field(body, "genres", &existing.genres);
    let genres_lock = body
        .get("genresLock")
        .and_then(Value::as_bool)
        .unwrap_or(existing.genres_lock);
    let tags = merge_string_list_field(body, "tags", &existing.tags);
    let tags_lock = body
        .get("tagsLock")
        .and_then(Value::as_bool)
        .unwrap_or(existing.tags_lock);
    let sharing_labels = merge_string_list_field(body, "sharingLabels", &existing.sharing_labels);
    let sharing_labels_lock = body
        .get("sharingLabelsLock")
        .and_then(Value::as_bool)
        .unwrap_or(existing.sharing_labels_lock);
    let links = merge_links_field(body, "links", &existing.links);
    let links_lock = body
        .get("linksLock")
        .and_then(Value::as_bool)
        .unwrap_or(existing.links_lock);
    let alternate_titles =
        merge_alternate_titles_field(body, "alternateTitles", &existing.alternate_titles);
    let alternate_titles_lock = body
        .get("alternateTitlesLock")
        .and_then(Value::as_bool)
        .unwrap_or(existing.alternate_titles_lock);
    let total_book_count = if body.contains_key("totalBookCount") {
        body.get("totalBookCount")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
    } else {
        existing.total_book_count
    };
    let total_book_count_lock = body
        .get("totalBookCountLock")
        .and_then(Value::as_bool)
        .unwrap_or(existing.total_book_count_lock);

    let update = SeriesMetadataUpdateRecord {
        status,
        status_lock,
        title,
        title_lock,
        title_sort,
        title_sort_lock,
        summary,
        summary_lock,
        reading_direction,
        reading_direction_lock,
        publisher,
        publisher_lock,
        age_rating,
        age_rating_lock,
        language,
        language_lock,
        genres,
        genres_lock,
        tags,
        tags_lock,
        total_book_count,
        total_book_count_lock,
        sharing_labels,
        sharing_labels_lock,
        links,
        links_lock,
        alternate_titles,
        alternate_titles_lock,
    };

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

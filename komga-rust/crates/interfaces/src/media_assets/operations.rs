use super::*;
use crate::state::HttpAppState;
use axum::extract::State;
use komga_application::media_assets::{
    BookMetadataAuthor as ApplicationBookMetadataAuthor,
    BookMetadataLink as ApplicationBookMetadataLink,
    BookMetadataPatch as ApplicationBookMetadataPatch,
};
use komga_application::runtime_sse::register_runtime_sse_event;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct BooksThumbnailsRegenerateQuery {
    #[serde(default)]
    pub for_bigger_result_only: bool,
}

pub async fn book_analyze(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&*app.services.runtime_identity, &headers) {
        return response;
    }

    let Some(book) = (match app
        .services
        .discovery_detail
        .load_persisted_book_detail(book_id.clone(), None)
        .await
    {
        Ok(book) => book,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    enqueue_task_records(
        &app,
        vec![
            TaskQueueRecord::new(format!("AnalyzeBook_{book_id}"), 6, Some(book.series_id))
                .with_simple_type("AnalyzeBook"),
        ],
    )
    .await
}

pub async fn book_metadata_refresh(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&*app.services.runtime_identity, &headers) {
        return response;
    }

    let Some(book) = (match app
        .services
        .discovery_detail
        .load_persisted_book_detail(book_id.clone(), None)
        .await
    {
        Ok(book) => book,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    enqueue_task_records(
        &app,
        vec![
            TaskQueueRecord::new(
                format!("RefreshBookMetadata_{book_id}"),
                6,
                Some(book.series_id.clone()),
            )
            .with_simple_type("RefreshBookMetadata"),
            TaskQueueRecord::new(format!("RefreshBookLocalArtwork_{book_id}"), 6, None)
                .with_simple_type("RefreshBookLocalArtwork"),
        ],
    )
    .await
}

pub async fn book_metadata_update(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if let Some(response) = require_admin(&*app.services.runtime_identity, &headers) {
        return response;
    }

    let patch = match body.as_object() {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "book metadata update payload must be a JSON object" })),
            )
                .into_response();
        }
    };

    let patch = match parse_book_metadata_patch(patch) {
        Ok(value) => value,
        Err(error) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": error }))).into_response();
        }
    };

    let service = app.services.media_assets.book_metadata_service();

    match service.update_book_metadata(&book_id, &patch).await {
        Ok(Some(series_id)) => {
            if let Err(error) = app
                .services
                .media_assets
                .refresh_book_search_documents_after_metadata_update(
                    app.operational.runtime.lucene_data_directory.clone(),
                    book_id.clone(),
                )
                .await
            {
                return internal_error_response(error);
            }

            if let Some(series_id) = series_id {
                let task = TaskQueueRecord::new(
                    format!("AggregateSeriesMetadata_{series_id}"),
                    80,
                    Some(series_id),
                )
                .with_simple_type("AggregateSeriesMetadata");
                if let Err(error) = process_task_side_effects(&app, vec![task]).await {
                    return internal_error_response(error);
                }
            }

            if let Ok(Some(book)) = app
                .services
                .discovery_detail
                .load_persisted_book_detail(book_id.clone(), None)
                .await
            {
                register_runtime_sse_event(
                    "BookChanged",
                    json!({
                        "bookId": book.id,
                        "seriesId": book.series_id,
                        "libraryId": book.library_id,
                    }),
                    false,
                    None,
                );
            }

            let mut response = StatusCode::NO_CONTENT.into_response();
            mark_runtime_owned(&mut response);
            response
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn book_metadata_batch_update(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if let Some(response) = require_admin(&*app.services.runtime_identity, &headers) {
        return response;
    }

    let batch = match body.as_object() {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "book metadata batch update payload must be a JSON object map",
                })),
            )
                .into_response();
        }
    };

    let mut updates = Vec::with_capacity(batch.len());
    for (book_id, patch_value) in batch {
        let patch = match patch_value.as_object() {
            Some(value) => value,
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": format!("book metadata patch for {book_id} must be a JSON object"),
                    })),
                )
                    .into_response();
            }
        };

        let patch = match parse_book_metadata_patch(patch) {
            Ok(value) => value,
            Err(error) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": format!("invalid metadata patch for {book_id}: {error}"),
                    })),
                )
                    .into_response();
            }
        };

        updates.push((book_id.clone(), patch));
    }

    let service = app.services.media_assets.book_metadata_service();
    let updated_book_ids = updates
        .iter()
        .map(|(book_id, _)| book_id.clone())
        .collect::<Vec<_>>();
    let affected_series_ids = match service.batch_update_book_metadata(updates).await {
        Ok(series_ids) => series_ids.into_iter().collect::<BTreeSet<_>>(),
        Err(error) => return internal_error_response(error),
    };

    if !affected_series_ids.is_empty() {
        let tasks = affected_series_ids
            .into_iter()
            .map(|series_id| {
                TaskQueueRecord::new(
                    format!("AggregateSeriesMetadata_{series_id}"),
                    80,
                    Some(series_id),
                )
                .with_simple_type("AggregateSeriesMetadata")
            })
            .collect::<Vec<_>>();
        if let Err(error) = process_task_side_effects(&app, tasks).await {
            return internal_error_response(error);
        }
    }

    for updated_book_id in updated_book_ids {
        if let Ok(Some(book)) = app
            .services
            .discovery_detail
            .load_persisted_book_detail(updated_book_id.clone(), None)
            .await
        {
            register_runtime_sse_event(
                "BookChanged",
                json!({
                    "bookId": book.id,
                    "seriesId": book.series_id,
                    "libraryId": book.library_id,
                }),
                false,
                None,
            );
        }
    }

    let mut response = StatusCode::NO_CONTENT.into_response();
    mark_runtime_owned(&mut response);
    response
}

fn parse_book_metadata_patch(
    patch: &serde_json::Map<String, Value>,
) -> Result<ApplicationBookMetadataPatch, String> {
    Ok(ApplicationBookMetadataPatch {
        title: optional_non_blank_string(patch, "title")?,
        title_lock: optional_bool(patch, "titleLock")?,
        summary: optional_nullable_string(patch, "summary")?,
        summary_lock: optional_bool(patch, "summaryLock")?,
        number: optional_non_blank_string(patch, "number")?,
        number_lock: optional_bool(patch, "numberLock")?,
        number_sort: optional_f64(patch, "numberSort")?,
        number_sort_lock: optional_bool(patch, "numberSortLock")?,
        release_date: optional_nullable_string(patch, "releaseDate")?,
        release_date_lock: optional_bool(patch, "releaseDateLock")?,
        authors: optional_authors(patch, "authors")?,
        authors_lock: optional_bool(patch, "authorsLock")?,
        tags: optional_string_vec(patch, "tags")?,
        tags_lock: optional_bool(patch, "tagsLock")?,
        isbn: optional_nullable_isbn(patch, "isbn")?,
        isbn_lock: optional_bool(patch, "isbnLock")?,
        links: optional_links(patch, "links")?,
        links_lock: optional_bool(patch, "linksLock")?,
    })
}

fn optional_bool(
    patch: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<bool>, String> {
    match patch.get(key) {
        Some(value) if value.is_null() => Ok(None),
        Some(value) => value
            .as_bool()
            .map(Some)
            .ok_or_else(|| format!("{key} must be a boolean or null")),
        None => Ok(None),
    }
}

fn optional_f64(patch: &serde_json::Map<String, Value>, key: &str) -> Result<Option<f64>, String> {
    match patch.get(key) {
        Some(value) if value.is_null() => Ok(None),
        Some(value) => value
            .as_f64()
            .map(Some)
            .ok_or_else(|| format!("{key} must be a number or null")),
        None => Ok(None),
    }
}

fn optional_non_blank_string(
    patch: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, String> {
    match patch.get(key) {
        Some(value) if value.is_null() => Ok(None),
        Some(value) => {
            let value = value
                .as_str()
                .ok_or_else(|| format!("{key} must be a string or null"))?;
            if value.trim().is_empty() {
                return Err(format!("{key} must not be blank"));
            }
            Ok(Some(value.to_string()))
        }
        None => Ok(None),
    }
}

fn optional_nullable_string(
    patch: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Option<String>>, String> {
    match patch.get(key) {
        Some(value) if value.is_null() => Ok(Some(None)),
        Some(value) => value
            .as_str()
            .map(|value| Some(Some(value.to_string())))
            .ok_or_else(|| format!("{key} must be a string or null")),
        None => Ok(None),
    }
}

fn optional_nullable_isbn(
    patch: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Option<String>>, String> {
    match patch.get(key) {
        Some(value) if value.is_null() => Ok(Some(None)),
        Some(value) => {
            let value = value
                .as_str()
                .ok_or_else(|| format!("{key} must be a string or null"))?;
            if !value.trim().is_empty() && !is_valid_isbn13(value) {
                return Err(format!("{key} must be null, blank, or a valid ISBN-13"));
            }
            Ok(Some(Some(value.to_string())))
        }
        None => Ok(None),
    }
}

fn is_valid_isbn13(value: &str) -> bool {
    let digits = value
        .chars()
        .filter_map(|character| character.to_digit(10))
        .collect::<Vec<_>>();
    if digits.len() != 13 {
        return false;
    }

    let checksum = digits
        .iter()
        .take(12)
        .enumerate()
        .map(|(index, digit)| if index % 2 == 0 { *digit } else { digit * 3 })
        .sum::<u32>();
    let expected_check_digit = (10 - (checksum % 10)) % 10;

    digits[12] == expected_check_digit
}

fn optional_string_vec(
    patch: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Vec<String>>, String> {
    match patch.get(key) {
        Some(value) if value.is_null() => Ok(Some(Vec::new())),
        Some(value) => value
            .as_array()
            .ok_or_else(|| format!("{key} must be an array or null"))?
            .iter()
            .map(|entry| {
                entry
                    .as_str()
                    .map(ToString::to_string)
                    .ok_or_else(|| format!("{key} entries must be strings"))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        None => Ok(None),
    }
}

fn optional_authors(
    patch: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Vec<ApplicationBookMetadataAuthor>>, String> {
    match patch.get(key) {
        Some(value) if value.is_null() => Ok(Some(Vec::new())),
        Some(value) => value
            .as_array()
            .ok_or_else(|| format!("{key} must be an array or null"))?
            .iter()
            .map(|entry| {
                let entry = entry
                    .as_object()
                    .ok_or_else(|| format!("{key} entries must be objects"))?;
                let name = entry
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "author.name must be a non-empty string".to_string())?;
                let role = entry
                    .get("role")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "author.role must be a non-empty string".to_string())?;
                if name.trim().is_empty() || role.trim().is_empty() {
                    return Err("author name/role must not be blank".to_string());
                }
                Ok(ApplicationBookMetadataAuthor {
                    name: name.to_string(),
                    role: role.to_string(),
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        None => Ok(None),
    }
}

fn optional_links(
    patch: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Vec<ApplicationBookMetadataLink>>, String> {
    match patch.get(key) {
        Some(value) if value.is_null() => Ok(Some(Vec::new())),
        Some(value) => value
            .as_array()
            .ok_or_else(|| format!("{key} must be an array or null"))?
            .iter()
            .map(|entry| {
                let entry = entry
                    .as_object()
                    .ok_or_else(|| format!("{key} entries must be objects"))?;
                let label = entry
                    .get("label")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "links.label must be a string".to_string())?;
                let url = entry
                    .get("url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "links.url must be a string".to_string())?;
                if label.trim().is_empty() {
                    return Err("links.label must not be blank".to_string());
                }
                if url.trim().is_empty() || reqwest::Url::parse(url).is_err() {
                    return Err("links.url must be a valid URL".to_string());
                }
                Ok(ApplicationBookMetadataLink {
                    label: label.to_string(),
                    url: url.to_string(),
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        None => Ok(None),
    }
}

pub async fn books_thumbnails_regenerate(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Query(query): Query<BooksThumbnailsRegenerateQuery>,
) -> Response {
    if let Some(response) = require_admin(&*app.services.runtime_identity, &headers) {
        return response;
    }

    enqueue_task_records(
        &app,
        vec![
            TaskQueueRecord::new("FindBookThumbnailsToRegenerate", 0, None).with_payload(
                json!({
                    "for_bigger_result_only": query.for_bigger_result_only,
                })
                .to_string(),
            ),
        ],
    )
    .await
}

pub async fn series_file_delete(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&*app.services.runtime_identity, &headers) {
        return response;
    }

    enqueue_delete_media_task(&app, format!("DeleteSeries_{series_id}"), "DeleteSeries", 8).await
}

pub async fn series_analyze(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&*app.services.runtime_identity, &headers) {
        return response;
    }

    let resolved_series_id = resolve_series_id_for_persisted(&app, &series_id).await;

    let book_ids = match app
        .services
        .media_assets
        .load_series_book_ids(resolved_series_id.clone())
        .await
    {
        Ok(book_ids) => book_ids,
        Err(error) => return internal_error_response(error),
    };
    let task_records = book_ids
        .into_iter()
        .map(|book_id| {
            TaskQueueRecord::new(
                format!("AnalyzeBook_{book_id}"),
                6,
                Some(resolved_series_id.clone()),
            )
            .with_simple_type("AnalyzeBook")
        })
        .collect::<Vec<_>>();

    enqueue_task_records(&app, task_records).await
}

pub async fn series_metadata_refresh(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&*app.services.runtime_identity, &headers) {
        return response;
    }

    let book_ids = match app
        .services
        .media_assets
        .load_series_book_ids(series_id.clone())
        .await
    {
        Ok(book_ids) => book_ids,
        Err(error) => return internal_error_response(error),
    };

    let mut task_records = vec![];
    for book_id in book_ids {
        task_records.push(
            TaskQueueRecord::new(
                format!("RefreshBookMetadata_{book_id}"),
                6,
                Some(series_id.clone()),
            )
            .with_simple_type("RefreshBookMetadata"),
        );
        task_records.push(
            TaskQueueRecord::new(format!("RefreshBookLocalArtwork_{book_id}"), 6, None)
                .with_simple_type("RefreshBookLocalArtwork"),
        );
    }
    task_records.push(
        TaskQueueRecord::new(format!("RefreshSeriesLocalArtwork_{series_id}"), 6, None)
            .with_simple_type("RefreshSeriesLocalArtwork"),
    );

    enqueue_task_records(&app, task_records).await
}

pub async fn book_file_delete(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&*app.services.runtime_identity, &headers) {
        return response;
    }

    enqueue_delete_media_task(&app, format!("DeleteBook_{book_id}"), "DeleteBook", 8).await
}

async fn enqueue_delete_media_task(
    app: &HttpAppState,
    task_id: String,
    simple_type: &'static str,
    priority: i32,
) -> Response {
    enqueue_task_records(
        app,
        vec![TaskQueueRecord::new(task_id, priority, None).with_simple_type(simple_type)],
    )
    .await
}

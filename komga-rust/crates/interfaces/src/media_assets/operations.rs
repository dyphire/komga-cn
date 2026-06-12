use crate::identity_access::auth::Admin;
use crate::state::MediaAssetsState;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use komga_application::discovery::resolve_persisted_series_id;
use komga_application::media_assets::{
    BookMetadataAuthor, BookMetadataLink, BookMetadataPatch, BookMetadataUpdate,
    BookMetadataUpdateError, MetadataUpdateResult,
};
use komga_application::task_processing::{
    TaskKind, TaskQueueAdmin, TaskRequest, book_analyze_task_record,
    book_metadata_refresh_task_records, series_analyze_task_records,
    series_metadata_refresh_task_records,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::enqueue_task_records;
use super::http_helpers::internal_error_response;

#[derive(Deserialize)]
pub(crate) struct BooksThumbnailsRegenerateQuery {
    #[serde(default)]
    pub(crate) for_bigger_result_only: bool,
}

pub(crate) async fn book_analyze(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path(book_id): Path<String>,
) -> Response {
    match app
        .book_detail
        .load_persisted_book_detail(&book_id, None)
        .await
    {
        Ok(Some(book)) => {
            enqueue_task_records(
                app.task_queue.queue.as_ref(),
                vec![book_analyze_task_record(&book_id, &book.series_id)],
            )
            .await
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn book_metadata_refresh(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path(book_id): Path<String>,
) -> Response {
    match app
        .book_detail
        .load_persisted_book_detail(&book_id, None)
        .await
    {
        Ok(Some(book)) => {
            enqueue_task_records(
                app.task_queue.queue.as_ref(),
                book_metadata_refresh_task_records(&book_id, &book.series_id),
            )
            .await
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn book_metadata_update(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path(book_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
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

    match app.metadata.update_book(&book_id, &patch).await {
        Ok(MetadataUpdateResult::Updated) => StatusCode::NO_CONTENT.into_response(),
        Ok(MetadataUpdateResult::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => metadata_update_error_response(error),
    }
}

pub(crate) async fn book_metadata_batch_update(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Json(body): Json<Value>,
) -> Response {
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

        updates.push(BookMetadataUpdate {
            book_id: book_id.clone(),
            patch,
        });
    }

    match app.metadata.update_books_batch(updates).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => metadata_update_error_response(error),
    }
}

fn metadata_update_error_response(error: BookMetadataUpdateError) -> Response {
    match error {
        BookMetadataUpdateError::Validation(error) => {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": error }))).into_response()
        }
        BookMetadataUpdateError::Persistence(error) => internal_error_response(error),
    }
}

fn parse_book_metadata_patch(
    patch: &serde_json::Map<String, Value>,
) -> Result<BookMetadataPatch, String> {
    Ok(BookMetadataPatch {
        title: optional_string(patch, "title")?,
        title_lock: optional_bool(patch, "titleLock")?,
        summary: optional_nullable_string(patch, "summary")?,
        summary_lock: optional_bool(patch, "summaryLock")?,
        number: optional_string(patch, "number")?,
        number_lock: optional_bool(patch, "numberLock")?,
        number_sort: optional_f64(patch, "numberSort")?,
        number_sort_lock: optional_bool(patch, "numberSortLock")?,
        release_date: optional_nullable_string(patch, "releaseDate")?,
        release_date_lock: optional_bool(patch, "releaseDateLock")?,
        authors: optional_authors(patch, "authors")?,
        authors_lock: optional_bool(patch, "authorsLock")?,
        tags: optional_string_vec(patch, "tags")?,
        tags_lock: optional_bool(patch, "tagsLock")?,
        isbn: optional_nullable_string(patch, "isbn")?,
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

fn optional_string(
    patch: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, String> {
    match patch.get(key) {
        Some(value) if value.is_null() => Ok(None),
        Some(value) => value
            .as_str()
            .map(|value| Some(value.to_string()))
            .ok_or_else(|| format!("{key} must be a string or null")),
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
) -> Result<Option<Vec<BookMetadataAuthor>>, String> {
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
                    .ok_or_else(|| "author.name must be a string".to_string())?;
                let role = entry
                    .get("role")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "author.role must be a string".to_string())?;
                Ok(BookMetadataAuthor {
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
) -> Result<Option<Vec<BookMetadataLink>>, String> {
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
                Ok(BookMetadataLink {
                    label: label.to_string(),
                    url: url.to_string(),
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        None => Ok(None),
    }
}

pub(crate) async fn books_thumbnails_regenerate(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Query(query): Query<BooksThumbnailsRegenerateQuery>,
) -> Response {
    enqueue_task_records(
        app.task_queue.queue.as_ref(),
        vec![
            TaskRequest::new(TaskKind::FindBookThumbnailsToRegenerate)
                .into_queue_record()
                .with_payload(
                    json!({
                        "for_bigger_result_only": query.for_bigger_result_only,
                    })
                    .to_string(),
                ),
        ],
    )
    .await
}

pub(crate) async fn series_file_delete(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path(series_id): Path<String>,
) -> Response {
    enqueue_delete_media_task(
        app.task_queue.queue.as_ref(),
        TaskKind::DeleteSeries,
        &series_id,
        8,
    )
    .await
}

pub(crate) async fn series_analyze(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path(series_id): Path<String>,
) -> Response {
    let resolved_series_id =
        resolve_persisted_series_id(app.series_id_resolver.as_ref(), &series_id).await;

    match app
        .series_relation
        .series_book_ids(&resolved_series_id)
        .await
    {
        Ok(book_ids) => {
            enqueue_task_records(
                app.task_queue.queue.as_ref(),
                series_analyze_task_records(book_ids, &resolved_series_id),
            )
            .await
        }
        Err(error) => internal_error_response(error),
    }
}

pub(crate) async fn series_metadata_refresh(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path(series_id): Path<String>,
) -> Response {
    match app.series_relation.series_book_ids(&series_id).await {
        Ok(book_ids) => {
            enqueue_task_records(
                app.task_queue.queue.as_ref(),
                series_metadata_refresh_task_records(book_ids, &series_id),
            )
            .await
        }
        Err(error) => internal_error_response(error),
    }
}

pub(crate) async fn book_file_delete(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path(book_id): Path<String>,
) -> Response {
    enqueue_delete_media_task(
        app.task_queue.queue.as_ref(),
        TaskKind::DeleteBook,
        &book_id,
        8,
    )
    .await
}

async fn enqueue_delete_media_task(
    task_queue: &dyn TaskQueueAdmin,
    kind: TaskKind,
    target_id: &str,
    priority: i32,
) -> Response {
    enqueue_task_records(
        task_queue,
        vec![
            TaskRequest::new(kind)
                .priority(priority)
                .into_queue_record_with_id(target_id),
        ],
    )
    .await
}

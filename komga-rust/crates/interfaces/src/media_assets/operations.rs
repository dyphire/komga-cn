use super::*;
use crate::identity_access::auth::Admin;
use crate::state::MediaAssetsState;
use axum::extract::State;
use komga_application::media_assets::{
    BookMetadataAuthor as ApplicationBookMetadataAuthor,
    BookMetadataLink as ApplicationBookMetadataLink,
    BookMetadataPatch as ApplicationBookMetadataPatch, MetadataUpdateResult,
};

#[derive(Deserialize)]
pub struct BooksThumbnailsRegenerateQuery {
    #[serde(default)]
    pub for_bigger_result_only: bool,
}

pub async fn book_analyze(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path(book_id): Path<String>,
) -> Response {
    let Some(book) = (match app
        .book_access
        .load_persisted_book_detail(&book_id, None)
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
            TaskRequest::with_payload(TaskKind::AnalyzeBook, BookPayload::new(&book_id))
                .priority(6)
                .group(book.series_id)
                .into_queue_record(),
        ],
    )
    .await
}

pub async fn book_metadata_refresh(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path(book_id): Path<String>,
) -> Response {
    let Some(book) = (match app
        .book_access
        .load_persisted_book_detail(&book_id, None)
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
            TaskRequest::with_payload(TaskKind::RefreshBookMetadata, BookPayload::new(&book_id))
                .priority(6)
                .group(book.series_id.clone())
                .into_queue_record(),
            TaskRequest::with_payload(
                TaskKind::RefreshBookLocalArtwork,
                BookPayload::new(&book_id),
            )
            .priority(6)
            .into_queue_record(),
        ],
    )
    .await
}

pub async fn book_metadata_update(
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
        Ok(MetadataUpdateResult::Updated) => {
            let mut response = StatusCode::NO_CONTENT.into_response();
            mark_runtime_owned(&mut response);
            response
        }
        Ok(MetadataUpdateResult::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn book_metadata_batch_update(
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

        updates.push((book_id.clone(), patch));
    }

    match app.metadata.update_books_batch(updates).await {
        Ok(_) => {
            let mut response = StatusCode::NO_CONTENT.into_response();
            mark_runtime_owned(&mut response);
            response
        }
        Err(error) => internal_error_response(error),
    }
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
    State(app): State<MediaAssetsState>,
    _: Admin,
    Query(query): Query<BooksThumbnailsRegenerateQuery>,
) -> Response {
    enqueue_task_records(
        &app,
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

pub async fn series_file_delete(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path(series_id): Path<String>,
) -> Response {
    enqueue_delete_media_task(&app, TaskKind::DeleteSeries, &series_id, 8).await
}

pub async fn series_analyze(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path(series_id): Path<String>,
) -> Response {
    let resolved_series_id = resolve_series_id_for_persisted(&app, &series_id).await;

    let book_ids = match app.reader.series_book_ids(&resolved_series_id).await {
        Ok(book_ids) => book_ids,
        Err(error) => return internal_error_response(error),
    };
    let task_records = book_ids
        .into_iter()
        .map(|book_id| {
            TaskRequest::with_payload(TaskKind::AnalyzeBook, BookPayload::new(book_id))
                .priority(6)
                .group(resolved_series_id.clone())
                .into_queue_record()
        })
        .collect::<Vec<_>>();

    enqueue_task_records(&app, task_records).await
}

pub async fn series_metadata_refresh(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path(series_id): Path<String>,
) -> Response {
    let book_ids = match app.reader.series_book_ids(&series_id).await {
        Ok(book_ids) => book_ids,
        Err(error) => return internal_error_response(error),
    };

    let mut task_records = vec![];
    for book_id in book_ids {
        task_records.push(
            TaskRequest::with_payload(TaskKind::RefreshBookMetadata, BookPayload::new(&book_id))
                .priority(6)
                .group(series_id.clone())
                .into_queue_record(),
        );
        task_records.push(
            TaskRequest::with_payload(
                TaskKind::RefreshBookLocalArtwork,
                BookPayload::new(&book_id),
            )
            .priority(6)
            .into_queue_record(),
        );
    }
    task_records.push(
        TaskRequest::with_payload(
            TaskKind::RefreshSeriesLocalArtwork,
            SeriesPayload::new(&series_id),
        )
        .priority(6)
        .into_queue_record(),
    );

    enqueue_task_records(&app, task_records).await
}

pub async fn book_file_delete(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path(book_id): Path<String>,
) -> Response {
    enqueue_delete_media_task(&app, TaskKind::DeleteBook, &book_id, 8).await
}

async fn enqueue_delete_media_task(
    app: &MediaAssetsState,
    kind: TaskKind,
    target_id: &str,
    priority: i32,
) -> Response {
    enqueue_task_records(
        app,
        vec![
            TaskRequest::new(kind)
                .priority(priority)
                .into_queue_record_with_id(target_id),
        ],
    )
    .await
}

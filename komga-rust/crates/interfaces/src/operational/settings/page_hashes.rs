use axum::Json;
use axum::body::Bytes;
use axum::extract::Path as AxumPath;
use axum::extract::State;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use komga_application::task_processing::{TaskKind, TaskRequest};
use serde::Deserialize;
use serde_json::Value;

use crate::identity_access::auth::Admin;

use super::{query_value, query_values};
use crate::state::OperationalApiState;

const REMOVE_HASHED_PAGES_PRIORITY: i32 = 4;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeletePageHashMatchRequest {
    book_id: String,
    url: String,
    page_number: i64,
    file_name: String,
    file_size: i64,
    media_type: String,
}

pub(crate) async fn get_page_hashes(
    State(app): State<OperationalApiState>,
    _admin: Admin,
    uri: Uri,
) -> Response {
    let query = uri.query().unwrap_or_default();
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(20);
    let actions = match parse_page_hash_actions(query_values(query, "action")) {
        Ok(actions) => actions,
        Err(status) => return status.into_response(),
    };
    let sorts = query_values(query, "sort");

    let page_data = match app
        .page_hashes
        .load_page_hashes_page(page, size, &actions, &sorts)
        .await
    {
        Ok(page_data) => page_data,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(page_data).into_response()
}

fn parse_page_hash_actions(raw_values: Vec<String>) -> Result<Vec<String>, StatusCode> {
    let mut actions = Vec::new();

    for raw_value in raw_values {
        for action in raw_value.split(',') {
            if !matches!(action, "DELETE_MANUAL" | "DELETE_AUTO" | "IGNORE") {
                return Err(StatusCode::BAD_REQUEST);
            }
            actions.push(action.to_string());
        }
    }

    Ok(actions)
}

fn remove_hashed_pages_task_page(
    file_name: String,
    media_type: String,
    file_hash: String,
    file_size: i64,
    page_number: i64,
) -> Value {
    serde_json::json!({
        "fileName": file_name,
        "mediaType": media_type,
        "fileHash": file_hash,
        "fileSize": file_size,
        "pageNumber": page_number,
    })
}

fn build_remove_hashed_pages_task(
    book_id: String,
    pages: Vec<Value>,
) -> Result<komga_application::task_processing::TaskQueueRecord, StatusCode> {
    let unique_id = format!("RemoveHashedPages_{book_id}");
    let payload = serde_json::to_string(&serde_json::json!({
        "bookId": book_id,
        "pages": pages,
        "priority": REMOVE_HASHED_PAGES_PRIORITY,
        "groupId": Value::Null,
        "uniqueId": unique_id.clone(),
    }))
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(TaskRequest::new(TaskKind::RemoveHashedPages)
        .priority(REMOVE_HASHED_PAGES_PRIORITY)
        .into_queue_record_with_id(&book_id)
        .with_payload(payload))
}

pub(crate) async fn get_page_hashes_unknown(
    State(app): State<OperationalApiState>,
    _admin: Admin,
    uri: Uri,
) -> Response {
    let query = uri.query().unwrap_or_default();
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(20);
    let sorts = query_values(query, "sort");

    let page_data = match app
        .page_hashes
        .load_page_hashes_unknown_page(page, size, &sorts)
        .await
    {
        Ok(page_data) => page_data,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(page_data).into_response()
}

pub(crate) async fn get_page_hash_matches(
    State(app): State<OperationalApiState>,
    _admin: Admin,
    AxumPath(page_hash): AxumPath<String>,
    uri: Uri,
) -> Response {
    let query = uri.query().unwrap_or_default();
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(20);
    let sorts = query_values(query, "sort");

    let page_data = match app
        .page_hashes
        .load_page_hash_matches_page(&page_hash, page, size, &sorts)
        .await
    {
        Ok(page_data) => page_data,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(page_data).into_response()
}

pub(crate) async fn get_page_hash_thumbnail(
    State(app): State<OperationalApiState>,
    _admin: Admin,
    AxumPath(page_hash): AxumPath<String>,
) -> Response {
    let thumbnail = match app.page_hashes.load_page_hash_thumbnail(&page_hash).await {
        Ok(Some(thumbnail)) => thumbnail,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    ([(header::CONTENT_TYPE, "image/jpeg")], thumbnail.bytes).into_response()
}

pub(crate) async fn get_page_hash_unknown_thumbnail(
    State(app): State<OperationalApiState>,
    _admin: Admin,
    AxumPath(page_hash): AxumPath<String>,
    uri: Uri,
) -> Response {
    let query = uri.query().unwrap_or_default();
    let resize_to = match query_value(query, "resize") {
        None => None,
        Some(value) => match value.parse::<u32>() {
            Ok(parsed) if parsed > 0 => Some(parsed),
            _ => return StatusCode::BAD_REQUEST.into_response(),
        },
    };

    let thumbnail = match app
        .page_hashes
        .load_unknown_page_hash_thumbnail(&page_hash, resize_to)
        .await
    {
        Ok(Some(thumbnail)) => thumbnail,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    (
        [(header::CONTENT_TYPE, thumbnail.media_type.as_str())],
        thumbnail.bytes,
    )
        .into_response()
}

pub(crate) async fn put_page_hash(
    State(app): State<OperationalApiState>,
    _admin: Admin,
    body: Bytes,
) -> Response {
    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(hash) = payload
        .get("hash")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let size = match payload.get("size") {
        None | Some(Value::Null) => None,
        Some(value) => match value.as_i64() {
            Some(size) => Some(size),
            None => return StatusCode::BAD_REQUEST.into_response(),
        },
    };
    let Some(action) = payload
        .get("action")
        .and_then(Value::as_str)
        .filter(|value| matches!(*value, "DELETE_MANUAL" | "DELETE_AUTO" | "IGNORE"))
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    match app.page_hashes.upsert_page_hash(hash, size, action).await {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn post_page_hash_delete_all(
    State(app): State<OperationalApiState>,
    _admin: Admin,
    AxumPath(page_hash): AxumPath<String>,
) -> Response {
    let delete_targets = match app
        .page_hashes
        .load_page_hash_delete_targets(&page_hash)
        .await
    {
        Ok(targets) => targets,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let mut task_records = Vec::with_capacity(delete_targets.len());
    for target in delete_targets {
        let pages = target
            .pages
            .into_iter()
            .map(|page| {
                remove_hashed_pages_task_page(
                    page.file_name,
                    page.media_type,
                    page.file_hash,
                    page.file_size,
                    page.page_number,
                )
            })
            .collect::<Vec<_>>();
        let task_record = match build_remove_hashed_pages_task(target.book_id, pages) {
            Ok(task_record) => task_record,
            Err(status) => return status.into_response(),
        };
        task_records.push(task_record);
    }

    match app
        .task_queue
        .queue
        .enqueue_records(
            task_records,
            komga_application::task_processing::SubmitUrgency::Immediate,
        )
        .await
    {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn post_page_hash_delete_match(
    State(app): State<OperationalApiState>,
    _admin: Admin,
    AxumPath(page_hash): AxumPath<String>,
    body: Bytes,
) -> Response {
    let Ok(DeletePageHashMatchRequest {
        book_id,
        url,
        page_number,
        file_name,
        file_size,
        media_type,
    }) = serde_json::from_slice::<DeletePageHashMatchRequest>(&body)
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    drop(url);

    let task_record = match build_remove_hashed_pages_task(
        book_id,
        vec![remove_hashed_pages_task_page(
            file_name,
            media_type,
            page_hash,
            file_size,
            page_number,
        )],
    ) {
        Ok(task_record) => task_record,
        Err(status) => return status.into_response(),
    };

    match app
        .task_queue
        .queue
        .enqueue_records(
            vec![task_record],
            komga_application::task_processing::SubmitUrgency::Immediate,
        )
        .await
    {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

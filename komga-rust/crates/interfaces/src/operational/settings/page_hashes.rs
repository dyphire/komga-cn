use axum::Json;
use axum::body::Bytes;
use axum::extract::Path as AxumPath;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use komga_application::task_processing::TaskQueueRecord;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

use crate::identity_access::auth::require_admin;

use super::{query_value, query_values};
use crate::state::HttpAppState;

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
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_admin(&*app.services.runtime_identity, &headers) {
        return response;
    }

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
        .services
        .operational_settings
        .load_page_hashes_page(page, size, actions, sorts)
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
) -> Result<TaskQueueRecord, StatusCode> {
    let unique_id = format!("RemoveHashedPages_{book_id}");
    let payload = serde_json::to_string(&serde_json::json!({
        "bookId": book_id,
        "pages": pages,
        "priority": REMOVE_HASHED_PAGES_PRIORITY,
        "groupId": Value::Null,
        "uniqueId": unique_id.clone(),
    }))
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(
        TaskQueueRecord::new(unique_id, REMOVE_HASHED_PAGES_PRIORITY, None)
            .with_simple_type("RemoveHashedPages")
            .with_payload(payload),
    )
}

pub(crate) async fn get_page_hashes_unknown(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_admin(&*app.services.runtime_identity, &headers) {
        return response;
    }

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
        .services
        .operational_settings
        .load_page_hashes_unknown_page(page, size, sorts)
        .await
    {
        Ok(page_data) => page_data,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(page_data).into_response()
}

pub(crate) async fn get_page_hash_matches(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    AxumPath(page_hash): AxumPath<String>,
    uri: Uri,
) -> Response {
    if let Some(response) = require_admin(&*app.services.runtime_identity, &headers) {
        return response;
    }

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
        .services
        .operational_settings
        .load_page_hash_matches_page(page_hash.clone(), page, size, sorts)
        .await
    {
        Ok(page_data) => page_data,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(page_data).into_response()
}

pub(crate) async fn get_page_hash_thumbnail(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    AxumPath(page_hash): AxumPath<String>,
) -> Response {
    if let Some(response) = require_admin(&*app.services.runtime_identity, &headers) {
        return response;
    }

    let thumbnail = match app
        .services
        .operational_settings
        .load_page_hash_thumbnail(page_hash)
        .await
    {
        Ok(Some(thumbnail)) => thumbnail,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    ([(header::CONTENT_TYPE, "image/jpeg")], thumbnail.bytes).into_response()
}

pub(crate) async fn get_page_hash_unknown_thumbnail(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    AxumPath(page_hash): AxumPath<String>,
    uri: Uri,
) -> Response {
    if let Some(response) = require_admin(&*app.services.runtime_identity, &headers) {
        return response;
    }

    let query = uri.query().unwrap_or_default();
    let resize_to = match query_value(query, "resize") {
        None => None,
        Some(value) => match value.parse::<u32>() {
            Ok(parsed) if parsed > 0 => Some(parsed),
            _ => return StatusCode::BAD_REQUEST.into_response(),
        },
    };

    let thumbnail = match app
        .services
        .operational_settings
        .load_unknown_page_hash_thumbnail(page_hash, resize_to)
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
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&*app.services.runtime_identity, &headers) {
        return response;
    }

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

    match app
        .services
        .operational_settings
        .upsert_page_hash(hash.to_string(), size, action.to_string())
        .await
    {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn post_page_hash_delete_all(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    AxumPath(page_hash): AxumPath<String>,
) -> Response {
    if let Some(response) = require_admin(&*app.services.runtime_identity, &headers) {
        return response;
    }

    let delete_targets = match app
        .services
        .operational_settings
        .load_page_hash_delete_targets(page_hash.clone())
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
        .services
        .task_queue
        .enqueue_task_records(task_records, true)
        .await
    {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn post_page_hash_delete_match(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    AxumPath(page_hash): AxumPath<String>,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&*app.services.runtime_identity, &headers) {
        return response;
    }

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
        .services
        .task_queue
        .enqueue_task_records(vec![task_record], true)
        .await
    {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

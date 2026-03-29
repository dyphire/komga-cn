use axum::Json;
use axum::body::Bytes;
use axum::extract::Extension;
use axum::extract::Path as AxumPath;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use serde_json::Value;

use crate::http::identity_access::auth::require_admin;
use crate::operational_settings_access::page_hashes as page_hashes_access;

use super::super::super::OperationalState;
use super::query_value;

pub(crate) async fn get_page_hashes(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_admin(&headers) {
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

    let page_data = match page_hashes_access::load_page_hashes_page(
        state.runtime.database_file.as_path(),
        page,
        size,
    )
    .await
    {
        Ok(page_data) => page_data,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(page_data).into_response()
}

pub(crate) async fn get_page_hashes_unknown(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_admin(&headers) {
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

    let page_data = match page_hashes_access::load_page_hashes_unknown_page(
        state.runtime.database_file.as_path(),
        page,
        size,
    )
    .await
    {
        Ok(page_data) => page_data,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(page_data).into_response()
}

pub(crate) async fn get_page_hash_matches(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    AxumPath(page_hash): AxumPath<String>,
    uri: Uri,
) -> Response {
    if let Some(response) = require_admin(&headers) {
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

    let page_data = match page_hashes_access::load_page_hash_matches_page(
        state.runtime.database_file.as_path(),
        &page_hash,
        page,
        size,
    )
    .await
    {
        Ok(page_data) => page_data,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(page_data).into_response()
}

pub(crate) async fn get_page_hash_thumbnail(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    AxumPath(page_hash): AxumPath<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let thumbnail = match page_hashes_access::load_page_hash_thumbnail(
        state.runtime.database_file.as_path(),
        &page_hash,
    )
    .await
    {
        Ok(Some(thumbnail)) => thumbnail,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    ([(header::CONTENT_TYPE, "image/jpeg")], thumbnail.bytes).into_response()
}

pub(crate) async fn get_page_hash_unknown_thumbnail(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    AxumPath(page_hash): AxumPath<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let thumbnail = match page_hashes_access::load_page_hash_thumbnail(
        state.runtime.database_file.as_path(),
        &page_hash,
    )
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
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(hash) = payload
        .get("hash")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let size = payload.get("size").and_then(Value::as_i64);
    let Some(action) = payload
        .get("action")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| matches!(*value, "DELETE_MANUAL" | "DELETE_AUTO" | "IGNORE"))
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    match page_hashes_access::upsert_page_hash(
        state.runtime.database_file.as_path(),
        hash,
        size,
        action,
    )
    .await
    {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn post_page_hash_delete_all(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    AxumPath(page_hash): AxumPath<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match page_hashes_access::delete_all_page_hash_matches(
        state.runtime.database_file.as_path(),
        &page_hash,
    )
    .await
    {
        Ok(_) => StatusCode::ACCEPTED.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn post_page_hash_delete_match(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    AxumPath(page_hash): AxumPath<String>,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let Some(book_id) = payload
        .get("bookId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(page_number) = payload.get("pageNumber").and_then(Value::as_i64) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if page_number <= 0 {
        return StatusCode::BAD_REQUEST.into_response();
    }

    match page_hashes_access::delete_page_hash_match(
        state.runtime.database_file.as_path(),
        &page_hash,
        book_id,
        page_number as u64,
    )
    .await
    {
        Ok(_) => StatusCode::ACCEPTED.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

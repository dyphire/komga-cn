use axum::Json;
use axum::body::Bytes;
use axum::extract::Extension;
use axum::extract::Path as AxumPath;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::Value;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::http::identity_access::auth::require_admin;
use crate::operational_settings_access::transient_books as transient_books_access;

use super::super::super::{OperationalState, TransientBookPageRecord, TransientBookRecord};

mod discovery;
mod payload;

use discovery::{infer_transient_series_and_number, list_transient_book_entries};
use payload::{transient_book_id, transient_book_payload};
use transient_books_access::{
    InfrastructureTransientBookPage, analyze_transient_book, load_transient_book_file_metadata,
    transient_book_page_content, validate_transient_scan_root,
};

const TRANSIENT_BOOKS_PATH: &str = "/api/v1/transient-books";

fn transient_books_bad_request(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({
            "error": "Bad Request",
            "message": message,
            "path": TRANSIENT_BOOKS_PATH,
            "status": 400,
            "timestamp": SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
        })),
    )
        .into_response()
}

fn transient_books_json_error_response(status: StatusCode, error: &str) -> Response {
    (status, Json(serde_json::json!({ "error": error }))).into_response()
}

pub(crate) async fn post_transient_books(
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
    let Some(requested_path) = payload.get("path").and_then(Value::as_str) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    match validate_transient_scan_root(state.runtime.database_file.as_path(), requested_path).await
    {
        Ok(()) => {}
        Err(error_code) if matches!(error_code.as_str(), "ERR_1016" | "ERR_1017") => {
            return transient_books_bad_request(&error_code);
        }
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }

    let scanned_books = list_transient_book_entries(PathBuf::from(requested_path).as_path());

    let mut store = state
        .transient_books
        .lock()
        .expect("transient books state lock should not be poisoned");

    let mut payload = Vec::new();
    for scanned in scanned_books {
        let Some(path) = scanned.get("path").and_then(Value::as_str) else {
            continue;
        };
        let Some(name) = scanned.get("name").and_then(Value::as_str) else {
            continue;
        };

        let Some(file_metadata) = load_transient_book_file_metadata(path) else {
            continue;
        };
        let id = transient_book_id();

        let record = TransientBookRecord {
            id: id.clone(),
            name: name.to_string(),
            path: path.to_string(),
            file_last_modified_unix_nanos: file_metadata.file_last_modified_unix_nanos,
            size_bytes: file_metadata.size_bytes,
            status: "UNKNOWN".to_string(),
            media_type: String::new(),
            page_count: 0,
            pages: Vec::new(),
            files: Vec::new(),
            comment: String::new(),
            number: None,
            series_id: None,
        };
        store.insert(record.clone());
        payload.push(transient_book_payload(&record));
    }

    drop(store);
    payload.sort_by(|left, right| {
        left["url"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["url"].as_str().unwrap_or_default())
    });
    Json(Value::Array(payload)).into_response()
}

pub(crate) async fn post_transient_book_analyze(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    AxumPath(transient_book_id): AxumPath<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let record = {
        let mut store = state
            .transient_books
            .lock()
            .expect("transient books state lock should not be poisoned");
        store.get_cloned(&transient_book_id)
    };
    let Some(record) = record else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let analysis = analyze_transient_book(record.path.as_str());
    let inferred_series_and_number = infer_transient_series_and_number(
        state.runtime.database_file.as_path(),
        record.path.as_str(),
    )
    .await;

    let mut store = state
        .transient_books
        .lock()
        .expect("transient books state lock should not be poisoned");
    let Some(entry) = store.get_mut(&transient_book_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let (inferred_series_id, inferred_number) = inferred_series_and_number;
    entry.status = analysis.status;
    entry.media_type = analysis.media_type;
    entry.page_count = analysis.page_count;
    entry.pages = analysis
        .pages
        .into_iter()
        .map(|page| TransientBookPageRecord {
            number: page.number,
            file_name: page.file_name,
            media_type: page.media_type,
            width: page.width,
            height: page.height,
            size_bytes: page.size_bytes,
        })
        .collect();
    entry.files = analysis.files;
    entry.comment = analysis.comment;
    entry.number = analysis.number.or(inferred_number);
    entry.series_id = analysis.series_id.or(inferred_series_id);

    let payload = transient_book_payload(entry);
    drop(store);

    Json(payload).into_response()
}

pub(crate) async fn get_transient_book_page(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    AxumPath((transient_book_id, page_number)): AxumPath<(String, i32)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }
    if page_number <= 0 {
        return transient_books_json_error_response(
            StatusCode::BAD_REQUEST,
            "Page number does not exist",
        );
    }
    let page_number = page_number as u32;
    let record = {
        let mut store = state
            .transient_books
            .lock()
            .expect("transient books state lock should not be poisoned");
        store.get_cloned(&transient_book_id)
    };
    let Some(record) = record else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !record.status.eq_ignore_ascii_case("READY") {
        return transient_books_json_error_response(StatusCode::NOT_FOUND, "Book analysis failed");
    }
    if !PathBuf::from(record.path.as_str()).exists() {
        return transient_books_json_error_response(
            StatusCode::NOT_FOUND,
            "File not found, it may have moved",
        );
    }
    if record.media_type == "application/epub+zip" && record.pages.is_empty() {
        if record.page_count > 0 && page_number > record.page_count {
            return transient_books_json_error_response(
                StatusCode::BAD_REQUEST,
                "Page number does not exist",
            );
        }
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let pages = record
        .pages
        .iter()
        .map(|page| InfrastructureTransientBookPage {
            number: page.number,
            file_name: page.file_name.clone(),
            media_type: page.media_type.clone(),
            width: page.width,
            height: page.height,
            size_bytes: page.size_bytes,
        })
        .collect::<Vec<_>>();
    let Some((content_type, bytes)) = transient_book_page_content(
        record.path.as_str(),
        record.media_type.as_str(),
        pages.as_slice(),
        page_number,
    ) else {
        return transient_books_json_error_response(
            StatusCode::BAD_REQUEST,
            "Page number does not exist",
        );
    };

    ([(header::CONTENT_TYPE, content_type)], bytes).into_response()
}

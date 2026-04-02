use axum::Json;
use axum::body::Bytes;
use axum::extract::Extension;
use axum::extract::Path as AxumPath;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::Value;

use crate::http::identity_access::auth::require_admin;
use crate::operational_settings_access::transient_books as transient_books_access;

use super::super::super::{OperationalState, TransientBookPageRecord, TransientBookRecord};
use super::normalize_requested_path;

#[path = "transient_books/discovery.rs"]
mod discovery;
#[path = "transient_books/payload.rs"]
mod payload;

use discovery::{infer_transient_series_and_number, list_transient_book_entries};
use payload::{transient_book_id, transient_book_payload};
use transient_books_access::{
    InfrastructureTransientBookPage, analyze_transient_book, load_transient_book_file_metadata,
    transient_book_exists, transient_book_media_type, transient_book_page_content,
};

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
    let Some(requested_path) = payload
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let resolved_path = normalize_requested_path(requested_path, state.runtime.config_dir.as_ref());
    let scanned_books = list_transient_book_entries(&resolved_path);

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
        let id = transient_book_id(path);
        let existing = store.records.get(&id).cloned();
        let status = existing
            .as_ref()
            .map(|record| record.status.clone())
            .unwrap_or_else(|| "UNPROCESSED".to_string());
        let media_type = existing
            .as_ref()
            .map(|record| record.media_type.clone())
            .unwrap_or_default();
        let pages = existing
            .as_ref()
            .map(|record| record.pages.clone())
            .unwrap_or_default();
        let files = existing
            .as_ref()
            .map(|record| record.files.clone())
            .unwrap_or_default();
        let comment = existing
            .as_ref()
            .map(|record| record.comment.clone())
            .unwrap_or_default();
        let number = existing.as_ref().and_then(|record| record.number);
        let series_id = existing
            .as_ref()
            .and_then(|record| record.series_id.clone());

        let record = TransientBookRecord {
            id: id.clone(),
            name: name.to_string(),
            path: path.to_string(),
            file_last_modified_epoch_seconds: file_metadata.file_last_modified_epoch_seconds,
            size_bytes: file_metadata.size_bytes,
            status,
            media_type,
            pages,
            files,
            comment,
            number,
            series_id,
        };
        store.records.insert(id, record.clone());
        payload.push(transient_book_payload(&record));
    }

    let records = store.records.clone();
    drop(store);
    let _ = (state.persist_transient_books_records)(&records);
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
        let store = state
            .transient_books
            .lock()
            .expect("transient books state lock should not be poisoned");
        store.records.get(&transient_book_id).cloned()
    };
    let Some(record) = record else {
        return StatusCode::NOT_FOUND.into_response();
    };

    if !transient_book_exists(record.path.as_str()) {
        return StatusCode::NOT_FOUND.into_response();
    }

    let analysis = analyze_transient_book(record.path.as_str());
    let inferred_series_and_number = infer_transient_series_and_number(
        state.runtime.database_file.as_path(),
        record.name.as_str(),
    )
    .await;

    let mut store = state
        .transient_books
        .lock()
        .expect("transient books state lock should not be poisoned");
    let Some(entry) = store.records.get_mut(&transient_book_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    match analysis {
        Ok(analysis) => {
            let (inferred_series_id, inferred_number) = inferred_series_and_number;
            entry.status = analysis.status;
            entry.media_type = analysis.media_type;
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
        }
        Err(comment) => {
            entry.media_type = transient_book_media_type(record.path.as_str());
            entry.status = if entry.media_type == "application/octet-stream" {
                "UNSUPPORTED".to_string()
            } else {
                "ERROR".to_string()
            };
            entry.pages.clear();
            entry.files.clear();
            entry.comment = comment;
            entry.number = None;
            entry.series_id = None;
        }
    }

    let payload = transient_book_payload(entry);
    let records = store.records.clone();
    drop(store);
    let _ = (state.persist_transient_books_records)(&records);

    Json(payload).into_response()
}

pub(crate) async fn get_transient_book_page(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    AxumPath((transient_book_id, page_number)): AxumPath<(String, u32)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }
    let store = state
        .transient_books
        .lock()
        .expect("transient books state lock should not be poisoned");
    let Some(record) = store.records.get(&transient_book_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !record.status.eq_ignore_ascii_case("READY") {
        return StatusCode::NOT_FOUND.into_response();
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
        return StatusCode::BAD_REQUEST.into_response();
    };

    ([(header::CONTENT_TYPE, content_type)], bytes).into_response()
}

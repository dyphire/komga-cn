use axum::Json;
use axum::body::Bytes;
use axum::extract::Path as AxumPath;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use komga_application::operational::{
    TransientBookAnalyzeError, TransientBookPageError, TransientBookScanError,
};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::identity_access::auth::Admin;
use crate::state::OperationalApiState;

mod payload;

use payload::transient_book_payload;

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
    State(app): State<OperationalApiState>,
    _admin: Admin,
    body: Bytes,
) -> Response {
    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(requested_path) = payload.get("path").and_then(Value::as_str) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let records = match app.transient_books.scan(requested_path).await {
        Ok(records) => records,
        Err(TransientBookScanError::BadRequest(error_code)) => {
            return transient_books_bad_request(&error_code);
        }
        Err(TransientBookScanError::Internal) => {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let mut payload = records
        .iter()
        .map(transient_book_payload)
        .collect::<Vec<_>>();
    payload.sort_by(|left, right| {
        left["url"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["url"].as_str().unwrap_or_default())
    });
    Json(Value::Array(payload)).into_response()
}

pub(crate) async fn post_transient_book_analyze(
    State(app): State<OperationalApiState>,
    _admin: Admin,
    AxumPath(transient_book_id): AxumPath<String>,
) -> Response {
    match app.transient_books.analyze(&transient_book_id).await {
        Ok(record) => Json(transient_book_payload(&record)).into_response(),
        Err(TransientBookAnalyzeError::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Err(TransientBookAnalyzeError::Internal) => {
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub(crate) async fn get_transient_book_page(
    State(app): State<OperationalApiState>,
    _admin: Admin,
    AxumPath((transient_book_id, page_number)): AxumPath<(String, i32)>,
) -> Response {
    let content = match app
        .transient_books
        .page_content(&transient_book_id, page_number)
    {
        Ok(content) => content,
        Err(TransientBookPageError::NotFound) => return StatusCode::NOT_FOUND.into_response(),
        Err(TransientBookPageError::AnalysisFailed) => {
            return transient_books_json_error_response(
                StatusCode::NOT_FOUND,
                "Book analysis failed",
            );
        }
        Err(TransientBookPageError::FileMissing) => {
            return transient_books_json_error_response(
                StatusCode::NOT_FOUND,
                "File not found, it may have moved",
            );
        }
        Err(TransientBookPageError::BadPageNumber) => {
            return transient_books_json_error_response(
                StatusCode::BAD_REQUEST,
                "Page number does not exist",
            );
        }
        Err(TransientBookPageError::Internal) => {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    (
        [(header::CONTENT_TYPE, content.content_type)],
        content.bytes,
    )
        .into_response()
}

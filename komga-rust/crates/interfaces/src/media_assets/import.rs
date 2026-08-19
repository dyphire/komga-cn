use std::path::PathBuf;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use komga_application::media_assets::{
    BookImportSubmissionFailure, BookImportSubmissionFailureKind, BooksImportEntry,
    BooksImportPayload, ImportCopyMode,
};
use serde_json::Value;
use tracing::error;

use crate::helpers::spring_error_response;
use crate::identity_access::auth::Admin;
use crate::state::MediaAssetsState;

pub(crate) async fn books_import(
    State(app): State<MediaAssetsState>,
    _admin: Admin,
    Json(body): Json<Value>,
) -> Response {
    let payload = match parse_books_import_request_body(&body) {
        Ok(payload) => payload,
        Err(error) => {
            return spring_error_response(StatusCode::BAD_REQUEST, error);
        }
    };

    let failures = app
        .import
        .submit_books_import(payload, app.task_queue.queue.as_ref())
        .await;
    books_import_submission_response(failures)
}

fn books_import_submission_response(failures: Vec<BookImportSubmissionFailure>) -> Response {
    let mut first_error = None::<String>;

    for failure in failures {
        let message = match failure.kind {
            BookImportSubmissionFailureKind::CreateTask => "Failed to create import task",
            BookImportSubmissionFailureKind::EnqueueTask => "Failed to enqueue import task",
        };
        let series_id = failure.series_id.as_str();
        let source_file = failure.source_file.as_str();
        let error = failure.error.as_str();
        error!(
            %series_id,
            %source_file,
            %error,
            message
        );
        if first_error.is_none() {
            first_error = Some(failure.error);
        }
    }

    if let Some(error) = first_error {
        return spring_error_response(StatusCode::INTERNAL_SERVER_ERROR, error);
    }

    StatusCode::ACCEPTED.into_response()
}

fn parse_books_import_request_body(body: &Value) -> anyhow::Result<BooksImportPayload> {
    let body = body
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("books import payload must be a JSON object"))?;

    let copy_mode = match body.get("copyMode").and_then(Value::as_str) {
        Some("MOVE") => ImportCopyMode::Move,
        Some("COPY") => ImportCopyMode::Copy,
        Some("HARDLINK") => ImportCopyMode::Hardlink,
        Some(_) => {
            return Err(anyhow::anyhow!(
                "copyMode must be one of MOVE, COPY, HARDLINK"
            ));
        }
        None => {
            return Err(anyhow::anyhow!("copyMode is required"));
        }
    };

    let books = match body.get("books") {
        Some(books) => books
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("books must be an array"))?
            .iter()
            .map(parse_books_import_entry)
            .collect::<Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };

    Ok(BooksImportPayload { copy_mode, books })
}

fn parse_books_import_entry(entry: &Value) -> anyhow::Result<BooksImportEntry> {
    let entry = entry
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("books entries must be objects"))?;

    let source_file = entry
        .get("sourceFile")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("books[].sourceFile must be a string"))?;
    let series_id = entry
        .get("seriesId")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("books[].seriesId must be a string"))?;
    if source_file.trim().is_empty() || series_id.trim().is_empty() {
        return Err(anyhow::anyhow!(
            "books[].sourceFile and books[].seriesId must not be blank"
        ));
    }

    let destination_name = entry
        .get("destinationName")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let upgrade_book_id = entry
        .get("upgradeBookId")
        .and_then(Value::as_str)
        .map(ToString::to_string);

    Ok(BooksImportEntry {
        source_file: PathBuf::from(source_file),
        series_id: series_id.to_string(),
        destination_name,
        upgrade_book_id,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use axum::body::to_bytes;
    use axum::http::StatusCode;
    use komga_application::media_assets::{
        BookImportSubmissionFailure, BookImportSubmissionFailureKind, BooksImportEntry,
        BooksImportPayload, ImportCopyMode,
    };
    use serde_json::json;

    #[test]
    fn books_import_request_parsing_lives_at_the_interfaces_boundary() {
        let payload = super::parse_books_import_request_body(&json!({
            "copyMode": "COPY",
            "books": [{
                "sourceFile": "/tmp/book-a.cbz",
                "seriesId": "series-1",
                "destinationName": "Book A",
                "upgradeBookId": "book-1"
            }]
        }))
        .expect("http import payload should parse");

        assert_eq!(
            payload,
            BooksImportPayload {
                copy_mode: ImportCopyMode::Copy,
                books: vec![BooksImportEntry {
                    source_file: PathBuf::from("/tmp/book-a.cbz"),
                    series_id: "series-1".to_string(),
                    destination_name: Some("Book A".to_string()),
                    upgrade_book_id: Some("book-1".to_string()),
                }]
            }
        );
    }

    #[tokio::test]
    async fn books_import_returns_internal_error_when_enqueue_fails() {
        let response = super::books_import_submission_response(vec![BookImportSubmissionFailure {
            kind: BookImportSubmissionFailureKind::EnqueueTask,
            series_id: "series-1".to_string(),
            source_file: "/tmp/book-a.cbz".to_string(),
            error: "tasks db unavailable".to_string(),
        }]);

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("enqueue failure response body should be readable");
        let payload = serde_json::from_slice::<serde_json::Value>(&body)
            .expect("enqueue failure response should be JSON");
        assert_eq!(payload.get("error"), Some(&json!("tasks db unavailable")));
    }
}

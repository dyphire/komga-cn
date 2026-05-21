use super::*;
use axum::extract::State;
use komga_application::media_assets::{
    BookImportSubmissionFailureKind, parse_books_import_payload,
};
use tracing::error;

use crate::identity_access::auth::Admin;
use crate::state::MediaAssetsState;

pub async fn books_import(
    State(app): State<MediaAssetsState>,
    _admin: Admin,
    Json(body): Json<Value>,
) -> Response {
    let payload = match parse_books_import_payload(&body) {
        Ok(payload) => payload,
        Err(error) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": error }))).into_response();
        }
    };

    for failure in app
        .import
        .submit_books_import(payload, app.task_queue.queue.as_ref())
        .await
    {
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
    }

    let mut response = StatusCode::ACCEPTED.into_response();
    mark_runtime_owned(&mut response);
    response
}

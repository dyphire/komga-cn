use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use komga_application::task_processing::TaskQueueRecord as ApplicationTaskQueueRecord;
use serde_json::json;

use crate::http::helpers::mark_runtime_owned;
use crate::http::state::HttpAppState;

pub(super) async fn enqueue_task_records(
    app: &HttpAppState,
    task_records: Vec<ApplicationTaskQueueRecord>,
) -> Response {
    enqueue_task_records_with_status(app, task_records, StatusCode::ACCEPTED).await
}

pub(super) async fn enqueue_task_records_with_status(
    app: &HttpAppState,
    task_records: Vec<ApplicationTaskQueueRecord>,
    status: StatusCode,
) -> Response {
    if let Err(error) = app
        .services
        .task_queue
        .enqueue_task_records(task_records, true)
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": error })),
        )
            .into_response();
    }

    let mut response = status.into_response();
    mark_runtime_owned(&mut response);
    response
}

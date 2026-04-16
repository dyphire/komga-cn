use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use komga_application::task_processing::TaskQueueRecord as ApplicationTaskQueueRecord;
use serde_json::json;

use crate::http::helpers::mark_runtime_owned;
use crate::http::state::OperationalState;

pub(super) fn enqueue_task_records(
    state: &OperationalState,
    task_records: Vec<ApplicationTaskQueueRecord>,
) -> Response {
    enqueue_task_records_with_status(state, task_records, StatusCode::ACCEPTED)
}

pub(super) fn enqueue_task_records_with_status(
    state: &OperationalState,
    task_records: Vec<ApplicationTaskQueueRecord>,
    status: StatusCode,
) -> Response {
    if let Err(error) = (state.enqueue_task_records)(task_records, true) {
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

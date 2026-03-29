use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use komga_application::task_processing::TaskQueueRecord as ApplicationTaskQueueRecord;
use serde_json::json;

use super::{OperationalState, mark_runtime_owned};
use komga_application::task_processing::TaskQueueRecord;

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
    let task_records = task_records
        .into_iter()
        .map(interface_task_record)
        .collect();
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

fn interface_task_record(task: ApplicationTaskQueueRecord) -> TaskQueueRecord {
    let mut record = TaskQueueRecord::new(task.id, task.priority, task.group);
    record.simple_type = task.simple_type;
    record.payload = task.payload;
    record.owner = task.owner;
    record.order = task.order;
    record
}

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use komga_application::task_processing::{SubmitUrgency, TaskQueueAdmin, TaskQueueRecord};

use crate::helpers::spring_error_response;

pub(crate) mod access_control;
mod files;
pub(crate) mod handlers;
pub(crate) mod http_helpers;
mod import;
pub(crate) mod manifest_renderer;
mod manifests;
pub(crate) mod media_helpers;
mod operations;
mod page_resolution;
mod pages;
pub(crate) mod read_progress;
pub(crate) mod thumbnails;
pub(crate) mod types;

async fn enqueue_task_records(
    task_queue: &dyn TaskQueueAdmin,
    task_records: Vec<TaskQueueRecord>,
) -> Response {
    if let Err(error) = task_queue
        .enqueue_records(task_records, SubmitUrgency::Immediate)
        .await
    {
        tracing::error!(?error, "media asset task enqueue failed");
        return spring_error_response(StatusCode::INTERNAL_SERVER_ERROR, error);
    }

    StatusCode::ACCEPTED.into_response()
}

#[cfg(test)]
mod tests;

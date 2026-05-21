use super::*;
use komga_application::task_processing::{SeriesPayload, TaskKind, TaskRequest};
use std::collections::BTreeSet;

use serde_json::Value;

pub(in crate::task_queue) async fn execute_refresh_book_metadata(
    runtime: &JobRuntime<'_>,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    let Some(book_id) = task_target else {
        return Err(TaskProcessingError::invalid_task(
            "RefreshBookMetadata task must include a book id",
        ));
    };
    let capabilities = refresh_book_metadata_capabilities(task);
    let series_id =
        super::super::metadata_tasks::refresh_book_metadata(runtime, book_id, &capabilities)
            .await?;
    let follow_up_tasks = series_id
        .into_iter()
        .map(|series_id| {
            TaskRequest::with_payload(
                TaskKind::RefreshSeriesMetadata,
                SeriesPayload::new(series_id.clone()),
            )
            .priority(task.priority - 1)
            .group(series_id)
            .into_queue_record()
        })
        .collect();
    Ok(TaskExecutionOutcome::with_follow_up_tasks(follow_up_tasks))
}

pub(in crate::task_queue) async fn execute_refresh_series_metadata(
    runtime: &JobRuntime<'_>,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    let Some(series_id) = task_target else {
        return Err(TaskProcessingError::invalid_task(
            "RefreshSeriesMetadata task must include a series id",
        ));
    };
    super::super::metadata_tasks::refresh_series_metadata(runtime, series_id).await?;
    Ok(TaskExecutionOutcome::with_follow_up_tasks(vec![
        TaskRequest::with_payload(
            TaskKind::AggregateSeriesMetadata,
            SeriesPayload::new(series_id.to_string()),
        )
        .priority(task.priority)
        .group(series_id.to_string())
        .into_queue_record(),
    ]))
}

pub(in crate::task_queue) async fn execute_aggregate_series_metadata(
    runtime: &JobRuntime<'_>,
    task_target: Option<&str>,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    let Some(series_id) = task_target else {
        return Err(TaskProcessingError::invalid_task(
            "AggregateSeriesMetadata task must include a series id",
        ));
    };
    super::super::metadata_tasks::aggregate_series_metadata(runtime, series_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
}

pub(in crate::task_queue) async fn execute_refresh_book_local_artwork(
    runtime: &JobRuntime<'_>,
    task_target: Option<&str>,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    let Some(book_id) = task_target else {
        return Err(TaskProcessingError::invalid_task(
            "RefreshBookLocalArtwork task must include a book id",
        ));
    };
    super::super::metadata_tasks::refresh_book_local_artwork(runtime, book_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
}

pub(in crate::task_queue) async fn execute_generate_book_thumbnail(
    runtime: &JobRuntime<'_>,
    task_target: Option<&str>,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    let Some(book_id) = task_target else {
        return Err(TaskProcessingError::invalid_task(
            "GenerateBookThumbnail task must include a book id",
        ));
    };
    super::super::metadata_tasks::generate_book_thumbnail(runtime, book_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
}

pub(in crate::task_queue) async fn execute_refresh_series_local_artwork(
    runtime: &JobRuntime<'_>,
    task_target: Option<&str>,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    let Some(series_id) = task_target else {
        return Err(TaskProcessingError::invalid_task(
            "RefreshSeriesLocalArtwork task must include a series id",
        ));
    };
    super::super::metadata_tasks::refresh_series_local_artwork(runtime, series_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
}

fn refresh_book_metadata_capabilities(task: &TaskQueueRecord) -> BTreeSet<String> {
    task.payload
        .as_deref()
        .and_then(|payload| serde_json::from_str::<Value>(payload).ok())
        .and_then(|payload| payload.get("capabilities").cloned())
        .and_then(|capabilities| capabilities.as_array().cloned())
        .map(|capabilities| {
            capabilities
                .into_iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect::<BTreeSet<_>>()
        })
        .filter(|capabilities| !capabilities.is_empty())
        .unwrap_or_else(default_refresh_book_metadata_capabilities)
}

fn default_refresh_book_metadata_capabilities() -> BTreeSet<String> {
    [
        "TITLE",
        "SUMMARY",
        "NUMBER",
        "NUMBER_SORT",
        "RELEASE_DATE",
        "AUTHORS",
        "TAGS",
        "ISBN",
        "READ_LISTS",
        "THUMBNAILS",
        "LINKS",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

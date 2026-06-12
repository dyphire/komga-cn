use std::collections::BTreeSet;

use komga_application::task_processing::{
    SeriesPayload, TaskExecutionOutcome, TaskKind, TaskProcessingError, TaskRequest,
};

use super::super::runtime_context::JobRuntime;

pub(in crate::task_queue) async fn execute_refresh_book_metadata(
    runtime: &JobRuntime<'_>,
    book_id: &str,
    capabilities: &BTreeSet<String>,
    priority: i32,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    let series_id =
        super::super::metadata_tasks::refresh_book_metadata(runtime, book_id, capabilities).await?;
    let follow_up_tasks = series_id
        .into_iter()
        .map(|series_id| {
            TaskRequest::with_payload(
                TaskKind::RefreshSeriesMetadata,
                SeriesPayload::new(series_id.clone()),
            )
            .priority(priority - 1)
            .group(series_id)
            .into_queue_record()
        })
        .collect();
    Ok(TaskExecutionOutcome::with_follow_up_tasks(follow_up_tasks))
}

pub(in crate::task_queue) async fn execute_refresh_series_metadata(
    runtime: &JobRuntime<'_>,
    series_id: &str,
    priority: i32,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    super::super::metadata_tasks::refresh_series_metadata(runtime, series_id).await?;
    Ok(TaskExecutionOutcome::with_follow_up_tasks(vec![
        TaskRequest::with_payload(
            TaskKind::AggregateSeriesMetadata,
            SeriesPayload::new(series_id.to_string()),
        )
        .priority(priority)
        .group(series_id.to_string())
        .into_queue_record(),
    ]))
}

pub(in crate::task_queue) async fn execute_aggregate_series_metadata(
    runtime: &JobRuntime<'_>,
    series_id: &str,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    super::super::metadata_tasks::aggregate_series_metadata(runtime, series_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
}

pub(in crate::task_queue) async fn execute_refresh_book_local_artwork(
    runtime: &JobRuntime<'_>,
    book_id: &str,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    super::super::metadata_tasks::refresh_book_local_artwork(runtime, book_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
}

pub(in crate::task_queue) async fn execute_generate_book_thumbnail(
    runtime: &JobRuntime<'_>,
    book_id: &str,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    super::super::metadata_tasks::generate_book_thumbnail(runtime, book_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
}

pub(in crate::task_queue) async fn execute_refresh_series_local_artwork(
    runtime: &JobRuntime<'_>,
    series_id: &str,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    super::super::metadata_tasks::refresh_series_local_artwork(runtime, series_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
}

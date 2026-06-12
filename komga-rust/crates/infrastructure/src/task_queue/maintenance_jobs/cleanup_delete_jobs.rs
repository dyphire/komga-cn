use komga_application::task_processing::{TaskExecutionOutcome, TaskProcessingError};

use super::super::runtime_context::JobRuntime;

pub(in crate::task_queue) async fn execute_empty_trash(
    runtime: &JobRuntime<'_>,
    library_id: &str,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    super::super::cleanup_tasks::empty_trash(runtime, library_id).await?;
    super::super::cleanup_tasks::cleanup_empty_sets(runtime).await?;
    Ok(TaskExecutionOutcome::completed())
}

pub(in crate::task_queue) async fn execute_delete_book(
    runtime: &JobRuntime<'_>,
    book_id: &str,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    super::super::delete_tasks::delete_book_task(runtime, book_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
}

pub(in crate::task_queue) async fn execute_delete_series(
    runtime: &JobRuntime<'_>,
    series_id: &str,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    super::super::delete_tasks::delete_series(runtime, series_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
}

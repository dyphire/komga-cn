use komga_application::task_processing::{TaskExecutionOutcome, TaskProcessingError};

use crate::tasks::JobRuntime;

pub(in crate::tasks) async fn execute_empty_trash(
    runtime: &JobRuntime<'_>,
    library_id: &str,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    crate::discovery::deletion::empty_trash(runtime, library_id).await?;
    crate::discovery::deletion::cleanup_empty_sets(runtime).await?;
    Ok(TaskExecutionOutcome::completed())
}

pub(in crate::tasks) async fn execute_delete_book(
    runtime: &JobRuntime<'_>,
    book_id: &str,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    crate::discovery::deletion::delete_book_task(runtime, book_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
}

pub(in crate::tasks) async fn execute_delete_series(
    runtime: &JobRuntime<'_>,
    series_id: &str,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    crate::discovery::deletion::delete_series(runtime, series_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
}

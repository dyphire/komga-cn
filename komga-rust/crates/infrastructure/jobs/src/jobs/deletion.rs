use komga_application::task_processing::{TaskExecutionOutcome, TaskProcessingError};

use crate::JobRuntime;

pub(crate) async fn execute_empty_trash(
    runtime: &JobRuntime<'_>,
    library_id: &str,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    if runtime.database().owns_main_database() {
        komga_infrastructure_discovery::empty_trash_rows(
            runtime.database().task_write_pool(),
            library_id,
        )
        .await
        .map_err(TaskProcessingError::runtime)?;
        let policy = runtime
            .cleanup_empty_sets_policy()
            .await
            .map_err(TaskProcessingError::runtime)?;
        komga_infrastructure_discovery::cleanup_empty_sets_rows(
            runtime.database().task_write_pool(),
            policy,
        )
        .await
        .map_err(TaskProcessingError::runtime)?;
    }
    Ok(TaskExecutionOutcome::completed())
}

pub(crate) async fn execute_delete_book(
    runtime: &JobRuntime<'_>,
    book_id: &str,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    crate::book_deletion::delete_book_task(runtime, book_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
}

pub(crate) async fn execute_delete_series(
    runtime: &JobRuntime<'_>,
    series_id: &str,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    crate::book_deletion::delete_series(runtime, series_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
}

use super::*;

pub(in crate::task_queue) async fn execute_empty_trash(
    runtime: &JobRuntime<'_>,
    task_target: Option<&str>,
) -> Result<TaskExecutionOutcome, TaskExecutionError> {
    let Some(library_id) = task_target else {
        return Err(TaskExecutionError::invalid_task(
            "EmptyTrash task must include a library id",
        ));
    };
    super::super::cleanup_tasks::empty_trash(runtime, library_id).await?;
    super::super::cleanup_tasks::cleanup_empty_sets(runtime).await?;
    Ok(TaskExecutionOutcome::completed())
}

pub(in crate::task_queue) async fn execute_delete_book(
    runtime: &JobRuntime<'_>,
    task_target: Option<&str>,
) -> Result<TaskExecutionOutcome, TaskExecutionError> {
    let Some(book_id) = task_target else {
        return Err(TaskExecutionError::invalid_task(
            "DeleteBook task must include a book id",
        ));
    };
    super::super::delete_tasks::delete_book_task(runtime, book_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
}

pub(in crate::task_queue) async fn execute_delete_series(
    runtime: &JobRuntime<'_>,
    task_target: Option<&str>,
) -> Result<TaskExecutionOutcome, TaskExecutionError> {
    let Some(series_id) = task_target else {
        return Err(TaskExecutionError::invalid_task(
            "DeleteSeries task must include a series id",
        ));
    };
    super::super::delete_tasks::delete_series(runtime, series_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
}

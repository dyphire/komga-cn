use super::*;
pub(super) async fn try_execute(
    runtime: &JobRuntime<'_>,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Option<Result<TaskExecutionOutcome, TaskExecutionError>> {
    Some(match task.simple_type.as_str() {
        "EmptyTrash" => {
            let Some(library_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "EmptyTrash task must include a library id",
                )));
            };
            execute_empty_trash(runtime, library_id).await
        }
        "DeleteBook" => {
            let Some(book_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "DeleteBook task must include a book id",
                )));
            };
            super::super::delete_tasks::delete_book_task(runtime, book_id)
                .await
                .map(|()| TaskExecutionOutcome::completed())
        }
        "DeleteSeries" => {
            let Some(series_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "DeleteSeries task must include a series id",
                )));
            };
            super::super::delete_tasks::delete_series(runtime, series_id)
                .await
                .map(|()| TaskExecutionOutcome::completed())
        }
        _ => return None,
    })
}

async fn execute_empty_trash(
    runtime: &JobRuntime<'_>,
    library_id: &str,
) -> Result<TaskExecutionOutcome, TaskExecutionError> {
    super::super::cleanup_tasks::empty_trash(runtime, library_id).await?;
    super::super::cleanup_tasks::cleanup_empty_sets(runtime).await?;
    Ok(TaskExecutionOutcome::completed())
}

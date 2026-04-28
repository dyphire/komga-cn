use super::*;
use komga_application::task_processing::TaskRuntimeContext;

pub(super) async fn try_execute(
    runtime: &TaskRuntimeContext,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Option<Result<TaskExecutionOutcome, TaskExecutionError>> {
    Some(match task.simple_type.as_str() {
        "EMPTY_TRASH" => {
            let Some(library_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "EMPTY_TRASH task must include a library id",
                )));
            };
            execute_empty_trash(runtime, library_id).await
        }
        "DELETE_BOOK" => {
            let Some(book_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "DELETE_BOOK task must include a book id",
                )));
            };
            super::super::delete_tasks::delete_book_task(runtime, book_id)
                .await
                .map(|()| TaskExecutionOutcome::completed())
        }
        "DELETE_SERIES" => {
            let Some(series_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "DELETE_SERIES task must include a series id",
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
    runtime: &RuntimeConfig,
    library_id: &str,
) -> Result<TaskExecutionOutcome, TaskExecutionError> {
    super::super::cleanup_tasks::empty_trash(runtime, library_id).await?;
    super::super::cleanup_tasks::cleanup_empty_sets(runtime).await?;
    Ok(TaskExecutionOutcome::completed())
}

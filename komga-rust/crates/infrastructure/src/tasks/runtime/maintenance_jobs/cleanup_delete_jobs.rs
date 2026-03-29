use super::*;

pub(super) fn try_execute(
    runtime: &RuntimeConfig,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Option<Result<(), TaskExecutionError>> {
    let result = match task.simple_type.as_str() {
        "EMPTY_TRASH" => {
            let Some(library_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "EMPTY_TRASH task must include a library id",
                )));
            };
            if let Err(error) = super::super::cleanup_tasks::empty_trash(runtime, library_id) {
                return Some(Err(error));
            }
            super::super::cleanup_tasks::cleanup_empty_sets(runtime)
        }
        "DELETE_BOOK" => {
            let Some(book_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "DELETE_BOOK task must include a book id",
                )));
            };
            super::super::delete_tasks::delete_book_task(runtime, book_id)
        }
        "DELETE_SERIES" => {
            let Some(series_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "DELETE_SERIES task must include a series id",
                )));
            };
            super::super::delete_tasks::delete_series(runtime, series_id)
        }
        _ => return None,
    };

    Some(result)
}

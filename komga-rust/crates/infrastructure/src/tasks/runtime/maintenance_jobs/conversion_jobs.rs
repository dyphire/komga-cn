use super::*;

pub(super) fn try_execute(
    scheduler: &mut TaskQueueScheduler,
    runtime: &RuntimeConfig,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Option<Result<(), TaskExecutionError>> {
    let result = match task.simple_type.as_str() {
        "REPAIR_EXTENSIONS" => {
            let Some(library_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "REPAIR_EXTENSIONS task must include a library id",
                )));
            };
            super::super::repair_extensions(runtime, library_id)
        }
        "FIND_BOOKS_TO_CONVERT" => {
            let Some(library_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "FIND_BOOKS_TO_CONVERT task must include a library id",
                )));
            };
            let book_ids = match super::super::find_books_to_convert(runtime, library_id) {
                Ok(ids) => ids,
                Err(error) => return Some(Err(error)),
            };
            for book_id in book_ids {
                scheduler.enqueue(TaskQueueRecord::new(
                    format!("CONVERT_BOOK:{book_id}"),
                    task.priority.saturating_sub(5),
                    Some(book_id),
                ));
            }
            Ok(())
        }
        "CONVERT_BOOK" => {
            let Some(book_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "CONVERT_BOOK task must include a book id",
                )));
            };
            super::super::convert_book(runtime, book_id)
        }
        _ => return None,
    };

    Some(result)
}

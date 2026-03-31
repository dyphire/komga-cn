use super::{RuntimeConfig, TaskExecutionError, TaskQueueRecord, TaskQueueScheduler};

pub(super) fn try_execute(
    scheduler: &mut TaskQueueScheduler,
    runtime: &RuntimeConfig,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Option<Result<(), TaskExecutionError>> {
    let result = match task.simple_type.as_str() {
        "ANALYZE_BOOK" => {
            let Some(book_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "ANALYZE_BOOK task must include a book id",
                )));
            };
            super::index_tasks::analyze_book(runtime, book_id)
        }
        "REBUILD_INDEX" => super::index_tasks::rebuild_index(runtime),
        "FIND_BOOK_THUMBNAILS_TO_REGENERATE" => {
            let book_ids = match super::find_books_without_selected_thumbnails(runtime) {
                Ok(ids) => ids,
                Err(error) => return Some(Err(error)),
            };
            for book_id in book_ids {
                scheduler.enqueue(TaskQueueRecord::new(
                    format!("GENERATE_BOOK_THUMBNAIL:{book_id}"),
                    task.priority.saturating_sub(5),
                    Some(book_id),
                ));
            }
            Ok(())
        }
        _ => return None,
    };

    Some(result)
}

use super::*;

pub(super) fn try_execute(
    scheduler: &mut TaskQueueScheduler,
    runtime: &RuntimeConfig,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Option<Result<(), TaskExecutionError>> {
    let owns_main_database = runtime.task_runtime_context().owns_main_database;
    let result = match task.simple_type.as_str() {
        "HASH_BOOK_PAGES" => {
            if !owns_main_database {
                return Some(Ok(()));
            }
            let Some(book_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "HASH_BOOK_PAGES task must include a book id",
                )));
            };
            super::super::hash_book_pages(runtime, book_id)
        }
        "HASH_BOOK" => {
            let Some(book_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "HASH_BOOK task must include a book id",
                )));
            };
            super::super::hash_book(runtime, book_id, false)
        }
        "HASH_BOOK_KOREADER" => {
            let Some(book_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "HASH_BOOK_KOREADER task must include a book id",
                )));
            };
            super::super::hash_book(runtime, book_id, true)
        }
        "FIND_BOOKS_WITH_MISSING_PAGE_HASH" => {
            if !owns_main_database {
                return Some(Ok(()));
            }
            let book_ids = match find_books_with_missing_page_hash(runtime, task_target) {
                Ok(ids) => ids,
                Err(error) => return Some(Err(error)),
            };
            for book_id in book_ids {
                scheduler.enqueue(TaskQueueRecord::new(
                    format!("HASH_BOOK_PAGES:{book_id}"),
                    task.priority.saturating_sub(5),
                    Some(book_id),
                ));
            }
            Ok(())
        }
        "FIND_DUPLICATE_PAGES_TO_DELETE" => {
            if !owns_main_database {
                return Some(Ok(()));
            }
            let Some(library_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "FIND_DUPLICATE_PAGES_TO_DELETE task must include a library id",
                )));
            };
            let targets = match find_duplicate_pages_to_delete(runtime, library_id) {
                Ok(targets) => targets,
                Err(error) => return Some(Err(error)),
            };
            for (book_id, pages) in targets {
                let payload = match serde_json::to_string(&super::super::RemoveHashedPagesPayload {
                    pages,
                }) {
                    Ok(payload) => payload,
                    Err(error) => {
                        return Some(Err(TaskExecutionError::runtime(format!(
                            "failed to serialize REMOVE_HASHED_PAGES payload: {error}",
                        ))));
                    }
                };
                scheduler.enqueue(
                    TaskQueueRecord::new(
                        format!("REMOVE_HASHED_PAGES:{book_id}"),
                        task.priority.saturating_sub(5),
                        Some(book_id),
                    )
                    .with_payload(payload),
                );
            }
            Ok(())
        }
        "REMOVE_HASHED_PAGES" => {
            if !owns_main_database {
                return Some(Ok(()));
            }
            let Some(book_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "REMOVE_HASHED_PAGES task must include a book id",
                )));
            };
            let Some(payload) = task.payload.as_deref() else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "REMOVE_HASHED_PAGES task requires serialized payload",
                )));
            };
            let parsed =
                match serde_json::from_str::<super::super::RemoveHashedPagesPayload>(payload) {
                    Ok(parsed) => parsed,
                    Err(error) => {
                        return Some(Err(TaskExecutionError::runtime(format!(
                            "failed to parse REMOVE_HASHED_PAGES payload: {error}",
                        ))));
                    }
                };

            let regenerate_thumbnail = match remove_hashed_pages(runtime, book_id, &parsed.pages) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            if regenerate_thumbnail {
                scheduler.enqueue(TaskQueueRecord::new(
                    format!("GENERATE_BOOK_THUMBNAIL:{book_id}"),
                    task.priority.saturating_sub(1),
                    Some(book_id.to_string()),
                ));
            }
            Ok(())
        }
        _ => return None,
    };

    Some(result)
}

use super::*;
use crate::tasks::load_books_with_missing_file_hash as load_persisted_books_with_missing_file_hash;

pub(in crate::task_queue) fn find_books_with_missing_file_hash(
    runtime: &RuntimeConfig,
    library_id: &str,
    koreader: bool,
) -> Result<Vec<String>, TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    load_persisted_books_with_missing_file_hash(
        runtime.database_file.as_path(),
        library_id,
        koreader,
    )
    .map_err(TaskExecutionError::runtime)
}

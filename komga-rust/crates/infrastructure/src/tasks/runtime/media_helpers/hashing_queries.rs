use super::hashed_pages::HashedPageToDelete;
use super::*;
use crate::tasks::{
    load_book_file_path, load_books_requiring_analysis as load_persisted_books_requiring_analysis,
    load_books_with_missing_page_hash as load_persisted_books_with_missing_page_hash,
    load_books_with_undersized_generated_thumbnails,
    load_books_without_selected_thumbnails as load_persisted_books_without_selected_thumbnails,
    load_duplicate_pages_to_delete as load_persisted_duplicate_pages_to_delete, persist_book_hash,
};

pub(in crate::task_queue) fn hash_book_pages(
    runtime: &RuntimeConfig,
    book_id: &str,
) -> Result<(), TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    let database_file = runtime.database_file.clone();
    let book_id = book_id.to_string();

    std::thread::spawn(move || {
        let async_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "build hash-book-pages runtime failed: {error}"
                ))
            })?;

        async_runtime.block_on(async move {
            crate::filesystem::persist_book_page_hashes_from_media_content(
                database_file.as_path(),
                &book_id,
            )
            .await
            .map_err(TaskExecutionError::runtime)
        })
    })
    .join()
    .map_err(|_| TaskExecutionError::runtime("hash-book-pages worker thread panicked"))?
}

pub(in crate::task_queue) fn hash_book(
    runtime: &RuntimeConfig,
    book_id: &str,
    koreader: bool,
) -> Result<(), TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    if !runtime.owns_main_database {
        return Ok(());
    }

    let Some(file_path) = load_book_file_path(runtime.database_file.as_path(), book_id)
        .map_err(TaskExecutionError::runtime)?
    else {
        return Ok(());
    };

    let bytes = fs::read(&file_path).map_err(|error| {
        TaskExecutionError::runtime(format!(
            "failed to read book file for hash task '{}': {error}",
            file_path.display(),
        ))
    })?;

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    let hash = digest
        .iter()
        .map(|value| format!("{value:02x}"))
        .collect::<String>();

    persist_book_hash(runtime.database_file.as_path(), book_id, &hash, koreader)
        .map_err(TaskExecutionError::runtime)
}

pub(in crate::task_queue) fn find_books_without_selected_thumbnails(
    runtime: &RuntimeConfig,
) -> Result<Vec<String>, TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    load_persisted_books_without_selected_thumbnails(runtime.database_file.as_path())
        .map_err(TaskExecutionError::runtime)
}

pub(in crate::task_queue) fn find_books_with_undersized_generated_thumbnails(
    runtime: &RuntimeConfig,
    max_edge: i64,
) -> Result<Vec<String>, TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    load_books_with_undersized_generated_thumbnails(runtime.database_file.as_path(), max_edge)
        .map_err(TaskExecutionError::runtime)
}

pub(in crate::task_queue) fn find_books_with_missing_page_hash(
    runtime: &RuntimeConfig,
    library_id: Option<&str>,
) -> Result<Vec<String>, TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    load_persisted_books_with_missing_page_hash(runtime.database_file.as_path(), library_id)
        .map_err(TaskExecutionError::runtime)
}

pub(in crate::task_queue) fn find_duplicate_pages_to_delete(
    runtime: &RuntimeConfig,
    library_id: &str,
) -> Result<HashMap<String, Vec<HashedPageToDelete>>, TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    let persisted =
        load_persisted_duplicate_pages_to_delete(runtime.database_file.as_path(), library_id)
            .map_err(TaskExecutionError::runtime)?;

    Ok(persisted
        .into_iter()
        .map(|(book_id, pages)| {
            (
                book_id,
                pages
                    .into_iter()
                    .map(|page| HashedPageToDelete {
                        file_hash: page.file_hash,
                        file_size: page.file_size,
                        file_name: page.file_name,
                        media_type: page.media_type,
                        page_number: page.page_number,
                    })
                    .collect(),
            )
        })
        .collect())
}

pub(in crate::task_queue) fn find_books_requiring_analysis(
    runtime: &RuntimeConfig,
    book_ids: &[String],
) -> Result<Vec<String>, TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    load_persisted_books_requiring_analysis(runtime.database_file.as_path(), book_ids)
        .map_err(TaskExecutionError::runtime)
}

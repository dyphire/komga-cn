use super::hashed_pages::HashedPageToDelete;
use super::media_queries::{
    load_book_file_path, load_book_hash_runtime_state, load_book_library_id,
    load_books_with_missing_page_hash as load_persisted_books_with_missing_page_hash,
    load_books_with_undersized_generated_thumbnails,
    load_duplicate_pages_to_delete as load_persisted_duplicate_pages_to_delete,
    load_non_deleted_book_ids as load_persisted_non_deleted_book_ids,
};
use super::media_updates::persist_book_hash;
use super::*;
use crate::filesystem::media_access::hashes::persist_book_page_hashes_from_media_content;
use crate::task_queue::TaskRuntimeContext;
use tokio::fs;

pub(in crate::task_queue) async fn hash_book_pages(
    runtime: &TaskRuntimeContext,
    book_id: &str,
) -> Result<(), TaskExecutionError> {
    let Some(library_id) = load_book_library_id(&runtime.task_write_pool, book_id)
        .await
        .map_err(TaskExecutionError::runtime)?
    else {
        return Ok(());
    };
    let hashing_flags = load_library_hashing_flags(runtime, &library_id).await?;
    if !hashing_flags.hash_pages {
        return Ok(());
    }

    persist_book_page_hashes_from_media_content(runtime.main_db.database_file(), book_id)
        .await
        .map_err(TaskExecutionError::runtime)
}

pub(in crate::task_queue) async fn hash_book(
    runtime: &TaskRuntimeContext,
    book_id: &str,
    koreader: bool,
) -> Result<(), TaskExecutionError> {
    if !runtime.owns_main_database {
        return Ok(());
    }

    let Some(state) = load_book_hash_runtime_state(&runtime.task_write_pool, book_id)
        .await
        .map_err(TaskExecutionError::runtime)?
    else {
        return Ok(());
    };
    let hashing_flags = load_library_hashing_flags(runtime, &state.library_id).await?;
    if koreader {
        if !hashing_flags.hash_koreader {
            return Ok(());
        }
        if state
            .file_hash_koreader
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        {
            return Ok(());
        }
    } else {
        if !hashing_flags.hash_files {
            return Ok(());
        }
        if state
            .file_hash
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        {
            return Ok(());
        }
    }

    let Some(file_path) = load_book_file_path(&runtime.task_write_pool, book_id)
        .await
        .map_err(TaskExecutionError::runtime)?
    else {
        return Ok(());
    };

    let bytes = fs::read(&file_path).await.map_err(|error| {
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

    persist_book_hash(&runtime.task_write_pool, book_id, &hash, koreader)
        .await
        .map_err(TaskExecutionError::runtime)
}

pub(in crate::task_queue) async fn find_books_for_thumbnail_regeneration(
    runtime: &TaskRuntimeContext,
) -> Result<Vec<String>, TaskExecutionError> {
    load_persisted_non_deleted_book_ids(&runtime.task_write_pool)
        .await
        .map_err(TaskExecutionError::runtime)
}

pub(in crate::task_queue) async fn find_books_with_undersized_generated_thumbnails(
    runtime: &TaskRuntimeContext,
    max_edge: i64,
) -> Result<Vec<String>, TaskExecutionError> {
    load_books_with_undersized_generated_thumbnails(&runtime.task_write_pool, max_edge)
        .await
        .map_err(TaskExecutionError::runtime)
}

pub(in crate::task_queue) async fn find_books_with_missing_page_hash(
    runtime: &TaskRuntimeContext,
    library_id: Option<&str>,
) -> Result<Vec<String>, TaskExecutionError> {
    load_persisted_books_with_missing_page_hash(&runtime.task_write_pool, library_id)
        .await
        .map_err(TaskExecutionError::runtime)
}

pub(in crate::task_queue) async fn find_duplicate_pages_to_delete(
    runtime: &TaskRuntimeContext,
    library_id: &str,
) -> Result<HashMap<String, Vec<HashedPageToDelete>>, TaskExecutionError> {
    let persisted = load_persisted_duplicate_pages_to_delete(&runtime.task_write_pool, library_id)
        .await
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

use std::collections::HashMap;

use komga_application::task_processing::TaskProcessingError;
use sha2::{Digest, Sha256};
use tokio::fs;

use super::hashed_pages::HashedPageToDelete;
use super::library_flags::load_library_hashing_flags;
use super::persistence::{
    load_book_file_path, load_book_hash_runtime_state, load_book_library_id,
    load_duplicate_pages_to_delete as load_persisted_duplicate_pages_to_delete,
};
use super::updates::persist_book_hash;
use crate::media::maintenance::page_hashing::persist_book_page_hashes_from_media_content;
use crate::tasks::JobRuntime;

pub(crate) async fn hash_book_pages(
    runtime: &JobRuntime<'_>,
    book_id: &str,
) -> Result<(), TaskProcessingError> {
    let Some(library_id) = load_book_library_id(runtime.database().read_pool(), book_id)
        .await
        .map_err(TaskProcessingError::runtime)?
    else {
        return Ok(());
    };
    let hashing_flags = load_library_hashing_flags(runtime, &library_id).await?;
    if !hashing_flags.hash_pages {
        return Ok(());
    }

    persist_book_page_hashes_from_media_content(runtime.database().read_pool(), book_id)
        .await
        .map_err(TaskProcessingError::runtime)
}

pub(crate) async fn hash_book(
    runtime: &JobRuntime<'_>,
    book_id: &str,
    koreader: bool,
) -> Result<(), TaskProcessingError> {
    if !runtime.database().owns_main_database() {
        return Ok(());
    }

    let Some(state) = load_book_hash_runtime_state(runtime.database().read_pool(), book_id)
        .await
        .map_err(TaskProcessingError::runtime)?
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

    let Some(file_path) = load_book_file_path(runtime.database().read_pool(), book_id)
        .await
        .map_err(TaskProcessingError::runtime)?
    else {
        return Ok(());
    };

    let bytes = fs::read(&file_path).await.map_err(|error| {
        TaskProcessingError::runtime(format!(
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

    persist_book_hash(runtime.database().write_pool(), book_id, &hash, koreader)
        .await
        .map_err(TaskProcessingError::runtime)
}

pub(crate) async fn find_duplicate_pages_to_delete(
    runtime: &JobRuntime<'_>,
    library_id: &str,
) -> Result<HashMap<String, Vec<HashedPageToDelete>>, TaskProcessingError> {
    let persisted =
        load_persisted_duplicate_pages_to_delete(runtime.database().read_pool(), library_id)
            .await
            .map_err(TaskProcessingError::runtime)?;

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

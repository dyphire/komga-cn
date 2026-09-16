use std::collections::HashMap;

use komga_application::task_processing::TaskProcessingError;
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::AsyncReadExt;

use super::hashed_pages::HashedPageToDelete;
use super::library_flags::load_library_hashing_flags;
use super::persistence::{
    load_book_file_path, load_book_hash_runtime_state, load_book_library_id,
    load_duplicate_pages_to_delete as load_persisted_duplicate_pages_to_delete,
};
use super::updates::persist_book_hash;
use crate::MediaLibraryJobContext;
use crate::maintenance::page_hashing::persist_book_page_hashes_from_media_content;

pub async fn hash_book_pages(
    runtime: &MediaLibraryJobContext,
    book_id: &str,
) -> Result<(), TaskProcessingError> {
    let Some(library_id) = load_book_library_id(runtime.database().task_read_pool(), book_id)
        .await
        .map_err(TaskProcessingError::runtime)?
    else {
        return Ok(());
    };
    let hashing_flags = load_library_hashing_flags(runtime, &library_id).await?;
    if !hashing_flags.hash_pages {
        return Ok(());
    }

    persist_book_page_hashes_from_media_content(runtime.database().task_write_pool(), book_id)
        .await
        .map_err(TaskProcessingError::runtime)
}

pub async fn hash_book(
    runtime: &MediaLibraryJobContext,
    book_id: &str,
) -> Result<(), TaskProcessingError> {
    if !runtime.database().owns_main_database() {
        return Ok(());
    }

    let Some(state) = load_book_hash_runtime_state(runtime.database().task_read_pool(), book_id)
        .await
        .map_err(TaskProcessingError::runtime)?
    else {
        return Ok(());
    };
    let hashing_flags = load_library_hashing_flags(runtime, &state.library_id).await?;

    let need_file_hash = hashing_flags.hash_files && hash_missing(&state.file_hash);
    let need_koreader_hash = hashing_flags.hash_koreader && hash_missing(&state.file_hash_koreader);
    if !need_file_hash && !need_koreader_hash {
        return Ok(());
    }

    let Some(file_path) = load_book_file_path(runtime.database().task_read_pool(), book_id)
        .await
        .map_err(TaskProcessingError::runtime)?
    else {
        return Ok(());
    };

    let mut file = fs::File::open(&file_path).await.map_err(|error| {
        TaskProcessingError::runtime(format!(
            "failed to open book file for hash task '{}': {error}",
            file_path.display(),
        ))
    })?;

    // One streaming read over the raw file bytes, feeding every requested
    // hasher: FILE_HASH and FILE_HASH_KOREADER are computed together, so
    // enabling both library hashing flags costs a single full read instead
    // of two.
    let mut hasher = need_file_hash.then(Sha256::new);
    let mut hasher_koreader = need_koreader_hash.then(Sha256::new);
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf).await.map_err(|error| {
            TaskProcessingError::runtime(format!(
                "failed to read book file for hash task '{}': {error}",
                file_path.display(),
            ))
        })?;
        if n == 0 {
            break;
        }
        if let Some(hasher) = hasher.as_mut() {
            hasher.update(&buf[..n]);
        }
        if let Some(hasher) = hasher_koreader.as_mut() {
            hasher.update(&buf[..n]);
        }
    }

    if let Some(hasher) = hasher {
        let digest = hasher.finalize();
        let hash = digest
            .iter()
            .map(|value| format!("{value:02x}"))
            .collect::<String>();
        persist_book_hash(runtime.database().task_write_pool(), book_id, &hash, false)
            .await
            .map_err(TaskProcessingError::runtime)?;
    }
    if let Some(hasher) = hasher_koreader {
        let digest = hasher.finalize();
        let hash = digest
            .iter()
            .map(|value| format!("{value:02x}"))
            .collect::<String>();
        persist_book_hash(runtime.database().task_write_pool(), book_id, &hash, true)
            .await
            .map_err(TaskProcessingError::runtime)?;
    }

    Ok(())
}

fn hash_missing(value: &Option<String>) -> bool {
    value.as_deref().map_or(true, |value| value.trim().is_empty())
}

pub async fn find_duplicate_pages_to_delete(
    runtime: &MediaLibraryJobContext,
    library_id: &str,
) -> Result<HashMap<String, Vec<HashedPageToDelete>>, TaskProcessingError> {
    let persisted =
        load_persisted_duplicate_pages_to_delete(runtime.database().task_read_pool(), library_id)
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

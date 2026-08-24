use std::collections::HashMap;
use std::fs;
use std::io::{ErrorKind, Read};
use std::path::PathBuf;

use komga_application::task_processing::{HashedPageToDeletePayload, TaskProcessingError};
use komga_domain::discovery::MediaStatus;
use zip::ZipArchive;

use super::archive::{StoredArchiveEntry, build_stored_zip_archive, metadata_updated_unix_seconds};
use super::persistence::{
    PersistedHashedPageToDelete, load_book_archive_source as load_persisted_book_archive_source,
    load_book_hashed_pages as load_persisted_book_hashed_pages,
};
use super::updates::{persist_duplicate_page_deleted_events, persist_removed_hashed_pages};
use crate::media::analysis::{is_supported_page_image_file_name, media_type_from_entry_name};
use crate::tasks::JobRuntime;

pub(crate) type HashedPageToDelete = HashedPageToDeletePayload;

pub(crate) struct BookArchiveSource {
    pub(crate) file_path: PathBuf,
    pub(crate) series_id: String,
    pub(crate) file_last_modified: i64,
    pub(crate) media_type: String,
    pub(crate) media_status: Option<MediaStatus>,
}

#[derive(Debug, PartialEq, Eq)]
struct BookFileMetadata {
    file_last_modified: i64,
    file_size: i64,
}

pub(crate) async fn remove_hashed_pages(
    runtime: &JobRuntime<'_>,
    book_id: &str,
    pages: &[HashedPageToDelete],
) -> Result<bool, TaskProcessingError> {
    if pages.is_empty() {
        return Ok(false);
    }

    let source = load_book_archive_source(runtime, book_id).await?;
    let Some(source) = source else {
        return Ok(false);
    };

    match fs::metadata(&source.file_path) {
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(TaskProcessingError::runtime(format!(
                "file not found for hashed-page removal '{}': {}",
                book_id,
                source.file_path.display(),
            )));
        }
        Err(error) => {
            return Err(TaskProcessingError::runtime(format!(
                "failed to inspect source path for hashed-page removal '{}': {}: {error}",
                book_id,
                source.file_path.display(),
            )));
        }
    }

    let metadata = load_book_file_metadata(book_id, &source.file_path, "source")?;

    if !source.media_type.eq_ignore_ascii_case("application/zip") {
        return Err(TaskProcessingError::runtime(format!(
            "unsupported media type for hashed-page removal '{}': {}",
            book_id, source.media_type,
        )));
    }

    if source.media_status != Some(MediaStatus::Ready) {
        return Err(TaskProcessingError::runtime(format!(
            "media not ready for hashed-page removal '{}': {}",
            book_id,
            source
                .media_status
                .map(MediaStatus::persisted_name)
                .unwrap_or("UNKNOWN"),
        )));
    };

    if metadata.file_last_modified != source.file_last_modified {
        return Ok(false);
    }

    let current_pages = load_book_hashed_pages(runtime, book_id).await?;
    let pages_to_remove = matching_hashed_pages_to_remove(current_pages.as_slice(), pages);
    if pages_to_remove.len() != pages.len() {
        return Ok(false);
    }

    let removed_pages =
        rewrite_zip_book_without_pages(&source.file_path, pages_to_remove.as_slice())?;
    if removed_pages.is_empty() {
        return Ok(false);
    }

    let mut deleted_count_by_hash = HashMap::<String, i64>::new();
    for removed in &removed_pages {
        *deleted_count_by_hash
            .entry(removed.file_hash.clone())
            .or_insert(0) += 1;
    }

    let metadata = load_book_file_metadata(book_id, &source.file_path, "rewritten source")?;

    let book_id = book_id.to_string();
    let analyze_book_id = book_id.clone();
    let removed_page_events = removed_pages
        .iter()
        .map(|page| PersistedHashedPageToDelete {
            file_hash: page.file_hash.clone(),
            file_size: page.file_size,
            file_name: page.file_name.clone(),
            media_type: page.media_type.clone(),
            page_number: page.page_number,
        })
        .collect::<Vec<_>>();

    persist_removed_hashed_pages(
        runtime.database().write_pool(),
        runtime.runtime_events(),
        &book_id,
        &deleted_count_by_hash,
        metadata.file_last_modified,
        metadata.file_size,
    )
    .await
    .map_err(TaskProcessingError::runtime)?;

    crate::media::analysis::analyze_book(runtime, analyze_book_id.as_str()).await?;

    persist_duplicate_page_deleted_events(
        runtime.database().write_pool(),
        &book_id,
        &source.series_id,
        &source.file_path,
        &removed_page_events,
    )
    .await
    .map_err(TaskProcessingError::runtime)?;

    Ok(removed_pages.iter().any(|page| page.page_number == 1))
}

fn load_book_file_metadata(
    book_id: &str,
    file_path: &PathBuf,
    source: &str,
) -> Result<BookFileMetadata, TaskProcessingError> {
    let metadata = fs::metadata(file_path).map_err(|error| {
        TaskProcessingError::runtime(format!(
            "failed to read {source} metadata for '{book_id}' ('{}'): {error}",
            file_path.display()
        ))
    })?;
    let file_size = i64::try_from(metadata.len()).map_err(|_| {
        TaskProcessingError::runtime(format!(
            "file size too large for '{book_id}' ('{}')",
            file_path.display()
        ))
    })?;

    Ok(BookFileMetadata {
        file_last_modified: metadata_updated_unix_seconds(&metadata, file_path)
            .map_err(TaskProcessingError::runtime)?,
        file_size,
    })
}

pub(crate) async fn load_book_archive_source(
    runtime: &JobRuntime<'_>,
    book_id: &str,
) -> Result<Option<BookArchiveSource>, TaskProcessingError> {
    Ok(
        load_persisted_book_archive_source(runtime.database().read_pool(), book_id)
            .await
            .map_err(TaskProcessingError::runtime)?
            .map(|source| BookArchiveSource {
                file_path: source.file_path,
                series_id: source.series_id,
                file_last_modified: source.file_last_modified,
                media_type: source.media_type,
                media_status: source.media_status,
            }),
    )
}

async fn load_book_hashed_pages(
    runtime: &JobRuntime<'_>,
    book_id: &str,
) -> Result<Vec<HashedPageToDelete>, TaskProcessingError> {
    load_persisted_book_hashed_pages(runtime.database().read_pool(), book_id)
        .await
        .map(|pages| {
            pages
                .into_iter()
                .map(|page| HashedPageToDelete {
                    file_hash: page.file_hash,
                    file_size: page.file_size,
                    file_name: page.file_name,
                    media_type: page.media_type,
                    page_number: page.page_number,
                })
                .collect()
        })
        .map_err(TaskProcessingError::runtime)
}

fn matching_hashed_pages_to_remove(
    current_pages: &[HashedPageToDelete],
    requested_pages: &[HashedPageToDelete],
) -> Vec<HashedPageToDelete> {
    current_pages
        .iter()
        .filter(|current| {
            requested_pages.iter().any(|candidate| {
                candidate.file_hash == current.file_hash
                    && candidate.media_type == current.media_type
                    && candidate.file_name == current.file_name
                    && candidate.page_number == current.page_number
            })
        })
        .cloned()
        .collect()
}

pub(crate) fn rewrite_zip_book_without_pages(
    archive_path: &PathBuf,
    pages_to_delete: &[HashedPageToDelete],
) -> Result<Vec<HashedPageToDelete>, TaskProcessingError> {
    let source_file = fs::File::open(archive_path).map_err(|error| {
        TaskProcessingError::runtime(format!(
            "failed to open archive '{}' for page deletion: {error}",
            archive_path.display(),
        ))
    })?;
    let mut archive = ZipArchive::new(source_file).map_err(|error| {
        TaskProcessingError::runtime(format!(
            "failed to read zip archive '{}' for page deletion: {error}",
            archive_path.display(),
        ))
    })?;

    let mut delete_by_page_number = HashMap::<i64, HashedPageToDelete>::new();
    for page in pages_to_delete {
        delete_by_page_number.insert(page.page_number, page.clone());
    }

    let mut kept_entries = Vec::<StoredArchiveEntry>::new();
    let mut removed_pages = Vec::<HashedPageToDelete>::new();
    let mut page_number = 0_i64;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            TaskProcessingError::runtime(format!(
                "failed to read zip entry index {index} for '{}': {error}",
                archive_path.display(),
            ))
        })?;
        if entry.is_dir() {
            continue;
        }

        let entry_name = entry
            .name()
            .map_err(|error| {
                TaskProcessingError::runtime(format!(
                    "failed to read zip entry name index {index} for '{}': {error}",
                    archive_path.display(),
                ))
            })?
            .into_owned();
        let should_remove = if is_supported_page_image_file_name(&entry_name) {
            page_number += 1;
            delete_by_page_number
                .get(&page_number)
                .filter(|candidate| {
                    candidate.file_name == entry_name
                        && candidate.media_type == media_type_from_entry_name(&entry_name)
                })
                .cloned()
        } else {
            None
        };

        if let Some(removed) = should_remove {
            removed_pages.push(removed);
            continue;
        }

        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|error| {
            TaskProcessingError::runtime(format!(
                "failed to read zip entry '{}' bytes for '{}': {error}",
                entry_name,
                archive_path.display(),
            ))
        })?;
        kept_entries.push(StoredArchiveEntry {
            file_name: entry_name,
            bytes,
        });
    }

    // Windows refuses to replace the archive while the source ZIP reader still owns the file.
    drop(archive);

    if removed_pages.is_empty() {
        return Ok(Vec::new());
    }

    if removed_pages.len() != pages_to_delete.len() {
        return Ok(Vec::new());
    }

    if kept_entries.is_empty() {
        return Err(TaskProcessingError::runtime(format!(
            "refused to rewrite '{}' with zero entries after page deletion",
            archive_path.display(),
        )));
    }

    let rewritten = build_stored_zip_archive(kept_entries)?;
    let temp_path = archive_path.with_extension("komga-page-removal.tmp");
    fs::write(&temp_path, rewritten).map_err(|error| {
        TaskProcessingError::runtime(format!(
            "failed to write temporary rewritten archive '{}': {error}",
            temp_path.display(),
        ))
    })?;
    fs::rename(&temp_path, archive_path).map_err(|error| {
        let _ = fs::remove_file(&temp_path);
        TaskProcessingError::runtime(format!(
            "failed to replace archive '{}' with rewritten file '{}': {error}",
            archive_path.display(),
            temp_path.display(),
        ))
    })?;

    Ok(removed_pages)
}

#[cfg(test)]
mod tests {
    use super::load_book_file_metadata;

    #[test]
    fn load_book_file_metadata_fails_when_metadata_cannot_be_read() {
        let missing_path = std::env::temp_dir().join(format!(
            "komga-missing-hashed-page-metadata-{}",
            std::process::id()
        ));

        let error = load_book_file_metadata("book-1", &missing_path, "rewritten source")
            .expect_err("missing rewritten source metadata should fail");

        assert!(
            error
                .message
                .contains("failed to read rewritten source metadata for 'book-1'")
        );
    }
}

use super::archive_utils::{build_stored_zip_archive, metadata_updated_unix_seconds};
use super::media_analysis::{is_supported_page_image_file_name, media_type_from_entry_name};
use super::media_queries::{
    PersistedHashedPageToDelete, load_book_archive_source as load_persisted_book_archive_source,
    load_book_hashed_pages as load_persisted_book_hashed_pages,
};
use super::media_updates::{persist_duplicate_page_deleted_events, persist_removed_hashed_pages};
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::task_queue) struct RemoveHashedPagesPayload {
    pub(in crate::task_queue) book_id: String,
    pub(in crate::task_queue) pages: Vec<HashedPageToDelete>,
    pub(in crate::task_queue) priority: i32,
    pub(in crate::task_queue) group_id: Option<String>,
    pub(in crate::task_queue) unique_id: String,
}

impl RemoveHashedPagesPayload {
    pub(in crate::task_queue) fn new(
        book_id: String,
        pages: Vec<HashedPageToDelete>,
        priority: i32,
    ) -> Self {
        let unique_id = remove_hashed_pages_task_id(book_id.as_str());
        Self {
            book_id,
            pages,
            priority,
            group_id: None,
            unique_id,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::task_queue) struct HashedPageToDelete {
    pub(in crate::task_queue) file_hash: String,
    pub(in crate::task_queue) file_size: i64,
    pub(in crate::task_queue) file_name: String,
    pub(in crate::task_queue) media_type: String,
    pub(in crate::task_queue) page_number: i64,
}

pub(in crate::task_queue) struct BookArchiveSource {
    pub(in crate::task_queue) file_path: PathBuf,
    pub(in crate::task_queue) series_id: String,
    pub(in crate::task_queue) file_last_modified: i64,
    pub(in crate::task_queue) media_type: String,
    pub(in crate::task_queue) media_status: String,
}

pub(in crate::task_queue) fn remove_hashed_pages_task_id(book_id: &str) -> String {
    format!("RemoveHashedPages_{book_id}")
}

pub(in crate::task_queue) async fn remove_hashed_pages(
    runtime: &RuntimeConfig,
    book_id: &str,
    pages: &[HashedPageToDelete],
) -> Result<bool, TaskExecutionError> {
    if pages.is_empty() {
        return Ok(false);
    }

    let source = load_book_archive_source(runtime, book_id).await?;
    let Some(source) = source else {
        return Ok(false);
    };

    if !source.file_path.exists() {
        return Err(TaskExecutionError::runtime(format!(
            "file not found for hashed-page removal '{}': {}",
            book_id,
            source.file_path.display(),
        )));
    }

    let metadata = fs::metadata(&source.file_path).map_err(|error| {
        TaskExecutionError::runtime(format!(
            "failed to read source metadata for hashed-page removal '{}' ('{}'): {error}",
            book_id,
            source.file_path.display(),
        ))
    })?;

    if !source.media_type.eq_ignore_ascii_case("application/zip") {
        return Err(TaskExecutionError::runtime(format!(
            "unsupported media type for hashed-page removal '{}': {}",
            book_id, source.media_type,
        )));
    }

    if !source.media_status.eq_ignore_ascii_case("READY") {
        return Err(TaskExecutionError::runtime(format!(
            "media not ready for hashed-page removal '{}': {}",
            book_id, source.media_status,
        )));
    };

    if metadata_updated_unix_seconds(&metadata) != source.file_last_modified {
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

    let file_size = fs::metadata(&source.file_path)
        .map(|metadata| metadata.len() as i64)
        .unwrap_or_default();
    let file_last_modified = fs::metadata(&source.file_path)
        .map(|metadata| metadata_updated_unix_seconds(&metadata))
        .unwrap_or_default();

    let runtime_context = runtime.task_runtime_context();
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
        &runtime_context.task_write_pool,
        &book_id,
        &deleted_count_by_hash,
        file_last_modified,
        file_size,
    )
    .await
    .map_err(TaskExecutionError::runtime)?;

    super::index_tasks::analyze_book(runtime, analyze_book_id.as_str()).await?;

    persist_duplicate_page_deleted_events(
        &runtime_context.task_write_pool,
        &book_id,
        &source.series_id,
        &source.file_path,
        &removed_page_events,
    )
    .await
    .map_err(TaskExecutionError::runtime)?;

    Ok(removed_pages.iter().any(|page| page.page_number == 1))
}

pub(in crate::task_queue) async fn load_book_archive_source(
    runtime: &RuntimeConfig,
    book_id: &str,
) -> Result<Option<BookArchiveSource>, TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    Ok(
        load_persisted_book_archive_source(&runtime.task_read_pool, book_id)
            .await
            .map_err(TaskExecutionError::runtime)?
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
    runtime: &RuntimeConfig,
    book_id: &str,
) -> Result<Vec<HashedPageToDelete>, TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    load_persisted_book_hashed_pages(&runtime.task_read_pool, book_id)
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
        .map_err(TaskExecutionError::runtime)
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

pub(in crate::task_queue) fn rewrite_zip_book_without_pages(
    archive_path: &PathBuf,
    pages_to_delete: &[HashedPageToDelete],
) -> Result<Vec<HashedPageToDelete>, TaskExecutionError> {
    let source_file = fs::File::open(archive_path).map_err(|error| {
        TaskExecutionError::runtime(format!(
            "failed to open archive '{}' for page deletion: {error}",
            archive_path.display(),
        ))
    })?;
    let mut archive = ZipArchive::new(source_file).map_err(|error| {
        TaskExecutionError::runtime(format!(
            "failed to read zip archive '{}' for page deletion: {error}",
            archive_path.display(),
        ))
    })?;

    let mut delete_by_page_number = HashMap::<i64, HashedPageToDelete>::new();
    for page in pages_to_delete {
        delete_by_page_number.insert(page.page_number, page.clone());
    }

    let mut kept_entries = Vec::<(String, Vec<u8>)>::new();
    let mut removed_pages = Vec::<HashedPageToDelete>::new();
    let mut page_number = 0_i64;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            TaskExecutionError::runtime(format!(
                "failed to read zip entry index {index} for '{}': {error}",
                archive_path.display(),
            ))
        })?;
        if entry.is_dir() {
            continue;
        }

        let entry_name = entry.name().to_string();
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
            TaskExecutionError::runtime(format!(
                "failed to read zip entry '{}' bytes for '{}': {error}",
                entry_name,
                archive_path.display(),
            ))
        })?;
        kept_entries.push((entry_name, bytes));
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
        return Err(TaskExecutionError::runtime(format!(
            "refused to rewrite '{}' with zero entries after page deletion",
            archive_path.display(),
        )));
    }

    let rewritten = build_stored_zip_archive(kept_entries)?;
    let temp_path = archive_path.with_extension("komga-page-removal.tmp");
    fs::write(&temp_path, rewritten).map_err(|error| {
        TaskExecutionError::runtime(format!(
            "failed to write temporary rewritten archive '{}': {error}",
            temp_path.display(),
        ))
    })?;
    fs::rename(&temp_path, archive_path).map_err(|error| {
        let _ = fs::remove_file(&temp_path);
        TaskExecutionError::runtime(format!(
            "failed to replace archive '{}' with rewritten file '{}': {error}",
            archive_path.display(),
            temp_path.display(),
        ))
    })?;

    Ok(removed_pages)
}

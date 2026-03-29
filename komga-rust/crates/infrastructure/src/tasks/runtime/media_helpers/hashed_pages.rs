use super::archive_utils::{build_stored_zip_archive, metadata_updated_unix_seconds};
use super::media_analysis::media_type_from_entry_name;
use super::*;
use crate::tasks::{
    load_book_archive_source as load_persisted_book_archive_source, persist_removed_hashed_pages,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::task_queue) struct RemoveHashedPagesPayload {
    pub(in crate::task_queue) pages: Vec<HashedPageToDelete>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::task_queue) struct HashedPageToDelete {
    pub(in crate::task_queue) hash: String,
    pub(in crate::task_queue) number: i64,
    pub(in crate::task_queue) file_name: String,
    pub(in crate::task_queue) media_type: String,
}

pub(in crate::task_queue) struct BookArchiveSource {
    pub(in crate::task_queue) file_path: PathBuf,
    pub(in crate::task_queue) media_type: String,
    pub(in crate::task_queue) media_status: String,
}

pub(in crate::task_queue) fn remove_hashed_pages(
    runtime: &RuntimeConfig,
    book_id: &str,
    pages: &[HashedPageToDelete],
) -> Result<bool, TaskExecutionError> {
    if pages.is_empty() {
        return Ok(false);
    }

    let source = load_book_archive_source(runtime, book_id)?;
    let Some(source) = source else {
        return Ok(false);
    };

    if !source.media_type.eq_ignore_ascii_case("application/zip")
        || !source.media_status.eq_ignore_ascii_case("READY")
    {
        return Ok(false);
    }

    let removed_pages = rewrite_zip_book_without_pages(&source.file_path, pages)?;
    if removed_pages.is_empty() {
        return Ok(false);
    }

    let mut deleted_count_by_hash = HashMap::<String, i64>::new();
    for removed in &removed_pages {
        *deleted_count_by_hash
            .entry(removed.hash.clone())
            .or_insert(0) += 1;
    }

    let file_size = fs::metadata(&source.file_path)
        .map(|metadata| metadata.len() as i64)
        .unwrap_or_default();
    let file_last_modified = fs::metadata(&source.file_path)
        .map(|metadata| metadata_updated_unix_seconds(&metadata))
        .unwrap_or_default();

    let runtime_context = runtime.task_runtime_context();
    let database_file = runtime_context.database_file.clone();
    let book_id = book_id.to_string();
    let analyze_book_id = book_id.clone();

    persist_removed_hashed_pages(
        database_file.as_path(),
        &book_id,
        &deleted_count_by_hash,
        file_last_modified,
        file_size,
    )
    .map_err(TaskExecutionError::runtime)?;

    super::index_tasks::analyze_book(runtime, analyze_book_id.as_str())?;

    Ok(removed_pages.iter().any(|page| page.number == 0))
}

pub(in crate::task_queue) fn load_book_archive_source(
    runtime: &RuntimeConfig,
    book_id: &str,
) -> Result<Option<BookArchiveSource>, TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    Ok(
        load_persisted_book_archive_source(runtime.database_file.as_path(), book_id)
            .map_err(TaskExecutionError::runtime)?
            .map(|source| BookArchiveSource {
                file_path: source.file_path,
                media_type: source.media_type,
                media_status: source.media_status,
            }),
    )
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

    let mut delete_by_name = HashMap::<String, Vec<HashedPageToDelete>>::new();
    for page in pages_to_delete {
        delete_by_name
            .entry(page.file_name.clone())
            .or_default()
            .push(page.clone());
    }

    let mut kept_entries = Vec::<(String, Vec<u8>)>::new();
    let mut removed_pages = Vec::<HashedPageToDelete>::new();

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
        let should_remove = delete_by_name
            .get(&entry_name)
            .and_then(|candidates| {
                candidates.iter().find(|candidate| {
                    candidate.media_type == media_type_from_entry_name(&entry_name)
                })
            })
            .cloned();

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

    if removed_pages.is_empty() {
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

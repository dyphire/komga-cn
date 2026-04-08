use super::*;
use crate::tasks::{
    PersistedHashedPageToDelete, load_book_conversion_target, load_book_hashed_pages,
    load_books_to_convert, load_library_maintenance_flags, persist_book_conversion,
    persist_book_conversion_events, persist_book_page_hashes,
};
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

static FAILED_BOOK_CONVERSIONS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn failed_book_conversions() -> &'static Mutex<HashSet<String>> {
    FAILED_BOOK_CONVERSIONS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn book_conversion_failed_before(book_id: &str) -> bool {
    failed_book_conversions()
        .lock()
        .expect("failed book conversion lock should not be poisoned")
        .contains(book_id)
}

fn mark_book_conversion_failed(book_id: &str) {
    failed_book_conversions()
        .lock()
        .expect("failed book conversion lock should not be poisoned")
        .insert(book_id.to_string());
}

fn restored_page_hashes(
    current_pages: &[PersistedHashedPageToDelete],
    previous_pages: &[PersistedHashedPageToDelete],
) -> Vec<(i64, String)> {
    current_pages
        .iter()
        .filter_map(|current_page| {
            previous_pages
                .iter()
                .find(|previous_page| {
                    previous_page.file_size == current_page.file_size
                        && previous_page.media_type == current_page.media_type
                        && previous_page.file_name == current_page.file_name
                        && !previous_page.file_hash.trim().is_empty()
                })
                .map(|previous_page| (current_page.page_number, previous_page.file_hash.clone()))
        })
        .collect()
}

pub(in crate::task_queue) fn find_books_to_convert(
    runtime: &RuntimeConfig,
    library_id: &str,
) -> Result<Vec<crate::tasks::PersistedBookToConvert>, TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    let maintenance_flags =
        load_library_maintenance_flags(runtime.database_file.as_path(), library_id)
            .map_err(TaskExecutionError::runtime)?;
    if !maintenance_flags.convert_to_cbz {
        return Ok(Vec::new());
    }

    load_books_to_convert(runtime.database_file.as_path(), library_id)
        .map_err(TaskExecutionError::runtime)
}

pub(in crate::task_queue) fn convert_book(
    runtime: &RuntimeConfig,
    book_id: &str,
) -> Result<(), TaskExecutionError> {
    let runtime_context = runtime.task_runtime_context();
    let database_file = runtime_context.database_file.clone();
    let book_id = book_id.to_string();
    let analyze_book_id = book_id.clone();

    let Some(source) = load_book_conversion_target(database_file.as_path(), &book_id)
        .map_err(TaskExecutionError::runtime)?
    else {
        return Ok(());
    };

    if !source.convert_to_cbz {
        return Ok(());
    }
    if book_conversion_failed_before(&book_id) {
        return Ok(());
    }

    if !source.media_status.eq_ignore_ascii_case("READY") {
        return Ok(());
    }
    if !is_rar_media_type(&source.media_type) {
        return Ok(());
    }

    let source_path = PathBuf::from(&source.library_root).join(&source.book_url);
    if !source_path.exists() {
        return Ok(());
    }
    let Ok(source_metadata) = fs::metadata(&source_path) else {
        return Ok(());
    };
    if metadata_updated_unix_seconds(&source_metadata) != source.file_last_modified {
        return Ok(());
    }

    let destination_path = source_path.with_extension("cbz");
    if destination_path.exists() {
        return Err(TaskExecutionError::runtime(format!(
            "failed to convert book '{book_id}' to CBZ: destination already exists '{}'",
            destination_path.display(),
        )));
    }

    let prepared_conversion: Result<(String, i64, i64), TaskExecutionError> = (|| {
        let archive_entries = load_rar_entries_for_conversion(&source_path)?;
        if archive_entries.is_empty() {
            return Err(TaskExecutionError::runtime(format!(
                "failed to convert book '{book_id}' to CBZ: no archive entries extracted",
            )));
        }

        let payload = build_stored_zip_archive(archive_entries)?;
        fs::write(&destination_path, payload).map_err(|error| {
            TaskExecutionError::runtime(format!(
                "failed to write converted CBZ file for '{book_id}' to '{}': {error}",
                destination_path.display(),
            ))
        })?;

        let destination_file = fs::File::open(&destination_path).map_err(|error| {
            TaskExecutionError::runtime(format!(
                "failed to open converted file for '{book_id}' ('{}'): {error}",
                destination_path.display(),
            ))
        })?;
        ZipArchive::new(destination_file).map_err(|error| {
            TaskExecutionError::runtime(format!(
                "failed to validate converted CBZ for '{book_id}': {error}",
            ))
        })?;

        let destination_url = normalize_library_relative_url(
            &PathBuf::from(&source.library_root),
            &destination_path,
        )?;
        let destination_metadata = fs::metadata(&destination_path).map_err(|error| {
            TaskExecutionError::runtime(format!(
                "failed to read converted CBZ metadata for '{book_id}' ('{}'): {error}",
                destination_path.display(),
            ))
        })?;

        Ok((
            destination_url,
            metadata_updated_unix_seconds(&destination_metadata),
            destination_metadata.len() as i64,
        ))
    })();

    let (destination_url, file_last_modified, file_size) = match prepared_conversion {
        Ok(prepared) => prepared,
        Err(error) => {
            let _ = fs::remove_file(&destination_path);
            mark_book_conversion_failed(&book_id);
            return Err(error);
        }
    };

    if let Err(error) = persist_book_conversion(
        database_file.as_path(),
        &book_id,
        &source.library_id,
        &source.book_url,
        &destination_url,
        file_last_modified,
        file_size,
    ) {
        let _ = fs::remove_file(&destination_path);
        return Err(TaskExecutionError::runtime(error));
    }

    let source_deleted = fs::remove_file(&source_path).is_ok();
    persist_book_conversion_events(
        database_file.as_path(),
        &book_id,
        &source.series_id,
        &source_path,
        &destination_path,
        source_deleted,
    )
    .map_err(TaskExecutionError::runtime)?;

    let previous_hashed_pages = load_book_hashed_pages(database_file.as_path(), &book_id)
        .map_err(TaskExecutionError::runtime)?;

    super::index_tasks::analyze_book(runtime, &analyze_book_id)?;

    let analyzed_pages = load_book_hashed_pages(database_file.as_path(), &book_id)
        .map_err(TaskExecutionError::runtime)?;
    let page_hashes_to_restore = restored_page_hashes(&analyzed_pages, &previous_hashed_pages);
    persist_book_page_hashes(database_file.as_path(), &book_id, &page_hashes_to_restore)
        .map_err(TaskExecutionError::runtime)?;

    Ok(())
}

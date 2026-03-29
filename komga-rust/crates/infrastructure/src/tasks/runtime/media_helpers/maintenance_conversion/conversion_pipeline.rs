use super::*;
use crate::tasks::{load_book_conversion_target, load_books_to_convert, persist_book_conversion};

pub(in crate::task_queue) fn find_books_to_convert(
    runtime: &RuntimeConfig,
    library_id: &str,
) -> Result<Vec<String>, TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
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

    let destination_path = source_path.with_extension("cbz");
    if destination_path.exists() {
        return Err(TaskExecutionError::runtime(format!(
            "failed to convert book '{book_id}' to CBZ: destination already exists '{}'",
            destination_path.display(),
        )));
    }

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

    let destination_url =
        normalize_library_relative_url(&PathBuf::from(&source.library_root), &destination_path)?;
    let file_size = fs::metadata(&destination_path)
        .map(|metadata| metadata.len() as i64)
        .unwrap_or_default();
    let file_last_modified = fs::metadata(&destination_path)
        .map(|metadata| metadata_updated_unix_seconds(&metadata))
        .unwrap_or_default();

    let converted = match persist_book_conversion(
        database_file.as_path(),
        &book_id,
        &source.library_id,
        &source.book_url,
        &destination_url,
        file_last_modified,
        file_size,
    ) {
        Ok(()) => {
            let _ = fs::remove_file(&source_path);
            true
        }
        Err(error) => {
            let _ = fs::remove_file(&destination_path);
            return Err(TaskExecutionError::runtime(error));
        }
    };

    if converted {
        super::index_tasks::analyze_book(runtime, &analyze_book_id)?;
    }

    Ok(())
}

use super::*;
use crate::tasks::{load_books_for_extension_repair, persist_book_extension_repair};

pub(in crate::task_queue) fn repair_extensions(
    runtime: &RuntimeConfig,
    library_id: &str,
) -> Result<(), TaskExecutionError> {
    let flags = load_library_maintenance_flags(runtime, library_id)?;
    if !flags.repair_extensions {
        return Ok(());
    }

    let runtime = runtime.task_runtime_context();
    let database_file = runtime.database_file.clone();
    let library_id = library_id.to_string();

    let rows = load_books_for_extension_repair(database_file.as_path(), &library_id)
        .map_err(TaskExecutionError::runtime)?;

    for row in rows {
        let book_id = row.book_id;
        let book_url = row.book_url;
        let library_root = row.library_root;
        let media_type = row.media_type;

        let Some(correct_extension) = expected_extension_for_media_type(&media_type) else {
            continue;
        };

        let source_path = PathBuf::from(&library_root).join(&book_url);
        if !source_path.exists() {
            continue;
        }

        let current_extension = source_path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .unwrap_or_default();
        if current_extension == correct_extension {
            continue;
        }

        if media_type == "application/zip" && current_extension == "epub" {
            continue;
        }

        let destination_path = source_path.with_extension(correct_extension);
        if destination_path.exists() {
            return Err(TaskExecutionError::runtime(format!(
                "failed to repair extension for '{book_id}': destination already exists '{}'",
                destination_path.display(),
            )));
        }

        fs::rename(&source_path, &destination_path).map_err(|error| {
            TaskExecutionError::runtime(format!(
                "failed to rename book file for extension repair '{}' -> '{}': {error}",
                source_path.display(),
                destination_path.display(),
            ))
        })?;

        let destination_url =
            normalize_library_relative_url(&PathBuf::from(&library_root), &destination_path)?;
        let file_size = fs::metadata(&destination_path)
            .map(|metadata| metadata.len() as i64)
            .unwrap_or_default();
        let file_last_modified = fs::metadata(&destination_path)
            .map(|metadata| metadata_updated_unix_seconds(&metadata))
            .unwrap_or_default();

        let repair_result = persist_book_extension_repair(
            database_file.as_path(),
            &book_id,
            &library_id,
            &book_url,
            &destination_url,
            file_last_modified,
            file_size,
        )
        .map_err(TaskExecutionError::runtime);

        if let Err(error) = repair_result {
            let _ = fs::rename(&destination_path, &source_path);
            return Err(error);
        }
    }

    Ok(())
}

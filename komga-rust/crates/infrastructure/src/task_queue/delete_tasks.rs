use std::io::ErrorKind;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::search::index_lifecycle::SearchEntityType;
use crate::search::runtime_tasks::sync_entity_delete_from_index;
use crate::tasks::delete_workflow::{
    load_book_delete_decision, load_book_delete_work, load_series_delete_work,
    soft_delete_book_rows, soft_delete_series_book_rows, soft_delete_series_rows,
};

pub(super) fn delete_book_task(
    runtime: &RuntimeConfig,
    book_id: &str,
) -> Result<(), TaskExecutionError> {
    if !runtime.task_runtime_context().owns_main_database {
        return Ok(());
    }

    let target = load_book_delete_target(runtime, book_id)?;
    let Some((series_id, oneshot)) = target else {
        return Ok(());
    };

    if oneshot {
        delete_series(runtime, &series_id)
    } else {
        delete_book(runtime, book_id)
    }
}

fn load_book_delete_target(
    runtime: &RuntimeConfig,
    book_id: &str,
) -> Result<Option<(String, bool)>, TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    Ok(
        load_book_delete_decision(runtime.database_file.as_path(), book_id)
            .map_err(TaskExecutionError::runtime)?
            .map(|target| (target.series_id, target.oneshot)),
    )
}

fn delete_book(runtime: &RuntimeConfig, book_id: &str) -> Result<(), TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    let Some(work) = load_book_delete_work(runtime.database_file.as_path(), book_id)
        .map_err(TaskExecutionError::runtime)?
    else {
        return Ok(());
    };

    // Delete tasks must still reconcile database state when the target file already vanished
    // before the worker runs; only existing paths should block on writability checks.
    if work.book_path.exists() && !deletion_prerequisites_met(&work.book_path) {
        return Ok(());
    }
    if !empty_parent_directory_cleanup_prerequisites_met(
        &work.book_path,
        &work.sidecar_thumbnail_paths,
    ) {
        return Ok(());
    }

    delete_file_if_exists(&work.book_path, "book file")?;
    remove_sidecar_thumbnail_files(&work.sidecar_thumbnail_paths)?;
    remove_empty_parent_directory(&work.book_path)?;

    soft_delete_book_rows(runtime.database_file.as_path(), book_id, &work.series_id)
        .map_err(TaskExecutionError::runtime)?;

    if runtime.owns_search_index {
        sync_entity_delete_from_index(
            runtime.lucene_data_directory.as_path(),
            SearchEntityType::Book,
            book_id,
        )
        .map_err(TaskExecutionError::runtime)?;
    }

    Ok(())
}
fn remove_empty_parent_directory(target_path: &Path) -> Result<(), TaskExecutionError> {
    let Some(parent_directory) = target_path.parent() else {
        return Ok(());
    };
    remove_empty_directory(parent_directory)
}

fn empty_parent_directory_cleanup_prerequisites_met(
    target_path: &Path,
    sidecar_thumbnail_paths: &[std::path::PathBuf],
) -> bool {
    let Some(parent_directory) = target_path.parent() else {
        return true;
    };
    if !parent_directory.exists() {
        return true;
    }
    let Ok(entries) = fs::read_dir(parent_directory) else {
        return false;
    };

    let mut pending_deletions = sidecar_thumbnail_paths
        .iter()
        .filter(|path| path.parent() == Some(parent_directory))
        .cloned()
        .collect::<Vec<_>>();
    pending_deletions.push(target_path.to_path_buf());

    if entries
        .filter_map(Result::ok)
        .any(|entry| !pending_deletions.iter().any(|path| path == &entry.path()))
    {
        return true;
    }

    // A delete-book task promises to remove the now-empty parent directory too. If that final
    // directory cleanup would fail, skipping early avoids partially deleting files and then
    // bailing out on Windows readonly directories.
    directory_delete_prerequisites_met(parent_directory)
}

fn remove_empty_directory(target_directory: &Path) -> Result<(), TaskExecutionError> {
    if !target_directory.exists() {
        return Ok(());
    }
    let mut entries = fs::read_dir(target_directory).map_err(|error| {
        TaskExecutionError::runtime(format!(
            "failed to list directory {} before deletion: {error}",
            target_directory.display()
        ))
    })?;
    if entries.next().is_none() {
        delete_directory_if_exists(target_directory)?;
    }
    Ok(())
}

fn deletion_prerequisites_met(target_path: &Path) -> bool {
    if !target_path.exists() {
        return false;
    }
    if target_path.is_dir() {
        return directory_delete_prerequisites_met(target_path);
    }
    fs::OpenOptions::new().write(true).open(target_path).is_ok()
}

fn directory_delete_prerequisites_met(target_directory: &Path) -> bool {
    // Windows can still allow child-file creation inside a readonly directory while refusing to
    // remove the directory itself, so delete preconditions must reject readonly metadata before
    // treating the directory as safe for book/series cleanup.
    match fs::metadata(target_directory) {
        Ok(metadata) if metadata.permissions().readonly() => false,
        Ok(_) => directory_is_writable(target_directory),
        Err(_) => false,
    }
}

fn remove_sidecar_thumbnail_files<T: AsRef<Path>>(
    sidecar_thumbnail_paths: &[T],
) -> Result<(), TaskExecutionError> {
    for sidecar_thumbnail_path in sidecar_thumbnail_paths {
        let sidecar_thumbnail_path = sidecar_thumbnail_path.as_ref();
        if deletion_prerequisites_met(sidecar_thumbnail_path) {
            delete_file_if_exists(sidecar_thumbnail_path, "sidecar thumbnail file")?;
        }
    }
    Ok(())
}

pub(super) fn delete_series(
    runtime: &RuntimeConfig,
    series_id: &str,
) -> Result<(), TaskExecutionError> {
    if !runtime.task_runtime_context().owns_main_database {
        return Ok(());
    }

    let runtime_context = runtime.task_runtime_context();
    let work = load_series_delete_work(runtime_context.database_file.as_path(), series_id)
        .map_err(TaskExecutionError::runtime)?;

    let Some(series_path) = &work.series_path else {
        return Ok(());
    };
    // A delete-series task promises to remove the series directory itself. If that directory is
    // already missing or cannot be deleted safely, abort before cascading into child soft-deletes
    // so the database never drifts ahead of the filesystem preconditions.
    if !deletion_prerequisites_met(series_path) {
        return Ok(());
    }

    for book_id in &work.book_ids {
        delete_book(runtime, book_id)?;
    }

    remove_sidecar_thumbnail_files(&work.sidecar_thumbnail_paths)?;
    remove_empty_directory(series_path)?;

    soft_delete_series_book_rows(runtime_context.database_file.as_path(), series_id)
        .map_err(TaskExecutionError::runtime)?;

    soft_delete_series_rows(runtime_context.database_file.as_path(), series_id)
        .map_err(TaskExecutionError::runtime)?;

    if runtime_context.owns_search_index {
        for book_id in &work.book_ids {
            sync_entity_delete_from_index(
                runtime_context.lucene_data_directory.as_path(),
                SearchEntityType::Book,
                book_id,
            )
            .map_err(TaskExecutionError::runtime)?;
        }
        sync_entity_delete_from_index(
            runtime_context.lucene_data_directory.as_path(),
            SearchEntityType::Series,
            series_id,
        )
        .map_err(TaskExecutionError::runtime)?;
    }

    Ok(())
}

fn delete_file_if_exists(target_path: &Path, target_kind: &str) -> Result<(), TaskExecutionError> {
    match fs::remove_file(target_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(TaskExecutionError::runtime(format!(
            "failed to delete {target_kind} {}: {error}",
            target_path.display()
        ))),
    }
}

fn delete_directory_if_exists(target_path: &Path) -> Result<(), TaskExecutionError> {
    match fs::remove_dir(target_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(TaskExecutionError::runtime(format!(
            "failed to delete directory {}: {error}",
            target_path.display()
        ))),
    }
}

fn directory_is_writable(target_directory: &Path) -> bool {
    for nonce in 0..3 {
        let probe_path = target_directory.join(format!(
            ".komga-delete-write-probe-{}-{}-{nonce}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or_default()
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&probe_path)
        {
            Ok(file) => {
                drop(file);
                let _ = fs::remove_file(probe_path);
                return true;
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(_) => return false,
        }
    }
    false
}

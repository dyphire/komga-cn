use std::path::Path;

use super::*;
use crate::search::index_lifecycle::SearchEntityType;
use crate::search::runtime_tasks::sync_entity_delete_from_index;
use crate::tasks::delete_workflow::{
    load_book_delete_decision, load_book_delete_work, load_series_delete_work,
    soft_delete_book_rows, soft_delete_series_rows,
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
        delete_oneshot_series(runtime, book_id, &series_id)
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

    if !deletion_prerequisites_met(&work.book_path) {
        return Ok(());
    }

    let _ = fs::remove_file(&work.book_path);
    for sidecar_thumbnail_path in &work.sidecar_thumbnail_paths {
        let _ = fs::remove_file(sidecar_thumbnail_path);
    }
    remove_empty_parent_directory(&work.book_path);

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

fn delete_oneshot_series(
    runtime: &RuntimeConfig,
    book_id: &str,
    series_id: &str,
) -> Result<(), TaskExecutionError> {
    let runtime_context = runtime.task_runtime_context();
    let work = load_series_delete_work(runtime_context.database_file.as_path(), series_id)
        .map_err(TaskExecutionError::runtime)?;

    let Some(series_path) = &work.series_path else {
        return Ok(());
    };
    if !deletion_prerequisites_met(series_path) {
        return Ok(());
    }

    delete_book(runtime, book_id)?;

    for sidecar_thumbnail_path in &work.sidecar_thumbnail_paths {
        let _ = fs::remove_file(sidecar_thumbnail_path);
    }
    remove_empty_directory(series_path);

    soft_delete_series_rows(runtime_context.database_file.as_path(), series_id)
        .map_err(TaskExecutionError::runtime)?;

    if runtime_context.owns_search_index {
        sync_entity_delete_from_index(
            runtime_context.lucene_data_directory.as_path(),
            SearchEntityType::Series,
            series_id,
        )
        .map_err(TaskExecutionError::runtime)?;
    }

    Ok(())
}

fn remove_empty_parent_directory(target_path: &Path) {
    let Some(parent_directory) = target_path.parent() else {
        return;
    };
    remove_empty_directory(parent_directory);
}

fn remove_empty_directory(target_directory: &Path) {
    let Ok(mut entries) = fs::read_dir(target_directory) else {
        return;
    };
    if entries.next().is_none() {
        let _ = fs::remove_dir(target_directory);
    }
}

fn deletion_prerequisites_met(target_path: &Path) -> bool {
    fs::metadata(target_path).is_ok_and(|metadata| !metadata.permissions().readonly())
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
    if !deletion_prerequisites_met(series_path) {
        return Ok(());
    }

    for book_id in &work.book_ids {
        delete_book(runtime, book_id)?;
    }

    for sidecar_thumbnail_path in &work.sidecar_thumbnail_paths {
        let _ = fs::remove_file(sidecar_thumbnail_path);
    }
    remove_empty_directory(series_path);

    soft_delete_series_rows(runtime_context.database_file.as_path(), series_id)
        .map_err(TaskExecutionError::runtime)?;

    if runtime_context.owns_search_index {
        sync_entity_delete_from_index(
            runtime_context.lucene_data_directory.as_path(),
            SearchEntityType::Series,
            series_id,
        )
        .map_err(TaskExecutionError::runtime)?;
    }

    Ok(())
}

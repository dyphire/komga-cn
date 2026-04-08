use super::*;
use std::collections::BTreeSet;

pub(super) fn refresh_book_metadata(
    runtime: &RuntimeConfig,
    book_id: &str,
    capabilities: &BTreeSet<String>,
) -> Result<Option<String>, TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    if !runtime.owns_sidecar_output {
        return Ok(None);
    }

    let outcome = crate::metadata::refresh_book_metadata(
        runtime.database_file.as_path(),
        book_id,
        capabilities,
    )
    .map_err(TaskExecutionError::runtime)?;

    if runtime.owns_search_index {
        crate::search::sync_entity_upsert_from_database(
            runtime.database_file.as_path(),
            runtime.lucene_data_directory.as_path(),
            crate::search::SearchEntityType::Book,
            book_id,
        )
        .map_err(TaskExecutionError::runtime)?;
        for readlist_id in &outcome.changed_readlist_ids {
            crate::search::sync_entity_upsert_from_database(
                runtime.database_file.as_path(),
                runtime.lucene_data_directory.as_path(),
                crate::search::SearchEntityType::ReadList,
                readlist_id,
            )
            .map_err(TaskExecutionError::runtime)?;
        }
    }

    Ok(outcome.series_id)
}

pub(super) fn refresh_series_metadata(
    runtime: &RuntimeConfig,
    series_id: &str,
) -> Result<(), TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    if !runtime.owns_sidecar_output {
        return Ok(());
    }

    crate::metadata::refresh_series_metadata(runtime.database_file.as_path(), series_id)
        .map_err(TaskExecutionError::runtime)?;

    if runtime.owns_search_index {
        crate::search::sync_series_and_oneshot_books_after_metadata_update(
            runtime.database_file.as_path(),
            runtime.lucene_data_directory.as_path(),
            series_id,
        )
        .map_err(TaskExecutionError::runtime)?;
    }

    Ok(())
}

pub(super) fn aggregate_series_metadata(
    runtime: &RuntimeConfig,
    series_id: &str,
) -> Result<(), TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    if !runtime.owns_main_database {
        return Ok(());
    }

    crate::metadata::aggregate_series_metadata(runtime.database_file.as_path(), series_id)
        .map_err(TaskExecutionError::runtime)?;

    if runtime.owns_search_index {
        crate::search::sync_entity_upsert_from_database(
            runtime.database_file.as_path(),
            runtime.lucene_data_directory.as_path(),
            crate::search::SearchEntityType::Series,
            series_id,
        )
        .map_err(TaskExecutionError::runtime)?;
    }

    Ok(())
}

pub(super) fn refresh_book_local_artwork(
    runtime: &RuntimeConfig,
    book_id: &str,
) -> Result<(), TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    if !runtime.owns_sidecar_output {
        return Ok(());
    }

    crate::metadata::refresh_book_local_artwork(runtime.database_file.as_path(), book_id)
        .map_err(TaskExecutionError::runtime)
}

pub(super) fn generate_book_thumbnail(
    runtime: &RuntimeConfig,
    book_id: &str,
) -> Result<(), TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    if !runtime.owns_main_database {
        return Ok(());
    }

    crate::metadata::generate_book_thumbnail(runtime.database_file.as_path(), book_id)
        .map_err(TaskExecutionError::runtime)
}

pub(super) fn refresh_series_local_artwork(
    runtime: &RuntimeConfig,
    series_id: &str,
) -> Result<(), TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    if !runtime.owns_sidecar_output {
        return Ok(());
    }

    crate::metadata::refresh_series_local_artwork(runtime.database_file.as_path(), series_id)
        .map_err(TaskExecutionError::runtime)
}

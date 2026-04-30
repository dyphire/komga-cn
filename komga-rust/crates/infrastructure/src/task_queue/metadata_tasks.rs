use super::*;
use std::collections::BTreeSet;

use super::TaskRuntimeContext;
use crate::search::index_lifecycle::SearchEntityType;
use crate::search::runtime_tasks::{
    sync_entity_upsert_from_database, sync_series_and_oneshot_books_after_metadata_update,
};

pub(super) async fn refresh_book_metadata(
    runtime: &TaskRuntimeContext,
    book_id: &str,
    capabilities: &BTreeSet<String>,
) -> Result<Option<String>, TaskExecutionError> {
    if !runtime.owns_sidecar_output {
        return Ok(None);
    }

    let outcome =
        crate::metadata::refresh_book_metadata(&runtime.task_write_pool, book_id, capabilities)
            .await
            .map_err(TaskExecutionError::runtime)?;

    if runtime.owns_search_index {
        sync_entity_upsert_from_database(
            &runtime.task_write_pool,
            runtime.main_db.database_file(),
            runtime.lucene_data_directory.as_path(),
            SearchEntityType::Book,
            book_id,
        )
        .await
        .map_err(TaskExecutionError::runtime)?;
        for readlist_id in &outcome.changed_readlist_ids {
            sync_entity_upsert_from_database(
                &runtime.task_write_pool,
                runtime.main_db.database_file(),
                runtime.lucene_data_directory.as_path(),
                SearchEntityType::ReadList,
                readlist_id,
            )
            .await
            .map_err(TaskExecutionError::runtime)?;
        }
    }

    Ok(outcome.series_id)
}

pub(super) async fn refresh_series_metadata(
    runtime: &TaskRuntimeContext,
    series_id: &str,
) -> Result<(), TaskExecutionError> {
    if !runtime.owns_sidecar_output {
        return Ok(());
    }

    crate::metadata::refresh_series_metadata(&runtime.task_write_pool, series_id)
        .await
        .map_err(TaskExecutionError::runtime)?;

    if runtime.owns_search_index {
        sync_series_and_oneshot_books_after_metadata_update(
            &runtime.task_write_pool,
            runtime.main_db.database_file(),
            runtime.lucene_data_directory.as_path(),
            series_id,
        )
        .await
        .map_err(TaskExecutionError::runtime)?;
    }

    Ok(())
}

pub(super) async fn aggregate_series_metadata(
    runtime: &TaskRuntimeContext,
    series_id: &str,
) -> Result<(), TaskExecutionError> {
    if !runtime.owns_main_database {
        return Ok(());
    }

    crate::metadata::aggregate_series_metadata(&runtime.task_write_pool, series_id)
        .await
        .map_err(TaskExecutionError::runtime)?;

    if runtime.owns_search_index {
        sync_entity_upsert_from_database(
            &runtime.task_write_pool,
            runtime.main_db.database_file(),
            runtime.lucene_data_directory.as_path(),
            SearchEntityType::Series,
            series_id,
        )
        .await
        .map_err(TaskExecutionError::runtime)?;
    }

    Ok(())
}

pub(super) async fn refresh_book_local_artwork(
    runtime: &TaskRuntimeContext,
    book_id: &str,
) -> Result<(), TaskExecutionError> {
    if !runtime.owns_sidecar_output {
        return Ok(());
    }

    crate::metadata::refresh_book_local_artwork(&runtime.task_write_pool, book_id)
        .await
        .map_err(TaskExecutionError::runtime)
}

pub(super) async fn generate_book_thumbnail(
    runtime: &TaskRuntimeContext,
    book_id: &str,
) -> Result<(), TaskExecutionError> {
    if !runtime.owns_main_database {
        return Ok(());
    }

    crate::metadata::generate_book_thumbnail(&runtime.task_write_pool, book_id)
        .await
        .map_err(TaskExecutionError::runtime)
}

pub(super) async fn refresh_series_local_artwork(
    runtime: &TaskRuntimeContext,
    series_id: &str,
) -> Result<(), TaskExecutionError> {
    if !runtime.owns_sidecar_output {
        return Ok(());
    }

    crate::metadata::refresh_series_local_artwork(&runtime.task_write_pool, series_id)
        .await
        .map_err(TaskExecutionError::runtime)
}

use super::*;
use std::collections::BTreeSet;

use crate::search::index_lifecycle::SearchEntityType;
use crate::search::runtime_tasks::{
    sync_entity_upsert_from_database, sync_series_and_oneshot_books_after_metadata_update,
};

pub(super) async fn refresh_book_metadata(
    runtime: &JobRuntime<'_>,
    book_id: &str,
    capabilities: &BTreeSet<String>,
) -> Result<Option<String>, TaskExecutionError> {
    if !runtime.filesystem().owns_sidecar_output() {
        return Ok(None);
    }

    let outcome = crate::metadata::refresh_book_metadata(
        runtime.database().write_pool(),
        book_id,
        capabilities,
    )
    .await
    .map_err(TaskExecutionError::runtime)?;

    if runtime.search().owns_search_index() {
        sync_entity_upsert_from_database(
            runtime.database().read_pool(),
            runtime.database().main_db().database_file(),
            runtime.search().lucene_data_directory(),
            SearchEntityType::Book,
            book_id,
        )
        .await
        .map_err(TaskExecutionError::runtime)?;
        for readlist_id in &outcome.changed_readlist_ids {
            sync_entity_upsert_from_database(
                runtime.database().read_pool(),
                runtime.database().main_db().database_file(),
                runtime.search().lucene_data_directory(),
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
    runtime: &JobRuntime<'_>,
    series_id: &str,
) -> Result<(), TaskExecutionError> {
    if !runtime.filesystem().owns_sidecar_output() {
        return Ok(());
    }

    crate::metadata::refresh_series_metadata(runtime.database().write_pool(), series_id)
        .await
        .map_err(TaskExecutionError::runtime)?;

    if runtime.search().owns_search_index() {
        sync_series_and_oneshot_books_after_metadata_update(
            runtime.database().read_pool(),
            runtime.database().main_db().database_file(),
            runtime.search().lucene_data_directory(),
            series_id,
        )
        .await
        .map_err(TaskExecutionError::runtime)?;
    }

    Ok(())
}

pub(super) async fn aggregate_series_metadata(
    runtime: &JobRuntime<'_>,
    series_id: &str,
) -> Result<(), TaskExecutionError> {
    if !runtime.database().owns_main_database() {
        return Ok(());
    }

    crate::metadata::aggregate_series_metadata(runtime.database().write_pool(), series_id)
        .await
        .map_err(TaskExecutionError::runtime)?;

    if runtime.search().owns_search_index() {
        sync_entity_upsert_from_database(
            runtime.database().read_pool(),
            runtime.database().main_db().database_file(),
            runtime.search().lucene_data_directory(),
            SearchEntityType::Series,
            series_id,
        )
        .await
        .map_err(TaskExecutionError::runtime)?;
    }

    Ok(())
}

pub(super) async fn refresh_book_local_artwork(
    runtime: &JobRuntime<'_>,
    book_id: &str,
) -> Result<(), TaskExecutionError> {
    if !runtime.filesystem().owns_sidecar_output() {
        return Ok(());
    }

    crate::metadata::refresh_book_local_artwork(runtime.database().write_pool(), book_id)
        .await
        .map_err(TaskExecutionError::runtime)
}

pub(super) async fn generate_book_thumbnail(
    runtime: &JobRuntime<'_>,
    book_id: &str,
) -> Result<(), TaskExecutionError> {
    if !runtime.database().owns_main_database() {
        return Ok(());
    }

    crate::metadata::generate_book_thumbnail(runtime.database().write_pool(), book_id)
        .await
        .map_err(TaskExecutionError::runtime)
}

pub(super) async fn refresh_series_local_artwork(
    runtime: &JobRuntime<'_>,
    series_id: &str,
) -> Result<(), TaskExecutionError> {
    if !runtime.filesystem().owns_sidecar_output() {
        return Ok(());
    }

    crate::metadata::refresh_series_local_artwork(runtime.database().write_pool(), series_id)
        .await
        .map_err(TaskExecutionError::runtime)
}

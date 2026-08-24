use std::collections::BTreeSet;

use komga_application::task_processing::TaskProcessingError;

use super::runtime_context::JobRuntime;

pub(super) async fn refresh_book_metadata(
    runtime: &JobRuntime<'_>,
    book_id: &str,
    capabilities: &BTreeSet<String>,
) -> Result<Option<String>, TaskProcessingError> {
    if !runtime.filesystem().owns_sidecar_output() {
        return Ok(None);
    }

    let outcome = crate::metadata::refresh_book_metadata(
        runtime.database().write_pool(),
        runtime.runtime_events(),
        book_id,
        capabilities,
    )
    .await
    .map_err(TaskProcessingError::runtime)?;

    let search = runtime.search_engine();
    search
        .upsert_book(book_id)
        .await
        .map_err(TaskProcessingError::runtime)?;
    for readlist_id in &outcome.changed_readlist_ids {
        search
            .upsert_readlist(readlist_id)
            .await
            .map_err(TaskProcessingError::runtime)?;
    }

    Ok(outcome.series_id)
}

pub(super) async fn refresh_series_metadata(
    runtime: &JobRuntime<'_>,
    series_id: &str,
) -> Result<(), TaskProcessingError> {
    if !runtime.filesystem().owns_sidecar_output() {
        return Ok(());
    }

    crate::metadata::refresh_series_metadata(
        runtime.database().write_pool(),
        runtime.runtime_events(),
        series_id,
    )
    .await
    .map_err(TaskProcessingError::runtime)?;

    runtime
        .search_engine()
        .refresh_series_after_metadata_update(series_id)
        .await
        .map_err(TaskProcessingError::runtime)?;

    Ok(())
}

pub(super) async fn aggregate_series_metadata(
    runtime: &JobRuntime<'_>,
    series_id: &str,
) -> Result<(), TaskProcessingError> {
    if !runtime.database().owns_main_database() {
        return Ok(());
    }

    crate::metadata::aggregate_series_metadata(
        runtime.database().write_pool(),
        runtime.runtime_events(),
        series_id,
    )
    .await
    .map_err(TaskProcessingError::runtime)?;

    runtime
        .search_engine()
        .upsert_series(series_id)
        .await
        .map_err(TaskProcessingError::runtime)?;

    Ok(())
}

pub(super) async fn refresh_book_local_artwork(
    runtime: &JobRuntime<'_>,
    book_id: &str,
) -> Result<(), TaskProcessingError> {
    if !runtime.filesystem().owns_sidecar_output() {
        return Ok(());
    }

    crate::metadata::refresh_book_local_artwork(
        runtime.database().write_pool(),
        runtime.runtime_events(),
        book_id,
    )
    .await
    .map_err(TaskProcessingError::runtime)
}

pub(super) async fn generate_book_thumbnail(
    runtime: &JobRuntime<'_>,
    book_id: &str,
) -> Result<(), TaskProcessingError> {
    if !runtime.database().owns_main_database() {
        return Ok(());
    }

    crate::metadata::generate_book_thumbnail(
        runtime.database().write_pool(),
        runtime.runtime_events(),
        book_id,
        runtime
            .thumbnail_regeneration_policy()
            .await
            .map_err(TaskProcessingError::runtime)?,
    )
    .await
    .map_err(TaskProcessingError::runtime)
}

pub(super) async fn refresh_series_local_artwork(
    runtime: &JobRuntime<'_>,
    series_id: &str,
) -> Result<(), TaskProcessingError> {
    if !runtime.filesystem().owns_sidecar_output() {
        return Ok(());
    }

    crate::metadata::refresh_series_local_artwork(
        runtime.database().write_pool(),
        runtime.runtime_events(),
        series_id,
    )
    .await
    .map_err(TaskProcessingError::runtime)
}

use std::collections::BTreeSet;

use komga_application::task_processing::{
    SeriesPayload, TaskExecutionOutcome, TaskKind, TaskProcessingError, TaskRequest,
};

use crate::tasks::JobRuntime;

pub(in crate::tasks) async fn execute_refresh_book_metadata(
    runtime: &JobRuntime<'_>,
    book_id: &str,
    capabilities: &BTreeSet<String>,
    priority: i32,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    let series_id = refresh_book_metadata(runtime, book_id, capabilities).await?;
    let follow_up_tasks = series_id
        .into_iter()
        .map(|series_id| {
            TaskRequest::with_payload(
                TaskKind::RefreshSeriesMetadata,
                SeriesPayload::new(series_id.clone()),
            )
            .priority(priority - 1)
            .group(series_id)
            .into_queue_record()
        })
        .collect();
    Ok(TaskExecutionOutcome::with_follow_up_tasks(follow_up_tasks))
}

pub(in crate::tasks) async fn execute_refresh_series_metadata(
    runtime: &JobRuntime<'_>,
    series_id: &str,
    priority: i32,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    refresh_series_metadata(runtime, series_id).await?;
    Ok(TaskExecutionOutcome::with_follow_up_tasks(vec![
        TaskRequest::with_payload(
            TaskKind::AggregateSeriesMetadata,
            SeriesPayload::new(series_id.to_string()),
        )
        .priority(priority)
        .group(series_id.to_string())
        .into_queue_record(),
    ]))
}

pub(in crate::tasks) async fn execute_aggregate_series_metadata(
    runtime: &JobRuntime<'_>,
    series_id: &str,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    aggregate_series_metadata(runtime, series_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
}

pub(in crate::tasks) async fn execute_refresh_book_local_artwork(
    runtime: &JobRuntime<'_>,
    book_id: &str,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    refresh_book_local_artwork(runtime, book_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
}

pub(in crate::tasks) async fn execute_generate_book_thumbnail(
    runtime: &JobRuntime<'_>,
    book_id: &str,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    generate_book_thumbnail(runtime, book_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
}

pub(in crate::tasks) async fn execute_refresh_series_local_artwork(
    runtime: &JobRuntime<'_>,
    series_id: &str,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    refresh_series_local_artwork(runtime, series_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
}

async fn refresh_book_metadata(
    runtime: &JobRuntime<'_>,
    book_id: &str,
    capabilities: &BTreeSet<String>,
) -> Result<Option<String>, TaskProcessingError> {
    if !runtime.filesystem().owns_sidecar_output() {
        return Ok(None);
    }

    let outcome = crate::media::metadata::refresh_book_metadata(
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

async fn refresh_series_metadata(
    runtime: &JobRuntime<'_>,
    series_id: &str,
) -> Result<(), TaskProcessingError> {
    if !runtime.filesystem().owns_sidecar_output() {
        return Ok(());
    }

    crate::media::metadata::refresh_series_metadata(
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
        .map_err(TaskProcessingError::runtime)
}

async fn aggregate_series_metadata(
    runtime: &JobRuntime<'_>,
    series_id: &str,
) -> Result<(), TaskProcessingError> {
    if !runtime.database().owns_main_database() {
        return Ok(());
    }

    crate::media::metadata::aggregate_series_metadata(
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

async fn refresh_book_local_artwork(
    runtime: &JobRuntime<'_>,
    book_id: &str,
) -> Result<(), TaskProcessingError> {
    if !runtime.filesystem().owns_sidecar_output() {
        return Ok(());
    }

    crate::media::metadata::refresh_book_local_artwork(
        runtime.database().write_pool(),
        runtime.runtime_events(),
        book_id,
    )
    .await
    .map_err(TaskProcessingError::runtime)
}

async fn generate_book_thumbnail(
    runtime: &JobRuntime<'_>,
    book_id: &str,
) -> Result<(), TaskProcessingError> {
    if !runtime.database().owns_main_database() {
        return Ok(());
    }

    crate::media::metadata::generate_book_thumbnail(
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

async fn refresh_series_local_artwork(
    runtime: &JobRuntime<'_>,
    series_id: &str,
) -> Result<(), TaskProcessingError> {
    if !runtime.filesystem().owns_sidecar_output() {
        return Ok(());
    }

    crate::media::metadata::refresh_series_local_artwork(
        runtime.database().write_pool(),
        runtime.runtime_events(),
        series_id,
    )
    .await
    .map_err(TaskProcessingError::runtime)
}

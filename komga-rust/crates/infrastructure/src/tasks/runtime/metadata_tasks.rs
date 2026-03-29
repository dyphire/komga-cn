use super::*;

pub(super) fn refresh_book_metadata(
    runtime: &RuntimeConfig,
    book_id: &str,
) -> Result<Option<String>, TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    crate::metadata::refresh_book_metadata(runtime.database_file.as_path(), book_id)
        .map_err(TaskExecutionError::runtime)
}

pub(super) fn refresh_series_metadata(
    runtime: &RuntimeConfig,
    series_id: &str,
) -> Result<(), TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    crate::metadata::refresh_series_metadata(runtime.database_file.as_path(), series_id)
        .map_err(TaskExecutionError::runtime)
}

pub(super) fn aggregate_series_metadata(
    runtime: &RuntimeConfig,
    series_id: &str,
) -> Result<(), TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    crate::metadata::aggregate_series_metadata(runtime.database_file.as_path(), series_id)
        .map_err(TaskExecutionError::runtime)
}

pub(super) fn refresh_book_local_artwork(
    runtime: &RuntimeConfig,
    book_id: &str,
) -> Result<(), TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    crate::metadata::refresh_book_local_artwork(runtime.database_file.as_path(), book_id)
        .map_err(TaskExecutionError::runtime)
}

pub(super) fn refresh_series_local_artwork(
    runtime: &RuntimeConfig,
    series_id: &str,
) -> Result<(), TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    crate::metadata::refresh_series_local_artwork(runtime.database_file.as_path(), series_id)
        .map_err(TaskExecutionError::runtime)
}

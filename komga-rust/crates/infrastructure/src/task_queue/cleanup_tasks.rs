use super::*;
use crate::tasks::cleanup_workflow::{cleanup_empty_sets_rows, empty_trash_rows};

pub(super) async fn empty_trash(
    runtime: &RuntimeConfig,
    library_id: &str,
) -> Result<(), TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    if !runtime.owns_main_database {
        return Ok(());
    }

    empty_trash_rows(runtime.database_file.as_path(), library_id)
        .await
        .map_err(TaskExecutionError::runtime)
}

pub(super) async fn cleanup_empty_sets(runtime: &RuntimeConfig) -> Result<(), TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    if !runtime.owns_main_database {
        return Ok(());
    }

    cleanup_empty_sets_rows(runtime.database_file.as_path())
        .await
        .map_err(TaskExecutionError::runtime)
}

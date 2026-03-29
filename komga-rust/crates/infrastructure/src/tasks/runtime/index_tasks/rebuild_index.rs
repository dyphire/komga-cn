use super::*;

pub(in crate::task_queue) fn rebuild_index(
    runtime: &RuntimeConfig,
) -> Result<(), TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    rebuild_index_from_database(
        runtime.database_file.as_path(),
        runtime.lucene_data_directory.as_path(),
    )
    .map_err(TaskExecutionError::runtime)
}

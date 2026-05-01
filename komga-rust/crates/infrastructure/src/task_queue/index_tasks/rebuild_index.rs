use super::*;
use crate::search::index_lifecycle::SearchEntityType;

pub(in crate::task_queue) async fn rebuild_index(
    runtime: &RuntimeConfig,
    entity_types: Option<&[SearchEntityType]>,
) -> Result<(), TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    if !runtime.owns_search_index {
        return Ok(());
    }

    rebuild_index_from_database_for_entities(
        &runtime.task_read_pool,
        runtime.main_db.database_file(),
        runtime.lucene_data_directory.as_path(),
        entity_types,
    )
    .await
    .map_err(TaskExecutionError::runtime)
}

use super::*;
use crate::search::index_lifecycle::SearchEntityType;

pub(in crate::task_queue) async fn rebuild_index(
    runtime: &JobRuntime<'_>,
    entity_types: Option<&[SearchEntityType]>,
) -> Result<(), TaskProcessingError> {
    if !runtime.search().owns_search_index() {
        return Ok(());
    }

    rebuild_index_from_database_for_entities(
        runtime.database().read_pool(),
        runtime.database().main_db().database_file(),
        runtime.search().lucene_data_directory(),
        entity_types,
    )
    .await
    .map_err(TaskProcessingError::runtime)
}

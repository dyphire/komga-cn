use std::path::Path;

use super::db;
use super::loaders;
use crate::search::index_lifecycle::{SearchEntityType, SearchIndexLifecycle};

pub(super) fn rebuild_index_from_database(
    database_file: &Path,
    index_dir: &Path,
) -> Result<(), String> {
    rebuild_index_from_database_for_entities(database_file, index_dir, None)
}

pub(super) fn rebuild_index_from_database_for_entities(
    database_file: &Path,
    index_dir: &Path,
    entity_types: Option<&[SearchEntityType]>,
) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let index_dir = index_dir.to_path_buf();
    let entity_types = entity_types.map(|entities| entities.to_vec());
    db::run_task_database_query_with_max_connections(database_file, 2, move |pool| {
        let index_dir = index_dir.clone();
        let entity_types = entity_types.clone();
        Box::pin(async move {
            let docs = if let Some(entity_types) = &entity_types {
                loaders::load_rebuild_search_documents_for_entities(pool, entity_types).await?
            } else {
                loaders::load_rebuild_search_documents(pool).await?
            };

            let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
                .map_err(|error| format!("failed to bootstrap search index: {error}"))?;
            if let Some(entity_types) = &entity_types {
                index
                    .rebuild_entities(entity_types, &docs)
                    .map_err(|error| format!("failed to rebuild scoped search index: {error}"))
            } else {
                index
                    .rebuild(&docs)
                    .map_err(|error| format!("failed to rebuild search index: {error}"))
            }
        })
    })
}

use std::path::{Path, PathBuf};

use super::loaders;
use crate::search::index_lifecycle::{SearchDocument, SearchEntityType, SearchIndexLifecycle};
use crate::sqlite::connect_private_task_pool;

pub(super) async fn rebuild_index_from_database(
    database_file: &Path,
    index_dir: &Path,
) -> Result<(), String> {
    rebuild_index_from_database_for_entities(database_file, index_dir, None).await
}

pub(super) async fn rebuild_index_from_database_for_entities(
    database_file: &Path,
    index_dir: &Path,
    entity_types: Option<&[SearchEntityType]>,
) -> Result<(), String> {
    let pool = connect_private_task_pool(database_file, 2)
        .await
        .map_err(|error| format!("failed to open private sqlite task pool: {error}"))?;
    let index_dir = index_dir.to_path_buf();
    let entity_types = entity_types.map(|entities| entities.to_vec());
    let docs = if let Some(entity_types) = &entity_types {
        loaders::load_rebuild_search_documents_for_entities(pool.clone(), entity_types).await?
    } else {
        loaders::load_rebuild_search_documents(pool.clone()).await?
    };
    pool.close().await;

    tokio::task::spawn_blocking(move || rebuild_index_with_documents(index_dir, entity_types, docs))
        .await
        .map_err(|error| format!("search index rebuild join failed: {error}"))?
}

fn rebuild_index_with_documents(
    index_dir: PathBuf,
    entity_types: Option<Vec<SearchEntityType>>,
    docs: Vec<SearchDocument>,
) -> Result<(), String> {
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
}

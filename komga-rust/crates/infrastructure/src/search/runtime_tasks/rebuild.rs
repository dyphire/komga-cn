use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use super::super::documents;
use crate::search::index_lifecycle::{SearchDocument, SearchEntityType, SearchIndexLifecycle};

pub(super) async fn rebuild_index_from_database(
    pool: &SqlitePool,
    index_dir: &Path,
) -> Result<(), String> {
    rebuild_index_from_database_for_entities(pool, index_dir, None).await
}

pub(super) async fn rebuild_index_from_database_for_entities(
    pool: &SqlitePool,
    index_dir: &Path,
    entity_types: Option<&[SearchEntityType]>,
) -> Result<(), String> {
    let index_dir = index_dir.to_path_buf();
    let entity_types = entity_types.map(|entities| entities.to_vec());
    let docs = if let Some(entity_types) = &entity_types {
        documents::load_rebuild_search_documents_for_entities(pool.clone(), entity_types).await?
    } else {
        documents::load_rebuild_search_documents(pool.clone()).await?
    };

    rebuild_index_with_documents(index_dir, entity_types, docs)
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
            .map_err(|error| format!("failed to rebuild scoped search index: {error}"))?;
    } else {
        index
            .rebuild(&docs)
            .map_err(|error| format!("failed to rebuild search index: {error}"))?;
    }
    index
        .shutdown()
        .map_err(|error| format!("failed to finalize rebuilt search writer: {error}"))
}

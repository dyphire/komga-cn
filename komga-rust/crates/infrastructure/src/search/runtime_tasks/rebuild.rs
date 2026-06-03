use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use super::super::documents;
use crate::search::index_lifecycle::{SearchDocument, SearchIndexLifecycle};

pub(super) async fn rebuild_index_from_database(
    pool: &SqlitePool,
    index_dir: &Path,
) -> Result<(), String> {
    let index_dir = index_dir.to_path_buf();
    let docs = documents::load_rebuild_search_documents(pool.clone()).await?;

    rebuild_index_with_documents(index_dir, docs)
}

fn rebuild_index_with_documents(
    index_dir: PathBuf,
    docs: Vec<SearchDocument>,
) -> Result<(), String> {
    let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
        .map_err(|error| format!("failed to bootstrap search index: {error}"))?;
    index
        .rebuild(&docs)
        .map_err(|error| format!("failed to rebuild search index: {error}"))?;
    index
        .shutdown()
        .map_err(|error| format!("failed to finalize rebuilt search writer: {error}"))
}

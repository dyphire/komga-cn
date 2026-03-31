use std::path::Path;

use super::super::SearchIndexLifecycle;
use super::db;
use super::loaders;

pub(super) fn rebuild_index_from_database(
    database_file: &Path,
    index_dir: &Path,
) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let index_dir = index_dir.to_path_buf();
    db::run_database_query_with_max_connections(database_file, 2, move |pool| {
        let index_dir = index_dir.clone();
        Box::pin(async move {
            let docs = loaders::load_rebuild_search_documents(pool).await?;

            let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
                .map_err(|error| format!("failed to bootstrap search index: {error}"))?;
            index
                .rebuild(&docs)
                .map_err(|error| format!("failed to rebuild search index: {error}"))
        })
    })
}

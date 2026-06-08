use std::future::Future;
use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use super::documents;
use super::index_lifecycle::{
    SearchEntityType, SearchError, SearchEvent, SearchIndexLifecycle, prepare_for_rebuild,
};
use super::runtime_tasks;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug)]
pub struct SearchIndexSync {
    pool: SqlitePool,
    index_dir: PathBuf,
    owns_search_index: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SearchEventAttempt {
    Applied,
    RebuildRequired,
}

struct SearchIndexMutationRunner<'a> {
    pool: &'a SqlitePool,
    index_dir: &'a Path,
}

impl<'a> SearchIndexMutationRunner<'a> {
    fn new(pool: &'a SqlitePool, index_dir: &'a Path) -> Self {
        Self { pool, index_dir }
    }

    async fn run<F, Fut>(&self, attempt: F) -> Result<(), String>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<SearchEventAttempt, String>>,
    {
        match attempt().await? {
            SearchEventAttempt::Applied => Ok(()),
            SearchEventAttempt::RebuildRequired => {
                recover_search_index(self.pool, self.index_dir).await?;

                match attempt().await? {
                    SearchEventAttempt::Applied => Ok(()),
                    SearchEventAttempt::RebuildRequired => Err(format!(
                        "failed to bootstrap search index after rebuild: corruption persisted at '{}'",
                        self.index_dir.display()
                    )),
                }
            }
        }
    }
}

impl SearchIndexSync {
    pub fn new(pool: SqlitePool, index_dir: PathBuf, owns_search_index: bool) -> Self {
        Self {
            pool,
            index_dir,
            owns_search_index,
        }
    }

    pub async fn upsert_book(&self, book_id: &str) -> Result<bool, String> {
        self.upsert_entity(SearchEntityType::Book, book_id).await
    }

    pub async fn upsert_series(&self, series_id: &str) -> Result<bool, String> {
        self.upsert_entity(SearchEntityType::Series, series_id)
            .await
    }

    pub async fn upsert_collection(&self, collection_id: &str) -> Result<bool, String> {
        self.upsert_entity(SearchEntityType::Collection, collection_id)
            .await
    }

    pub async fn upsert_readlist(&self, readlist_id: &str) -> Result<bool, String> {
        self.upsert_entity(SearchEntityType::ReadList, readlist_id)
            .await
    }

    pub async fn delete_book(&self, book_id: &str) -> Result<(), String> {
        self.delete_entity(SearchEntityType::Book, book_id).await
    }

    pub async fn delete_series(&self, series_id: &str) -> Result<(), String> {
        self.delete_entity(SearchEntityType::Series, series_id)
            .await
    }

    pub async fn delete_collection(&self, collection_id: &str) -> Result<(), String> {
        self.delete_entity(SearchEntityType::Collection, collection_id)
            .await
    }

    pub async fn delete_readlist(&self, readlist_id: &str) -> Result<(), String> {
        self.delete_entity(SearchEntityType::ReadList, readlist_id)
            .await
    }

    pub async fn refresh_series_after_metadata_update(
        &self,
        series_id: &str,
    ) -> Result<(), String> {
        if !self.owns_search_index {
            return Ok(());
        }

        sync_series_and_oneshot_books_after_metadata_update(
            &self.pool,
            self.index_dir.as_path(),
            series_id,
        )
        .await
    }

    pub async fn rebuild_all(&self) -> Result<(), String> {
        if !self.owns_search_index {
            return Ok(());
        }

        recover_search_index(&self.pool, self.index_dir.as_path()).await
    }

    pub async fn rebuild_entities(&self, entity_types: &[SearchEntityType]) -> Result<(), String> {
        if !self.owns_search_index || entity_types.is_empty() {
            return Ok(());
        }

        rebuild_search_index_for_entities(&self.pool, self.index_dir.as_path(), entity_types).await
    }

    async fn upsert_entity(
        &self,
        entity_type: SearchEntityType,
        entity_id: &str,
    ) -> Result<bool, String> {
        if !self.owns_search_index {
            return Ok(false);
        }

        sync_entity_upsert_from_database(
            &self.pool,
            self.index_dir.as_path(),
            entity_type,
            entity_id,
        )
        .await
    }

    async fn delete_entity(
        &self,
        entity_type: SearchEntityType,
        entity_id: &str,
    ) -> Result<(), String> {
        if !self.owns_search_index {
            return Ok(());
        }

        sync_entity_delete_from_index(&self.pool, self.index_dir.as_path(), entity_type, entity_id)
            .await
    }
}

async fn recover_search_index(pool: &SqlitePool, index_dir: &Path) -> Result<(), String> {
    prepare_for_rebuild(index_dir)
        .map_err(|error| format!("failed to prepare search index rebuild: {error}"))?;

    runtime_tasks::rebuild_index_from_database(pool, index_dir).await
}

async fn try_rebuild_search_index_for_entities(
    pool: &SqlitePool,
    index_dir: &Path,
    entity_types: &[SearchEntityType],
) -> Result<SearchEventAttempt, String> {
    let docs =
        documents::load_rebuild_search_documents_for_entities(pool.clone(), entity_types).await?;
    match SearchIndexLifecycle::bootstrap(index_dir) {
        Ok(index) => {
            index
                .rebuild_entities(entity_types, &docs)
                .map_err(|error| format!("failed to rebuild scoped search index: {error}"))?;
            index
                .shutdown()
                .map_err(|error| format!("failed to finalize rebuilt search writer: {error}"))?;
            Ok(SearchEventAttempt::Applied)
        }
        Err(SearchError::CorruptedIndexRequiresExplicitRebuild(_, _)) => {
            Ok(SearchEventAttempt::RebuildRequired)
        }
        Err(error) => Err(format!("failed to bootstrap search index: {error}")),
    }
}

async fn rebuild_search_index_for_entities(
    pool: &SqlitePool,
    index_dir: &Path,
    entity_types: &[SearchEntityType],
) -> Result<(), String> {
    SearchIndexMutationRunner::new(pool, index_dir)
        .run(|| try_rebuild_search_index_for_entities(pool, index_dir, entity_types))
        .await
}

async fn try_apply_search_event(
    index_dir: &Path,
    event: SearchEvent,
) -> Result<SearchEventAttempt, String> {
    match SearchIndexLifecycle::bootstrap(index_dir) {
        Ok(index) => {
            index
                .apply_event(event)
                .map_err(|error| format!("failed to apply search event: {error}"))?;
            index
                .shutdown()
                .map_err(|error| format!("failed to finalize search event writer: {error}"))?;
            Ok(SearchEventAttempt::Applied)
        }
        Err(SearchError::CorruptedIndexRequiresExplicitRebuild(_, _)) => {
            Ok(SearchEventAttempt::RebuildRequired)
        }
        Err(error) => Err(format!("failed to bootstrap search index: {error}")),
    }
}

async fn apply_search_event(
    pool: &SqlitePool,
    index_dir: &Path,
    event: SearchEvent,
) -> Result<(), String> {
    SearchIndexMutationRunner::new(pool, index_dir)
        .run(|| try_apply_search_event(index_dir, event.clone()))
        .await
}

async fn sync_entity_upsert_from_database(
    pool: &SqlitePool,
    index_dir: &Path,
    entity_type: SearchEntityType,
    entity_id: &str,
) -> Result<bool, String> {
    let document = match entity_type {
        SearchEntityType::Book => {
            documents::load_book_search_document(pool.clone(), entity_id).await?
        }
        SearchEntityType::Series => {
            documents::load_series_search_document(pool.clone(), entity_id).await?
        }
        SearchEntityType::Collection => {
            documents::load_collection_search_document(pool.clone(), entity_id).await?
        }
        SearchEntityType::ReadList => {
            documents::load_readlist_search_document(pool.clone(), entity_id).await?
        }
    };

    let Some(document) = document else {
        return Ok(false);
    };

    apply_search_event(pool, index_dir, SearchEvent::Upsert(document)).await?;
    Ok(true)
}

async fn sync_series_and_oneshot_books_after_metadata_update(
    pool: &SqlitePool,
    index_dir: &Path,
    series_id: &str,
) -> Result<(), String> {
    let series_document = documents::load_series_search_document(pool.clone(), series_id).await?;
    let oneshot_documents =
        documents::load_oneshot_book_search_documents(pool.clone(), series_id).await?;

    if let Some(document) = series_document {
        apply_search_event(pool, index_dir, SearchEvent::Upsert(document)).await?;
    }

    for document in oneshot_documents {
        apply_search_event(pool, index_dir, SearchEvent::Upsert(document)).await?;
    }

    Ok(())
}

async fn sync_entity_delete_from_index(
    pool: &SqlitePool,
    index_dir: &Path,
    entity_type: SearchEntityType,
    entity_id: &str,
) -> Result<(), String> {
    apply_search_event(
        pool,
        index_dir,
        SearchEvent::Delete {
            entity_type,
            id: entity_id.to_string(),
        },
    )
    .await
}

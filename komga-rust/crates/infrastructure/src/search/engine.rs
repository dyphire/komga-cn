use anyhow::Context;
use std::future::Future;
use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use super::documents;
use super::lifecycle::{
    SearchDocument, SearchEntityType, SearchError, SearchEvent, SearchIndexLifecycle,
    SearchQueryLifecycle, SearchScoredHit, prepare_for_rebuild,
};

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;

#[derive(Clone, Debug)]
pub(crate) struct SearchIndexEngine {
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

impl SearchIndexEngine {
    pub(crate) fn new(pool: SqlitePool, index_dir: PathBuf, owns_search_index: bool) -> Self {
        Self {
            pool,
            index_dir,
            owns_search_index,
        }
    }

    pub(crate) fn read_only(pool: SqlitePool, index_dir: PathBuf) -> Self {
        Self::new(pool, index_dir, false)
    }

    pub(crate) fn index_dir(&self) -> &Path {
        self.index_dir.as_path()
    }

    pub(crate) fn search_ids(
        &self,
        query: &str,
        entity_type: SearchEntityType,
        limit: usize,
    ) -> anyhow::Result<Vec<String>> {
        let index = SearchQueryLifecycle::bootstrap(self.index_dir())
            .context("failed to open search index for query")?;
        index
            .search_ids(query, entity_type, limit)
            .context("failed to execute search query")
    }

    pub(crate) fn search_scored_ids(
        &self,
        query: &str,
        entity_type: SearchEntityType,
        limit: usize,
    ) -> anyhow::Result<Vec<SearchScoredHit>> {
        let index = SearchQueryLifecycle::bootstrap(self.index_dir())
            .context("failed to open search index for query")?;
        index
            .search_scored_ids(query, entity_type, limit)
            .context("failed to execute scored search query")
    }

    pub(crate) async fn upsert_book(&self, book_id: &str) -> anyhow::Result<bool> {
        self.upsert_entity(SearchEntityType::Book, book_id).await
    }

    pub(crate) async fn upsert_series(&self, series_id: &str) -> anyhow::Result<bool> {
        self.upsert_entity(SearchEntityType::Series, series_id)
            .await
    }

    pub(crate) async fn upsert_collection(&self, collection_id: &str) -> anyhow::Result<bool> {
        self.upsert_entity(SearchEntityType::Collection, collection_id)
            .await
    }

    pub(crate) async fn upsert_readlist(&self, readlist_id: &str) -> anyhow::Result<bool> {
        self.upsert_entity(SearchEntityType::ReadList, readlist_id)
            .await
    }

    pub(crate) async fn delete_book(&self, book_id: &str) -> anyhow::Result<()> {
        self.delete_entity(SearchEntityType::Book, book_id).await
    }

    pub(crate) async fn delete_series(&self, series_id: &str) -> anyhow::Result<()> {
        self.delete_entity(SearchEntityType::Series, series_id)
            .await
    }

    pub(crate) async fn delete_collection(&self, collection_id: &str) -> anyhow::Result<()> {
        self.delete_entity(SearchEntityType::Collection, collection_id)
            .await
    }

    pub(crate) async fn delete_readlist(&self, readlist_id: &str) -> anyhow::Result<()> {
        self.delete_entity(SearchEntityType::ReadList, readlist_id)
            .await
    }

    pub(crate) async fn refresh_series_after_metadata_update(
        &self,
        series_id: &str,
    ) -> anyhow::Result<()> {
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

    pub(crate) async fn rebuild_all(&self) -> anyhow::Result<()> {
        if !self.owns_search_index {
            return Ok(());
        }

        recover_search_index(&self.pool, self.index_dir.as_path()).await
    }

    pub(crate) async fn rebuild_entities(
        &self,
        entity_types: &[SearchEntityType],
    ) -> anyhow::Result<()> {
        if !self.owns_search_index || entity_types.is_empty() {
            return Ok(());
        }

        rebuild_search_index_for_entities(&self.pool, self.index_dir.as_path(), entity_types).await
    }

    async fn upsert_entity(
        &self,
        entity_type: SearchEntityType,
        entity_id: &str,
    ) -> anyhow::Result<bool> {
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
    ) -> anyhow::Result<()> {
        if !self.owns_search_index {
            return Ok(());
        }

        sync_entity_delete_from_index(&self.pool, self.index_dir.as_path(), entity_type, entity_id)
            .await
    }
}

impl<'a> SearchIndexMutationRunner<'a> {
    fn new(pool: &'a SqlitePool, index_dir: &'a Path) -> Self {
        Self { pool, index_dir }
    }

    async fn run<F, Fut>(&self, attempt: F) -> anyhow::Result<()>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = anyhow::Result<SearchEventAttempt>>,
    {
        match attempt().await? {
            SearchEventAttempt::Applied => Ok(()),
            SearchEventAttempt::RebuildRequired => {
                recover_search_index(self.pool, self.index_dir).await?;

                match attempt().await? {
                    SearchEventAttempt::Applied => Ok(()),
                    SearchEventAttempt::RebuildRequired => Err(anyhow::anyhow!(format!(
                        "failed to bootstrap search index after rebuild: corruption persisted at '{}'",
                        self.index_dir.display()
                    ))),
                }
            }
        }
    }
}

async fn recover_search_index(pool: &SqlitePool, index_dir: &Path) -> anyhow::Result<()> {
    prepare_for_rebuild(index_dir).context("failed to prepare search index rebuild")?;

    rebuild_index_from_database(pool, index_dir).await
}

pub async fn rebuild_index_from_database(
    pool: &SqlitePool,
    index_dir: &Path,
) -> anyhow::Result<()> {
    let docs = documents::load_rebuild_search_documents(pool.clone()).await?;
    rebuild_index_with_documents(index_dir, &docs)
}

fn rebuild_index_with_documents(index_dir: &Path, docs: &[SearchDocument]) -> anyhow::Result<()> {
    let index =
        SearchIndexLifecycle::bootstrap(index_dir).context("failed to bootstrap search index")?;
    index
        .rebuild(docs)
        .context("failed to rebuild search index")?;
    index
        .shutdown()
        .context("failed to finalize rebuilt search writer")
}

async fn try_rebuild_search_index_for_entities(
    pool: &SqlitePool,
    index_dir: &Path,
    entity_types: &[SearchEntityType],
) -> anyhow::Result<SearchEventAttempt> {
    let docs =
        documents::load_rebuild_search_documents_for_entities(pool.clone(), entity_types).await?;
    match SearchIndexLifecycle::bootstrap(index_dir) {
        Ok(index) => {
            index
                .rebuild_entities(entity_types, &docs)
                .context("failed to rebuild scoped search index")?;
            index
                .shutdown()
                .context("failed to finalize rebuilt search writer")?;
            Ok(SearchEventAttempt::Applied)
        }
        Err(SearchError::CorruptedIndexRequiresExplicitRebuild(_, _)) => {
            Ok(SearchEventAttempt::RebuildRequired)
        }
        Err(error) => Err(anyhow::anyhow!(format!(
            "failed to bootstrap search index: {error}"
        ))),
    }
}

async fn rebuild_search_index_for_entities(
    pool: &SqlitePool,
    index_dir: &Path,
    entity_types: &[SearchEntityType],
) -> anyhow::Result<()> {
    SearchIndexMutationRunner::new(pool, index_dir)
        .run(|| try_rebuild_search_index_for_entities(pool, index_dir, entity_types))
        .await
}

async fn try_apply_search_event(
    index_dir: &Path,
    event: SearchEvent,
) -> anyhow::Result<SearchEventAttempt> {
    match SearchIndexLifecycle::bootstrap(index_dir) {
        Ok(index) => {
            index
                .apply_event(event)
                .context("failed to apply search event")?;
            index
                .shutdown()
                .context("failed to finalize search event writer")?;
            Ok(SearchEventAttempt::Applied)
        }
        Err(SearchError::CorruptedIndexRequiresExplicitRebuild(_, _)) => {
            Ok(SearchEventAttempt::RebuildRequired)
        }
        Err(error) => Err(anyhow::anyhow!(format!(
            "failed to bootstrap search index: {error}"
        ))),
    }
}

async fn apply_search_event(
    pool: &SqlitePool,
    index_dir: &Path,
    event: SearchEvent,
) -> anyhow::Result<()> {
    SearchIndexMutationRunner::new(pool, index_dir)
        .run(|| try_apply_search_event(index_dir, event.clone()))
        .await
}

async fn sync_entity_upsert_from_database(
    pool: &SqlitePool,
    index_dir: &Path,
    entity_type: SearchEntityType,
    entity_id: &str,
) -> anyhow::Result<bool> {
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
) -> anyhow::Result<()> {
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
) -> anyhow::Result<()> {
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

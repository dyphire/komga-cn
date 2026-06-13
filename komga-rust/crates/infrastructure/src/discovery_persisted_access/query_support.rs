use std::path::PathBuf;

use komga_application::discovery::{
    AuthorFacetPort, BookSpecialListPort, CollectionSearchPort, LibraryIdMappingPort,
    PersistedAuthorEntry, PersistedAuthorsScope, PersistedBookBrowseEntry, ReadlistSearchPort,
    ScoredSearchHit,
};

use crate::database_handle::DatabaseHandle;
use crate::search::engine::SearchIndexEngine;
use crate::search::index_lifecycle::SearchEntityType;

use super::{authors, library_mappings, models, runtime_queries};

#[derive(Clone)]
pub struct DiscoveryQuerySupportAccess {
    db: DatabaseHandle,
    search: SearchIndexEngine,
}

impl DiscoveryQuerySupportAccess {
    pub fn new(db: DatabaseHandle, index_dir: PathBuf) -> Self {
        let search = SearchIndexEngine::read_only(db.read_pool().clone(), index_dir);
        Self { db, search }
    }
}

fn persisted_book_browse_entry(row: models::BookBrowseEntry) -> PersistedBookBrowseEntry {
    PersistedBookBrowseEntry {
        id: row.id,
        library_id: row.library_id,
        name: row.name,
        title: row.title,
    }
}

#[async_trait::async_trait]
impl AuthorFacetPort for DiscoveryQuerySupportAccess {
    async fn load_author_names(
        &self,
        search: &str,
        authorized_library_ids: Option<&[String]>,
    ) -> Result<Vec<String>, String> {
        authors::load_persisted_author_names(self.db.read_pool(), search, authorized_library_ids)
            .await
    }

    async fn load_author_roles(
        &self,
        authorized_library_ids: Option<&[String]>,
    ) -> Result<Vec<String>, String> {
        authors::load_persisted_author_roles(self.db.read_pool(), authorized_library_ids).await
    }

    async fn load_authors_by_scope(
        &self,
        scope: PersistedAuthorsScope,
        authorized_library_ids: Option<&[String]>,
    ) -> Result<Vec<PersistedAuthorEntry>, String> {
        let mapped_scope = match scope {
            PersistedAuthorsScope::All => models::AuthorsScope::All,
            PersistedAuthorsScope::Libraries(ids) => models::AuthorsScope::Libraries(ids),
            PersistedAuthorsScope::Collection(id) => models::AuthorsScope::Collection(id),
            PersistedAuthorsScope::Series(id) => models::AuthorsScope::Series(id),
            PersistedAuthorsScope::ReadList(id) => models::AuthorsScope::ReadList(id),
        };
        let rows = authors::load_persisted_authors_by_scope(
            self.db.read_pool(),
            &mapped_scope,
            authorized_library_ids,
        )
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| PersistedAuthorEntry {
                name: row.name,
                role: row.role,
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl LibraryIdMappingPort for DiscoveryQuerySupportAccess {
    async fn load_persisted_library_ids(&self) -> Result<Vec<String>, String> {
        library_mappings::load_persisted_library_ids(self.db.read_pool()).await
    }
}

#[async_trait::async_trait]
impl BookSpecialListPort for DiscoveryQuerySupportAccess {
    async fn load_ondeck_books(
        &self,
        user_id: &str,
    ) -> Result<Vec<PersistedBookBrowseEntry>, String> {
        runtime_queries::load_persisted_ondeck_books(self.db.read_pool(), user_id)
            .await
            .map(|rows| rows.into_iter().map(persisted_book_browse_entry).collect())
    }

    async fn load_duplicate_books(&self) -> Result<Vec<PersistedBookBrowseEntry>, String> {
        runtime_queries::load_persisted_duplicate_books(self.db.read_pool())
            .await
            .map(|rows| rows.into_iter().map(persisted_book_browse_entry).collect())
    }
}

#[async_trait::async_trait]
impl CollectionSearchPort for DiscoveryQuerySupportAccess {
    async fn search_collection_ids(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<String>, String> {
        self.search
            .search_ids(query, SearchEntityType::Collection, limit)
    }
}

#[async_trait::async_trait]
impl ReadlistSearchPort for DiscoveryQuerySupportAccess {
    async fn search_readlist_scored_ids(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ScoredSearchHit>, String> {
        Ok(self
            .search
            .search_scored_ids(query, SearchEntityType::ReadList, limit)?
            .into_iter()
            .map(|hit| ScoredSearchHit {
                score: hit.score,
                id: hit.id,
            })
            .collect())
    }
}

use super::index_dirs::resolve_discovery_index_dir;
use super::*;

use komga_infrastructure::database_handle::DatabaseHandle;
use komga_infrastructure::search::index_lifecycle::SearchQueryLifecycle;
use komga_interfaces::state::DiscoverySearchService;

fn search_ids_or_empty(
    index_dir: &std::path::Path,
    query: &str,
    entity_type: SearchEntityType,
    limit: usize,
) -> Vec<String> {
    let Ok(index) = SearchQueryLifecycle::bootstrap(index_dir) else {
        return Vec::new();
    };

    index
        .search_ids(query, entity_type, limit)
        .unwrap_or_default()
}

fn search_scored_ids_or_empty(
    index_dir: &std::path::Path,
    query: &str,
    entity_type: SearchEntityType,
    limit: usize,
) -> Vec<(f32, String)> {
    let Ok(index) = SearchQueryLifecycle::bootstrap(index_dir) else {
        return Vec::new();
    };

    index
        .search_scored_ids(query, entity_type, limit)
        .unwrap_or_default()
}

fn persisted_book_browse_entry(row: models::BookBrowseEntry) -> PersistedBookBrowseEntry {
    PersistedBookBrowseEntry {
        id: row.id,
        library_id: row.library_id,
        name: row.name,
        title: row.title,
    }
}

#[derive(Clone)]
pub(super) struct RuntimePersistedDiscoveryAccess {
    db: DatabaseHandle,
    index_dir: PathBuf,
}

#[async_trait::async_trait]
impl DiscoverySearchService for RuntimePersistedDiscoveryAccess {
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

    async fn load_persisted_library_ids(&self) -> Result<Vec<String>, String> {
        library_mappings::load_persisted_library_ids(self.db.read_pool()).await
    }

    async fn search_collection_ids(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<String>, String> {
        Ok(search_ids_or_empty(
            resolve_discovery_index_dir(self.db.database_file(), self.index_dir.as_path())
                .as_path(),
            query,
            SearchEntityType::Collection,
            limit,
        ))
    }

    async fn search_readlist_scored_ids(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(f32, String)>, String> {
        Ok(search_scored_ids_or_empty(
            resolve_discovery_index_dir(self.db.database_file(), self.index_dir.as_path())
                .as_path(),
            query,
            SearchEntityType::ReadList,
            limit,
        ))
    }

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

pub(super) fn compose_discovery_search_service(
    db: DatabaseHandle,
    lucene_data_directory: PathBuf,
) -> Box<dyn DiscoverySearchService> {
    Box::new(RuntimePersistedDiscoveryAccess {
        db,
        index_dir: lucene_data_directory,
    })
}

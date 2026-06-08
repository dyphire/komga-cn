use async_trait::async_trait;
use komga_application::library_catalog::{
    CreateLibraryResult, LibraryCatalogCommandService, LibraryCatalogMutationError,
    LibraryCatalogPort, LibraryCatalogQueryService, LibraryChangeSet, LibraryDetailAccess,
    LibraryRecord, LibraryTaskResult,
};
use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext};
use sqlx::SqlitePool;

use super::SqliteLibraryCatalogAdapter;

#[derive(Clone)]
pub struct LibraryCatalogAccess {
    adapter: SqliteLibraryCatalogAdapter,
}

impl LibraryCatalogAccess {
    pub fn new(read_pool: SqlitePool, write_pool: SqlitePool) -> Self {
        Self {
            adapter: SqliteLibraryCatalogAdapter::new(read_pool, write_pool),
        }
    }
}

#[async_trait]
impl LibraryCatalogPort for LibraryCatalogAccess {
    async fn list_libraries(
        &self,
        context: DiscoveryQueryContext,
    ) -> Result<Vec<LibraryRecord>, DiscoveryError> {
        LibraryCatalogQueryService::new(self.adapter.clone())
            .list_libraries(&context)
            .await
    }

    async fn get_library(
        &self,
        context: DiscoveryQueryContext,
        library_id: &str,
    ) -> Result<Option<LibraryRecord>, DiscoveryError> {
        LibraryCatalogQueryService::new(self.adapter.clone())
            .get_library(&context, library_id)
            .await
    }

    async fn library_detail_access(
        &self,
        context: DiscoveryQueryContext,
        library_id: &str,
    ) -> Result<LibraryDetailAccess, DiscoveryError> {
        LibraryCatalogQueryService::new(self.adapter.clone())
            .library_detail_access(&context, library_id)
            .await
    }

    async fn create_library(
        &self,
        changes: LibraryChangeSet,
    ) -> Result<CreateLibraryResult, LibraryCatalogMutationError> {
        LibraryCatalogCommandService::new(self.adapter.clone())
            .create_library(changes)
            .await
    }

    async fn update_library(
        &self,
        library_id: &str,
        changes: LibraryChangeSet,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError> {
        LibraryCatalogCommandService::new(self.adapter.clone())
            .update_library(library_id, changes)
            .await
    }

    async fn delete_library(&self, library_id: &str) -> Result<bool, LibraryCatalogMutationError> {
        LibraryCatalogCommandService::new(self.adapter.clone())
            .delete_library(library_id)
            .await
    }

    async fn scan_library(
        &self,
        library_id: &str,
        deep_scan: bool,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError> {
        LibraryCatalogCommandService::new(self.adapter.clone())
            .scan_library(library_id, deep_scan)
            .await
    }

    async fn analyze_library(
        &self,
        library_id: &str,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError> {
        LibraryCatalogCommandService::new(self.adapter.clone())
            .analyze_library(library_id)
            .await
    }

    async fn refresh_metadata(
        &self,
        library_id: &str,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError> {
        LibraryCatalogCommandService::new(self.adapter.clone())
            .refresh_metadata(library_id)
            .await
    }

    async fn empty_trash(
        &self,
        library_id: &str,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError> {
        LibraryCatalogCommandService::new(self.adapter.clone())
            .empty_trash(library_id)
            .await
    }
}

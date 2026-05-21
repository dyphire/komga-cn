use komga_application::library_catalog::{
    CreateLibraryResult, CreateLibraryService, DeleteLibraryService, LibraryCatalogMutationError,
    LibraryCatalogQueryService, LibraryChangeSet, LibraryRecord, LibraryTaskResult,
    LibraryTaskService, UpdateLibraryService,
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

    pub async fn list_libraries(
        &self,
        context: DiscoveryQueryContext,
    ) -> Result<Vec<LibraryRecord>, DiscoveryError> {
        LibraryCatalogQueryService::new(self.adapter.clone())
            .list_libraries(&context)
            .await
    }

    pub async fn get_library(
        &self,
        context: DiscoveryQueryContext,
        library_id: &str,
    ) -> Result<Option<LibraryRecord>, DiscoveryError> {
        LibraryCatalogQueryService::new(self.adapter.clone())
            .get_library(&context, library_id)
            .await
    }

    pub async fn create_library(
        &self,
        changes: LibraryChangeSet,
    ) -> Result<CreateLibraryResult, LibraryCatalogMutationError> {
        CreateLibraryService::new(self.adapter.clone())
            .create_library(changes)
            .await
    }

    pub async fn update_library(
        &self,
        library_id: &str,
        changes: LibraryChangeSet,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError> {
        UpdateLibraryService::new(self.adapter.clone())
            .update_library(library_id, changes)
            .await
    }

    pub async fn delete_library(
        &self,
        library_id: &str,
    ) -> Result<bool, LibraryCatalogMutationError> {
        DeleteLibraryService::new(self.adapter.clone())
            .delete_library(library_id)
            .await
    }

    pub async fn scan_library(
        &self,
        library_id: &str,
        deep_scan: bool,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError> {
        LibraryTaskService::new(self.adapter.clone())
            .scan_library(library_id, deep_scan)
            .await
    }

    pub async fn analyze_library(
        &self,
        library_id: &str,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError> {
        LibraryTaskService::new(self.adapter.clone())
            .analyze_library(library_id)
            .await
    }

    pub async fn refresh_metadata(
        &self,
        library_id: &str,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError> {
        LibraryTaskService::new(self.adapter.clone())
            .refresh_metadata(library_id)
            .await
    }

    pub async fn empty_trash(
        &self,
        library_id: &str,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError> {
        LibraryTaskService::new(self.adapter.clone())
            .empty_trash(library_id)
            .await
    }
}

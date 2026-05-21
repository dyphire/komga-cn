use async_trait::async_trait;
use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext};

use super::{
    CreateLibraryResult, LibraryCatalogMutationError, LibraryChangeSet, LibraryRecord,
    LibraryTaskResult,
};

/// Port for library catalog operations (CRUD + task triggers).
#[async_trait]
pub trait LibraryCatalogPort: Send + Sync {
    async fn list_libraries(
        &self,
        context: DiscoveryQueryContext,
    ) -> Result<Vec<LibraryRecord>, DiscoveryError>;

    async fn get_library(
        &self,
        context: DiscoveryQueryContext,
        library_id: &str,
    ) -> Result<Option<LibraryRecord>, DiscoveryError>;

    async fn create_library(
        &self,
        changes: LibraryChangeSet,
    ) -> Result<CreateLibraryResult, LibraryCatalogMutationError>;

    async fn update_library(
        &self,
        library_id: &str,
        changes: LibraryChangeSet,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError>;

    async fn delete_library(&self, library_id: &str) -> Result<bool, LibraryCatalogMutationError>;

    async fn scan_library(
        &self,
        library_id: &str,
        deep_scan: bool,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError>;

    async fn analyze_library(
        &self,
        library_id: &str,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError>;

    async fn refresh_metadata(
        &self,
        library_id: &str,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError>;

    async fn empty_trash(
        &self,
        library_id: &str,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError>;
}

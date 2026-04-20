use super::*;

#[async_trait]
pub trait LibraryCatalogService: Send + Sync {
    async fn list_libraries(
        &self,
        context: DiscoveryQueryContext,
    ) -> Result<Vec<LibraryRecord>, DiscoveryError>;

    async fn get_library(
        &self,
        context: DiscoveryQueryContext,
        library_id: String,
    ) -> Result<Option<LibraryRecord>, DiscoveryError>;

    async fn create_library(
        &self,
        changes: LibraryChangeSet,
    ) -> Result<CreateLibraryResult, LibraryCatalogMutationError>;

    async fn update_library(
        &self,
        library_id: String,
        changes: LibraryChangeSet,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError>;

    async fn delete_library(&self, library_id: String)
    -> Result<bool, LibraryCatalogMutationError>;

    async fn scan_library(
        &self,
        library_id: String,
        deep_scan: bool,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError>;

    async fn analyze_library(
        &self,
        library_id: String,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError>;

    async fn refresh_metadata(
        &self,
        library_id: String,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError>;

    async fn empty_trash(
        &self,
        library_id: String,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError>;
}

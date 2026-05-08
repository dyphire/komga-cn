use super::*;
use axum::extract::FromRef;

#[derive(Clone)]
pub struct LibraryCatalogState {
    pub discovery_auth: DiscoveryAuthState,
    pub identity: IdentityState,
    pub library_catalog: Arc<dyn LibraryCatalogService>,
    pub task_queue: TaskQueueState,
}

impl FromRef<Arc<HttpAppState>> for LibraryCatalogState {
    fn from_ref(app: &Arc<HttpAppState>) -> Self {
        Self {
            discovery_auth: app.discovery_auth.clone(),
            identity: IdentityState::from_ref(app),
            library_catalog: app.services.library_catalog.clone(),
            task_queue: TaskQueueState::from_ref(app),
        }
    }
}

#[async_trait]
pub trait LibraryCatalogService: Send + Sync {
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

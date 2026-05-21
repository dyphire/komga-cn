use super::*;
use axum::extract::FromRef;
use komga_application::library_catalog::LibraryCatalogPort;

#[derive(Clone)]
pub struct LibraryCatalogState {
    pub discovery_auth: DiscoveryAuthState,
    pub identity: IdentityState,
    pub library_catalog: Arc<dyn LibraryCatalogPort>,
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

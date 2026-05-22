use super::*;
use axum::extract::FromRef;

use komga_application::operational::{
    AnnouncementPort, ClaimPort, ClientSettingsPort, FilesystemBrowsePort, FontPort, HistoryPort,
    OperationalMetricsPort, PageHashPort, ServerSettingsService, SyncpointPort,
    TransientBookService,
};

#[derive(Clone)]
pub struct OperationalApiState {
    pub(crate) auth_db: AuthDatabaseState,
    pub(crate) operational: OperationalState,
    pub(crate) identity: IdentityState,
    pub(crate) task_queue: TaskQueueState,
    pub(crate) operational_runtime: Arc<dyn OperationalMetricsPort>,
    pub(crate) announcements: Arc<dyn AnnouncementPort>,
    pub(crate) claim: Arc<dyn ClaimPort>,
    pub(crate) client_settings: Arc<dyn ClientSettingsPort>,
    pub(crate) filesystem_browse: Arc<dyn FilesystemBrowsePort>,
    pub(crate) fonts: Arc<dyn FontPort>,
    pub(crate) history: Arc<dyn HistoryPort>,
    pub(crate) page_hashes: Arc<dyn PageHashPort>,
    pub(crate) syncpoints: Arc<dyn SyncpointPort>,
    pub(crate) transient_books: Arc<TransientBookService>,
}

impl FromRef<Arc<HttpAppState>> for OperationalApiState {
    fn from_ref(app: &Arc<HttpAppState>) -> Self {
        Self {
            auth_db: app.auth_db.clone(),
            operational: app.operational.clone(),
            identity: IdentityState::from_ref(app),
            task_queue: TaskQueueState::from_ref(app),
            operational_runtime: app.services.operational_runtime.clone(),
            announcements: app.services.announcements.clone(),
            claim: app.services.claim.clone(),
            client_settings: app.services.client_settings.clone(),
            filesystem_browse: app.services.filesystem_browse.clone(),
            fonts: app.services.fonts.clone(),
            history: app.services.history.clone(),
            page_hashes: app.services.page_hashes.clone(),
            syncpoints: app.services.syncpoints.clone(),
            transient_books: app.services.transient_books.clone(),
        }
    }
}

#[derive(Clone)]
pub struct ServerSettingsState {
    pub runtime: RuntimeState,
    pub(crate) server_settings: Arc<ServerSettingsService>,
}

impl FromRef<Arc<HttpAppState>> for ServerSettingsState {
    fn from_ref(app: &Arc<HttpAppState>) -> Self {
        Self {
            runtime: app.operational.runtime.clone(),
            server_settings: app.services.server_settings_control.clone(),
        }
    }
}

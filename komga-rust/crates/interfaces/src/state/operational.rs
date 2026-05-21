use super::*;
use axum::extract::FromRef;

use komga_infrastructure::operational_metrics_access::OperationalMetricsAccess;
use komga_infrastructure::operational_settings_access::OperationalSettingsAccess;
use komga_infrastructure::sqlite::write_models::server_settings::ServerSettingsStore;

#[derive(Clone)]
pub struct OperationalApiState {
    pub(crate) auth_db: AuthDatabaseState,
    pub(crate) operational: OperationalState,
    pub(crate) identity: IdentityState,
    pub(crate) task_queue: TaskQueueState,
    pub(crate) operational_runtime: Arc<OperationalMetricsAccess>,
    pub(crate) operational_settings: Arc<OperationalSettingsAccess>,
}

impl FromRef<Arc<HttpAppState>> for OperationalApiState {
    fn from_ref(app: &Arc<HttpAppState>) -> Self {
        Self {
            auth_db: app.auth_db.clone(),
            operational: app.operational.clone(),
            identity: IdentityState::from_ref(app),
            task_queue: TaskQueueState::from_ref(app),
            operational_runtime: app.services.operational_runtime.clone(),
            operational_settings: app.services.operational_settings.clone(),
        }
    }
}

#[derive(Clone)]
pub struct ServerSettingsState {
    pub runtime: RuntimeState,
    pub(crate) server_settings: Arc<ServerSettingsStore>,
    pub(crate) task_queue: TaskQueueState,
}

impl FromRef<Arc<HttpAppState>> for ServerSettingsState {
    fn from_ref(app: &Arc<HttpAppState>) -> Self {
        Self {
            runtime: app.operational.runtime.clone(),
            server_settings: app.services.server_settings.clone(),
            task_queue: TaskQueueState::from_ref(app),
        }
    }
}

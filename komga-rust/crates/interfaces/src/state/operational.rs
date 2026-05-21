use super::*;
use axum::extract::FromRef;

use komga_application::operational::{
    OperationalMetricsPort, OperationalSettingsPort, ServerSettingsPort,
};

#[derive(Clone)]
pub struct OperationalApiState {
    pub(crate) auth_db: AuthDatabaseState,
    pub(crate) operational: OperationalState,
    pub(crate) identity: IdentityState,
    pub(crate) task_queue: TaskQueueState,
    pub(crate) operational_runtime: Arc<dyn OperationalMetricsPort>,
    pub(crate) operational_settings: Arc<dyn OperationalSettingsPort>,
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
    pub(crate) server_settings: Arc<dyn ServerSettingsPort>,
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

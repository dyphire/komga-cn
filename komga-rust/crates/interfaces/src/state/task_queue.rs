use super::*;
use axum::extract::FromRef;
use komga_application::operational::PersistedServerSettings;

#[derive(Clone)]
pub struct TaskQueueState {
    pub engine: Arc<dyn TaskEngine>,
}

impl FromRef<Arc<HttpAppState>> for TaskQueueState {
    fn from_ref(app: &Arc<HttpAppState>) -> Self {
        Self {
            engine: app.services.task_queue.clone(),
        }
    }
}

#[async_trait]
pub trait ServerSettingsService: Send + Sync {
    async fn load_map(&self) -> Result<BTreeMap<String, Option<String>>, String>;

    async fn load_settings(&self) -> Result<PersistedServerSettings, String>;

    async fn apply_changes(&self, changes: &[(String, Option<String>)]) -> Result<(), String>;
}

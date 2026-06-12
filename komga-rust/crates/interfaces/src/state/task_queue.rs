use std::sync::Arc;

use axum::extract::FromRef;
use komga_application::task_processing::TaskQueueAdmin;

use super::app_state::HttpAppState;

#[derive(Clone)]
pub struct TaskQueueState {
    pub queue: Arc<dyn TaskQueueAdmin>,
}

impl FromRef<Arc<HttpAppState>> for TaskQueueState {
    fn from_ref(app: &Arc<HttpAppState>) -> Self {
        Self {
            queue: app.services.task_queue.clone(),
        }
    }
}

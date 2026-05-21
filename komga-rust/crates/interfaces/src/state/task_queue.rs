use super::*;
use axum::extract::FromRef;

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

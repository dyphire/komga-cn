use super::*;
use axum::extract::FromRef;

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

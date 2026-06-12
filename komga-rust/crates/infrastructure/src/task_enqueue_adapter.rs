use std::sync::Arc;

use async_trait::async_trait;
use komga_application::media_assets::TaskEnqueuePort;
use komga_application::task_processing::{SubmitUrgency, TaskQueueAdmin, TaskQueueRecord};

#[derive(Clone)]
pub struct TaskEnqueueAdapter {
    queue: Arc<dyn TaskQueueAdmin>,
}

impl TaskEnqueueAdapter {
    pub fn new(queue: Arc<dyn TaskQueueAdmin>) -> Self {
        Self { queue }
    }
}

#[async_trait]
impl TaskEnqueuePort for TaskEnqueueAdapter {
    async fn enqueue(&self, records: Vec<TaskQueueRecord>) -> Result<(), String> {
        self.queue
            .enqueue_records(records, SubmitUrgency::Immediate)
            .await
    }
}

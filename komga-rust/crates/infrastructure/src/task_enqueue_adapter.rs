use std::sync::Arc;

use async_trait::async_trait;
use komga_application::media_assets::metadata_writer::TaskEnqueuePort;
use komga_application::task_processing::{TaskEngine, TaskQueueRecord};

#[derive(Clone)]
pub struct TaskEnqueueAdapter {
    engine: Arc<dyn TaskEngine>,
}

impl TaskEnqueueAdapter {
    pub fn new(engine: Arc<dyn TaskEngine>) -> Self {
        Self { engine }
    }
}

#[async_trait]
impl TaskEnqueuePort for TaskEnqueueAdapter {
    async fn enqueue(&self, records: Vec<TaskQueueRecord>) -> Result<(), String> {
        self.engine.enqueue_task_records(records, true).await
    }
}

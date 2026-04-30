use std::collections::BTreeMap;

use super::TaskQueueRecord;
use super::task_enqueuer::TaskEnqueuer;

#[derive(Clone, Debug, Default)]
pub struct QueueStatus {
    pub counts: BTreeMap<String, usize>,
}

#[async_trait::async_trait]
pub trait TaskEngine: TaskEnqueuer {
    async fn status(&self) -> QueueStatus;

    async fn clear_unowned_tasks(&self) -> usize;

    async fn apply_task_pool_size(&self, value: usize) -> Result<(), String>;

    async fn enqueue_task_records(
        &self,
        task_records: Vec<TaskQueueRecord>,
        urgent: bool,
    ) -> Result<(), String>;

    fn wakeup(&self);
}

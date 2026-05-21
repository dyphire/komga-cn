use std::collections::BTreeMap;

use super::TaskQueueRecord;
use super::protocol::LibraryTaskBatch;
use super::task_registry::{TaskKind, TaskRequest};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SubmitUrgency {
    /// Wake the background worker immediately.
    Immediate,
    /// Let the worker pick it up on its next natural cycle.
    #[default]
    Normal,
}

#[derive(Clone, Debug, Default)]
pub struct QueueStatus {
    pub counts: BTreeMap<String, usize>,
}

/// Primary task queue port. HTTP handlers and application services depend on this.
#[async_trait::async_trait]
pub trait TaskQueue: Send + Sync {
    async fn enqueue(&self, kind: TaskKind, target_id: &str);

    async fn enqueue_request(&self, request: TaskRequest);

    async fn enqueue_batch(&self, batch: LibraryTaskBatch);

    async fn enqueue_records(
        &self,
        records: Vec<TaskQueueRecord>,
        urgency: SubmitUrgency,
    ) -> Result<(), String>;

    async fn status(&self) -> QueueStatus;
}

/// Administrative operations for startup and settings endpoints.
#[async_trait::async_trait]
pub trait TaskQueueAdmin: TaskQueue {
    async fn clear_unowned_tasks(&self) -> usize;

    async fn apply_pool_size(&self, value: usize) -> Result<(), String>;

    fn wakeup(&self);
}

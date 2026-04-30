use std::sync::Arc;

use komga_application::task_processing::{
    LibraryTaskBatch, QueueStatus, TaskEngine, TaskEnqueuer, TaskKind, TaskQueueRecord, TaskRequest,
};
use tokio::sync::{Mutex, Notify};

use super::TaskExecutionPoolHandle;
use super::queue_scheduler::TaskQueueScheduler;

pub struct RuntimeTaskEngine {
    scheduler: Arc<Mutex<TaskQueueScheduler>>,
    execution_pool: TaskExecutionPoolHandle,
    wakeup: Arc<Notify>,
}

impl RuntimeTaskEngine {
    pub fn new(
        scheduler: Arc<Mutex<TaskQueueScheduler>>,
        execution_pool: TaskExecutionPoolHandle,
        wakeup: Arc<Notify>,
    ) -> Self {
        Self {
            scheduler,
            execution_pool,
            wakeup,
        }
    }
}

#[async_trait::async_trait]
impl TaskEnqueuer for RuntimeTaskEngine {
    async fn enqueue(&self, kind: TaskKind, target_id: &str) {
        let scheduler = self.scheduler.lock().await;
        TaskEnqueuer::enqueue(&*scheduler, kind, target_id).await;
    }

    async fn enqueue_request(&self, request: TaskRequest) {
        let scheduler = self.scheduler.lock().await;
        TaskEnqueuer::enqueue_request(&*scheduler, request).await;
    }

    async fn enqueue_batch(&self, batch: LibraryTaskBatch) {
        let scheduler = self.scheduler.lock().await;
        TaskEnqueuer::enqueue_batch(&*scheduler, batch).await;
    }
}

#[async_trait::async_trait]
impl TaskEngine for RuntimeTaskEngine {
    async fn status(&self) -> QueueStatus {
        let scheduler = self.scheduler.lock().await;
        TaskEngine::status(&*scheduler).await
    }

    async fn clear_unowned_tasks(&self) -> usize {
        let scheduler = self.scheduler.lock().await;
        TaskEngine::clear_unowned_tasks(&*scheduler).await
    }

    async fn apply_task_pool_size(&self, value: usize) -> Result<(), String> {
        self.execution_pool.resize(value);
        self.wakeup.notify_one();
        Ok(())
    }

    async fn enqueue_task_records(
        &self,
        task_records: Vec<TaskQueueRecord>,
        urgent: bool,
    ) -> Result<(), String> {
        let scheduler = self.scheduler.lock().await;
        TaskEngine::enqueue_task_records(&*scheduler, task_records, false).await?;
        if urgent {
            self.wakeup.notify_one();
        }
        Ok(())
    }

    fn wakeup(&self) {
        self.wakeup.notify_one();
    }
}

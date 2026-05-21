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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database_handle::DatabaseHandle;
    use crate::sqlite::{connect_task_pool, connect_task_write_pool, default_read_max_connections};
    use crate::task_queue::queue_scheduler::TaskQueueScheduler;
    use crate::task_queue::{TaskExecutionPoolHandle, TaskRuntimeContext};
    use komga_application::task_processing::TaskEngine;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use tokio::sync::Mutex;

    static NEXT_TEST_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    fn test_temp_root() -> PathBuf {
        let sequence = NEXT_TEST_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "komga-runtime-task-engine-test-{}-{nanos}-{sequence}",
            std::process::id(),
        ));
        std::fs::create_dir(&root).expect("test temp dir should be created");
        root
    }

    async fn test_task_runtime_context() -> TaskRuntimeContext {
        let root = test_temp_root();
        let main_db = DatabaseHandle::file_backed(root.join("database.sqlite"))
            .await
            .expect("test db should open");
        let task_write_pool = connect_task_write_pool(main_db.database_file())
            .await
            .expect("test task write pool should open");
        let task_read_pool =
            connect_task_pool(main_db.database_file(), default_read_max_connections())
                .await
                .expect("test task read pool should open");
        TaskRuntimeContext::new(
            main_db,
            root.join("tasks.sqlite"),
            root.join("lucene"),
            true,
            1,
            task_write_pool,
            task_read_pool,
        )
    }

    fn scan_library_task() -> TaskQueueRecord {
        use komga_application::task_processing::{ScanLibraryPayload, TaskKind, TaskRequest};
        TaskRequest::with_payload(
            TaskKind::ScanLibrary,
            ScanLibraryPayload::new("library-1", false),
        )
        .priority(8)
        .into_queue_record_with_id("library-1_DEEP_false")
    }

    #[tokio::test]
    async fn enqueue_task_records_respects_urgent_wakeup_policy() {
        for (urgent, timeout_ms, should_notify) in [(true, 100_u64, true), (false, 25_u64, false)] {
            let runtime = test_task_runtime_context().await;
            let task_execution_pool =
                TaskExecutionPoolHandle::new(runtime.worker().task_pool_size());
            let task_queue = Arc::new(Mutex::new(
                TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await,
            ));
            let task_wakeup = Arc::new(tokio::sync::Notify::new());
            let engine: Box<dyn TaskEngine> = Box::new(RuntimeTaskEngine::new(
                task_queue.clone(),
                task_execution_pool,
                task_wakeup.clone(),
            ));

            engine
                .enqueue_task_records(vec![scan_library_task()], urgent)
                .await
                .expect("task enqueue should succeed");

            let notified =
                tokio::time::timeout(Duration::from_millis(timeout_ms), task_wakeup.notified())
                    .await
                    .is_ok();
            assert_eq!(
                notified, should_notify,
                "urgent={urgent} should control background worker wakeup"
            );

            let queued_tasks = task_queue.lock().await.count_by_simple_type().await;
            assert_eq!(queued_tasks.get("ScanLibrary"), Some(&1), "urgent={urgent}");
        }
    }

    #[tokio::test]
    async fn apply_task_pool_size_resizes_execution_pool_and_wakes_scheduler() {
        let runtime = test_task_runtime_context().await;
        let task_execution_pool = TaskExecutionPoolHandle::new(runtime.worker().task_pool_size());
        let task_queue = Arc::new(Mutex::new(
            TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await,
        ));
        let task_wakeup = Arc::new(tokio::sync::Notify::new());
        let engine: Box<dyn TaskEngine> = Box::new(RuntimeTaskEngine::new(
            task_queue,
            task_execution_pool.clone(),
            task_wakeup.clone(),
        ));

        engine
            .apply_task_pool_size(3)
            .await
            .expect("task pool resize should succeed");

        tokio::time::timeout(Duration::from_millis(100), task_wakeup.notified())
            .await
            .expect("task pool resize should wake the background scheduler");
        assert_eq!(task_execution_pool.desired_size(), 3);
    }
}

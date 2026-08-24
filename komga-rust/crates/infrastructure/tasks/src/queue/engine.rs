use std::sync::Arc;

use komga_application::task_processing::{
    LibraryTaskBatch, QueueStatus, SubmitUrgency, TaskKind, TaskQueue, TaskQueueAdmin,
    TaskQueueRecord, TaskRequest,
};
use tokio::sync::{Mutex, Notify};

use super::scheduler::TaskQueueScheduler;
use crate::execution_pool::TaskExecutionPoolHandle;

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
impl TaskQueue for RuntimeTaskEngine {
    async fn enqueue(&self, kind: TaskKind, target_id: &str) {
        let scheduler = self.scheduler.lock().await;
        TaskQueue::enqueue(&*scheduler, kind, target_id).await;
    }

    async fn enqueue_request(&self, request: TaskRequest) {
        let scheduler = self.scheduler.lock().await;
        TaskQueue::enqueue_request(&*scheduler, request).await;
    }

    async fn enqueue_batch(&self, batch: LibraryTaskBatch) {
        let scheduler = self.scheduler.lock().await;
        TaskQueue::enqueue_batch(&*scheduler, batch).await;
    }

    async fn enqueue_records(
        &self,
        records: Vec<TaskQueueRecord>,
        urgency: SubmitUrgency,
    ) -> anyhow::Result<()> {
        let scheduler = self.scheduler.lock().await;
        TaskQueue::enqueue_records(&*scheduler, records, SubmitUrgency::Normal).await?;
        if urgency == SubmitUrgency::Immediate {
            self.wakeup.notify_one();
        }
        Ok(())
    }

    async fn status(&self) -> anyhow::Result<QueueStatus> {
        let scheduler = self.scheduler.lock().await;
        TaskQueue::status(&*scheduler).await
    }
}

#[async_trait::async_trait]
impl TaskQueueAdmin for RuntimeTaskEngine {
    async fn clear_unowned_tasks(&self) -> anyhow::Result<usize> {
        let scheduler = self.scheduler.lock().await;
        TaskQueueAdmin::clear_unowned_tasks(&*scheduler).await
    }

    async fn apply_pool_size(&self, value: usize) -> anyhow::Result<()> {
        self.execution_pool.resize(value);
        self.wakeup.notify_one();
        Ok(())
    }

    fn wakeup(&self) {
        self.wakeup.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_pool::TaskExecutor;
    use komga_application::task_processing::{SubmitUrgency, TaskQueueAdmin};
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

    fn test_task_config() -> crate::TaskQueueConfig {
        let root = test_temp_root();
        crate::TaskQueueConfig::new(root.join("tasks.sqlite"), true)
    }

    fn test_task_config_with_tasks_db(
        tasks_db_file: PathBuf,
        root: PathBuf,
    ) -> crate::TaskQueueConfig {
        let _ = root;
        crate::TaskQueueConfig::new(tasks_db_file, true)
    }

    fn test_executor() -> TaskExecutor {
        Arc::new(|_task| {
            Box::pin(async {
                Ok(komga_application::task_processing::TaskExecutionOutcome::completed())
            })
        })
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
    async fn enqueue_records_respects_urgent_wakeup_policy() {
        for (urgency, timeout_ms, should_notify) in [
            (SubmitUrgency::Immediate, 100_u64, true),
            (SubmitUrgency::Normal, 25_u64, false),
        ] {
            let config = test_task_config();
            let task_execution_pool = TaskExecutionPoolHandle::new(1, test_executor());
            let task_queue = Arc::new(Mutex::new(
                TaskQueueScheduler::for_config(config, "rust-main").await,
            ));
            let task_wakeup = Arc::new(tokio::sync::Notify::new());
            let engine: Box<dyn TaskQueueAdmin> = Box::new(RuntimeTaskEngine::new(
                task_queue.clone(),
                task_execution_pool,
                task_wakeup.clone(),
            ));

            engine
                .enqueue_records(vec![scan_library_task()], urgency)
                .await
                .expect("task enqueue should succeed");

            let notified =
                tokio::time::timeout(Duration::from_millis(timeout_ms), task_wakeup.notified())
                    .await
                    .is_ok();
            assert_eq!(
                notified, should_notify,
                "urgency={urgency:?} should control background worker wakeup"
            );

            let queued_tasks = task_queue
                .lock()
                .await
                .count_by_simple_type()
                .await
                .expect("runtime task engine fixture queue counts should load");
            assert_eq!(
                queued_tasks.get("ScanLibrary"),
                Some(&1),
                "urgency={urgency:?}"
            );
        }
    }

    #[tokio::test]
    async fn enqueue_records_reports_persisted_store_initialization_errors() {
        let root = test_temp_root();
        let tasks_db_file = root.join("tasks.sqlite");
        std::fs::create_dir(&tasks_db_file).expect("directory at tasks db path should be created");
        let config = test_task_config_with_tasks_db(tasks_db_file, root);
        let task_execution_pool = TaskExecutionPoolHandle::new(1, test_executor());
        let task_queue = Arc::new(Mutex::new(
            TaskQueueScheduler::for_config(config, "rust-main").await,
        ));
        let task_wakeup = Arc::new(tokio::sync::Notify::new());
        let engine: Box<dyn TaskQueueAdmin> = Box::new(RuntimeTaskEngine::new(
            task_queue,
            task_execution_pool,
            task_wakeup,
        ));

        let error = engine
            .enqueue_records(vec![scan_library_task()], SubmitUrgency::Immediate)
            .await
            .expect_err("task queue persistence initialization errors should be reported");

        assert!(
            error.to_string().contains("open tasks sqlite pool"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn apply_pool_size_resizes_execution_pool_and_wakes_scheduler() {
        let config = test_task_config();
        let task_execution_pool = TaskExecutionPoolHandle::new(1, test_executor());
        let task_queue = Arc::new(Mutex::new(
            TaskQueueScheduler::for_config(config, "rust-main").await,
        ));
        let task_wakeup = Arc::new(tokio::sync::Notify::new());
        let engine: Box<dyn TaskQueueAdmin> = Box::new(RuntimeTaskEngine::new(
            task_queue,
            task_execution_pool.clone(),
            task_wakeup.clone(),
        ));

        engine
            .apply_pool_size(3)
            .await
            .expect("task pool resize should succeed");

        tokio::time::timeout(Duration::from_millis(100), task_wakeup.notified())
            .await
            .expect("task pool resize should wake the background scheduler");
        assert_eq!(task_execution_pool.desired_size(), 3);
    }
}

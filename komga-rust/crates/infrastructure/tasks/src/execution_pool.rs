use futures_util::{FutureExt, future::BoxFuture};
use komga_application::task_processing::{
    TaskExecutionOutcome, TaskExecutionResult, TaskProcessingError, TaskQueueRecord,
};
#[cfg(test)]
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::JoinHandle;
use tokio::sync::{Mutex as AsyncMutex, mpsc};

pub type TaskExecutor = Arc<
    dyn Fn(TaskQueueRecord) -> BoxFuture<'static, Result<TaskExecutionOutcome, TaskProcessingError>>
        + Send
        + Sync,
>;

enum TaskExecutionCommand {
    Run(Box<TaskExecutionJob>),
    Retire,
    Shutdown,
}

struct TaskExecutionJob {
    task: TaskQueueRecord,
}

struct TaskExecutionPoolInner {
    desired_size: AtomicUsize,
    active_workers: AtomicUsize,
    next_worker_id: AtomicUsize,
    shutdown: AtomicBool,
    stopping: AtomicBool,
    executor: TaskExecutor,
    job_tx: mpsc::UnboundedSender<TaskExecutionCommand>,
    job_rx: AsyncMutex<mpsc::UnboundedReceiver<TaskExecutionCommand>>,
    result_tx: mpsc::UnboundedSender<TaskExecutionResult>,
    result_rx: StdMutex<Option<mpsc::UnboundedReceiver<TaskExecutionResult>>>,
    worker_handles: StdMutex<Vec<JoinHandle<()>>>,
}

#[derive(Clone)]
pub struct TaskExecutionPoolHandle {
    owner: Arc<TaskExecutionPoolOwner>,
}

struct TaskExecutionPoolOwner {
    inner: Arc<TaskExecutionPoolInner>,
}

impl TaskExecutionPoolHandle {
    pub fn new(task_pool_size: usize, executor: TaskExecutor) -> Self {
        let (job_tx, job_rx) = mpsc::unbounded_channel();
        let (result_tx, result_rx) = mpsc::unbounded_channel();
        let inner = Arc::new(TaskExecutionPoolInner {
            desired_size: AtomicUsize::new(task_pool_size.max(1)),
            active_workers: AtomicUsize::new(0),
            next_worker_id: AtomicUsize::new(1),
            shutdown: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
            executor,
            job_tx,
            job_rx: AsyncMutex::new(job_rx),
            result_tx,
            result_rx: StdMutex::new(Some(result_rx)),
            worker_handles: StdMutex::new(Vec::new()),
        });
        inner.spawn_missing_workers();
        Self {
            owner: Arc::new(TaskExecutionPoolOwner { inner }),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_for_test<F, Fut>(task_pool_size: usize, execute_task: F) -> Self
    where
        F: Fn(TaskQueueRecord) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<TaskExecutionOutcome, TaskProcessingError>> + Send + 'static,
    {
        Self::new(
            task_pool_size,
            Arc::new(move |task| Box::pin(execute_task(task))),
        )
    }

    pub fn desired_size(&self) -> usize {
        self.owner.inner.desired_size.load(Ordering::SeqCst)
    }

    pub fn resize(&self, task_pool_size: usize) {
        let inner = &self.owner.inner;
        if inner.stopping.load(Ordering::SeqCst) {
            return;
        }
        let next_size = task_pool_size.max(1);
        let previous_size = inner.desired_size.swap(next_size, Ordering::SeqCst);
        if next_size > previous_size {
            inner.spawn_missing_workers();
            return;
        }

        for _ in 0..previous_size.saturating_sub(next_size) {
            let _ = inner.job_tx.send(TaskExecutionCommand::Retire);
        }
    }

    pub fn submit(&self, task: TaskQueueRecord) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.owner.inner.shutdown.load(Ordering::SeqCst),
            "task execution pool is closed"
        );
        self.owner
            .inner
            .job_tx
            .send(TaskExecutionCommand::Run(Box::new(TaskExecutionJob {
                task,
            })))
            .map_err(|_| anyhow::anyhow!("task execution pool job channel closed"))
    }

    pub fn take_result_receiver(&self) -> Option<mpsc::UnboundedReceiver<TaskExecutionResult>> {
        self.owner
            .inner
            .result_rx
            .lock()
            .expect("task execution pool result receiver lock should not be poisoned")
            .take()
    }

    pub fn stop_claiming(&self) {
        self.owner.inner.stopping.store(true, Ordering::SeqCst);
    }

    pub fn is_stopping(&self) -> bool {
        self.owner.inner.stopping.load(Ordering::SeqCst)
    }

    pub async fn shutdown(&self) {
        self.owner.inner.request_shutdown();
        // Keep handles owned here if the caller cancels shutdown. Never block a Tokio
        // worker on a thread that may still be executing synchronous media code.
        loop {
            {
                let mut handles = self
                    .owner
                    .inner
                    .worker_handles
                    .lock()
                    .expect("worker handles lock should not be poisoned");
                let mut index = 0;
                while index < handles.len() {
                    if handles[index].is_finished() {
                        if handles.swap_remove(index).join().is_err() {
                            tracing::error!("task execution worker panicked during shutdown");
                        }
                    } else {
                        index += 1;
                    }
                }
                if handles.is_empty() {
                    return;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    }
}

impl TaskExecutionPoolInner {
    fn request_shutdown(&self) {
        let _handles = self
            .worker_handles
            .lock()
            .expect("worker handles lock should not be poisoned");
        self.stopping.store(true, Ordering::SeqCst);
        if self.shutdown.swap(true, Ordering::SeqCst) {
            return;
        }
        for _ in 0..self.active_workers.load(Ordering::SeqCst) {
            let _ = self.job_tx.send(TaskExecutionCommand::Shutdown);
        }
    }

    fn spawn_missing_workers(self: &Arc<Self>) {
        let mut handles = self
            .worker_handles
            .lock()
            .expect("worker handles lock should not be poisoned");
        loop {
            if self.shutdown.load(Ordering::SeqCst) {
                return;
            }

            let desired_size = self.desired_size.load(Ordering::SeqCst);
            let active_workers = self.active_workers.load(Ordering::SeqCst);
            if active_workers >= desired_size {
                return;
            }

            if self
                .active_workers
                .compare_exchange(
                    active_workers,
                    active_workers + 1,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                )
                .is_err()
            {
                continue;
            }

            let worker_id = self.next_worker_id.fetch_add(1, Ordering::SeqCst);
            let thread_name = format!("komga-task-worker-{worker_id}");
            let inner = Arc::clone(self);
            let handle = std::thread::Builder::new()
                .name(thread_name)
                .spawn(move || inner.worker_main())
                .expect("task execution worker thread should spawn");
            handles.push(handle);
        }
    }

    fn worker_main(self: Arc<Self>) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("task execution worker runtime should build");

        runtime.block_on(async {
            loop {
                // Drive this runtime while idle too: SQLx returns connections in spawned tasks.
                let command = self.job_rx.lock().await.recv().await;
                let Some(command) = command else {
                    break;
                };

                match command {
                    TaskExecutionCommand::Run(job) => {
                        let task = job.task.clone();
                        let outcome = AssertUnwindSafe(async { (self.executor)(job.task).await })
                            .catch_unwind()
                            .await
                            .unwrap_or_else(|panic_payload| {
                                Err(TaskProcessingError::runtime(format!(
                                    "task execution worker panicked while processing {}: {}",
                                    task.id,
                                    panic_payload_message(&panic_payload),
                                )))
                            });
                        let _ = self.result_tx.send(TaskExecutionResult { task, outcome });
                    }
                    TaskExecutionCommand::Retire | TaskExecutionCommand::Shutdown => break,
                }
            }
        });

        self.active_workers.fetch_sub(1, Ordering::SeqCst);
        self.spawn_missing_workers();
    }
}

impl Drop for TaskExecutionPoolOwner {
    fn drop(&mut self) {
        // Workers own Inner, never Owner, so dropping the last public handle can
        // actually signal them. Normal server shutdown explicitly joins them first.
        self.inner.request_shutdown();
    }
}

fn panic_payload_message(panic_payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = panic_payload.downcast_ref::<&str>() {
        return (*message).to_string();
    }
    if let Some(message) = panic_payload.downcast_ref::<String>() {
        return message.clone();
    }
    "unknown panic payload".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn idle_executor_returns_sqlite_connections_and_joins_all_threads() {
        let sqlite = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let pool = TaskExecutionPoolHandle::new_for_test(2, {
            let sqlite = sqlite.clone();
            move |_| {
                let sqlite = sqlite.clone();
                async move {
                    let connection = sqlite.acquire().await.unwrap();
                    drop(connection);
                    Ok(TaskExecutionOutcome::completed())
                }
            }
        });
        let mut results = pool.take_result_receiver().unwrap();
        pool.submit(TaskQueueRecord::new("UpgradeIndex:idle-close", 0, None))
            .unwrap();
        results.recv().await.unwrap().outcome.unwrap();
        // No more jobs are submitted. Connection return must progress on the idle worker.
        tokio::time::timeout(Duration::from_secs(1), sqlite.close())
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), pool.shutdown())
            .await
            .unwrap();
        assert!(pool.owner.inner.worker_handles.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn dropping_last_owner_releases_idle_workers() {
        let pool = TaskExecutionPoolHandle::new_for_test(2, |_| async {
            Ok(TaskExecutionOutcome::completed())
        });
        let inner = Arc::downgrade(&pool.owner.inner);
        drop(pool);
        tokio::time::timeout(Duration::from_secs(1), async {
            while inner.strong_count() != 0 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
    }
}

use futures_util::future::BoxFuture;
use komga_application::task_processing::{
    TaskExecutionOutcome, TaskExecutionResult, TaskProcessingError, TaskQueueRecord,
};
#[cfg(test)]
use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::JoinHandle;
use tokio::sync::mpsc;

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
    executor: TaskExecutor,
    job_tx: mpsc::UnboundedSender<TaskExecutionCommand>,
    job_rx: StdMutex<mpsc::UnboundedReceiver<TaskExecutionCommand>>,
    result_tx: mpsc::UnboundedSender<TaskExecutionResult>,
    result_rx: StdMutex<Option<mpsc::UnboundedReceiver<TaskExecutionResult>>>,
    worker_handles: StdMutex<Vec<JoinHandle<()>>>,
}

#[derive(Clone)]
pub struct TaskExecutionPoolHandle {
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
            executor,
            job_tx,
            job_rx: StdMutex::new(job_rx),
            result_tx,
            result_rx: StdMutex::new(Some(result_rx)),
            worker_handles: StdMutex::new(Vec::new()),
        });
        inner.spawn_missing_workers();
        Self { inner }
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
        self.inner.desired_size.load(Ordering::SeqCst)
    }

    pub fn resize(&self, task_pool_size: usize) {
        let next_size = task_pool_size.max(1);
        let previous_size = self.inner.desired_size.swap(next_size, Ordering::SeqCst);
        if next_size > previous_size {
            self.inner.spawn_missing_workers();
            return;
        }

        for _ in 0..previous_size.saturating_sub(next_size) {
            let _ = self.inner.job_tx.send(TaskExecutionCommand::Retire);
        }
    }

    pub fn submit(&self, task: TaskQueueRecord) -> anyhow::Result<()> {
        self.inner
            .job_tx
            .send(TaskExecutionCommand::Run(Box::new(TaskExecutionJob {
                task,
            })))
            .map_err(|_| anyhow::anyhow!("task execution pool job channel closed"))
    }

    pub fn take_result_receiver(&self) -> Option<mpsc::UnboundedReceiver<TaskExecutionResult>> {
        self.inner
            .result_rx
            .lock()
            .expect("task execution pool result receiver lock should not be poisoned")
            .take()
    }
}

impl TaskExecutionPoolInner {
    fn spawn_missing_workers(self: &Arc<Self>) {
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
            self.worker_handles
                .lock()
                .expect("task execution worker handles lock should not be poisoned")
                .push(handle);
        }
    }

    fn worker_main(self: Arc<Self>) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("task execution worker runtime should build");

        loop {
            let command = self
                .job_rx
                .lock()
                .expect("task execution worker receiver lock should not be poisoned")
                .blocking_recv();
            let Some(command) = command else {
                break;
            };

            match command {
                TaskExecutionCommand::Run(job) => {
                    let task = job.task.clone();
                    let outcome = catch_unwind(AssertUnwindSafe(|| {
                        runtime.block_on((self.executor)(job.task))
                    }))
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

        self.active_workers.fetch_sub(1, Ordering::SeqCst);
        self.spawn_missing_workers();
    }
}

impl Drop for TaskExecutionPoolInner {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let active_workers = self.active_workers.load(Ordering::SeqCst);
        for _ in 0..active_workers {
            let _ = self.job_tx.send(TaskExecutionCommand::Shutdown);
        }

        let mut worker_handles = self
            .worker_handles
            .lock()
            .expect("task execution worker handles lock should not be poisoned");
        while let Some(handle) = worker_handles.pop() {
            let _ = handle.join();
        }
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

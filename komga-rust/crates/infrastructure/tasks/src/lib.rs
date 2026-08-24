use std::path::PathBuf;

mod enqueue_adapter;
mod execution_loop;
mod execution_pool;
mod queue;

pub use enqueue_adapter::TaskEnqueueAdapter;
pub use execution_loop::{
    BackgroundTaskExecutionLoop, SharedTaskQueue, process_available_serial, recover_and_process,
};
pub use execution_pool::{TaskExecutionPoolHandle, TaskExecutor};
pub use queue::RuntimeTaskEngine;
pub use queue::TaskQueueScheduler;

#[derive(Clone, Debug)]
pub struct TaskQueueConfig {
    tasks_db_file: PathBuf,
    consumes_queue: bool,
}

impl TaskQueueConfig {
    pub fn new(tasks_db_file: PathBuf, consumes_queue: bool) -> Self {
        Self {
            tasks_db_file,
            consumes_queue,
        }
    }

    pub fn tasks_db_file(&self) -> &std::path::Path {
        self.tasks_db_file.as_path()
    }

    pub fn consumes_queue(&self) -> bool {
        self.consumes_queue
    }
}

pub trait TaskQueueConfigProvider {
    fn task_queue_config(&self) -> TaskQueueConfig;
}

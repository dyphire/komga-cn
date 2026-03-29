use komga_application::task_processing::{TaskRuntimeConfig, TaskRuntimeContext};
pub use komga_infrastructure::task_queue::{
    RuntimeBackgroundState, ScheduledLibraryScan, SharedTaskQueue,
};

use crate::config::RuntimeConfig;

pub fn prepare_task_queue(
    config: &RuntimeConfig,
    startup_search_task: Option<&'static str>,
) -> RuntimeBackgroundState {
    komga_infrastructure::task_queue::prepare_task_queue(config.clone(), startup_search_task)
}

pub fn spawn_runtime_workers(
    task_queue: SharedTaskQueue,
    config: RuntimeConfig,
    scheduled_scans: Vec<ScheduledLibraryScan>,
) {
    let runtime: TaskRuntimeContext = config.task_runtime_context();
    komga_infrastructure::task_queue::spawn_runtime_workers(task_queue, runtime, scheduled_scans);
}

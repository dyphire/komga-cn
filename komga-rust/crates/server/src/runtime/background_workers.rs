use komga_application::task_processing::TaskRuntimeContext;
pub use komga_infrastructure::task_queue::{
    RuntimeBackgroundState, SharedTaskQueue, TaskQueueWakeSignal,
};
use tokio::sync::watch;

use crate::config::RuntimeConfig;

pub fn prepare_task_queue(
    config: &RuntimeConfig,
    startup_search_task: Option<&'static str>,
) -> RuntimeBackgroundState {
    komga_infrastructure::task_queue::prepare_task_queue(
        crate::config::task_runtime_context(config),
        startup_search_task,
    )
}

pub fn spawn_runtime_workers(
    task_queue: SharedTaskQueue,
    config: RuntimeConfig,
    task_wakeup: TaskQueueWakeSignal,
    shutdown_rx: Option<watch::Receiver<bool>>,
) {
    let runtime: TaskRuntimeContext = crate::config::task_runtime_context(&config);
    komga_infrastructure::task_queue::spawn_runtime_workers(
        task_queue,
        runtime,
        task_wakeup,
        shutdown_rx,
    );
}

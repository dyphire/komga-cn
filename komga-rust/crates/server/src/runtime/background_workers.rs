use komga_application::task_processing::TaskRuntimeContext;
use komga_infrastructure::task_queue::TaskExecutionPoolHandle;
pub use komga_infrastructure::task_queue::worker_runtime::{
    RuntimeBackgroundState, SharedTaskQueue, TaskQueueWakeSignal,
};
use std::sync::Arc;
use tokio::sync::watch;

use komga_config::env_config::RuntimeConfig;

#[derive(Debug)]
pub struct WorkerRuntimeLifecycleGuard {
    shutdown_tx: watch::Sender<bool>,
}

pub type WorkerRuntimeGuard = Arc<WorkerRuntimeLifecycleGuard>;

pub async fn prepare_task_queue(
    config: &RuntimeConfig,
    startup_search_task: Option<&'static str>,
) -> RuntimeBackgroundState {
    komga_infrastructure::task_queue::worker_runtime::prepare_task_queue(
        crate::config::task_runtime_context(config),
        startup_search_task,
    )
    .await
}

pub fn spawn_runtime_workers(
    task_queue: SharedTaskQueue,
    task_execution_pool: TaskExecutionPoolHandle,
    config: RuntimeConfig,
    task_wakeup: TaskQueueWakeSignal,
    shutdown_rx: Option<watch::Receiver<bool>>,
) -> WorkerRuntimeGuard {
    let runtime: TaskRuntimeContext = crate::config::task_runtime_context(&config);
    let (internal_shutdown_tx, internal_shutdown_rx) = watch::channel(false);
    if let Some(mut external_shutdown_rx) = shutdown_rx {
        let forward_shutdown_tx = internal_shutdown_tx.clone();
        tokio::spawn(async move {
            wait_for_shutdown_signal(&mut external_shutdown_rx).await;
            let _ = forward_shutdown_tx.send(true);
        });
    }

    komga_infrastructure::task_queue::worker_runtime::spawn_runtime_workers(
        task_queue,
        task_execution_pool,
        runtime,
        task_wakeup,
        Some(internal_shutdown_rx),
    );

    Arc::new(WorkerRuntimeLifecycleGuard {
        shutdown_tx: internal_shutdown_tx,
    })
}

impl Drop for WorkerRuntimeLifecycleGuard {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.send(true);
    }
}

async fn wait_for_shutdown_signal(shutdown_rx: &mut watch::Receiver<bool>) {
    loop {
        if *shutdown_rx.borrow() {
            break;
        }
        if shutdown_rx.changed().await.is_err() {
            break;
        }
    }
}

use komga_application::task_processing::TaskRuntimeContext;
pub use komga_infrastructure::task_queue::worker_runtime::{
    RuntimeBackgroundState, SharedTaskQueue, TaskQueueWakeSignal,
};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use tokio::sync::watch;

use komga_config::env_config::RuntimeConfig;

#[derive(Debug)]
pub struct IsolatedWorkerRuntimeGuard {
    shutdown_tx: watch::Sender<bool>,
    thread_handle: Mutex<Option<JoinHandle<()>>>,
}

pub type WorkerRuntimeGuard = Arc<IsolatedWorkerRuntimeGuard>;

pub fn prepare_task_queue(
    config: &RuntimeConfig,
    startup_search_task: Option<&'static str>,
) -> RuntimeBackgroundState {
    komga_infrastructure::task_queue::worker_runtime::prepare_task_queue(
        crate::config::task_runtime_context(config),
        startup_search_task,
    )
}

pub fn spawn_runtime_workers(
    task_queue: SharedTaskQueue,
    config: RuntimeConfig,
    task_wakeup: TaskQueueWakeSignal,
    shutdown_rx: Option<watch::Receiver<bool>>,
) -> WorkerRuntimeGuard {
    let runtime: TaskRuntimeContext = crate::config::task_runtime_context(&config);
    let (internal_shutdown_tx, internal_shutdown_rx) = watch::channel(false);
    let thread_shutdown_tx = internal_shutdown_tx.clone();
    let thread_handle = std::thread::Builder::new()
        .name("komga-task-runtime".to_string())
        .spawn(move || {
            let dedicated_runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("isolated task runtime should build");

            dedicated_runtime.block_on(async move {
                if let Some(mut external_shutdown_rx) = shutdown_rx {
                    let forward_shutdown_tx = thread_shutdown_tx.clone();
                    tokio::spawn(async move {
                        wait_for_shutdown_signal(&mut external_shutdown_rx).await;
                        let _ = forward_shutdown_tx.send(true);
                    });
                }

                komga_infrastructure::task_queue::worker_runtime::spawn_runtime_workers(
                    task_queue,
                    runtime,
                    task_wakeup,
                    Some(internal_shutdown_rx.clone()),
                );

                let mut runtime_shutdown_rx = internal_shutdown_rx;
                wait_for_shutdown_signal(&mut runtime_shutdown_rx).await;
            });
        })
        .expect("isolated task runtime thread should spawn");

    Arc::new(IsolatedWorkerRuntimeGuard {
        shutdown_tx: internal_shutdown_tx,
        thread_handle: Mutex::new(Some(thread_handle)),
    })
}

impl Drop for IsolatedWorkerRuntimeGuard {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.send(true);
        if let Some(thread_handle) = self
            .thread_handle
            .lock()
            .expect("isolated task runtime thread handle lock should not be poisoned")
            .take()
        {
            let _ = thread_handle.join();
        }
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

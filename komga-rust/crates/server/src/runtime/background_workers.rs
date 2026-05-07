use axum::Extension;
use axum::Router;
use komga_application::task_processing::TaskEngine;
use komga_infrastructure::database_handle::DatabaseHandle;
use komga_infrastructure::task_queue::RuntimeTaskEngine;
use komga_infrastructure::task_queue::TaskExecutionPoolHandle;
use komga_infrastructure::task_queue::TaskRuntimeContext;
use komga_infrastructure::task_queue::worker_runtime::{
    RuntimeBackgroundState, SharedTaskQueue, TaskQueueWakeSignal,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::watch;

use komga_config::env_config::RuntimeConfig;

pub enum TaskRuntimeMode {
    WorkersEnabled {
        startup_search_task: Option<&'static str>,
        shutdown_rx: Option<watch::Receiver<bool>>,
    },
    WorkersDisabled {
        startup_search_task: Option<&'static str>,
    },
}

pub struct StartedTaskRuntime {
    parts: HttpRuntimeParts,
    lifecycle: RouterRuntimeLifecycle,
}

pub struct HttpRuntimeParts {
    pub main_db: DatabaseHandle,
    pub tasks_db: DatabaseHandle,
    pub task_engine: Box<dyn TaskEngine>,
}

pub struct RouterRuntimeLifecycle {
    worker_runtime_guard: Option<WorkerRuntimeGuard>,
}

#[derive(Debug)]
struct WorkerRuntimeLifecycleGuard {
    shutdown_tx: watch::Sender<bool>,
}

type WorkerRuntimeGuard = Arc<WorkerRuntimeLifecycleGuard>;

impl StartedTaskRuntime {
    pub async fn start(config: &RuntimeConfig, mode: TaskRuntimeMode) -> std::io::Result<Self> {
        let runtime = crate::config::task_runtime_context(config).await;
        let startup_search_task = mode.startup_search_task();
        let background = prepare_task_queue(runtime.clone(), startup_search_task).await;
        let tasks_db = open_database_handle(runtime.tasks_db_file.clone(), "tasks").await?;
        let worker_runtime_guard = match mode {
            TaskRuntimeMode::WorkersEnabled { shutdown_rx, .. } => Some(spawn_runtime_workers(
                background.task_queue.clone(),
                background.task_execution_pool.clone(),
                runtime.clone(),
                background.task_wakeup.clone(),
                shutdown_rx,
            )),
            TaskRuntimeMode::WorkersDisabled { .. } => None,
        };
        let task_engine = Box::new(RuntimeTaskEngine::new(
            background.task_queue,
            background.task_execution_pool,
            background.task_wakeup,
        ));

        Ok(Self {
            parts: HttpRuntimeParts {
                main_db: runtime.main_db,
                tasks_db,
                task_engine,
            },
            lifecycle: RouterRuntimeLifecycle {
                worker_runtime_guard,
            },
        })
    }

    pub fn into_parts(self) -> (HttpRuntimeParts, RouterRuntimeLifecycle) {
        (self.parts, self.lifecycle)
    }
}

impl TaskRuntimeMode {
    fn startup_search_task(&self) -> Option<&'static str> {
        match self {
            TaskRuntimeMode::WorkersEnabled {
                startup_search_task,
                ..
            }
            | TaskRuntimeMode::WorkersDisabled {
                startup_search_task,
            } => *startup_search_task,
        }
    }
}

impl RouterRuntimeLifecycle {
    pub fn attach(self, router: Router) -> Router {
        match self.worker_runtime_guard {
            Some(worker_runtime_guard) => router.layer(Extension(worker_runtime_guard)),
            None => router,
        }
    }
}

async fn prepare_task_queue(
    runtime: TaskRuntimeContext,
    startup_search_task: Option<&'static str>,
) -> RuntimeBackgroundState {
    komga_infrastructure::task_queue::worker_runtime::prepare_task_queue(
        runtime,
        startup_search_task,
    )
    .await
}

async fn open_database_handle(
    database_file: PathBuf,
    role: &str,
) -> std::io::Result<DatabaseHandle> {
    DatabaseHandle::file_backed(database_file.clone())
        .await
        .map_err(|error| {
            std::io::Error::other(format!(
                "failed to open {role} database handle at {}: {error}",
                database_file.display()
            ))
        })
}

fn spawn_runtime_workers(
    task_queue: SharedTaskQueue,
    task_execution_pool: TaskExecutionPoolHandle,
    runtime: TaskRuntimeContext,
    task_wakeup: TaskQueueWakeSignal,
    shutdown_rx: Option<watch::Receiver<bool>>,
) -> WorkerRuntimeGuard {
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

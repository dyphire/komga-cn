use axum::Extension;
use axum::Router;
use komga_application::task_processing::TaskEngine;
use komga_config::env_config::RuntimeConfig;
use komga_config::profile::RuntimeProfile;
use komga_config::writer_ownership::{WriterDecision, WriterKind};
use komga_infrastructure::database_handle::DatabaseHandle;
use komga_infrastructure::search::index_lifecycle::{
    SearchStartupLifecycle, decide_startup_lifecycle, prepare_for_rebuild,
};
use komga_infrastructure::task_queue::RuntimeTaskEngine;
use komga_infrastructure::task_queue::TaskExecutionPoolHandle;
use komga_infrastructure::task_queue::TaskRuntimeContext;
use komga_infrastructure::task_queue::worker_runtime::{
    RuntimeBackgroundState, SharedTaskQueue, TaskQueueWakeSignal,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::watch;

pub(crate) enum TaskRuntimeMode {
    WorkersEnabled {
        shutdown_rx: Option<watch::Receiver<bool>>,
    },
    WorkersDisabled,
}

pub(crate) struct TaskRuntime;

pub(crate) struct StartedTaskRuntime {
    router_parts: TaskRouterParts,
}

pub(crate) struct TaskRouterParts {
    pub(crate) http: HttpRuntimeParts,
    pub(crate) lifecycle: RouterRuntimeLifecycle,
}

pub(crate) struct HttpRuntimeParts {
    pub(crate) main_db: DatabaseHandle,
    pub(crate) tasks_db: DatabaseHandle,
    pub(crate) task_engine: Box<dyn TaskEngine>,
}

pub(crate) struct RouterRuntimeLifecycle {
    worker_runtime_guard: Option<WorkerRuntimeGuard>,
}

#[derive(Debug)]
struct WorkerRuntimeLifecycleGuard {
    shutdown_tx: watch::Sender<bool>,
}

#[derive(Clone, Copy)]
struct StartupSearchPlan {
    writer_decision: WriterDecision,
    lifecycle: &'static str,
    startup_task: Option<&'static str>,
}

type WorkerRuntimeGuard = Arc<WorkerRuntimeLifecycleGuard>;

impl TaskRuntime {
    pub(crate) async fn start(
        config: &RuntimeConfig,
        mode: TaskRuntimeMode,
    ) -> std::io::Result<StartedTaskRuntime> {
        if matches!(config.runtime_profile, RuntimeProfile::LiveLocaldb) {
            crate::runtime::startup_scan::bootstrap_library_scan(config).await;
        }

        let startup_search_plan = plan_startup_search_task_with_logging(config)?;
        let runtime = crate::config::task_runtime_context(config).await;
        let background =
            prepare_task_queue(runtime.clone(), startup_search_plan.startup_task).await;
        let tasks_db = open_database_handle(runtime.tasks_db_file.clone(), "tasks").await?;
        let worker_runtime_guard = match mode {
            TaskRuntimeMode::WorkersEnabled { shutdown_rx } => Some(spawn_runtime_workers(
                background.task_queue.clone(),
                background.task_execution_pool.clone(),
                runtime.clone(),
                background.task_wakeup.clone(),
                shutdown_rx,
            )),
            TaskRuntimeMode::WorkersDisabled => None,
        };
        let task_engine = Box::new(RuntimeTaskEngine::new(
            background.task_queue,
            background.task_execution_pool,
            background.task_wakeup,
        ));

        Ok(StartedTaskRuntime {
            router_parts: TaskRouterParts {
                http: HttpRuntimeParts {
                    main_db: runtime.main_db,
                    tasks_db,
                    task_engine,
                },
                lifecycle: RouterRuntimeLifecycle {
                    worker_runtime_guard,
                },
            },
        })
    }
}

impl StartedTaskRuntime {
    pub(crate) fn into_router_parts(self) -> TaskRouterParts {
        self.router_parts
    }
}

impl RouterRuntimeLifecycle {
    pub(crate) fn attach(self, router: Router) -> Router {
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

fn plan_startup_search_task_with_logging(
    config: &RuntimeConfig,
) -> std::io::Result<StartupSearchPlan> {
    match plan_startup_search_task(config) {
        Ok(startup_search_plan) => {
            emit_search_startup_event(config, startup_search_plan, None);
            Ok(startup_search_plan)
        }
        Err(error) => {
            emit_search_startup_event(config, failed_search_startup_plan(config), Some(&error));
            Err(error)
        }
    }
}

fn plan_startup_search_task(config: &RuntimeConfig) -> std::io::Result<StartupSearchPlan> {
    let writer_decision = config.writer_decision(WriterKind::SearchIndex);
    if !writer_decision.allows_write() {
        return Ok(StartupSearchPlan {
            writer_decision,
            lifecycle: "skipped_writer_blocked",
            startup_task: None,
        });
    }

    match decide_startup_lifecycle(config.lucene_data_directory.as_path()) {
        Ok(SearchStartupLifecycle::Ready) => Ok(StartupSearchPlan {
            writer_decision,
            lifecycle: "ready",
            startup_task: None,
        }),
        Ok(SearchStartupLifecycle::RebuildRequired) => {
            prepare_for_rebuild(config.lucene_data_directory.as_path()).map_err(|error| {
                std::io::Error::other(format!(
                    "search startup rebuild preparation failed: {error}"
                ))
            })?;
            Ok(StartupSearchPlan {
                writer_decision,
                lifecycle: "rebuild_required",
                startup_task: Some("RebuildIndex"),
            })
        }
        Err(error) => Err(std::io::Error::other(format!(
            "search startup lifecycle decision failed: {error}"
        ))),
    }
}

fn emit_search_startup_event(
    config: &RuntimeConfig,
    startup_search_plan: StartupSearchPlan,
    error: Option<&std::io::Error>,
) {
    let error_message = error.map_or_else(String::new, std::string::ToString::to_string);

    if error.is_some() {
        tracing::error!(
            event = "search_startup_decision",
            outcome = search_startup_outcome(startup_search_plan, error),
            search_writer_decision = search_writer_decision_label(startup_search_plan.writer_decision),
            search_writer_reason = search_writer_reason(startup_search_plan.writer_decision),
            search_startup_lifecycle = startup_search_plan.lifecycle,
            startup_task = startup_search_plan.startup_task.unwrap_or(""),
            lucene_data_directory = %config.lucene_data_directory.display(),
            error = error_message.as_str(),
            "Resolved startup search decision",
        );
    } else {
        tracing::info!(
            event = "search_startup_decision",
            outcome = search_startup_outcome(startup_search_plan, error),
            search_writer_decision = search_writer_decision_label(startup_search_plan.writer_decision),
            search_writer_reason = search_writer_reason(startup_search_plan.writer_decision),
            search_startup_lifecycle = startup_search_plan.lifecycle,
            startup_task = startup_search_plan.startup_task.unwrap_or(""),
            lucene_data_directory = %config.lucene_data_directory.display(),
            error = error_message.as_str(),
            "Resolved startup search decision",
        );
    }
}

fn search_writer_decision_label(decision: WriterDecision) -> &'static str {
    match decision {
        WriterDecision::Allowed => "allowed",
        WriterDecision::Isolated => "isolated",
        WriterDecision::Blocked { .. } => "blocked",
    }
}

fn search_writer_reason(decision: WriterDecision) -> &'static str {
    match decision {
        WriterDecision::Allowed | WriterDecision::Isolated => "",
        WriterDecision::Blocked { reason } => reason,
    }
}

fn failed_search_startup_plan(config: &RuntimeConfig) -> StartupSearchPlan {
    StartupSearchPlan {
        writer_decision: config.writer_decision(WriterKind::SearchIndex),
        lifecycle: "failed",
        startup_task: None,
    }
}

fn search_startup_outcome(
    startup_search_plan: StartupSearchPlan,
    error: Option<&std::io::Error>,
) -> &'static str {
    if error.is_some() {
        return "failed";
    }

    match startup_search_plan.lifecycle {
        "ready" => "ready",
        "rebuild_required" => "rebuild_required",
        "skipped_writer_blocked" => "skipped",
        _ => "ready",
    }
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

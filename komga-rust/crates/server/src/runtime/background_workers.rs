use axum::Extension;
use axum::Router;
use komga_application::task_processing::TaskQueueAdmin;
use komga_config::env_config::RuntimeConfig;
use komga_config::profile::RuntimeProfile;
use komga_config::writer_ownership::{WriterDecision, WriterKind};
use komga_infrastructure_base::DatabaseHandle;
use komga_infrastructure_jobs::{
    RuntimeBackgroundState, TaskRuntimeContext, prepare_task_queue, process_startup_library_scans,
};
use komga_infrastructure_search::{
    SearchStartupLifecycle, decide_startup_lifecycle, prepare_for_rebuild,
};
use komga_interfaces::state::RuntimeSseEventHub;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::watch;

pub(crate) enum TaskRuntimeMode {
    WorkersEnabled {
        shutdown_rx: Option<watch::Receiver<bool>>,
    },
    WorkersDisabled,
}

pub(crate) struct TaskRouterParts {
    pub(crate) http: HttpRuntimeParts,
    pub(crate) lifecycle: RouterRuntimeLifecycle,
}

pub(crate) struct HttpRuntimeParts {
    pub(crate) main_db: DatabaseHandle,
    pub(crate) tasks_db: DatabaseHandle,
    pub(crate) task_engine: Box<dyn TaskQueueAdmin>,
    pub(crate) runtime_events: Arc<RuntimeSseEventHub>,
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

pub(crate) async fn start_task_runtime(
    config: &RuntimeConfig,
    mode: TaskRuntimeMode,
) -> std::io::Result<TaskRouterParts> {
    let runtime_events = RuntimeSseEventHub::new();
    start_task_runtime_with_events(config, mode, runtime_events).await
}

pub(crate) async fn start_task_runtime_with_events(
    config: &RuntimeConfig,
    mode: TaskRuntimeMode,
    runtime_events: Arc<RuntimeSseEventHub>,
) -> std::io::Result<TaskRouterParts> {
    let startup_scan_runtime = if matches!(config.runtime_profile, RuntimeProfile::LiveLocaldb) {
        let runtime = crate::config::task_runtime_context(config, runtime_events.clone()).await;
        process_startup_library_scans(runtime.clone()).await;
        Some(runtime)
    } else {
        None
    };

    let startup_search_plan = plan_startup_search_task_with_logging(config)?;
    let runtime = match startup_scan_runtime {
        Some(runtime) => runtime,
        None => crate::config::task_runtime_context(config, runtime_events.clone()).await,
    };
    let background = prepare_task_queue(runtime.clone(), startup_search_plan.startup_task).await;
    let tasks_db =
        open_database_handle(runtime.worker().tasks_db_file().to_path_buf(), "tasks").await?;
    let worker_runtime_guard = match mode {
        TaskRuntimeMode::WorkersEnabled { shutdown_rx } => Some(spawn_runtime_workers(
            &background,
            runtime.clone(),
            shutdown_rx,
        )),
        TaskRuntimeMode::WorkersDisabled => None,
    };
    let task_engine = background.task_engine();

    Ok(TaskRouterParts {
        http: HttpRuntimeParts {
            main_db: runtime.job().database().main_db().clone(),
            tasks_db,
            task_engine,
            runtime_events,
        },
        lifecycle: RouterRuntimeLifecycle {
            worker_runtime_guard,
        },
    })
}

impl RouterRuntimeLifecycle {
    pub(crate) fn attach(self, router: Router) -> Router {
        match self.worker_runtime_guard {
            Some(worker_runtime_guard) => router.layer(Extension(worker_runtime_guard)),
            None => router,
        }
    }
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
    background: &RuntimeBackgroundState,
    runtime: TaskRuntimeContext,
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

    background.spawn_workers(runtime, Some(internal_shutdown_rx));

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

use anyhow::Context;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use super::{TaskRuntimeConfig, TaskRuntimeContext};
use komga_application::task_processing::{
    LibraryScanPipeline, LibraryScanScheduleState, ScanSchedulingTrigger,
    ScheduledLibraryScanBatch, TaskExecutionResult, TaskKind, TaskQueueAdmin, TaskQueueRecord,
    TaskRequest,
};
use tokio::runtime::Handle;
use tokio::sync::{Notify, mpsc, watch};
use tokio::time::interval;
use tracing::{Instrument, Span, error, info};

use super::execution_loop::BackgroundTaskExecutionLoop;
use super::execution_loop::SharedTaskQueue;
use super::execution_pool::TaskExecutionPoolHandle;
use crate::media::library_scan::SqliteFilesystemLibraryScanPipeline;
use crate::tasks::queue::{RuntimeTaskEngine, TaskQueueScheduler, process_available_serial};
pub type TaskQueueWakeSignal = Arc<Notify>;

pub struct RuntimeBackgroundState {
    task_queue: SharedTaskQueue,
    task_wakeup: TaskQueueWakeSignal,
    task_execution_pool: TaskExecutionPoolHandle,
}

impl RuntimeBackgroundState {
    pub fn task_engine(&self) -> Box<dyn TaskQueueAdmin> {
        Box::new(RuntimeTaskEngine::new(
            self.task_queue.clone(),
            self.task_execution_pool.clone(),
            self.task_wakeup.clone(),
        ))
    }

    pub fn spawn_workers(
        &self,
        runtime: TaskRuntimeContext,
        shutdown_rx: Option<watch::Receiver<bool>>,
    ) {
        spawn_runtime_workers(
            self.task_queue.clone(),
            self.task_execution_pool.clone(),
            runtime,
            self.task_wakeup.clone(),
            shutdown_rx,
        );
    }

    pub async fn queued_task_counts(&self) -> anyhow::Result<BTreeMap<String, usize>> {
        let task_queue = self.task_queue.lock().await;
        task_queue
            .count_by_simple_type()
            .await
            .map_err(anyhow::Error::from)
    }

    pub fn task_pool_size(&self) -> usize {
        self.task_execution_pool.desired_size()
    }
}

const WORKER_BOOTSTRAP_EVENT: &str = "worker_bootstrap";
const WORKER_SHUTDOWN_EVENT: &str = "worker_shutdown";
const STARTUP_LIBRARY_SCANS_COMPONENT: &str = "startup_library_scans";
const STARTUP_SEARCH_TASK_COMPONENT: &str = "startup_search_task";
const STARTUP_LIBRARY_SCAN_PROCESSING_COMPONENT: &str = "startup_library_scan_processing";
const TASK_QUEUE_RECOVERY_COMPONENT: &str = "task_queue_recovery";
const PERIODIC_LIBRARY_SCAN_WORKER: &str = "periodic_library_scan";
const BACKGROUND_TASK_WORKER: &str = "background_task";
const AUTHENTICATION_ACTIVITY_CLEANUP_WORKER: &str = "authentication_activity_cleanup";

fn log_and_skip_if_main_db_unowned(component: &str, runtime: &TaskRuntimeContext) -> bool {
    if runtime.job().database().owns_main_database() {
        return false;
    }

    log_runtime_bootstrap(
        component,
        "skipped",
        runtime,
        RuntimeLifecycleFields::default().with_skip_reason("main_database_not_owned"),
    );
    true
}

pub async fn prepare_task_queue(
    config: impl TaskRuntimeConfig,
    startup_search_task: Option<&'static str>,
) -> RuntimeBackgroundState {
    let runtime = config.task_runtime_context();
    let startup_task = startup_search_task.unwrap_or("");
    let wakeup = std::sync::Arc::new(Notify::new());
    let task_queue = TaskQueueScheduler::for_runtime_with_wakeup(
        runtime.clone(),
        "rust-runtime-http",
        wakeup.clone(),
    )
    .await;
    if runtime.worker().consumes_queue()
        && let Err(error) = task_queue.disown_all().await
    {
        let error_message = error.to_string();
        log_runtime_bootstrap(
            TASK_QUEUE_RECOVERY_COMPONENT,
            "failed",
            &runtime,
            RuntimeLifecycleFields::default().with_error(&error_message),
        );
        panic!("load persisted task queue: {error_message}");
    }

    if !log_and_skip_if_main_db_unowned(STARTUP_LIBRARY_SCANS_COMPONENT, &runtime) {
        log_runtime_bootstrap(
            STARTUP_LIBRARY_SCANS_COMPONENT,
            "started",
            &runtime,
            RuntimeLifecycleFields::default(),
        );
        let enqueued = bootstrap_startup_library_scans_inner(&task_queue, &runtime)
            .await
            .unwrap_or_else(|error| {
                let error_message = error.to_string();
                log_runtime_bootstrap(
                    STARTUP_LIBRARY_SCANS_COMPONENT,
                    "failed",
                    &runtime,
                    RuntimeLifecycleFields::default().with_error(&error_message),
                );
                panic!("bootstrap startup library scans: {error}");
            });

        if enqueued == 0 {
            log_runtime_bootstrap(
                STARTUP_LIBRARY_SCANS_COMPONENT,
                "skipped",
                &runtime,
                RuntimeLifecycleFields::default().with_skip_reason("no_startup_library_scans"),
            );
        } else {
            log_runtime_bootstrap(
                STARTUP_LIBRARY_SCANS_COMPONENT,
                "completed",
                &runtime,
                RuntimeLifecycleFields::default()
                    .with_enqueued(enqueued)
                    .with_scheduled_scans(enqueued),
            );
        }
    }

    if !runtime.worker().consumes_queue() {
        log_runtime_bootstrap(
            STARTUP_SEARCH_TASK_COMPONENT,
            "skipped",
            &runtime,
            RuntimeLifecycleFields::default().with_skip_reason("queue_consumption_disabled"),
        );
    } else if !runtime.job().search().owns_search_index() {
        log_runtime_bootstrap(
            STARTUP_SEARCH_TASK_COMPONENT,
            "skipped",
            &runtime,
            RuntimeLifecycleFields::default().with_skip_reason("search_index_not_owned"),
        );
    } else if startup_search_task.is_none() {
        log_runtime_bootstrap(
            STARTUP_SEARCH_TASK_COMPONENT,
            "skipped",
            &runtime,
            RuntimeLifecycleFields::default().with_skip_reason("startup_task_not_requested"),
        );
    } else {
        log_runtime_bootstrap(
            STARTUP_SEARCH_TASK_COMPONENT,
            "started",
            &runtime,
            RuntimeLifecycleFields::default().with_startup_task(startup_task),
        );
        match bootstrap_startup_search_task_inner(&task_queue, &runtime, startup_search_task).await
        {
            Ok(enqueued) => log_runtime_bootstrap(
                STARTUP_SEARCH_TASK_COMPONENT,
                "completed",
                &runtime,
                RuntimeLifecycleFields::default()
                    .with_startup_task(startup_task)
                    .with_enqueued(enqueued),
            ),
            Err(error) => {
                let error_message = error.to_string();
                log_runtime_bootstrap(
                    STARTUP_SEARCH_TASK_COMPONENT,
                    "failed",
                    &runtime,
                    RuntimeLifecycleFields::default()
                        .with_startup_task(startup_task)
                        .with_error(&error_message),
                );
                panic!("bootstrap startup search task: {error}");
            }
        }
    }

    RuntimeBackgroundState {
        task_queue: Arc::new(tokio::sync::Mutex::new(task_queue)),
        task_wakeup: wakeup,
        task_execution_pool: TaskExecutionPoolHandle::new(runtime.worker().task_pool_size()),
    }
}

fn spawn_runtime_workers(
    task_queue: SharedTaskQueue,
    task_execution_pool: TaskExecutionPoolHandle,
    runtime: TaskRuntimeContext,
    task_wakeup: TaskQueueWakeSignal,
    shutdown_rx: Option<watch::Receiver<bool>>,
) {
    spawn_periodic_library_scan_workers(
        task_queue.clone(),
        task_wakeup.clone(),
        runtime.clone(),
        shutdown_rx.clone(),
    );
    spawn_background_task_worker(
        task_queue,
        task_execution_pool,
        runtime.clone(),
        task_wakeup,
        shutdown_rx.clone(),
    );
    spawn_authentication_activity_cleanup_worker(runtime, shutdown_rx);
}

pub async fn process_startup_library_scans(config: impl TaskRuntimeConfig) {
    let runtime = config.task_runtime_context();
    if log_and_skip_if_main_db_unowned(STARTUP_LIBRARY_SCAN_PROCESSING_COMPONENT, &runtime) {
        return;
    }

    log_runtime_bootstrap(
        STARTUP_LIBRARY_SCAN_PROCESSING_COMPONENT,
        "started",
        &runtime,
        RuntimeLifecycleFields::default(),
    );

    match process_startup_library_scans_inner(&runtime).await {
        Ok(startup_scan_task_count) => log_runtime_bootstrap(
            STARTUP_LIBRARY_SCAN_PROCESSING_COMPONENT,
            "completed",
            &runtime,
            RuntimeLifecycleFields::default().with_processed(startup_scan_task_count),
        ),
        Err(error) => {
            let error_message = error.to_string();
            log_runtime_bootstrap(
                STARTUP_LIBRARY_SCAN_PROCESSING_COMPONENT,
                "failed",
                &runtime,
                RuntimeLifecycleFields::default().with_error(&error_message),
            );
            panic!("process startup library scans: {error_message}");
        }
    }
}

async fn process_startup_library_scans_inner(
    runtime: &TaskRuntimeContext,
) -> anyhow::Result<usize> {
    let startup_scan_batch = schedule_startup_library_scan_batch(
        runtime,
        "schedule startup library scans for processing",
    )
    .await?;
    if startup_scan_batch.is_empty() {
        log_runtime_bootstrap(
            STARTUP_LIBRARY_SCAN_PROCESSING_COMPONENT,
            "skipped",
            runtime,
            RuntimeLifecycleFields::default().with_skip_reason(
                if startup_scan_batch.configured_library_count == 0 {
                    "no_libraries"
                } else {
                    "no_startup_library_scans"
                },
            ),
        );
        return Ok(0);
    }

    let task_queue = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    let startup_scan_task_count = startup_scan_batch.len();
    task_queue
        .enqueue_batch(startup_scan_batch.into_task_batch())
        .await
        .map_err(anyhow::Error::from)?;
    process_available_serial(&task_queue, &runtime.job())
        .await
        .map_err(anyhow::Error::from)?;
    Ok(startup_scan_task_count)
}

fn spawn_periodic_library_scan_workers(
    task_queue: SharedTaskQueue,
    task_wakeup: TaskQueueWakeSignal,
    runtime: TaskRuntimeContext,
    shutdown_rx: Option<watch::Receiver<bool>>,
) {
    if !runtime.worker().consumes_queue() || !runtime.job().database().owns_main_database() {
        log_worker_event(
            PERIODIC_LIBRARY_SCAN_WORKER,
            "skipped",
            &runtime,
            RuntimeLifecycleFields::default().with_skip_reason(
                if !runtime.worker().consumes_queue() {
                    "queue_consumption_disabled"
                } else {
                    "main_database_not_owned"
                },
            ),
        );
        return;
    }

    let Some(handle) = current_runtime_handle_or_log_skip(PERIODIC_LIBRARY_SCAN_WORKER, &runtime)
    else {
        return;
    };

    let worker_span =
        tracing::info_span!("runtime_worker", worker_id = PERIODIC_LIBRARY_SCAN_WORKER);
    handle.spawn(
        async move {
            let _guard = WorkerLifecycleGuard::new(PERIODIC_LIBRARY_SCAN_WORKER, &runtime);
            let mut ticker = interval(Duration::from_secs(60));
            ticker.tick().await;
            let mut last_run_by_library: HashMap<String, tokio::time::Instant> = HashMap::new();
            let mut shutdown_rx = shutdown_rx;

            loop {
                if wait_for_tick_or_shutdown(&mut ticker, &mut shutdown_rx).await {
                    break;
                }

                let _ = run_periodic_library_scan_iteration(
                    task_queue.clone(),
                    Some(task_wakeup.clone()),
                    runtime.clone(),
                    &mut last_run_by_library,
                )
                .await;
            }
        }
        .instrument(worker_span.or_current()),
    );
}

pub async fn run_periodic_library_scan_iteration(
    task_queue: SharedTaskQueue,
    task_wakeup: Option<TaskQueueWakeSignal>,
    runtime: TaskRuntimeContext,
    last_run_by_library: &mut HashMap<String, tokio::time::Instant>,
) -> anyhow::Result<usize> {
    match run_periodic_library_scan_iteration_inner(
        task_queue,
        task_wakeup.as_ref(),
        &runtime,
        last_run_by_library,
    )
    .await
    {
        Ok(iteration) => {
            if iteration.due_libraries.is_empty() {
                return Ok(0);
            }

            log_worker_event(
                PERIODIC_LIBRARY_SCAN_WORKER,
                "completed",
                &runtime,
                RuntimeLifecycleFields::default()
                    .with_library_id(single_value_or_empty(&iteration.due_libraries))
                    .with_enqueued(iteration.due_libraries.len())
                    .with_processed(0),
            );
            Ok(iteration.enqueued)
        }
        Err(error) => {
            log_worker_event(
                PERIODIC_LIBRARY_SCAN_WORKER,
                "failed",
                &runtime,
                RuntimeLifecycleFields::default()
                    .with_library_id(single_value_or_empty(&error.due_libraries))
                    .with_enqueued(error.due_libraries.len())
                    .with_error(&error.message),
            );
            Err(anyhow::anyhow!(error.message))
        }
    }
}

fn spawn_background_task_worker(
    task_queue: SharedTaskQueue,
    task_execution_pool: TaskExecutionPoolHandle,
    runtime: TaskRuntimeContext,
    task_wakeup: TaskQueueWakeSignal,
    shutdown_rx: Option<watch::Receiver<bool>>,
) {
    if !runtime.worker().consumes_queue() {
        log_worker_event(
            BACKGROUND_TASK_WORKER,
            "skipped",
            &runtime,
            RuntimeLifecycleFields::default().with_skip_reason("queue_consumption_disabled"),
        );
        return;
    }

    let Some(handle) = current_runtime_handle_or_log_skip(BACKGROUND_TASK_WORKER, &runtime) else {
        return;
    };

    let worker_span = tracing::info_span!("runtime_worker", worker_id = BACKGROUND_TASK_WORKER);
    handle.spawn(
        async move {
            let _guard = WorkerLifecycleGuard::new(BACKGROUND_TASK_WORKER, &runtime);
            let mut result_rx = task_execution_pool.take_result_receiver().expect(
                "background task worker should own the dedicated execution result receiver",
            );
            let _ = run_background_task_iteration_with_pool(
                task_queue.clone(),
                &task_execution_pool,
                runtime.clone(),
                &mut result_rx,
            )
            .await;

            let mut ticker = interval(Duration::from_secs(300));
            ticker.tick().await;
            let task_wakeup = task_wakeup;
            let mut shutdown_rx = shutdown_rx;

            loop {
                if wait_for_background_task_wakeup_or_shutdown(
                    &mut ticker,
                    task_wakeup.as_ref(),
                    &mut shutdown_rx,
                )
                .await
                {
                    break;
                }
                let _ = run_background_task_iteration_with_pool(
                    task_queue.clone(),
                    &task_execution_pool,
                    runtime.clone(),
                    &mut result_rx,
                )
                .await;
            }
        }
        .instrument(worker_span.or_current()),
    );
}

pub async fn run_background_task_iteration(
    task_queue: SharedTaskQueue,
    runtime: TaskRuntimeContext,
) -> anyhow::Result<usize> {
    let task_execution_pool = TaskExecutionPoolHandle::new(runtime.worker().task_pool_size());
    let mut result_rx = task_execution_pool
        .take_result_receiver()
        .expect("one-shot background task iteration should own the result receiver");
    run_background_task_iteration_with_pool(
        task_queue,
        &task_execution_pool,
        runtime,
        &mut result_rx,
    )
    .await
}

async fn run_background_task_iteration_with_pool(
    task_queue: SharedTaskQueue,
    task_execution_pool: &TaskExecutionPoolHandle,
    runtime: TaskRuntimeContext,
    result_rx: &mut mpsc::UnboundedReceiver<TaskExecutionResult>,
) -> anyhow::Result<usize> {
    let queued_tasks = {
        let task_queue = task_queue.lock().await;
        task_queue
            .count_by_simple_type()
            .await
            .map_err(anyhow::Error::from)?
            .values()
            .sum::<usize>()
    };

    if queued_tasks == 0 {
        return Ok(0);
    }

    log_worker_event(
        BACKGROUND_TASK_WORKER,
        "running",
        &runtime,
        RuntimeLifecycleFields::default().with_queued_tasks(queued_tasks),
    );

    let processed = match BackgroundTaskExecutionLoop::new(
        &task_queue,
        task_execution_pool,
        &runtime,
        result_rx,
    )
    .drain()
    .await
    {
        Ok(processed) => processed,
        Err(error) => {
            let error_message = error.to_string();
            log_worker_event(
                BACKGROUND_TASK_WORKER,
                "failed",
                &runtime,
                RuntimeLifecycleFields::default()
                    .with_queued_tasks(queued_tasks)
                    .with_error(&error_message),
            );
            return Err(anyhow::anyhow!(error_message));
        }
    };

    log_worker_event(
        BACKGROUND_TASK_WORKER,
        "completed",
        &runtime,
        RuntimeLifecycleFields::default()
            .with_queued_tasks(queued_tasks)
            .with_processed(processed),
    );
    Ok(processed)
}

fn spawn_authentication_activity_cleanup_worker(
    runtime: TaskRuntimeContext,
    shutdown_rx: Option<watch::Receiver<bool>>,
) {
    if !runtime.job().database().owns_main_database() {
        log_worker_event(
            AUTHENTICATION_ACTIVITY_CLEANUP_WORKER,
            "skipped",
            &runtime,
            RuntimeLifecycleFields::default().with_skip_reason("main_database_not_owned"),
        );
        return;
    }

    let Some(handle) =
        current_runtime_handle_or_log_skip(AUTHENTICATION_ACTIVITY_CLEANUP_WORKER, &runtime)
    else {
        return;
    };

    let worker_span = tracing::info_span!(
        "runtime_worker",
        worker_id = AUTHENTICATION_ACTIVITY_CLEANUP_WORKER
    );
    handle.spawn(
        async move {
            let _guard =
                WorkerLifecycleGuard::new(AUTHENTICATION_ACTIVITY_CLEANUP_WORKER, &runtime);
            let mut ticker = interval(Duration::from_secs(86_400));
            ticker.tick().await;
            let mut shutdown_rx = shutdown_rx;

            loop {
                if wait_for_tick_or_shutdown(&mut ticker, &mut shutdown_rx).await {
                    break;
                }
                let _ = cleanup_authentication_activity_once(&runtime).await;
            }
        }
        .instrument(worker_span.or_current()),
    );
}

async fn wait_for_tick_or_shutdown(
    ticker: &mut tokio::time::Interval,
    shutdown_rx: &mut Option<watch::Receiver<bool>>,
) -> bool {
    match shutdown_rx {
        Some(shutdown_rx) => {
            tokio::select! {
                _ = ticker.tick() => false,
                _ = wait_for_worker_shutdown(shutdown_rx) => true,
            }
        }
        None => {
            ticker.tick().await;
            false
        }
    }
}

async fn wait_for_background_task_wakeup_or_shutdown(
    ticker: &mut tokio::time::Interval,
    task_wakeup: &Notify,
    shutdown_rx: &mut Option<watch::Receiver<bool>>,
) -> bool {
    match shutdown_rx {
        Some(shutdown_rx) => {
            tokio::select! {
                _ = ticker.tick() => false,
                _ = task_wakeup.notified() => false,
                _ = wait_for_worker_shutdown(shutdown_rx) => true,
            }
        }
        None => {
            tokio::select! {
                _ = ticker.tick() => false,
                _ = task_wakeup.notified() => false,
            }
        }
    }
}

async fn wait_for_worker_shutdown(shutdown_rx: &mut watch::Receiver<bool>) {
    loop {
        if *shutdown_rx.borrow() {
            break;
        }
        if shutdown_rx.changed().await.is_err() {
            break;
        }
    }
}

pub async fn cleanup_authentication_activity_once(
    runtime: &TaskRuntimeContext,
) -> anyhow::Result<()> {
    if !runtime.job().database().owns_main_database() {
        log_worker_event(
            AUTHENTICATION_ACTIVITY_CLEANUP_WORKER,
            "skipped",
            runtime,
            RuntimeLifecycleFields::default().with_skip_reason("main_database_not_owned"),
        );
        return Ok(());
    }

    log_worker_event(
        AUTHENTICATION_ACTIVITY_CLEANUP_WORKER,
        "running",
        runtime,
        RuntimeLifecycleFields::default(),
    );

    if runtime.job().database().main_db().database_file().is_dir() {
        let error_message = format!(
            "failed to open sqlite database at {}: path is a directory",
            runtime.job().database().main_db().database_file().display()
        );
        log_worker_event(
            AUTHENTICATION_ACTIVITY_CLEANUP_WORKER,
            "failed",
            runtime,
            RuntimeLifecycleFields::default().with_error(&error_message),
        );
        return Err(anyhow::anyhow!(error_message));
    }

    crate::identity::users::authentication::persisted_cleanup_authentication_activity(
        runtime.job().database().main_db().write_pool(),
    )
    .await
    .map_err(|error| {
        let context = format!(
            "failed to clean up authentication activity using {}",
            runtime.job().database().main_db().database_file().display(),
        );
        let error_message = format!("{context}: {error}");
        log_worker_event(
            AUTHENTICATION_ACTIVITY_CLEANUP_WORKER,
            "failed",
            runtime,
            RuntimeLifecycleFields::default().with_error(&error_message),
        );
        anyhow::anyhow!(error).context(context)
    })?;

    log_worker_event(
        AUTHENTICATION_ACTIVITY_CLEANUP_WORKER,
        "completed",
        runtime,
        RuntimeLifecycleFields::default(),
    );
    Ok(())
}

fn startup_task_record(task_name: &str) -> TaskQueueRecord {
    match TaskKind::parse(task_name) {
        Ok(kind) => TaskRequest::new(kind).priority(1_000).into_queue_record(),
        Err(_) => TaskQueueRecord::new(task_name.to_string(), 1_000, None),
    }
}

async fn bootstrap_startup_search_task_inner(
    task_queue: &TaskQueueScheduler,
    runtime: &TaskRuntimeContext,
    startup_search_task: Option<&'static str>,
) -> anyhow::Result<usize> {
    if !runtime.job().search().owns_search_index() {
        return Ok(0);
    }

    let Some(task_name) = startup_search_task else {
        return Ok(0);
    };

    task_queue
        .enqueue(startup_task_record(task_name))
        .await
        .map_err(anyhow::Error::from)?;
    Ok(1)
}

fn current_runtime_handle_or_log_skip(
    worker_id: &str,
    runtime: &TaskRuntimeContext,
) -> Option<Handle> {
    let Ok(handle) = Handle::try_current() else {
        log_worker_event(
            worker_id,
            "skipped",
            runtime,
            RuntimeLifecycleFields::default().with_skip_reason("runtime_handle_unavailable"),
        );
        return None;
    };

    Some(handle)
}

struct PeriodicLibraryScanIteration {
    enqueued: usize,
    due_libraries: Vec<String>,
}

struct PeriodicLibraryScanIterationError {
    message: String,
    due_libraries: Vec<String>,
}

async fn run_periodic_library_scan_iteration_inner(
    task_queue: SharedTaskQueue,
    task_wakeup: Option<&TaskQueueWakeSignal>,
    runtime: &TaskRuntimeContext,
    last_run_by_library: &mut HashMap<String, tokio::time::Instant>,
) -> Result<PeriodicLibraryScanIteration, PeriodicLibraryScanIterationError> {
    sync_periodic_library_scan_state(runtime, last_run_by_library)
        .await
        .map_err(|error| PeriodicLibraryScanIterationError {
            message: format!("{error:#}"),
            due_libraries: Vec::new(),
        })?;
    let due_tasks = schedule_periodic_library_scan_batch(runtime, last_run_by_library)
        .await
        .map_err(|error| PeriodicLibraryScanIterationError {
            message: format!("{error:#}"),
            due_libraries: Vec::new(),
        })?
        .into_scheduled_tasks();
    let due_libraries = due_tasks
        .iter()
        .map(|scheduled| scheduled.library_id.clone())
        .collect::<Vec<_>>();

    if due_libraries.is_empty() {
        return Ok(PeriodicLibraryScanIteration {
            enqueued: 0,
            due_libraries,
        });
    }

    log_worker_event(
        PERIODIC_LIBRARY_SCAN_WORKER,
        "running",
        runtime,
        RuntimeLifecycleFields::default()
            .with_library_id(single_value_or_empty(&due_libraries))
            .with_enqueued(due_libraries.len()),
    );

    for scheduled in due_tasks {
        {
            let queue = task_queue.lock().await;
            queue.enqueue(scheduled.task).await.map_err(|error| {
                PeriodicLibraryScanIterationError {
                    message: error.to_string(),
                    due_libraries: due_libraries.clone(),
                }
            })?;
        }
        if let Some(next_due) = last_run_by_library.get_mut(&scheduled.library_id) {
            *next_due = tokio::time::Instant::now();
        }
    }
    if let Some(task_wakeup) = task_wakeup {
        task_wakeup.notify_one();
    }

    Ok(PeriodicLibraryScanIteration {
        enqueued: due_libraries.len(),
        due_libraries,
    })
}

async fn bootstrap_startup_library_scans_inner(
    task_queue: &TaskQueueScheduler,
    runtime: &TaskRuntimeContext,
) -> anyhow::Result<usize> {
    if !runtime.job().database().owns_main_database() {
        return Ok(0);
    }

    let startup_batch =
        schedule_startup_library_scan_batch(runtime, "schedule startup library scans").await?;
    let enqueued = startup_batch.len();
    task_queue
        .enqueue_batch(startup_batch.into_task_batch())
        .await
        .map_err(anyhow::Error::from)?;

    Ok(enqueued)
}

async fn schedule_startup_library_scan_batch(
    runtime: &TaskRuntimeContext,
    action: &str,
) -> anyhow::Result<ScheduledLibraryScanBatch> {
    SqliteFilesystemLibraryScanPipeline::for_runtime(&runtime.job())
        .await?
        .schedule(
            ScanSchedulingTrigger::Startup,
            &LibraryScanScheduleState::default(),
        )
        .await
        .map_err(|error| anyhow::anyhow!(error).context(action.to_string()))
}

async fn schedule_periodic_library_scan_batch(
    runtime: &TaskRuntimeContext,
    last_run_by_library: &HashMap<String, tokio::time::Instant>,
) -> anyhow::Result<ScheduledLibraryScanBatch> {
    SqliteFilesystemLibraryScanPipeline::for_runtime(&runtime.job())
        .await?
        .schedule(
            ScanSchedulingTrigger::Tick,
            &LibraryScanScheduleState {
                elapsed_since_last_run_by_library: last_run_by_library
                    .iter()
                    .map(|(library_id, last_run)| (library_id.clone(), last_run.elapsed()))
                    .collect(),
            },
        )
        .await
        .context("schedule periodic library scans")
}

async fn sync_periodic_library_scan_state(
    runtime: &TaskRuntimeContext,
    last_run_by_library: &mut HashMap<String, tokio::time::Instant>,
) -> anyhow::Result<()> {
    SqliteFilesystemLibraryScanPipeline::for_runtime(&runtime.job())
        .await?
        .sync_periodic_library_scan_state(last_run_by_library)
        .await
        .context("build periodic library scan state")
}

fn single_value_or_empty(values: &[String]) -> &str {
    if values.len() == 1 {
        values[0].as_str()
    } else {
        ""
    }
}

#[derive(Default)]
struct RuntimeLifecycleFields<'a> {
    skip_reason: &'a str,
    error: &'a str,
    startup_task: &'a str,
    library_id: &'a str,
    enqueued: usize,
    processed: usize,
    scheduled_scans: usize,
    queued_tasks: usize,
}

impl<'a> RuntimeLifecycleFields<'a> {
    fn with_skip_reason(mut self, skip_reason: &'a str) -> Self {
        self.skip_reason = skip_reason;
        self
    }

    fn with_error(mut self, error: &'a str) -> Self {
        self.error = error;
        self
    }

    fn with_startup_task(mut self, startup_task: &'a str) -> Self {
        self.startup_task = startup_task;
        self
    }

    fn with_library_id(mut self, library_id: &'a str) -> Self {
        self.library_id = library_id;
        self
    }

    fn with_enqueued(mut self, enqueued: usize) -> Self {
        self.enqueued = enqueued;
        self
    }

    fn with_processed(mut self, processed: usize) -> Self {
        self.processed = processed;
        self
    }

    fn with_scheduled_scans(mut self, scheduled_scans: usize) -> Self {
        self.scheduled_scans = scheduled_scans;
        self
    }

    fn with_queued_tasks(mut self, queued_tasks: usize) -> Self {
        self.queued_tasks = queued_tasks;
        self
    }
}

fn log_runtime_bootstrap(
    component: &str,
    outcome: &str,
    runtime: &TaskRuntimeContext,
    fields: RuntimeLifecycleFields<'_>,
) {
    if outcome == "failed" {
        error!(
            event = WORKER_BOOTSTRAP_EVENT,
            component,
            outcome,
            consumes_queue = runtime.worker().consumes_queue(),
            owns_main_database = runtime.job().database().owns_main_database(),
            owns_search_index = runtime.job().search().owns_search_index(),
            skip_reason = fields.skip_reason,
            startup_task = fields.startup_task,
            library_id = fields.library_id,
            enqueued = fields.enqueued,
            processed = fields.processed,
            scheduled_scans = fields.scheduled_scans,
            queued_tasks = fields.queued_tasks,
            error = fields.error,
            "runtime bootstrap lifecycle"
        );
    } else {
        info!(
            event = WORKER_BOOTSTRAP_EVENT,
            component,
            outcome,
            consumes_queue = runtime.worker().consumes_queue(),
            owns_main_database = runtime.job().database().owns_main_database(),
            owns_search_index = runtime.job().search().owns_search_index(),
            skip_reason = fields.skip_reason,
            startup_task = fields.startup_task,
            library_id = fields.library_id,
            enqueued = fields.enqueued,
            processed = fields.processed,
            scheduled_scans = fields.scheduled_scans,
            queued_tasks = fields.queued_tasks,
            error = fields.error,
            "runtime bootstrap lifecycle"
        );
    }
}

fn log_worker_event(
    worker_id: &str,
    outcome: &str,
    runtime: &TaskRuntimeContext,
    fields: RuntimeLifecycleFields<'_>,
) {
    let event = if outcome == "shutdown" {
        WORKER_SHUTDOWN_EVENT
    } else {
        WORKER_BOOTSTRAP_EVENT
    };

    if outcome == "failed" {
        error!(
            event,
            worker_id,
            outcome,
            consumes_queue = runtime.worker().consumes_queue(),
            owns_main_database = runtime.job().database().owns_main_database(),
            owns_search_index = runtime.job().search().owns_search_index(),
            in_span = Span::current().id().is_some(),
            skip_reason = fields.skip_reason,
            library_id = fields.library_id,
            enqueued = fields.enqueued,
            processed = fields.processed,
            queued_tasks = fields.queued_tasks,
            error = fields.error,
            "runtime worker lifecycle"
        );
    } else {
        info!(
            event,
            worker_id,
            outcome,
            consumes_queue = runtime.worker().consumes_queue(),
            owns_main_database = runtime.job().database().owns_main_database(),
            owns_search_index = runtime.job().search().owns_search_index(),
            in_span = Span::current().id().is_some(),
            skip_reason = fields.skip_reason,
            library_id = fields.library_id,
            enqueued = fields.enqueued,
            processed = fields.processed,
            queued_tasks = fields.queued_tasks,
            error = fields.error,
            "runtime worker lifecycle"
        );
    }
}

struct WorkerLifecycleGuard {
    worker: &'static str,
    runtime: TaskRuntimeContext,
}

impl WorkerLifecycleGuard {
    fn new(worker: &'static str, runtime: &TaskRuntimeContext) -> Self {
        log_worker_event(
            worker,
            "started",
            runtime,
            RuntimeLifecycleFields::default(),
        );
        Self {
            worker,
            runtime: runtime.clone(),
        }
    }
}

impl Drop for WorkerLifecycleGuard {
    fn drop(&mut self) {
        log_worker_event(
            self.worker,
            "shutdown",
            &self.runtime,
            RuntimeLifecycleFields::default(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::runtime::TaskExecutionPoolHandle;
    use std::collections::BTreeSet;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::{Barrier, Mutex as AsyncMutex, watch};

    use crate::persistence::DatabaseHandle;
    use crate::persistence::sqlite::{
        connect_task_pool, connect_task_write_pool, default_read_max_connections,
    };

    async fn runtime_context() -> TaskRuntimeContext {
        let root = std::env::temp_dir().join(format!(
            "komga-task-execution-pool-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("test temp dir should be created");
        let db_path = root.join("main.sqlite");
        let task_write_pool = connect_task_write_pool(&db_path)
            .await
            .expect("test private write pool should open");
        let task_read_pool = connect_task_pool(&db_path, default_read_max_connections())
            .await
            .expect("test private read pool should open");
        TaskRuntimeContext::new(
            DatabaseHandle::file_backed(db_path)
                .await
                .expect("test db should open"),
            root.join("tasks.sqlite"),
            root.join("lucene"),
            true,
            1,
            task_write_pool,
            task_read_pool,
        )
    }

    #[tokio::test]
    async fn task_execution_pool_resize_allows_parallel_fake_tasks_without_restart() {
        let runtime = runtime_context().await;
        let started = Arc::new(Barrier::new(3));
        let (release_tx, release_rx) = watch::channel(false);
        let worker_threads = Arc::new(AsyncMutex::new(Vec::new()));
        let pool = TaskExecutionPoolHandle::new_for_test(1, {
            let started = started.clone();
            let worker_threads = worker_threads.clone();
            move |_runtime, _task| {
                let started = started.clone();
                let mut release_rx = release_rx.clone();
                let worker_threads = worker_threads.clone();
                async move {
                    worker_threads.lock().await.push(
                        std::thread::current()
                            .name()
                            .unwrap_or("<unnamed>")
                            .to_string(),
                    );
                    started.wait().await;
                    loop {
                        if *release_rx.borrow() {
                            break;
                        }
                        release_rx
                            .changed()
                            .await
                            .expect("fake task release signal should remain open");
                    }
                    Ok(komga_application::task_processing::TaskExecutionOutcome::completed())
                }
            }
        });
        let mut result_rx = pool
            .take_result_receiver()
            .expect("task execution pool test should expose a single result receiver");

        pool.submit(
            TaskQueueRecord::new("TEST_TASK:1", 0, None),
            runtime.clone(),
        )
        .expect("first fake task should be submitted");
        tokio::time::sleep(Duration::from_millis(25)).await;

        pool.resize(2);
        pool.submit(TaskQueueRecord::new("TEST_TASK:2", 0, None), runtime)
            .expect("second fake task should be submitted after resize");

        tokio::time::timeout(Duration::from_secs(1), started.wait())
            .await
            .expect("resized pool should start the second fake task without restart");

        release_tx
            .send(true)
            .expect("fake task release signal should send");

        let _ = tokio::time::timeout(Duration::from_secs(1), result_rx.recv())
            .await
            .expect("first fake task result should arrive")
            .expect("first fake task result should exist");
        let _ = tokio::time::timeout(Duration::from_secs(1), result_rx.recv())
            .await
            .expect("second fake task result should arrive")
            .expect("second fake task result should exist");

        let worker_threads = worker_threads.lock().await.clone();
        assert_eq!(worker_threads.len(), 2);
        assert!(
            worker_threads
                .iter()
                .all(|name| name.starts_with("komga-task-worker-")),
            "fake tasks should run on dedicated worker threads: {worker_threads:?}",
        );
        assert_eq!(
            worker_threads
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>()
                .len(),
            2,
            "resize 1 -> 2 should let two distinct worker threads enter execution",
        );
    }
}

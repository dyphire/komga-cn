use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use komga_application::task_processing::{
    DefaultLibraryTaskEmitter, LibraryScanPipeline, LibraryScanProfile, LibraryScanScheduleState,
    LibraryTaskBatch, ScanSchedulingTrigger, TaskRuntimeConfig, TaskRuntimeContext,
};
use serde_json::Value;
use tokio::runtime::Handle;
use tokio::sync::{Notify, watch};
use tokio::time::interval;
use tracing::{Instrument, Span, error, info};

use super::library_scan_pipeline::SqliteFilesystemLibraryScanPipeline;
use super::task_protocol::runtime_startup_task;
use super::{TaskQueueRecord, TaskQueueScheduler};
use crate::tasks::library_scan_profiles::load_persisted_library_scan_profiles;

pub type SharedTaskQueue = Arc<Mutex<TaskQueueScheduler>>;
pub type TaskQueueWakeSignal = Arc<Notify>;

pub struct RuntimeBackgroundState {
    pub task_queue: SharedTaskQueue,
    pub task_wakeup: TaskQueueWakeSignal,
}

const WORKER_BOOTSTRAP_EVENT: &str = "worker_bootstrap";
const WORKER_SHUTDOWN_EVENT: &str = "worker_shutdown";
const STARTUP_LIBRARY_SCANS_COMPONENT: &str = "startup_library_scans";
const STARTUP_SEARCH_TASK_COMPONENT: &str = "startup_search_task";
const STARTUP_LIBRARY_SCAN_PROCESSING_COMPONENT: &str = "startup_library_scan_processing";
const PERIODIC_LIBRARY_SCAN_WORKER: &str = "periodic_library_scan";
const BACKGROUND_TASK_WORKER: &str = "background_task";
const AUTHENTICATION_ACTIVITY_CLEANUP_WORKER: &str = "authentication_activity_cleanup";

fn log_and_skip_if_main_db_unowned(component: &str, runtime: &TaskRuntimeContext) -> bool {
    if runtime.owns_main_database {
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

pub fn prepare_task_queue(
    config: impl TaskRuntimeConfig,
    startup_search_task: Option<&'static str>,
) -> RuntimeBackgroundState {
    let runtime = config.task_runtime_context();
    let startup_task = startup_search_task.unwrap_or("");
    let mut task_queue = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-runtime-http");
    if runtime.consumes_queue {
        let _ = task_queue.disown_all();
    }

    if !log_and_skip_if_main_db_unowned(STARTUP_LIBRARY_SCANS_COMPONENT, &runtime) {
        log_runtime_bootstrap(
            STARTUP_LIBRARY_SCANS_COMPONENT,
            "started",
            &runtime,
            RuntimeLifecycleFields::default(),
        );
        let enqueued = bootstrap_startup_library_scans_inner(&mut task_queue, &runtime)
            .unwrap_or_else(|error| {
                log_runtime_bootstrap(
                    STARTUP_LIBRARY_SCANS_COMPONENT,
                    "failed",
                    &runtime,
                    RuntimeLifecycleFields::default().with_error(&error),
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

    if !runtime.consumes_queue {
        log_runtime_bootstrap(
            STARTUP_SEARCH_TASK_COMPONENT,
            "skipped",
            &runtime,
            RuntimeLifecycleFields::default().with_skip_reason("queue_consumption_disabled"),
        );
    } else if !runtime.owns_search_index {
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
        match bootstrap_startup_search_task_inner(&mut task_queue, &runtime, startup_search_task) {
            Ok(enqueued) => log_runtime_bootstrap(
                STARTUP_SEARCH_TASK_COMPONENT,
                "completed",
                &runtime,
                RuntimeLifecycleFields::default()
                    .with_startup_task(startup_task)
                    .with_enqueued(enqueued),
            ),
            Err(error) => {
                log_runtime_bootstrap(
                    STARTUP_SEARCH_TASK_COMPONENT,
                    "failed",
                    &runtime,
                    RuntimeLifecycleFields::default()
                        .with_startup_task(startup_task)
                        .with_error(&error),
                );
                panic!("bootstrap startup search task: {error}");
            }
        }
    }

    RuntimeBackgroundState {
        task_queue: Arc::new(Mutex::new(task_queue)),
        task_wakeup: Arc::new(Notify::new()),
    }
}

pub fn spawn_runtime_workers(
    task_queue: SharedTaskQueue,
    runtime: TaskRuntimeContext,
    task_wakeup: TaskQueueWakeSignal,
    shutdown_rx: Option<watch::Receiver<bool>>,
) {
    spawn_periodic_library_scan_workers(task_queue.clone(), runtime.clone(), shutdown_rx.clone());
    spawn_background_task_worker(
        task_queue,
        runtime.clone(),
        task_wakeup,
        shutdown_rx.clone(),
    );
    spawn_authentication_activity_cleanup_worker(runtime, shutdown_rx);
}

pub fn bootstrap_startup_search_task(
    task_queue: &mut TaskQueueScheduler,
    runtime: &TaskRuntimeContext,
    startup_search_task: Option<&'static str>,
) {
    bootstrap_startup_search_task_inner(task_queue, runtime, startup_search_task)
        .unwrap_or_else(|error| panic!("bootstrap startup search task: {error}"));
}

pub fn process_startup_library_scans(config: impl TaskRuntimeConfig) {
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

    let startup_scan_batch = schedule_startup_library_scan_batch(
        &runtime,
        "schedule startup library scans for processing",
    )
    .unwrap_or_else(|error| {
        log_runtime_bootstrap(
            STARTUP_LIBRARY_SCAN_PROCESSING_COMPONENT,
            "failed",
            &runtime,
            RuntimeLifecycleFields::default().with_error(&error),
        );
        panic!("process startup library scans: {error}");
    });
    if startup_scan_batch.is_empty() {
        let profiles = load_scan_profiles(
            runtime.database_file.as_path(),
            "load startup library scan profiles for processing skip boundary",
        )
        .unwrap_or_else(|error| {
            log_runtime_bootstrap(
                STARTUP_LIBRARY_SCAN_PROCESSING_COMPONENT,
                "failed",
                &runtime,
                RuntimeLifecycleFields::default().with_error(&error),
            );
            panic!("process startup library scans: {error}");
        });

        log_runtime_bootstrap(
            STARTUP_LIBRARY_SCAN_PROCESSING_COMPONENT,
            "skipped",
            &runtime,
            RuntimeLifecycleFields::default().with_skip_reason(if profiles.is_empty() {
                "no_libraries"
            } else {
                "no_startup_library_scans"
            }),
        );
        return;
    }

    let mut task_queue = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    let startup_scan_tasks = startup_scan_batch.into_queue_records();
    let startup_scan_task_count = startup_scan_tasks.len();
    for task in startup_scan_tasks {
        task_queue.enqueue(task);
    }
    match task_queue.process_available(&runtime) {
        Ok(_) => log_runtime_bootstrap(
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

fn spawn_periodic_library_scan_workers(
    task_queue: SharedTaskQueue,
    runtime: TaskRuntimeContext,
    shutdown_rx: Option<watch::Receiver<bool>>,
) {
    if !runtime.consumes_queue || !runtime.owns_main_database {
        log_worker_event(
            PERIODIC_LIBRARY_SCAN_WORKER,
            "skipped",
            &runtime,
            RuntimeLifecycleFields::default().with_skip_reason(if !runtime.consumes_queue {
                "queue_consumption_disabled"
            } else {
                "main_database_not_owned"
            }),
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
                    runtime.clone(),
                    &mut last_run_by_library,
                );
            }
        }
        .instrument(worker_span.or_current()),
    );
}

pub fn run_periodic_library_scan_iteration(
    task_queue: SharedTaskQueue,
    runtime: TaskRuntimeContext,
    last_run_by_library: &mut HashMap<String, tokio::time::Instant>,
) -> Result<usize, String> {
    match run_periodic_library_scan_iteration_inner(task_queue, &runtime, last_run_by_library) {
        Ok((scheduler_processed, due_libraries)) => {
            if due_libraries.is_empty() {
                return Ok(0);
            }

            log_worker_event(
                PERIODIC_LIBRARY_SCAN_WORKER,
                "completed",
                &runtime,
                RuntimeLifecycleFields::default()
                    .with_library_id(single_value_or_empty(&due_libraries))
                    .with_enqueued(due_libraries.len())
                    .with_processed(scheduler_processed),
            );
            Ok(scheduler_processed)
        }
        Err((error, due_libraries)) => {
            log_worker_event(
                PERIODIC_LIBRARY_SCAN_WORKER,
                "failed",
                &runtime,
                RuntimeLifecycleFields::default()
                    .with_library_id(single_value_or_empty(&due_libraries))
                    .with_enqueued(due_libraries.len())
                    .with_error(&error),
            );
            Err(error)
        }
    }
}

fn spawn_background_task_worker(
    task_queue: SharedTaskQueue,
    runtime: TaskRuntimeContext,
    task_wakeup: TaskQueueWakeSignal,
    shutdown_rx: Option<watch::Receiver<bool>>,
) {
    if !runtime.consumes_queue {
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
            let startup_task_queue = task_queue.clone();
            let startup_runtime = runtime.clone();
            let _ = tokio::task::spawn_blocking(move || {
                run_background_task_iteration(startup_task_queue, startup_runtime)
            })
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
                let iteration_task_queue = task_queue.clone();
                let iteration_runtime = runtime.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    run_background_task_iteration(iteration_task_queue, iteration_runtime)
                })
                .await;
            }
        }
        .instrument(worker_span.or_current()),
    );
}

pub fn run_background_task_iteration(
    task_queue: SharedTaskQueue,
    runtime: TaskRuntimeContext,
) -> Result<usize, String> {
    let queued_tasks = {
        let task_queue = task_queue
            .lock()
            .expect("task queue state lock should not be poisoned");
        task_queue.count_by_simple_type().values().sum::<usize>()
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

    let processed = {
        let mut task_queue = task_queue
            .lock()
            .expect("task queue state lock should not be poisoned");
        match task_queue.process_available(&runtime) {
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
                return Err(error_message);
            }
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
    if !runtime.owns_main_database {
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
) -> Result<(), String> {
    if !runtime.owns_main_database {
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

    if runtime.database_file.is_dir() {
        let error_message = format!(
            "failed to open sqlite database at {}: path is a directory",
            runtime.database_file.display()
        );
        log_worker_event(
            AUTHENTICATION_ACTIVITY_CLEANUP_WORKER,
            "failed",
            runtime,
            RuntimeLifecycleFields::default().with_error(&error_message),
        );
        return Err(error_message);
    }

    crate::auth::runtime_identity_access::persisted_cleanup_authentication_activity(
        runtime.database_file.as_path(),
    )
        .await
        .ok_or_else(|| {
            let error_message = format!(
                "failed to clean up authentication activity using {}",
                runtime.database_file.display()
            );
            log_worker_event(
                AUTHENTICATION_ACTIVITY_CLEANUP_WORKER,
                "failed",
                runtime,
                RuntimeLifecycleFields::default().with_error(&error_message),
            );
            error_message
        })?;

    log_worker_event(
        AUTHENTICATION_ACTIVITY_CLEANUP_WORKER,
        "completed",
        runtime,
        RuntimeLifecycleFields::default(),
    );
    Ok(())
}

fn bootstrap_startup_search_task_inner(
    task_queue: &mut TaskQueueScheduler,
    runtime: &TaskRuntimeContext,
    startup_search_task: Option<&'static str>,
) -> Result<usize, String> {
    if !runtime.owns_search_index {
        return Ok(0);
    }

    let Some(task_name) = startup_search_task else {
        return Ok(0);
    };

    task_queue.enqueue(runtime_startup_task(task_name));
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

fn run_periodic_library_scan_iteration_inner(
    task_queue: SharedTaskQueue,
    runtime: &TaskRuntimeContext,
    last_run_by_library: &mut HashMap<String, tokio::time::Instant>,
) -> Result<(usize, Vec<String>), (String, Vec<String>)> {
    sync_periodic_library_scan_state(runtime, last_run_by_library)
        .map_err(|error| (error, Vec::new()))?;
    let due_tasks = schedule_periodic_library_scan_batch(runtime, last_run_by_library)
        .and_then(periodic_library_scan_tasks)
        .map_err(|error| (error, Vec::new()))?;
    let due_libraries = due_tasks
        .iter()
        .map(|(library_id, _)| library_id.clone())
        .collect::<Vec<_>>();

    if due_libraries.is_empty() {
        return Ok((0, due_libraries));
    }

    log_worker_event(
        PERIODIC_LIBRARY_SCAN_WORKER,
        "running",
        runtime,
        RuntimeLifecycleFields::default()
            .with_library_id(single_value_or_empty(&due_libraries))
            .with_enqueued(due_libraries.len()),
    );

    let mut scheduler_processed = 0;
    for (library_id, task) in due_tasks {
        let mut queue = task_queue
            .lock()
            .expect("task queue state lock should not be poisoned");
        queue.enqueue(task);
        scheduler_processed += queue
            .process_available(runtime)
            .map_err(|error| (error.to_string(), due_libraries.clone()))?;
        if let Some(next_due) = last_run_by_library.get_mut(&library_id) {
            *next_due = tokio::time::Instant::now();
        }
    }

    Ok((scheduler_processed, due_libraries))
}

fn bootstrap_startup_library_scans_inner(
    task_queue: &mut TaskQueueScheduler,
    runtime: &TaskRuntimeContext,
) -> Result<usize, String> {
    if !runtime.owns_main_database {
        return Ok(0);
    }

    let profiles = load_scan_profiles(
        runtime.database_file.as_path(),
        "load startup library scan profiles",
    )?;
    if profiles.is_empty() {
        return Ok(0);
    }

    let startup_tasks =
        schedule_startup_library_scan_batch(runtime, "schedule startup library scans")?
            .into_queue_records();
    let enqueued = startup_tasks.len();
    for task in startup_tasks {
        task_queue.enqueue(task);
    }

    Ok(enqueued)
}

fn load_scan_profiles(
    database_file: &std::path::Path,
    action: &str,
) -> Result<Vec<LibraryScanProfile>, String> {
    load_persisted_library_scan_profiles(database_file)
        .map_err(|error| format!("{action}: {error}"))
        .map(|profiles| {
            profiles
                .into_iter()
                .map(|profile| LibraryScanProfile {
                    library_id: profile.library_id,
                    scan_startup: profile.scan_startup,
                    scan_interval: profile.scan_interval,
                })
                .collect::<Vec<_>>()
        })
}

fn schedule_startup_library_scan_batch(
    runtime: &TaskRuntimeContext,
    action: &str,
) -> Result<LibraryTaskBatch, String> {
    SqliteFilesystemLibraryScanPipeline::new(
        runtime.database_file.clone(),
        DefaultLibraryTaskEmitter::default(),
    )
    .schedule(
        ScanSchedulingTrigger::Startup,
        &LibraryScanScheduleState::default(),
    )
    .map_err(|error| format!("{action}: {error}"))
}

fn schedule_periodic_library_scan_batch(
    runtime: &TaskRuntimeContext,
    last_run_by_library: &HashMap<String, tokio::time::Instant>,
) -> Result<LibraryTaskBatch, String> {
    SqliteFilesystemLibraryScanPipeline::new(
        runtime.database_file.clone(),
        DefaultLibraryTaskEmitter::default(),
    )
    .schedule(
        ScanSchedulingTrigger::Tick,
        &LibraryScanScheduleState {
            elapsed_since_last_run_by_library: last_run_by_library
                .iter()
                .map(|(library_id, last_run)| (library_id.clone(), last_run.elapsed()))
                .collect(),
        },
    )
    .map_err(|error| format!("schedule periodic library scans: {error}"))
}

fn sync_periodic_library_scan_state(
    runtime: &TaskRuntimeContext,
    last_run_by_library: &mut HashMap<String, tokio::time::Instant>,
) -> Result<(), String> {
    SqliteFilesystemLibraryScanPipeline::new(
        runtime.database_file.clone(),
        DefaultLibraryTaskEmitter::default(),
    )
    .sync_periodic_library_scan_state(last_run_by_library)
    .map_err(|error| format!("build periodic library scan state: {error}"))
}

fn periodic_library_scan_tasks(
    batch: LibraryTaskBatch,
) -> Result<Vec<(String, TaskQueueRecord)>, String> {
    batch
        .tasks
        .into_iter()
        .map(|task| {
            let library_id = periodic_scan_task_library_id(task.payload.as_deref())?;
            Ok((library_id, task.into_queue_record()))
        })
        .collect()
}

fn periodic_scan_task_library_id(payload: Option<&str>) -> Result<String, String> {
    let Some(payload) = payload else {
        return Err("periodic library scan task requires serialized payload".to_string());
    };
    let payload = serde_json::from_str::<Value>(payload).map_err(|error| {
        format!("periodic library scan task payload must be valid JSON: {error}")
    })?;

    payload
        .get("libraryId")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            "periodic library scan task payload field 'libraryId' must be a string".to_string()
        })
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
            consumes_queue = runtime.consumes_queue,
            owns_main_database = runtime.owns_main_database,
            owns_search_index = runtime.owns_search_index,
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
            consumes_queue = runtime.consumes_queue,
            owns_main_database = runtime.owns_main_database,
            owns_search_index = runtime.owns_search_index,
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
            consumes_queue = runtime.consumes_queue,
            owns_main_database = runtime.owns_main_database,
            owns_search_index = runtime.owns_search_index,
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
            consumes_queue = runtime.consumes_queue,
            owns_main_database = runtime.owns_main_database,
            owns_search_index = runtime.owns_search_index,
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

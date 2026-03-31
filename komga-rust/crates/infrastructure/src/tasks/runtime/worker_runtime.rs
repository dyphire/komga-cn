use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use komga_application::task_processing::{
    LibraryScanProfile, TaskRuntimeConfig, TaskRuntimeContext, build_library_scan_tasks,
    build_scheduled_library_scans, build_startup_library_scan_tasks, library_scan_due_periods,
};
use tokio::runtime::Handle;
use tokio::time::interval;

use super::{ScheduledLibraryScan, TaskQueueRecord, TaskQueueScheduler};
use crate::tasks::{load_persisted_library_ids, load_persisted_library_scan_profiles};

pub type SharedTaskQueue = Arc<Mutex<TaskQueueScheduler>>;

pub struct RuntimeBackgroundState {
    pub task_queue: SharedTaskQueue,
    pub scheduled_scans: Vec<ScheduledLibraryScan>,
}

pub fn prepare_task_queue(
    config: impl TaskRuntimeConfig,
    startup_search_task: Option<&'static str>,
) -> RuntimeBackgroundState {
    let runtime = config.task_runtime_context();
    let mut task_queue = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-runtime-http");
    if runtime.consumes_queue {
        let _ = task_queue.disown_all();
    }
    let scheduled_scans = bootstrap_startup_library_scans(&mut task_queue, &runtime);
    if runtime.consumes_queue {
        bootstrap_startup_search_task(&mut task_queue, &runtime, startup_search_task);
    }

    RuntimeBackgroundState {
        task_queue: Arc::new(Mutex::new(task_queue)),
        scheduled_scans,
    }
}

pub fn spawn_runtime_workers(
    task_queue: SharedTaskQueue,
    runtime: TaskRuntimeContext,
    scheduled_scans: Vec<ScheduledLibraryScan>,
) {
    spawn_periodic_library_scan_workers(task_queue.clone(), runtime.clone(), scheduled_scans);
    spawn_background_task_worker(task_queue, runtime.clone());
    spawn_authentication_activity_cleanup_worker(runtime);
}

pub fn bootstrap_startup_search_task(
    task_queue: &mut TaskQueueScheduler,
    runtime: &TaskRuntimeContext,
    startup_search_task: Option<&'static str>,
) {
    let Some(task_name) = startup_search_task else {
        return;
    };

    task_queue.enqueue(TaskQueueRecord::new(task_name.to_string(), 1_000, None));
    drop(task_queue.process_available(runtime));
}

pub fn bootstrap_startup_library_scans(
    task_queue: &mut TaskQueueScheduler,
    runtime: &TaskRuntimeContext,
) -> Vec<ScheduledLibraryScan> {
    if !runtime.owns_main_database {
        return Vec::new();
    }

    let profiles = load_persisted_library_scan_profiles(runtime.database_file.as_path())
        .unwrap_or_else(|error| panic!("load startup library scan profiles: {error}"))
        .into_iter()
        .map(|profile| LibraryScanProfile {
            library_id: profile.library_id,
            scan_startup: profile.scan_startup,
            scan_interval: profile.scan_interval,
        })
        .collect::<Vec<_>>();

    if profiles.is_empty() {
        return Vec::new();
    }

    for task in build_startup_library_scan_tasks(&profiles) {
        task_queue.enqueue(TaskQueueRecord::new(task.id, task.priority, task.group));
    }

    build_scheduled_library_scans(&profiles)
        .unwrap_or_else(|error| panic!("build scheduled library scans: {error}"))
}

pub fn process_startup_library_scans(config: impl TaskRuntimeConfig) {
    let runtime = config.task_runtime_context();
    if !runtime.owns_main_database {
        return;
    }

    let library_ids = load_persisted_library_ids(runtime.database_file.as_path())
        .unwrap_or_else(|error| panic!("load startup library ids: {error}"));
    if library_ids.is_empty() {
        return;
    }

    let mut task_queue = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    for task in build_library_scan_tasks(&library_ids) {
        task_queue.enqueue(task);
    }
    drop(task_queue.process_available(&runtime));
}

fn spawn_periodic_library_scan_workers(
    task_queue: SharedTaskQueue,
    runtime: TaskRuntimeContext,
    _scheduled_scans: Vec<ScheduledLibraryScan>,
) {
    let Ok(handle) = Handle::try_current() else {
        return;
    };

    handle.spawn(async move {
        let mut ticker = interval(Duration::from_secs(60));
        ticker.tick().await;
        let mut last_run_by_library: HashMap<String, tokio::time::Instant> = HashMap::new();

        loop {
            ticker.tick().await;

            let profiles = load_persisted_library_scan_profiles(runtime.database_file.as_path())
                .unwrap_or_else(|error| panic!("load periodic library scan profiles: {error}"))
                .into_iter()
                .map(|profile| LibraryScanProfile {
                    library_id: profile.library_id,
                    scan_startup: profile.scan_startup,
                    scan_interval: profile.scan_interval,
                })
                .collect::<Vec<_>>();
            let active_libraries = library_scan_due_periods(&profiles)
                .unwrap_or_else(|error| panic!("build periodic library scan periods: {error}"));

            for (library_id, period) in active_libraries.clone() {
                let next_due = last_run_by_library
                    .entry(library_id.clone())
                    .or_insert_with(tokio::time::Instant::now);

                if next_due.elapsed() < period {
                    continue;
                }

                let mut queue = task_queue
                    .lock()
                    .expect("task queue state lock should not be poisoned");
                queue.enqueue(TaskQueueRecord::new(
                    format!("SCAN_LIBRARY:{library_id}"),
                    100,
                    Some(library_id),
                ));
                drop(queue.process_available(&runtime));
                *next_due = tokio::time::Instant::now();
            }

            last_run_by_library.retain(|library_id, _| active_libraries.contains_key(library_id));
        }
    });
}

fn spawn_background_task_worker(task_queue: SharedTaskQueue, runtime: TaskRuntimeContext) {
    let Ok(handle) = Handle::try_current() else {
        return;
    };

    handle.spawn(async move {
        let mut ticker = interval(Duration::from_secs(300));
        ticker.tick().await;

        loop {
            ticker.tick().await;
            let mut task_queue = task_queue
                .lock()
                .expect("task queue state lock should not be poisoned");
            drop(task_queue.process_available(&runtime));
        }
    });
}

fn spawn_authentication_activity_cleanup_worker(runtime: TaskRuntimeContext) {
    let Ok(handle) = Handle::try_current() else {
        return;
    };

    handle.spawn(async move {
        let mut ticker = interval(Duration::from_secs(86_400));
        ticker.tick().await;

        loop {
            ticker.tick().await;
            cleanup_authentication_activity_once(&runtime).await;
        }
    });
}

pub async fn cleanup_authentication_activity_once(runtime: &TaskRuntimeContext) {
    if !runtime.owns_main_database {
        return;
    }

    let _ = crate::auth::persisted_cleanup_authentication_activity(runtime.database_file.as_path())
        .await;
}

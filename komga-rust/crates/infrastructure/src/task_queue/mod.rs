use komga_application::task_processing::{TaskExecutionResult, TaskQueueRecord};

mod runtime_context;
pub use runtime_context::{
    DatabaseRuntime, FilesystemRuntime, JobRuntime, SearchRuntime, TaskRuntimeConfig,
    TaskRuntimeContext, TaskRuntimeOwnershipOverrides, WorkerRuntime,
};
mod cleanup_tasks;
mod delete_tasks;
mod execution_loop;
mod execution_pool;
mod import_jobs;
mod index_jobs;
mod index_tasks;
pub(crate) mod library_scan_pipeline;
mod maintenance_jobs;
pub(crate) mod media_helpers;
mod metadata_tasks;
mod queue_core;
mod queue_orchestration;
pub(crate) mod queue_scheduler;
mod runtime_task_engine;
mod scan_follow_up;
mod scanner_jobs;
mod scanner_support;
mod task_job_dispatch;
#[cfg(test)]
pub(crate) mod test_support;
mod worker_runtime;

pub use library_scan_pipeline::SqliteFilesystemLibraryScanPipeline;
pub use queue_scheduler::TaskQueueScheduler;
pub use worker_runtime::{
    RuntimeBackgroundState, SharedTaskQueue, TaskQueueWakeSignal,
    cleanup_authentication_activity_once, prepare_task_queue, process_startup_library_scans,
    run_background_task_iteration, run_periodic_library_scan_iteration,
};

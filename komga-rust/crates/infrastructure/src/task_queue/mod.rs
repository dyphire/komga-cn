use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Read;
use std::path::PathBuf;

use crate::search::runtime_tasks::{
    AnalyzedBookMedia, AnalyzedBookPage, analyze_book_input, persist_book_analysis,
    rebuild_index_from_database_for_entities,
};
use komga_application::task_processing::{TaskProcessingError, TaskQueueOrchestrator};

mod runtime_context;
use queue_core::{PersistedTaskStoreRecord, SqliteTaskQueueStore};
pub use runtime_context::{
    DatabaseRuntime, FilesystemRuntime, JobRuntime, SearchRuntime, TaskRuntimeConfig,
    TaskRuntimeContext, TaskRuntimeOwnershipOverrides, WorkerRuntime,
};
use sha2::{Digest, Sha256};
use zip::ZipArchive;

mod cleanup_tasks;
mod delete_tasks;
mod execution_pool;
mod import_jobs;
mod index_jobs;
mod index_tasks;
pub mod library_scan_pipeline;
mod maintenance_jobs;
pub(crate) mod media_helpers;
mod metadata_tasks;
mod queue_core;
pub mod queue_scheduler;
mod runtime_task_engine;
mod scanner_jobs;
mod scanner_support;
mod task_executor;
pub mod task_handlers;
#[cfg(test)]
pub(crate) mod test_support;
pub mod worker_runtime;

use library_scan_pipeline::SqliteFilesystemLibraryScanPipeline;
use media_helpers::*;
use queue_scheduler::TaskQueueScheduler;
use scanner_support::*;

pub use execution_pool::TaskExecutionPoolHandle;
pub use komga_application::task_processing::{LibraryScanInterval, TaskQueueRecord};
pub use runtime_task_engine::RuntimeTaskEngine;
pub type TaskQueueAdmin = TaskQueueOrchestrator;

type TaskExecutionError = TaskProcessingError;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TaskExecutionOutcome {
    follow_up_tasks: Vec<TaskQueueRecord>,
}

impl TaskExecutionOutcome {
    fn completed() -> Self {
        Self::default()
    }

    fn with_follow_up_tasks(follow_up_tasks: Vec<TaskQueueRecord>) -> Self {
        Self { follow_up_tasks }
    }

    async fn enqueue_into(self, scheduler: &TaskQueueScheduler) {
        for task in self.follow_up_tasks {
            scheduler.enqueue(task).await;
        }
    }
}

#[derive(Debug)]
pub(crate) struct TaskExecutionResult {
    task: TaskQueueRecord,
    outcome: Result<TaskExecutionOutcome, TaskExecutionError>,
}

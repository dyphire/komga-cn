use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Read;
use std::path::PathBuf;

use crate::search::runtime_tasks::{
    AnalyzedBookMedia, AnalyzedBookPage, analyze_book_input, persist_book_analysis,
    rebuild_index_from_database_for_entities,
};
use komga_application::task_processing::{
    TaskExecutionOutcome, TaskExecutionResult, TaskProcessingError, TaskQueueOrchestrator,
};

mod runtime_context;
use queue_core::SqliteTaskQueueStore;
pub use runtime_context::{
    DatabaseRuntime, FilesystemRuntime, JobRuntime, SearchRuntime, TaskRuntimeConfig,
    TaskRuntimeContext, TaskRuntimeOwnershipOverrides, WorkerRuntime,
};
use sha2::{Digest, Sha256};
use task_identity::PersistedTaskStoreRecord;
use zip::ZipArchive;

mod cleanup_tasks;
mod delete_tasks;
mod execution_loop;
mod execution_pool;
mod import_jobs;
mod index_jobs;
mod index_tasks;
pub mod library_scan_pipeline;
mod maintenance_jobs;
pub(crate) mod media_helpers;
mod metadata_tasks;
mod queue_core;
mod queue_orchestration;
pub mod queue_scheduler;
mod runtime_task_engine;
mod scanner_jobs;
mod scanner_support;
mod task_identity;
mod task_job_pipeline;
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

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::PathBuf;

use crate::search::runtime_tasks::{
    AnalyzedBookMedia, AnalyzedBookPage, analyze_book_input, persist_book_analysis,
    rebuild_index_from_database_for_entities,
};
use crate::tasks::persisted_queue::{PersistedTaskStoreRecord, SqliteTaskQueueStore};
use komga_application::task_processing::{
    TaskProcessingError, TaskQueueOrchestrator, TaskRuntimeConfig,
};
use sha2::{Digest, Sha256};
use zip::ZipArchive;

mod cleanup_tasks;
mod delete_tasks;
mod import_jobs;
mod index_jobs;
mod index_tasks;
pub mod library_scan_pipeline;
mod maintenance_jobs;
mod media_helpers;
mod metadata_tasks;
mod queue_core;
pub mod queue_scheduler;
mod scanner_jobs;
mod scanner_support;
mod task_protocol;
#[cfg(test)]
pub(crate) mod test_support;
pub mod worker_runtime;

use library_scan_pipeline::SqliteFilesystemLibraryScanPipeline;
use media_helpers::*;
use queue_scheduler::TaskQueueScheduler;
use scanner_support::*;
use task_protocol::{RuntimeFollowUpTask, runtime_follow_up_task};

pub use komga_application::task_processing::{LibraryScanInterval, TaskQueueRecord};
pub type TaskQueueAdmin = TaskQueueOrchestrator;

type RuntimeConfig = dyn TaskRuntimeConfig;

type TaskExecutionError = TaskProcessingError;

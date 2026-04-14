use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::PathBuf;

use crate::search::{
    AnalyzedBookMedia, AnalyzedBookPage, analyze_book_input, persist_book_analysis,
    rebuild_index_from_database_for_entities,
};
use crate::tasks::{PersistedTaskStoreRecord, SqliteTaskQueueStore};
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
mod library_scan_pipeline;
mod maintenance_jobs;
mod media_helpers;
mod metadata_tasks;
mod queue_core;
mod queue_scheduler;
mod scanner_jobs;
mod scanner_support;
mod task_protocol;
mod worker_runtime;

use media_helpers::*;
use scanner_support::*;
use task_protocol::{RuntimeFollowUpTask, runtime_follow_up_task};

pub use komga_application::task_processing::{LibraryScanInterval, TaskQueueRecord};
pub use library_scan_pipeline::SqliteFilesystemLibraryScanPipeline;
pub type TaskQueueAdmin = TaskQueueOrchestrator;
pub use queue_scheduler::TaskQueueScheduler;
pub use worker_runtime::{
    RuntimeBackgroundState, SharedTaskQueue, TaskQueueWakeSignal, bootstrap_startup_search_task,
    cleanup_authentication_activity_once, prepare_task_queue, process_startup_library_scans,
    run_background_task_iteration, run_periodic_library_scan_iteration, spawn_runtime_workers,
};

type RuntimeConfig = dyn TaskRuntimeConfig;

type TaskExecutionError = TaskProcessingError;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::Command;

use crate::search::{
    AnalyzedBookMedia, AnalyzedBookPage, analyze_book_input, persist_book_analysis,
    rebuild_index_from_database,
};
use crate::tasks::{PersistedTaskStoreRecord, SqliteTaskQueueStore, scan_library};
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
mod maintenance_jobs;
mod media_helpers;
mod metadata_tasks;
mod queue_core;
mod queue_scheduler;
mod scanner_jobs;
mod scanner_support;
mod worker_runtime;

use media_helpers::*;
use scanner_support::*;

pub use komga_application::task_processing::{
    LibraryScanInterval, ScheduledLibraryScan, TaskQueueRecord,
};
pub type TaskQueueAdmin = TaskQueueOrchestrator;
pub use queue_scheduler::TaskQueueScheduler;
pub use worker_runtime::{
    RuntimeBackgroundState, SharedTaskQueue, bootstrap_startup_library_scans,
    bootstrap_startup_search_task, prepare_task_queue, process_startup_library_scans,
    spawn_runtime_workers,
};

type RuntimeConfig = dyn TaskRuntimeConfig;

type TaskExecutionError = TaskProcessingError;

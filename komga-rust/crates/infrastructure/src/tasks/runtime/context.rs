use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use komga_application::operational::ServerSettingsPort;
use komga_application::runtime_sse::{RuntimeSseEventSink, RuntimeSseEventStore};
use komga_application::task_processing::{CleanupEmptySetsPolicy, ThumbnailRegenerationPolicy};
use sqlx::SqlitePool;

use crate::operational::ServerSettingsStore;
use crate::persistence::{DatabaseHandle, SqlitePersistenceContext};
use crate::search::engine::SearchIndexEngine;

#[derive(Clone)]
pub struct TaskRuntimeContext {
    main_db: DatabaseHandle,
    tasks_db_file: PathBuf,
    lucene_data_directory: PathBuf,
    consumes_queue: bool,
    owns_main_database: bool,
    owns_filesystem_scan_output: bool,
    owns_sidecar_output: bool,
    owns_search_index: bool,
    task_pool_size: usize,
    task_write_pool: SqlitePool,
    task_read_pool: SqlitePool,
    runtime_events: Arc<dyn RuntimeSseEventSink>,
    runtime_state: Arc<TaskRuntimeState>,
}

#[derive(Default)]
struct TaskRuntimeState {
    failed_book_conversions: Mutex<HashSet<String>>,
    skipped_extension_repairs: Mutex<HashSet<String>>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TaskRuntimeOwnershipOverrides {
    pub owns_main_database: Option<bool>,
    pub owns_filesystem_scan_output: Option<bool>,
    pub owns_sidecar_output: Option<bool>,
    pub owns_search_index: Option<bool>,
}

#[derive(Clone, Copy, Debug)]
pub struct WorkerRuntime<'a> {
    runtime: &'a TaskRuntimeContext,
}

#[derive(Clone, Copy, Debug)]
pub struct JobRuntime<'a> {
    runtime: &'a TaskRuntimeContext,
}

#[derive(Clone, Copy, Debug)]
pub struct DatabaseRuntime<'a> {
    runtime: &'a TaskRuntimeContext,
}

#[derive(Clone, Copy, Debug)]
pub struct SearchRuntime<'a> {
    runtime: &'a TaskRuntimeContext,
}

#[derive(Clone, Copy, Debug)]
pub struct FilesystemRuntime<'a> {
    runtime: &'a TaskRuntimeContext,
}

impl TaskRuntimeContext {
    pub fn new(
        main_db: DatabaseHandle,
        tasks_db_file: PathBuf,
        lucene_data_directory: PathBuf,
        consumes_queue: bool,
        task_pool_size: usize,
        task_write_pool: SqlitePool,
        task_read_pool: SqlitePool,
    ) -> Self {
        Self {
            main_db,
            tasks_db_file,
            lucene_data_directory,
            consumes_queue,
            owns_main_database: true,
            owns_filesystem_scan_output: true,
            owns_sidecar_output: true,
            owns_search_index: true,
            task_pool_size,
            task_write_pool,
            task_read_pool,
            runtime_events: Arc::new(RuntimeSseEventStore::default()),
            runtime_state: Arc::new(TaskRuntimeState::default()),
        }
    }

    pub fn with_ownership_overrides(mut self, overrides: TaskRuntimeOwnershipOverrides) -> Self {
        if let Some(value) = overrides.owns_main_database {
            self.owns_main_database = value;
        }
        if let Some(value) = overrides.owns_filesystem_scan_output {
            self.owns_filesystem_scan_output = value;
        }
        if let Some(value) = overrides.owns_sidecar_output {
            self.owns_sidecar_output = value;
        }
        if let Some(value) = overrides.owns_search_index {
            self.owns_search_index = value;
        }
        self
    }

    pub fn with_task_pool_size(mut self, task_pool_size: usize) -> Self {
        self.task_pool_size = task_pool_size;
        self
    }

    pub fn with_runtime_events(mut self, runtime_events: Arc<dyn RuntimeSseEventSink>) -> Self {
        self.runtime_events = runtime_events;
        self
    }

    pub fn worker(&self) -> WorkerRuntime<'_> {
        WorkerRuntime { runtime: self }
    }

    pub fn job(&self) -> JobRuntime<'_> {
        JobRuntime { runtime: self }
    }
}

impl WorkerRuntime<'_> {
    pub fn consumes_queue(&self) -> bool {
        self.runtime.consumes_queue
    }

    pub fn task_pool_size(&self) -> usize {
        self.runtime.task_pool_size
    }

    pub fn tasks_db_file(&self) -> &std::path::Path {
        self.runtime.tasks_db_file.as_path()
    }
}

impl JobRuntime<'_> {
    pub fn database(&self) -> DatabaseRuntime<'_> {
        DatabaseRuntime {
            runtime: self.runtime,
        }
    }

    pub fn search(&self) -> SearchRuntime<'_> {
        SearchRuntime {
            runtime: self.runtime,
        }
    }

    pub(crate) fn search_engine(&self) -> SearchIndexEngine {
        SearchIndexEngine::new(
            self.runtime.task_read_pool.clone(),
            self.runtime.lucene_data_directory.clone(),
            self.runtime.owns_search_index,
        )
    }

    pub fn filesystem(&self) -> FilesystemRuntime<'_> {
        FilesystemRuntime {
            runtime: self.runtime,
        }
    }

    fn server_settings(&self) -> ServerSettingsStore {
        ServerSettingsStore::from_context(SqlitePersistenceContext::new(
            self.database().write_pool().clone(),
        ))
    }

    pub(crate) async fn cleanup_empty_sets_policy(&self) -> anyhow::Result<CleanupEmptySetsPolicy> {
        let settings = self
            .server_settings()
            .load_settings()
            .await
            .map_err(|error| anyhow::anyhow!(error).context("load cleanup policy setting"))?;
        Ok(CleanupEmptySetsPolicy {
            delete_empty_collections: settings.delete_empty_collections,
            delete_empty_read_lists: settings.delete_empty_read_lists,
        })
    }

    pub(in crate::tasks) async fn thumbnail_regeneration_policy(
        &self,
    ) -> anyhow::Result<ThumbnailRegenerationPolicy> {
        let settings = self
            .server_settings()
            .load_settings()
            .await
            .map_err(|error| anyhow::anyhow!(error).context("load thumbnail size setting"))?;
        Ok(ThumbnailRegenerationPolicy {
            generated_thumbnail_max_edge: settings.thumbnail_size.max_edge(),
        })
    }

    pub(crate) fn runtime_events(&self) -> &dyn RuntimeSseEventSink {
        self.runtime.runtime_events.as_ref()
    }

    pub(crate) fn runtime_events_arc(&self) -> Arc<dyn RuntimeSseEventSink> {
        self.runtime.runtime_events.clone()
    }

    pub(crate) fn book_conversion_failed_before(&self, book_id: &str) -> bool {
        self.runtime
            .runtime_state
            .failed_book_conversions
            .lock()
            .expect("failed book conversion state lock should not be poisoned")
            .contains(book_id)
    }

    pub(crate) fn mark_book_conversion_failed(&self, book_id: &str) {
        self.runtime
            .runtime_state
            .failed_book_conversions
            .lock()
            .expect("failed book conversion state lock should not be poisoned")
            .insert(book_id.to_string());
    }

    pub(crate) fn extension_repair_was_skipped(&self, book_id: &str) -> bool {
        self.runtime
            .runtime_state
            .skipped_extension_repairs
            .lock()
            .expect("skipped extension repair state lock should not be poisoned")
            .contains(book_id)
    }

    pub(crate) fn mark_extension_repair_skipped(&self, book_id: &str) {
        self.runtime
            .runtime_state
            .skipped_extension_repairs
            .lock()
            .expect("skipped extension repair state lock should not be poisoned")
            .insert(book_id.to_string());
    }
}

impl std::fmt::Debug for TaskRuntimeContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskRuntimeContext")
            .field("main_db", &self.main_db)
            .field("tasks_db_file", &self.tasks_db_file)
            .field("lucene_data_directory", &self.lucene_data_directory)
            .field("consumes_queue", &self.consumes_queue)
            .field("owns_main_database", &self.owns_main_database)
            .field(
                "owns_filesystem_scan_output",
                &self.owns_filesystem_scan_output,
            )
            .field("owns_sidecar_output", &self.owns_sidecar_output)
            .field("owns_search_index", &self.owns_search_index)
            .field("task_pool_size", &self.task_pool_size)
            .field("task_write_pool", &self.task_write_pool)
            .field("task_read_pool", &self.task_read_pool)
            .field("runtime_events", &"<runtime event sink>")
            .field("runtime_state", &"<runtime state>")
            .finish()
    }
}

impl DatabaseRuntime<'_> {
    pub fn main_db(&self) -> &DatabaseHandle {
        &self.runtime.main_db
    }

    pub fn read_pool(&self) -> &SqlitePool {
        &self.runtime.task_read_pool
    }

    pub fn write_pool(&self) -> &SqlitePool {
        &self.runtime.task_write_pool
    }

    pub fn owns_main_database(&self) -> bool {
        self.runtime.owns_main_database
    }
}

impl SearchRuntime<'_> {
    pub fn lucene_data_directory(&self) -> &std::path::Path {
        self.runtime.lucene_data_directory.as_path()
    }

    pub fn owns_search_index(&self) -> bool {
        self.runtime.owns_search_index
    }
}

impl FilesystemRuntime<'_> {
    pub fn owns_filesystem_scan_output(&self) -> bool {
        self.runtime.owns_filesystem_scan_output
    }

    pub fn owns_sidecar_output(&self) -> bool {
        self.runtime.owns_sidecar_output
    }
}

pub trait TaskRuntimeConfig {
    fn task_runtime_context(&self) -> TaskRuntimeContext;
}

impl TaskRuntimeConfig for TaskRuntimeContext {
    fn task_runtime_context(&self) -> TaskRuntimeContext {
        self.clone()
    }
}

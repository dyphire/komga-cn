use std::path::PathBuf;
use std::sync::Arc;

use komga_application::operational::ServerSettingsPort;
use komga_application::runtime_sse::RuntimeSseEventSink;
use komga_application::task_processing::{CleanupEmptySetsPolicy, ThumbnailRegenerationPolicy};
use sqlx::SqlitePool;

use komga_infrastructure_base::{DatabaseHandle, SqlitePersistenceContext};
use komga_infrastructure_media_library::MediaLibraryJobContext;
use komga_infrastructure_operational::ServerSettingsStore;
use komga_infrastructure_search::engine::SearchIndexEngine;

#[derive(Clone)]
pub struct TaskRuntimeContext {
    main_db: DatabaseHandle,
    tasks_db_file: PathBuf,
    lucene_data_directory: PathBuf,
    consumes_queue: bool,
    ownership: TaskRuntimeOwnership,
    task_pool_size: usize,
    task_write_pool: SqlitePool,
    task_read_pool: SqlitePool,
    runtime_events: Arc<dyn RuntimeSseEventSink>,
    media_library: MediaLibraryJobContext,
}

pub struct TaskRuntimeContextParams {
    pub main_db: DatabaseHandle,
    pub tasks_db_file: PathBuf,
    pub lucene_data_directory: PathBuf,
    pub consumes_queue: bool,
    pub ownership: TaskRuntimeOwnership,
    pub task_pool_size: usize,
    pub task_write_pool: SqlitePool,
    pub task_read_pool: SqlitePool,
    pub runtime_events: Arc<dyn RuntimeSseEventSink>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskRuntimeOwnership {
    pub owns_main_database: bool,
    pub owns_filesystem_scan_output: bool,
    pub owns_sidecar_output: bool,
    pub owns_search_index: bool,
}

impl TaskRuntimeOwnership {
    pub const fn all_owned() -> Self {
        Self {
            owns_main_database: true,
            owns_filesystem_scan_output: true,
            owns_sidecar_output: true,
            owns_search_index: true,
        }
    }
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
    pub fn new(params: TaskRuntimeContextParams) -> Self {
        let TaskRuntimeContextParams {
            main_db,
            tasks_db_file,
            lucene_data_directory,
            consumes_queue,
            ownership,
            task_pool_size,
            task_write_pool,
            task_read_pool,
            runtime_events,
        } = params;
        let media_library = MediaLibraryJobContext::new(
            main_db.clone(),
            ownership.owns_main_database,
            ownership.owns_filesystem_scan_output,
            runtime_events.clone(),
        );
        Self {
            main_db,
            tasks_db_file,
            lucene_data_directory,
            consumes_queue,
            ownership,
            task_pool_size,
            task_write_pool,
            task_read_pool,
            runtime_events,
            media_library,
        }
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
    pub(crate) fn media_library(&self) -> &MediaLibraryJobContext {
        &self.runtime.media_library
    }

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
            self.runtime.ownership.owns_search_index,
        )
    }

    pub fn filesystem(&self) -> FilesystemRuntime<'_> {
        FilesystemRuntime {
            runtime: self.runtime,
        }
    }

    fn server_settings(&self) -> ServerSettingsStore {
        ServerSettingsStore::from_context(SqlitePersistenceContext::new(
            self.database().task_write_pool().clone(),
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

    pub(crate) async fn thumbnail_regeneration_policy(
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
}

impl std::fmt::Debug for TaskRuntimeContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskRuntimeContext")
            .field("main_db", &self.main_db)
            .field("tasks_db_file", &self.tasks_db_file)
            .field("lucene_data_directory", &self.lucene_data_directory)
            .field("consumes_queue", &self.consumes_queue)
            .field("owns_main_database", &self.ownership.owns_main_database)
            .field(
                "owns_filesystem_scan_output",
                &self.ownership.owns_filesystem_scan_output,
            )
            .field("owns_sidecar_output", &self.ownership.owns_sidecar_output)
            .field("owns_search_index", &self.ownership.owns_search_index)
            .field("task_pool_size", &self.task_pool_size)
            .field("task_write_pool", &self.task_write_pool)
            .field("task_read_pool", &self.task_read_pool)
            .field("runtime_events", &"<runtime event sink>")
            .finish()
    }
}

impl DatabaseRuntime<'_> {
    pub fn main_db(&self) -> &DatabaseHandle {
        &self.runtime.main_db
    }

    pub fn task_read_pool(&self) -> &SqlitePool {
        &self.runtime.task_read_pool
    }

    pub fn task_write_pool(&self) -> &SqlitePool {
        &self.runtime.task_write_pool
    }

    pub fn owns_main_database(&self) -> bool {
        self.runtime.ownership.owns_main_database
    }
}

impl SearchRuntime<'_> {
    pub fn lucene_data_directory(&self) -> &std::path::Path {
        self.runtime.lucene_data_directory.as_path()
    }

    pub fn owns_search_index(&self) -> bool {
        self.runtime.ownership.owns_search_index
    }
}

impl FilesystemRuntime<'_> {
    pub fn owns_filesystem_scan_output(&self) -> bool {
        self.runtime.ownership.owns_filesystem_scan_output
    }

    pub fn owns_sidecar_output(&self) -> bool {
        self.runtime.ownership.owns_sidecar_output
    }
}

impl komga_infrastructure_tasks::TaskQueueConfigProvider for TaskRuntimeContext {
    fn task_queue_config(&self) -> komga_infrastructure_tasks::TaskQueueConfig {
        komga_infrastructure_tasks::TaskQueueConfig::new(
            self.tasks_db_file.clone(),
            self.consumes_queue,
        )
    }
}

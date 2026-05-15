use std::path::PathBuf;

use sqlx::SqlitePool;

use crate::database_handle::DatabaseHandle;

#[derive(Clone, Debug)]
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

    pub fn filesystem(&self) -> FilesystemRuntime<'_> {
        FilesystemRuntime {
            runtime: self.runtime,
        }
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

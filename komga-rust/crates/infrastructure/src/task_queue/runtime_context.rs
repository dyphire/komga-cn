use std::path::PathBuf;

use sqlx::SqlitePool;

use crate::database_handle::DatabaseHandle;

#[derive(Clone, Debug)]
pub struct TaskRuntimeContext {
    pub main_db: DatabaseHandle,
    pub tasks_db_file: PathBuf,
    pub lucene_data_directory: PathBuf,
    pub consumes_queue: bool,
    pub owns_main_database: bool,
    pub owns_filesystem_scan_output: bool,
    pub owns_sidecar_output: bool,
    pub owns_search_index: bool,
    pub task_pool_size: usize,
    pub task_write_pool: SqlitePool,
    pub task_read_pool: SqlitePool,
}

pub trait TaskRuntimeConfig {
    fn task_runtime_context(&self) -> TaskRuntimeContext;
}

impl TaskRuntimeConfig for TaskRuntimeContext {
    fn task_runtime_context(&self) -> TaskRuntimeContext {
        self.clone()
    }
}

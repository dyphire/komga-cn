use std::path::PathBuf;

use async_trait::async_trait;

/// Snapshot of a SQLite connection pool's state.
#[derive(Clone, Debug)]
pub struct SqlitePoolSnapshot {
    pub path: PathBuf,
    pub max_connections: u32,
    pub min_connections: u32,
    pub total_connections: u32,
    pub idle_connections: u32,
    pub in_use_connections: u32,
    pub is_closed: bool,
}

/// Port for reading operational metrics (library counts, task stats, pool state).
#[async_trait]
pub trait OperationalMetricsPort: Send + Sync {
    async fn load_task_execution_values(&self) -> Result<Vec<(String, f64)>, String>;

    async fn load_libraries_count(&self) -> Result<f64, String>;

    async fn load_series_grouped_by_library(&self) -> Result<Vec<(String, f64)>, String>;

    async fn load_books_grouped_by_library(&self) -> Result<Vec<(String, f64)>, String>;

    async fn load_books_filesize_grouped_by_library(&self) -> Result<Vec<(String, f64)>, String>;

    async fn load_sidecars_grouped_by_library(&self) -> Result<Vec<(String, f64)>, String>;

    async fn load_collections_count(&self) -> Result<f64, String>;

    async fn load_readlists_count(&self) -> Result<f64, String>;

    async fn load_task_failure_count(&self) -> Result<f64, String>;

    async fn load_sqlite_pool_snapshots(
        &self,
        paths: &[PathBuf],
    ) -> Result<Vec<SqlitePoolSnapshot>, String>;
}

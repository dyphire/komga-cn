use std::path::PathBuf;

/// Snapshot of a database connection pool's state.
#[derive(Clone, Debug)]
pub struct DatabasePoolSnapshot {
    pub path: PathBuf,
    pub max_connections: u32,
    pub min_connections: u32,
    pub total_connections: u32,
    pub idle_connections: u32,
    pub in_use_connections: u32,
    pub is_closed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TaskExecutionMetricValue {
    pub task_type: String,
    pub count: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LibraryMetricValue {
    pub library_id: String,
    pub value: f64,
}

/// Port for reading operational metrics (library counts, task stats, pool state).
#[async_trait::async_trait]
pub trait OperationalMetricsPort: Send + Sync {
    async fn load_task_execution_values(&self) -> anyhow::Result<Vec<TaskExecutionMetricValue>>;

    async fn load_libraries_count(&self) -> anyhow::Result<f64>;

    async fn load_series_grouped_by_library(&self) -> anyhow::Result<Vec<LibraryMetricValue>>;

    async fn load_books_grouped_by_library(&self) -> anyhow::Result<Vec<LibraryMetricValue>>;

    async fn load_books_filesize_grouped_by_library(
        &self,
    ) -> anyhow::Result<Vec<LibraryMetricValue>>;

    async fn load_sidecars_grouped_by_library(&self) -> anyhow::Result<Vec<LibraryMetricValue>>;

    async fn load_collections_count(&self) -> anyhow::Result<f64>;

    async fn load_readlists_count(&self) -> anyhow::Result<f64>;

    async fn load_task_failure_count(&self) -> anyhow::Result<f64>;

    async fn load_database_pool_snapshots(
        &self,
        paths: &[PathBuf],
    ) -> anyhow::Result<Vec<DatabasePoolSnapshot>>;
}

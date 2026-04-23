use std::path::PathBuf;

use crate::search::index_lifecycle::SearchFieldEntry;
use crate::sqlite::{connect_private_task_pool, connect_private_write_pool};

pub(super) type BoxFuture<T> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, String>> + Send>>;

pub(super) fn run_task_database_query<T>(
    database_file: PathBuf,
    operation: impl FnOnce(sqlx::SqlitePool) -> BoxFuture<T> + Send + 'static,
) -> Result<T, String>
where
    T: Send + 'static,
{
    std::thread::spawn(move || {
        let runtime = crate::tokio_runtime::current_thread_runtime()
            .map_err(|error| format!("failed to build task runtime: {error}"))?;

        runtime.block_on(async move {
            let pool = connect_private_write_pool(&database_file)
                .await
                .map_err(|error| format!("failed to open private sqlite write pool: {error}"))?;
            operation(pool).await
        })
    })
    .join()
    .map_err(|_| "database operation worker thread panicked".to_string())?
}

pub(super) fn run_task_database_query_with_max_connections<T>(
    database_file: PathBuf,
    max_connections: u32,
    operation: impl FnOnce(sqlx::SqlitePool) -> BoxFuture<T> + Send + 'static,
) -> Result<T, String>
where
    T: Send + 'static,
{
    std::thread::spawn(move || {
        let runtime = crate::tokio_runtime::current_thread_runtime()
            .map_err(|error| format!("failed to build task runtime: {error}"))?;

        runtime.block_on(async move {
            let pool = connect_private_task_pool(&database_file, max_connections)
                .await
                .map_err(|error| format!("failed to open private sqlite task pool: {error}"))?;
            operation(pool).await
        })
    })
    .join()
    .map_err(|_| "database operation worker thread panicked".to_string())?
}

pub(super) fn search_field(field: &str, value: String) -> SearchFieldEntry {
    SearchFieldEntry {
        field: field.to_string(),
        value,
    }
}

pub(super) fn search_fields(field: &str, values: String) -> Vec<SearchFieldEntry> {
    values
        .split('|')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| SearchFieldEntry {
            field: field.to_string(),
            value: value.to_string(),
        })
        .collect()
}

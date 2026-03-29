use super::*;

pub(super) fn run_database_query<T>(
    database_file: PathBuf,
    operation: impl FnOnce(SqlitePool) -> BoxFuture<Result<T, TaskExecutionError>> + Send + 'static,
) -> Result<T, TaskExecutionError>
where
    T: Send + 'static,
{
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| {
                TaskExecutionError::runtime(format!("failed to build task runtime: {error}"))
            })?;

        runtime.block_on(async move {
            let pool = connect_pool(&database_file, 1).await.map_err(|error| {
                TaskExecutionError::runtime(format!("failed to open sqlite pool: {error}"))
            })?;
            operation(pool.clone()).await
        })
    })
    .join()
    .map_err(|_| TaskExecutionError::runtime("database operation worker thread panicked"))?
}

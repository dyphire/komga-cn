use std::path::Path;

use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;

pub async fn connect_test_pool(
    path: impl AsRef<Path>,
    max_connections: u32,
) -> Result<SqlitePool, sqlx::Error> {
    SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(komga_infrastructure_base::file_backed_connect_options(path))
        .await
}

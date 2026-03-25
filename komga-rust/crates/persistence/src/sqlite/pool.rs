use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

use crate::context::SqlitePersistenceContext;
use crate::sqlite::setup;

pub const DEFAULT_MAX_CONNECTIONS: u32 = 4;

pub fn reject_or_quarantine_pool_topology(
    database_url: &str,
    max_connections: u32,
) -> Result<(), String> {
    if database_url == "sqlite::memory:" && max_connections > 1 {
        return Err(
            "pooled sqlite::memory: is quarantined; use deterministic file-backed sqlite topology instead"
                .to_string(),
        );
    }

    Ok(())
}

pub fn file_backed_connect_options(path: impl AsRef<Path>) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
}

pub async fn connect_pool(
    path: impl AsRef<Path>,
    max_connections: u32,
) -> Result<SqlitePool, sqlx::Error> {
    connect_bootstrapped_pool(path, max_connections, BootstrapTarget::None).await
}

pub async fn connect_tasks_pool(
    path: impl AsRef<Path>,
    max_connections: u32,
) -> Result<SqlitePool, sqlx::Error> {
    connect_bootstrapped_pool(path, max_connections, BootstrapTarget::Tasks).await
}

async fn connect_bootstrapped_pool(
    path: impl AsRef<Path>,
    max_connections: u32,
    bootstrap_target: BootstrapTarget,
) -> Result<SqlitePool, sqlx::Error> {
    let pool = SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(file_backed_connect_options(path))
        .await?;

    match bootstrap_target {
        BootstrapTarget::None => Ok(pool),
        BootstrapTarget::Main => {
            setup::bootstrap_pool(&pool).await?;
            Ok(pool)
        }
        BootstrapTarget::Tasks => {
            setup::bootstrap_tasks_pool(&pool).await?;
            Ok(pool)
        }
    }
}

pub async fn connect_persistence_context(
    path: impl AsRef<Path>,
    max_connections: u32,
) -> Result<SqlitePersistenceContext, sqlx::Error> {
    if max_connections != 1 {
        return Err(sqlx::Error::Protocol(
            "persistence write path requires single-writer sqlite pool (max_connections=1)"
                .to_string(),
        ));
    }

    let pool = connect_bootstrapped_pool(path, max_connections, BootstrapTarget::Main).await?;
    Ok(SqlitePersistenceContext::new(pool))
}

#[derive(Copy, Clone)]
enum BootstrapTarget {
    None,
    Main,
    Tasks,
}

pub struct SqliteTempPool {
    pool: SqlitePool,
    db_path: PathBuf,
}

impl SqliteTempPool {
    pub async fn new(case_id: &str) -> Result<Self, sqlx::Error> {
        let db_path = deterministic_temp_db_path(case_id);
        let pool = connect_pool(&db_path, DEFAULT_MAX_CONNECTIONS).await?;

        Ok(Self { pool, db_path })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn persistence_context(&self) -> SqlitePersistenceContext {
        SqlitePersistenceContext::new(self.pool.clone())
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub async fn cleanup(self) {
        self.pool.close().await;
        if self.db_path.exists()
            && let Err(error) = std::fs::remove_file(&self.db_path)
            && error.kind() != std::io::ErrorKind::NotFound
        {}
    }
}

fn deterministic_temp_db_path(case_id: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after epoch")
        .as_nanos();
    let pid = std::process::id();
    std::env::temp_dir().join(format!("komga-sqlite-topology-{case_id}-{pid}-{nanos}.db"))
}

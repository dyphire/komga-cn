use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

use crate::context::SqlitePersistenceContext;
use crate::sqlite::setup;

pub const DEFAULT_MAX_CONNECTIONS: u32 = 4;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct PoolKey {
    path: PathBuf,
    max_connections: u32,
}

impl PoolKey {
    fn new(path: &Path, max_connections: u32) -> Self {
        Self {
            path: absolute_pool_path(path),
            max_connections,
        }
    }
}

fn absolute_pool_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }

    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

fn shared_pools() -> &'static Mutex<HashMap<PoolKey, SqlitePool>> {
    static SHARED_POOLS: OnceLock<Mutex<HashMap<PoolKey, SqlitePool>>> = OnceLock::new();
    SHARED_POOLS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub async fn close_all_shared_pools() {
    let pools = {
        let mut pools = shared_pools()
            .lock()
            .expect("shared sqlite pool map lock should not be poisoned");
        pools.drain().map(|(_, pool)| pool).collect::<Vec<_>>()
    };

    for pool in pools {
        pool.close().await;
    }
}

fn get_shared_pool(pool_key: &PoolKey) -> Option<SqlitePool> {
    let mut pools = shared_pools()
        .lock()
        .expect("shared sqlite pool map lock should not be poisoned");
    let pool = pools.get(pool_key)?;

    if pool.is_closed() {
        pools.remove(pool_key);
        return None;
    }

    Some(pool.clone())
}

fn insert_shared_pool(pool_key: PoolKey, pool: &SqlitePool) -> Option<SqlitePool> {
    let mut pools = shared_pools()
        .lock()
        .expect("shared sqlite pool map lock should not be poisoned");
    if let Some(existing) = pools.get(&pool_key) {
        if existing.is_closed() {
            pools.remove(&pool_key);
        } else {
            return Some(existing.clone());
        }
    }

    pools.insert(pool_key, pool.clone());
    None
}

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
    let pool_key = PoolKey::new(path.as_ref(), max_connections);

    if let Some(pool) = get_shared_pool(&pool_key) {
        bootstrap_pool_for_target(&pool, bootstrap_target).await?;
        return Ok(pool);
    }

    let created_pool = SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(file_backed_connect_options(&pool_key.path))
        .await?;
    bootstrap_pool_for_target(&created_pool, bootstrap_target).await?;

    if let Some(existing_pool) = insert_shared_pool(pool_key, &created_pool) {
        created_pool.close().await;
        bootstrap_pool_for_target(&existing_pool, bootstrap_target).await?;
        return Ok(existing_pool);
    }

    Ok(created_pool)
}

async fn bootstrap_pool_for_target(
    pool: &SqlitePool,
    bootstrap_target: BootstrapTarget,
) -> Result<(), sqlx::Error> {
    match bootstrap_target {
        BootstrapTarget::None => Ok(()),
        BootstrapTarget::Main => setup::bootstrap_pool(pool).await,
        BootstrapTarget::Tasks => setup::bootstrap_tasks_pool(pool).await,
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

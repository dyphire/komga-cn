use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};

use crate::context::SqlitePersistenceContext;
use crate::filesystem::remove_file_after_release;
use crate::sqlite::setup;

pub const DEFAULT_MAX_CONNECTIONS: u32 = 4;
pub const WRITE_MAX_CONNECTIONS: u32 = 1;

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

pub async fn connect_shared_pool(
    path: impl AsRef<Path>,
    max_connections: u32,
) -> Result<SqlitePool, sqlx::Error> {
    connect_bootstrapped_pool(path, max_connections, BootstrapTarget::None).await
}

pub async fn connect_read_pool(path: impl AsRef<Path>) -> Result<SqlitePool, sqlx::Error> {
    connect_shared_pool(path, default_read_max_connections()).await
}

pub async fn connect_write_pool(path: impl AsRef<Path>) -> Result<SqlitePool, sqlx::Error> {
    connect_shared_pool(path, WRITE_MAX_CONNECTIONS).await
}

pub async fn connect_main_write_context(
    path: impl AsRef<Path>,
) -> Result<SqlitePersistenceContext, sqlx::Error> {
    let pool =
        connect_bootstrapped_pool(path, WRITE_MAX_CONNECTIONS, BootstrapTarget::Main).await?;
    Ok(SqlitePersistenceContext::new(pool))
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

#[derive(Copy, Clone)]
enum BootstrapTarget {
    None,
    Main,
}

async fn bootstrap_pool_for_target(
    pool: &SqlitePool,
    bootstrap_target: BootstrapTarget,
) -> Result<(), sqlx::Error> {
    match bootstrap_target {
        BootstrapTarget::None => Ok(()),
        BootstrapTarget::Main => setup::bootstrap_pool(pool).await,
    }
}

pub async fn connect_task_pool(
    path: impl AsRef<Path>,
    max_connections: u32,
) -> Result<SqlitePool, sqlx::Error> {
    SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(file_backed_connect_options(path))
        .await
}

pub async fn connect_task_write_pool(path: impl AsRef<Path>) -> Result<SqlitePool, sqlx::Error> {
    connect_task_pool(path, WRITE_MAX_CONNECTIONS).await
}

pub async fn connect_test_pool(
    path: impl AsRef<Path>,
    max_connections: u32,
) -> Result<SqlitePool, sqlx::Error> {
    connect_task_pool(path, max_connections).await
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

pub fn evict_shared_pools_for_paths(paths: &[PathBuf]) -> Vec<SqlitePool> {
    if paths.is_empty() {
        return Vec::new();
    }

    let target_paths = paths
        .iter()
        .map(|path| absolute_pool_path(path.as_path()))
        .collect::<HashSet<_>>();
    let mut pools = shared_pools()
        .lock()
        .expect("shared sqlite pool map lock should not be poisoned");
    let matching_keys = pools
        .keys()
        .filter(|pool_key| target_paths.contains(&pool_key.path))
        .cloned()
        .collect::<Vec<_>>();
    let mut removed = Vec::with_capacity(matching_keys.len());

    for pool_key in matching_keys {
        if let Some(pool) = pools.remove(&pool_key) {
            removed.push(pool);
        }
    }

    removed
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedSqlitePoolSnapshot {
    pub path: PathBuf,
    pub max_connections: u32,
    pub min_connections: u32,
    pub total_connections: u32,
    pub idle_connections: u32,
    pub in_use_connections: u32,
    pub is_closed: bool,
}

pub fn shared_pool_snapshots_for_paths(paths: &[PathBuf]) -> Vec<SharedSqlitePoolSnapshot> {
    if paths.is_empty() {
        return Vec::new();
    }

    let target_paths = paths
        .iter()
        .map(|path| absolute_pool_path(path.as_path()))
        .collect::<HashSet<_>>();
    let mut pools = shared_pools()
        .lock()
        .expect("shared sqlite pool map lock should not be poisoned");

    let closed_keys = pools
        .iter()
        .filter(|(_, pool)| pool.is_closed())
        .map(|(pool_key, _)| pool_key.clone())
        .collect::<Vec<_>>();
    for pool_key in closed_keys {
        pools.remove(&pool_key);
    }

    pools
        .iter()
        .filter(|(pool_key, _)| target_paths.contains(&pool_key.path))
        .map(|(pool_key, pool)| {
            let total_connections = pool.size();
            let idle_connections = u32::try_from(pool.num_idle()).unwrap_or(u32::MAX);
            SharedSqlitePoolSnapshot {
                path: pool_key.path.clone(),
                max_connections: pool.options().get_max_connections(),
                min_connections: pool.options().get_min_connections(),
                total_connections,
                idle_connections,
                in_use_connections: total_connections.saturating_sub(idle_connections),
                is_closed: pool.is_closed(),
            }
        })
        .collect()
}

pub fn file_backed_connect_options(path: impl AsRef<Path>) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
}

/// Sizing heuristic for read-heavy pools: `max(4, NumCPU)`.
pub fn default_read_max_connections() -> u32 {
    let minimum = usize::try_from(DEFAULT_MAX_CONNECTIONS).unwrap_or(usize::MAX);
    let available = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(minimum);

    u32::try_from(available.max(minimum)).unwrap_or(u32::MAX)
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

pub struct SqliteTempPool {
    pool: SqlitePool,
    db_path: PathBuf,
}

impl SqliteTempPool {
    pub async fn new(case_id: &str) -> Result<Self, sqlx::Error> {
        let db_path = deterministic_temp_db_path(case_id);
        let pool = SqlitePoolOptions::new()
            .max_connections(DEFAULT_MAX_CONNECTIONS)
            .connect_with(file_backed_connect_options(&db_path))
            .await?;

        Ok(Self { pool, db_path })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub async fn cleanup(self) {
        let Self { pool, db_path } = self;

        let shared_pools = evict_shared_pools_for_paths(std::slice::from_ref(&db_path));
        for shared_pool in shared_pools {
            shared_pool.close().await;
        }

        pool.close().await;
        drop(pool);

        for path in sqlite_sidecar_paths(&db_path) {
            remove_file_after_release(&path).unwrap_or_else(|error| {
                panic!(
                    "failed to remove temp sqlite file {} after pool close: {error}",
                    path.display()
                )
            });
        }
    }
}

fn sqlite_sidecar_paths(db_path: &Path) -> [PathBuf; 4] {
    let base_name = db_path.to_string_lossy().to_string();
    [
        PathBuf::from(format!("{base_name}-wal")),
        PathBuf::from(format!("{base_name}-shm")),
        PathBuf::from(format!("{base_name}-journal")),
        db_path.to_path_buf(),
    ]
}

fn deterministic_temp_db_path(case_id: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after epoch")
        .as_nanos();
    let pid = std::process::id();
    std::env::temp_dir().join(format!("komga-sqlite-topology-{case_id}-{pid}-{nanos}.db"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::setup;

    #[tokio::test]
    async fn sqlite_temp_pool_cleanup_removes_wal_sidecars() {
        let temp_pool = SqliteTempPool::new("cleanup-removes-wal-sidecars")
            .await
            .expect("temp pool should open");
        let db_path = temp_pool.db_path().to_path_buf();
        let wal_path = PathBuf::from(format!("{}-wal", db_path.display()));
        let shm_path = PathBuf::from(format!("{}-shm", db_path.display()));

        setup::bootstrap_pool(temp_pool.pool())
            .await
            .expect("temp pool should bootstrap main schema");
        sqlx::query("INSERT INTO USER (ID, EMAIL, PASSWORD) VALUES (?, ?, ?)")
            .bind("cleanup-user")
            .bind("cleanup-user@example.org")
            .bind("test-password")
            .execute(temp_pool.pool())
            .await
            .expect("fixture user row should be inserted");

        assert!(
            wal_path.exists() || shm_path.exists(),
            "WAL-backed temp pool should materialize sidecar files before cleanup",
        );

        temp_pool.cleanup().await;

        assert!(
            !db_path.exists(),
            "temp pool cleanup should remove the sqlite database file",
        );
        assert!(
            !wal_path.exists(),
            "temp pool cleanup should remove the WAL sidecar file",
        );
        assert!(
            !shm_path.exists(),
            "temp pool cleanup should remove the shared-memory sidecar file",
        );
    }

    #[tokio::test]
    async fn shared_pool_snapshots_report_live_sqlx_pool_stats_by_path_and_capacity() {
        let temp_pool = SqliteTempPool::new("shared-pool-snapshot-live-stats")
            .await
            .expect("temp pool should open");

        let _pool_one = connect_shared_pool(temp_pool.db_path(), 1)
            .await
            .expect("shared pool with max=1 should open");
        let _pool_two = connect_shared_pool(temp_pool.db_path(), 2)
            .await
            .expect("shared pool with max=2 should open");

        let snapshots = shared_pool_snapshots_for_paths(&[temp_pool.db_path().to_path_buf()]);
        assert_eq!(
            snapshots.len(),
            2,
            "each shared sqlx pool topology should be reported separately"
        );

        let snapshot_one = snapshots
            .iter()
            .find(|snapshot| snapshot.max_connections == 1)
            .expect("max=1 snapshot should exist");
        assert_eq!(snapshot_one.path, absolute_pool_path(temp_pool.db_path()));
        assert!(snapshot_one.total_connections >= 1);
        assert!(snapshot_one.idle_connections >= 1);
        assert_eq!(
            snapshot_one.total_connections,
            snapshot_one.idle_connections + snapshot_one.in_use_connections,
            "total connections should split into idle + in-use",
        );

        let snapshot_two = snapshots
            .iter()
            .find(|snapshot| snapshot.max_connections == 2)
            .expect("max=2 snapshot should exist");
        assert_eq!(snapshot_two.path, absolute_pool_path(temp_pool.db_path()));
        assert!(snapshot_two.total_connections >= 1);
        assert_eq!(
            snapshot_two.total_connections,
            snapshot_two.idle_connections + snapshot_two.in_use_connections,
            "total connections should split into idle + in-use",
        );

        temp_pool.cleanup().await;
    }
}

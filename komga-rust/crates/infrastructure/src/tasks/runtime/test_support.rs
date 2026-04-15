use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use komga_application::task_processing::TaskRuntimeContext;
use sqlx::SqlitePool;

use crate::sqlite::{
    connect_persistence_context, connect_tasks_pool, evict_shared_pools_for_paths,
};

pub(crate) struct RuntimeTestFixture {
    pub(crate) database_file: PathBuf,
    pub(crate) tasks_db_file: PathBuf,
    pub(crate) lucene_dir: PathBuf,
    pub(crate) library_root: PathBuf,
}

impl RuntimeTestFixture {
    pub(crate) fn new(case: &str) -> Self {
        Self {
            database_file: unique_temp_path(&format!("komga-{case}-main")),
            tasks_db_file: unique_temp_path(&format!("komga-{case}-tasks")),
            lucene_dir: unique_temp_path(&format!("komga-{case}-lucene")),
            library_root: unique_temp_path(&format!("komga-{case}-root")),
        }
    }

    pub(crate) async fn main_pool(&self) -> SqlitePool {
        connect_persistence_context(&self.database_file, 1)
            .await
            .expect("runtime test main db should bootstrap")
            .pool()
            .clone()
    }

    pub(crate) async fn tasks_pool(&self) -> SqlitePool {
        connect_tasks_pool(&self.tasks_db_file, 1)
            .await
            .expect("runtime test tasks db should bootstrap")
    }

    pub(crate) fn runtime_context(
        &self,
        consumes_queue: bool,
        owns_search_index: bool,
    ) -> TaskRuntimeContext {
        TaskRuntimeContext {
            database_file: self.database_file.clone(),
            tasks_db_file: self.tasks_db_file.clone(),
            lucene_data_directory: self.lucene_dir.clone(),
            consumes_queue,
            owns_main_database: true,
            owns_filesystem_scan_output: true,
            owns_sidecar_output: true,
            owns_search_index,
        }
    }

    pub(crate) async fn cleanup(self) {
        let db_paths = [self.database_file.clone(), self.tasks_db_file.clone()];
        for pool in evict_shared_pools_for_paths(&db_paths) {
            pool.close().await;
        }

        for db_path in db_paths {
            for sidecar in sqlite_sidecar_paths(db_path.as_path()) {
                let _ = std::fs::remove_file(sidecar);
            }
        }

        let _ = std::fs::remove_dir_all(self.library_root);
        let _ = std::fs::remove_dir_all(self.lucene_dir);
    }
}

fn unique_temp_path(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos()
    ))
}

fn sqlite_sidecar_paths(db_path: &Path) -> [PathBuf; 4] {
    let base = db_path.to_string_lossy().to_string();
    [
        db_path.to_path_buf(),
        PathBuf::from(format!("{base}-wal")),
        PathBuf::from(format!("{base}-shm")),
        PathBuf::from(format!("{base}-journal")),
    ]
}

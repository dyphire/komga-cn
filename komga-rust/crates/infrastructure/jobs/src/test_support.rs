use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{TaskRuntimeContext, TaskRuntimeContextParams, TaskRuntimeOwnership};
use komga_application::runtime_sse::RuntimeSseEventStore;
use komga_application::task_processing::{
    TaskExecutionResult, TaskProcessingError, TaskQueueRecord, finalize_task_execution,
};
use sqlx::SqlitePool;

use komga_infrastructure_base::DatabaseHandle;
use komga_infrastructure_base::sqlite::{
    connect_main_write_context, connect_task_pool, connect_task_write_pool, connect_write_pool,
    default_read_max_connections, evict_shared_pools_for_paths, schema,
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
        connect_main_write_context(&self.database_file)
            .await
            .expect("runtime test main db should bootstrap")
            .pool()
            .clone()
    }

    pub(crate) async fn tasks_pool(&self) -> SqlitePool {
        let pool = connect_write_pool(&self.tasks_db_file)
            .await
            .expect("runtime test tasks db should open");
        schema::bootstrap_tasks_pool(&pool)
            .await
            .expect("runtime test tasks db should bootstrap");
        pool
    }

    pub(crate) async fn runtime_context(
        &self,
        consumes_queue: bool,
        owns_search_index: bool,
    ) -> TaskRuntimeContext {
        let main_db = DatabaseHandle::file_backed(self.database_file.clone())
            .await
            .expect("runtime test main db should open");
        let task_write_pool = connect_task_write_pool(&self.database_file)
            .await
            .expect("runtime test private write pool should open");
        let task_read_pool = connect_task_pool(&self.database_file, default_read_max_connections())
            .await
            .expect("runtime test private read pool should open");
        TaskRuntimeContext::new(TaskRuntimeContextParams {
            main_db,
            tasks_db_file: self.tasks_db_file.clone(),
            lucene_data_directory: self.lucene_dir.clone(),
            consumes_queue,
            ownership: TaskRuntimeOwnership {
                owns_search_index,
                ..TaskRuntimeOwnership::all_owned()
            },
            task_pool_size: 1,
            task_write_pool,
            task_read_pool,
            runtime_events: Arc::new(RuntimeSseEventStore::default()),
        })
    }

    pub(crate) async fn cleanup(self) {
        let db_paths = [self.database_file.clone(), self.tasks_db_file.clone()];
        for pool in evict_shared_pools_for_paths(&db_paths) {
            pool.close().await;
        }

        for db_path in db_paths {
            let base = db_path.to_string_lossy().to_string();
            for sidecar in [
                db_path.clone(),
                PathBuf::from(format!("{base}-wal")),
                PathBuf::from(format!("{base}-shm")),
                PathBuf::from(format!("{base}-journal")),
            ] {
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

pub(crate) async fn execute_and_enqueue(
    scheduler: &komga_infrastructure_tasks::TaskQueueScheduler,
    runtime: &TaskRuntimeContext,
    task: &TaskQueueRecord,
) -> Option<Result<(), TaskProcessingError>> {
    let outcome = super::dispatch::TaskJobDispatcher::new(runtime.job())
        .execute_record(task)
        .await;
    Some(
        finalize_task_execution(
            scheduler,
            TaskExecutionResult {
                task: task.clone(),
                outcome,
            },
        )
        .await,
    )
}

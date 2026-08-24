use komga_application::runtime_sse::RuntimeSseEventStore;
use komga_infrastructure_base::DatabaseHandle;
use komga_infrastructure_base::{
    connect_task_pool, connect_task_write_pool, default_read_max_connections,
};
use komga_infrastructure_jobs::{TaskRuntimeContextParams, TaskRuntimeOwnership};
use komga_infrastructure_search::search_analyzer_version;

mod runtime_startup_contract_cases;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tantivy::Index;
use tantivy::schema::{STORED, STRING, Schema};

async fn runtime_task_context(
    config: &komga_config::env_config::RuntimeConfig,
) -> komga_infrastructure_jobs::TaskRuntimeContext {
    let task_write_pool = connect_task_write_pool(&config.database_file)
        .await
        .expect("test private write pool should open");
    let task_read_pool = connect_task_pool(&config.database_file, default_read_max_connections())
        .await
        .expect("test private read pool should open");
    komga_infrastructure_jobs::TaskRuntimeContext::new(TaskRuntimeContextParams {
        main_db: DatabaseHandle::file_backed(config.database_file.clone())
            .await
            .expect("test db should open"),
        tasks_db_file: config.tasks_db_file.clone(),
        lucene_data_directory: config.lucene_data_directory.clone(),
        consumes_queue: config
            .writer_decision(komga_config::writer_ownership::WriterKind::TasksDatabase)
            .allows_write(),
        ownership: TaskRuntimeOwnership {
            owns_main_database: config
                .writer_decision(komga_config::writer_ownership::WriterKind::MainDatabase)
                .allows_write(),
            owns_filesystem_scan_output: config
                .writer_decision(komga_config::writer_ownership::WriterKind::FilesystemScanOutput)
                .allows_write(),
            owns_sidecar_output: config
                .writer_decision(komga_config::writer_ownership::WriterKind::SidecarOutput)
                .allows_write(),
            owns_search_index: config
                .writer_decision(komga_config::writer_ownership::WriterKind::SearchIndex)
                .allows_write(),
        },
        task_pool_size: config.task_pool_size,
        task_write_pool,
        task_read_pool,
        runtime_events: Arc::new(RuntimeSseEventStore::default()),
    })
}

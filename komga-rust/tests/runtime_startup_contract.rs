use komga_infrastructure::database_handle::DatabaseHandle;
use komga_infrastructure::search::analyzer_profiles::search_analyzer_version;
use komga_infrastructure::sqlite::{
    connect_task_pool, connect_task_write_pool, default_read_max_connections,
};

mod runtime_startup_contract_cases;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tantivy::Index;
use tantivy::schema::{STORED, STRING, Schema};

async fn runtime_task_context(
    config: &komga_config::env_config::RuntimeConfig,
) -> komga_infrastructure::task_queue::TaskRuntimeContext {
    let task_write_pool = connect_task_write_pool(&config.database_file)
        .await
        .expect("test private write pool should open");
    let task_read_pool = connect_task_pool(&config.database_file, default_read_max_connections())
        .await
        .expect("test private read pool should open");
    komga_infrastructure::task_queue::TaskRuntimeContext {
        main_db: DatabaseHandle::file_backed(config.database_file.clone())
            .await
            .expect("test db should open"),
        tasks_db_file: config.tasks_db_file.clone(),
        lucene_data_directory: config.lucene_data_directory.clone(),
        consumes_queue: matches!(
            config.writer_decision(komga_config::writer_ownership::WriterKind::TasksDatabase),
            komga_config::writer_ownership::WriterDecision::Allowed
                | komga_config::writer_ownership::WriterDecision::Isolated
        ),
        owns_main_database: matches!(
            config.writer_decision(komga_config::writer_ownership::WriterKind::MainDatabase),
            komga_config::writer_ownership::WriterDecision::Allowed
                | komga_config::writer_ownership::WriterDecision::Isolated
        ),
        owns_filesystem_scan_output: matches!(
            config
                .writer_decision(komga_config::writer_ownership::WriterKind::FilesystemScanOutput),
            komga_config::writer_ownership::WriterDecision::Allowed
                | komga_config::writer_ownership::WriterDecision::Isolated
        ),
        owns_sidecar_output: matches!(
            config.writer_decision(komga_config::writer_ownership::WriterKind::SidecarOutput),
            komga_config::writer_ownership::WriterDecision::Allowed
                | komga_config::writer_ownership::WriterDecision::Isolated
        ),
        owns_search_index: matches!(
            config.writer_decision(komga_config::writer_ownership::WriterKind::SearchIndex),
            komga_config::writer_ownership::WriterDecision::Allowed
                | komga_config::writer_ownership::WriterDecision::Isolated
        ),
        task_pool_size: config.task_pool_size,
        task_write_pool,
        task_read_pool,
    }
}

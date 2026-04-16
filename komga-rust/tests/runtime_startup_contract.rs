use komga_infrastructure::search::analyzer_profiles::search_analyzer_version;

mod runtime_startup_contract_cases;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tantivy::Index;
use tantivy::schema::{STORED, STRING, Schema};

fn runtime_task_context(
    config: &komga_config::env_config::RuntimeConfig,
) -> komga_application::task_processing::TaskRuntimeContext {
    komga_application::task_processing::TaskRuntimeContext {
        database_file: config.database_file.clone(),
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
            config.writer_decision(komga_config::writer_ownership::WriterKind::FilesystemScanOutput),
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
    }
}

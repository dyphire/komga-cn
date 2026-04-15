use komga_rust::infrastructure::search::search_analyzer_version;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tantivy::Index;
use tantivy::schema::{STORED, STRING, Schema};

fn runtime_task_context(config: &komga_rust::config::RuntimeConfig) -> komga_rust::application::task_processing::TaskRuntimeContext {
    komga_rust::application::task_processing::TaskRuntimeContext {
        database_file: config.database_file.clone(),
        tasks_db_file: config.tasks_db_file.clone(),
        lucene_data_directory: config.lucene_data_directory.clone(),
        consumes_queue: matches!(
            config.writer_decision(komga_rust::config::WriterKind::TasksDatabase),
            komga_rust::config::WriterDecision::Allowed
                | komga_rust::config::WriterDecision::Isolated
        ),
        owns_main_database: matches!(
            config.writer_decision(komga_rust::config::WriterKind::MainDatabase),
            komga_rust::config::WriterDecision::Allowed
                | komga_rust::config::WriterDecision::Isolated
        ),
        owns_filesystem_scan_output: matches!(
            config.writer_decision(komga_rust::config::WriterKind::FilesystemScanOutput),
            komga_rust::config::WriterDecision::Allowed
                | komga_rust::config::WriterDecision::Isolated
        ),
        owns_sidecar_output: matches!(
            config.writer_decision(komga_rust::config::WriterKind::SidecarOutput),
            komga_rust::config::WriterDecision::Allowed
                | komga_rust::config::WriterDecision::Isolated
        ),
        owns_search_index: matches!(
            config.writer_decision(komga_rust::config::WriterKind::SearchIndex),
            komga_rust::config::WriterDecision::Allowed
                | komga_rust::config::WriterDecision::Isolated
        ),
    }
}

#[path = "runtime_startup_contract/cli_preflight.rs"]
mod cli_preflight;
#[path = "runtime_startup_contract/config_resolution.rs"]
mod config_resolution;
#[path = "runtime_startup_contract/lifecycle_logging.rs"]
mod lifecycle_logging;
#[path = "runtime_startup_contract/logging_foundation.rs"]
mod logging_foundation;
#[path = "runtime_startup_contract/search_lifecycle.rs"]
mod search_lifecycle;
#[path = "runtime_startup_contract/support.rs"]
mod support;
#[path = "runtime_startup_contract/worker_lifecycle.rs"]
mod worker_lifecycle;

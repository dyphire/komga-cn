use komga_application::task_processing::TaskRuntimeContext;
use komga_config::env_config::RuntimeConfig;
use komga_config::writer_ownership::{WriterDecision, WriterKind};

pub(crate) fn task_runtime_context(config: &RuntimeConfig) -> TaskRuntimeContext {
    TaskRuntimeContext {
        database_file: config.database_file.clone(),
        tasks_db_file: config.tasks_db_file.clone(),
        lucene_data_directory: config.lucene_data_directory.clone(),
        consumes_queue: matches!(
            config.writer_decision(WriterKind::TasksDatabase),
            WriterDecision::Allowed | WriterDecision::Isolated
        ),
        owns_main_database: matches!(
            config.writer_decision(WriterKind::MainDatabase),
            WriterDecision::Allowed | WriterDecision::Isolated
        ),
        owns_filesystem_scan_output: matches!(
            config.writer_decision(WriterKind::FilesystemScanOutput),
            WriterDecision::Allowed | WriterDecision::Isolated
        ),
        owns_sidecar_output: matches!(
            config.writer_decision(WriterKind::SidecarOutput),
            WriterDecision::Allowed | WriterDecision::Isolated
        ),
        owns_search_index: matches!(
            config.writer_decision(WriterKind::SearchIndex),
            WriterDecision::Allowed | WriterDecision::Isolated
        ),
        task_pool_size: config.task_pool_size,
    }
}

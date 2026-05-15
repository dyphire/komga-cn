use komga_config::env_config::RuntimeConfig;
use komga_config::writer_ownership::{WriterDecision, WriterKind};
use komga_infrastructure::database_handle::DatabaseHandle;
use komga_infrastructure::sqlite::{
    connect_task_pool, connect_task_write_pool, default_read_max_connections,
};
use komga_infrastructure::task_queue::{TaskRuntimeContext, TaskRuntimeOwnershipOverrides};

pub(crate) async fn task_runtime_context(config: &RuntimeConfig) -> TaskRuntimeContext {
    let main_db = DatabaseHandle::file_backed(config.database_file.clone())
        .await
        .expect("failed to open main database");
    let task_write_pool = connect_task_write_pool(main_db.database_file())
        .await
        .expect("failed to create private write pool");
    let task_read_pool = connect_task_pool(main_db.database_file(), default_read_max_connections())
        .await
        .expect("failed to create private read pool");
    TaskRuntimeContext::new(
        main_db,
        config.tasks_db_file.clone(),
        config.lucene_data_directory.clone(),
        matches!(
            config.writer_decision(WriterKind::TasksDatabase),
            WriterDecision::Allowed | WriterDecision::Isolated
        ),
        config.task_pool_size,
        task_write_pool,
        task_read_pool,
    )
    .with_ownership_overrides(TaskRuntimeOwnershipOverrides {
        owns_main_database: Some(matches!(
            config.writer_decision(WriterKind::MainDatabase),
            WriterDecision::Allowed | WriterDecision::Isolated
        )),
        owns_filesystem_scan_output: Some(matches!(
            config.writer_decision(WriterKind::FilesystemScanOutput),
            WriterDecision::Allowed | WriterDecision::Isolated
        )),
        owns_sidecar_output: Some(matches!(
            config.writer_decision(WriterKind::SidecarOutput),
            WriterDecision::Allowed | WriterDecision::Isolated
        )),
        owns_search_index: Some(matches!(
            config.writer_decision(WriterKind::SearchIndex),
            WriterDecision::Allowed | WriterDecision::Isolated
        )),
    })
}

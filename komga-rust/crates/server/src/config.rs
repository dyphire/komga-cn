use std::sync::Arc;

use komga_application::runtime_sse::RuntimeSseEventSink;
use komga_config::env_config::RuntimeConfig;
use komga_config::writer_ownership::WriterKind;
use komga_infrastructure_base::{
    DatabaseHandle, connect_task_pool, connect_task_write_pool, default_read_max_connections,
};
use komga_infrastructure_jobs::{
    TaskRuntimeContext, TaskRuntimeContextParams, TaskRuntimeOwnership,
};

pub(crate) async fn task_runtime_context(
    config: &RuntimeConfig,
    runtime_events: Arc<dyn RuntimeSseEventSink>,
) -> TaskRuntimeContext {
    let main_db = DatabaseHandle::file_backed(config.database_file.clone())
        .await
        .expect("failed to open main database");
    let task_write_pool = connect_task_write_pool(main_db.database_file())
        .await
        .expect("failed to create private write pool");
    let task_read_pool = connect_task_pool(main_db.database_file(), default_read_max_connections())
        .await
        .expect("failed to create private read pool");
    TaskRuntimeContext::new(TaskRuntimeContextParams {
        main_db,
        tasks_db_file: config.tasks_db_file.clone(),
        lucene_data_directory: config.lucene_data_directory.clone(),
        consumes_queue: config
            .writer_decision(WriterKind::TasksDatabase)
            .allows_write(),
        ownership: TaskRuntimeOwnership {
            owns_main_database: config
                .writer_decision(WriterKind::MainDatabase)
                .allows_write(),
            owns_filesystem_scan_output: config
                .writer_decision(WriterKind::FilesystemScanOutput)
                .allows_write(),
            owns_sidecar_output: config
                .writer_decision(WriterKind::SidecarOutput)
                .allows_write(),
            owns_search_index: config
                .writer_decision(WriterKind::SearchIndex)
                .allows_write(),
        },
        task_pool_size: config.task_pool_size,
        task_write_pool,
        task_read_pool,
        runtime_events,
    })
}

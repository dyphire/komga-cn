use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use komga_application::runtime_sse::{
    RuntimeSseEvent, RuntimeSseEventLog, RuntimeSseEventSink, RuntimeSseEventStore,
};
use komga_application::task_processing::TaskQueueRecord;
use komga_config::profile::RuntimeMode;
use komga_config::writer_ownership::WriterOwnershipPolicy;
use komga_infrastructure_base::DatabaseHandle;
use komga_infrastructure_base::{
    connect_task_pool, connect_task_write_pool, default_read_max_connections,
};
use komga_infrastructure_jobs::{
    TaskRuntimeContext, TaskRuntimeContextParams, TaskRuntimeOwnership,
};
use komga_infrastructure_search::{
    SearchEntityType, SearchIndexLifecycle, search_analyzer_version,
};
use komga_infrastructure_tasks::TaskQueueScheduler;
use serde_json::{Value, json};
use sqlx::Row;
use std::fs;
use std::sync::Arc;
use tower::util::ServiceExt;

mod support;

use support::fixture::TestFixture;
use support::runtime_router_contract_support::{
    RuntimeDbPaths, log_capture::*, media_file_fixtures::*, response_helpers::*,
};
use support::sqlite::connect_test_pool;

mod task_runtime_contract_cases;

const ANALYZER_VERSION_MARKER_FILE: &str = ".komga-search-analyzer-version";

async fn runtime_task_context(paths: &RuntimeDbPaths) -> TaskRuntimeContext {
    runtime_task_context_with(
        paths,
        TaskRuntimeOwnership::all_owned(),
        Arc::new(RuntimeSseEventStore::default()),
        1,
    )
    .await
}

async fn runtime_task_context_with(
    paths: &RuntimeDbPaths,
    ownership: TaskRuntimeOwnership,
    runtime_events: Arc<dyn RuntimeSseEventSink>,
    task_pool_size: usize,
) -> TaskRuntimeContext {
    let task_write_pool = connect_task_write_pool(&paths.main_db)
        .await
        .expect("test private write pool should open");
    let task_read_pool = connect_task_pool(&paths.main_db, default_read_max_connections())
        .await
        .expect("test private read pool should open");
    TaskRuntimeContext::new(TaskRuntimeContextParams {
        main_db: DatabaseHandle::file_backed(paths.main_db.clone())
            .await
            .expect("test db should open"),
        tasks_db_file: paths.tasks_db.clone(),
        lucene_data_directory: paths.config_dir.join("lucene"),
        consumes_queue: true,
        ownership,
        task_pool_size,
        task_write_pool,
        task_read_pool,
        runtime_events,
    })
}

async fn runtime_task_context_with_runtime_events(
    paths: &RuntimeDbPaths,
    runtime_events: Arc<dyn RuntimeSseEventSink>,
) -> TaskRuntimeContext {
    runtime_task_context_with(paths, TaskRuntimeOwnership::all_owned(), runtime_events, 1).await
}

async fn runtime_task_context_with_ownership(
    paths: &RuntimeDbPaths,
    ownership: TaskRuntimeOwnership,
) -> TaskRuntimeContext {
    runtime_task_context_with(
        paths,
        ownership,
        Arc::new(RuntimeSseEventStore::default()),
        1,
    )
    .await
}

async fn runtime_task_context_from_config(
    config: &komga_config::env_config::RuntimeConfig,
) -> TaskRuntimeContext {
    runtime_task_context_from_config_with_task_pool_size(config, config.task_pool_size).await
}

async fn runtime_task_context_from_config_with_task_pool_size(
    config: &komga_config::env_config::RuntimeConfig,
    task_pool_size: usize,
) -> TaskRuntimeContext {
    let task_write_pool = connect_task_write_pool(&config.database_file)
        .await
        .expect("test private write pool should open");
    let task_read_pool = connect_task_pool(&config.database_file, default_read_max_connections())
        .await
        .expect("test private read pool should open");
    TaskRuntimeContext::new(TaskRuntimeContextParams {
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
        task_pool_size,
        task_write_pool,
        task_read_pool,
        runtime_events: Arc::new(RuntimeSseEventStore::default()),
    })
}

fn write_stale_analyzer_version_marker(index_dir: &std::path::Path) {
    fs::write(
        index_dir.join(ANALYZER_VERSION_MARKER_FILE),
        search_analyzer_version().saturating_add(1).to_string(),
    )
    .expect("stale analyzer version marker should be written");
}

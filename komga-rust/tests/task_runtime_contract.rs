use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use komga_application::runtime_sse::{RuntimeSseEvent, RuntimeSseEventLog, RuntimeSseEventSink};
use komga_application::task_processing::{CleanupEmptySetsPolicy, TaskQueueRecord};
use komga_config::profile::RuntimeMode;
use komga_config::writer_ownership::WriterOwnershipPolicy;
use komga_infrastructure::{
    DatabaseHandle, SearchEntityType, SearchIndexLifecycle, search_analyzer_version,
};
use komga_infrastructure::{TaskQueueScheduler, TaskRuntimeContext, TaskRuntimeOwnershipOverrides};
use komga_infrastructure::{
    connect_task_pool, connect_task_write_pool, default_read_max_connections,
};
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
    let task_write_pool = connect_task_write_pool(&paths.main_db)
        .await
        .expect("test private write pool should open");
    let task_read_pool = connect_task_pool(&paths.main_db, default_read_max_connections())
        .await
        .expect("test private read pool should open");
    TaskRuntimeContext::new(
        DatabaseHandle::file_backed(paths.main_db.clone())
            .await
            .expect("test db should open"),
        paths.tasks_db.clone(),
        paths.config_dir.join("lucene"),
        true,
        1,
        task_write_pool,
        task_read_pool,
    )
}

async fn runtime_task_context_with_runtime_events(
    paths: &RuntimeDbPaths,
    runtime_events: Arc<dyn RuntimeSseEventSink>,
) -> TaskRuntimeContext {
    runtime_task_context(paths)
        .await
        .with_runtime_events(runtime_events)
}

async fn runtime_task_context_with_cleanup_policy(
    paths: &RuntimeDbPaths,
    cleanup_policy: CleanupEmptySetsPolicy,
) -> TaskRuntimeContext {
    runtime_task_context(paths)
        .await
        .with_cleanup_empty_sets_policy(cleanup_policy)
}

async fn runtime_task_context_with_overrides(
    paths: &RuntimeDbPaths,
    overrides: TaskRuntimeOwnershipOverrides,
) -> TaskRuntimeContext {
    runtime_task_context(paths)
        .await
        .with_ownership_overrides(overrides)
}

async fn runtime_task_context_from_config(
    config: &komga_config::env_config::RuntimeConfig,
) -> TaskRuntimeContext {
    let task_write_pool = connect_task_write_pool(&config.database_file)
        .await
        .expect("test private write pool should open");
    let task_read_pool = connect_task_pool(&config.database_file, default_read_max_connections())
        .await
        .expect("test private read pool should open");
    TaskRuntimeContext::new(
        DatabaseHandle::file_backed(config.database_file.clone())
            .await
            .expect("test db should open"),
        config.tasks_db_file.clone(),
        config.lucene_data_directory.clone(),
        matches!(
            config.writer_decision(komga_config::writer_ownership::WriterKind::TasksDatabase),
            komga_config::writer_ownership::WriterDecision::Allowed
                | komga_config::writer_ownership::WriterDecision::Isolated
        ),
        config.task_pool_size,
        task_write_pool,
        task_read_pool,
    )
    .with_ownership_overrides(TaskRuntimeOwnershipOverrides {
        owns_main_database: Some(matches!(
            config.writer_decision(komga_config::writer_ownership::WriterKind::MainDatabase),
            komga_config::writer_ownership::WriterDecision::Allowed
                | komga_config::writer_ownership::WriterDecision::Isolated
        )),
        owns_filesystem_scan_output: Some(matches!(
            config
                .writer_decision(komga_config::writer_ownership::WriterKind::FilesystemScanOutput),
            komga_config::writer_ownership::WriterDecision::Allowed
                | komga_config::writer_ownership::WriterDecision::Isolated
        )),
        owns_sidecar_output: Some(matches!(
            config.writer_decision(komga_config::writer_ownership::WriterKind::SidecarOutput),
            komga_config::writer_ownership::WriterDecision::Allowed
                | komga_config::writer_ownership::WriterDecision::Isolated
        )),
        owns_search_index: Some(matches!(
            config.writer_decision(komga_config::writer_ownership::WriterKind::SearchIndex),
            komga_config::writer_ownership::WriterDecision::Allowed
                | komga_config::writer_ownership::WriterDecision::Isolated
        )),
    })
}

fn write_stale_analyzer_version_marker(index_dir: &std::path::Path) {
    fs::write(
        index_dir.join(ANALYZER_VERSION_MARKER_FILE),
        search_analyzer_version().saturating_add(1).to_string(),
    )
    .expect("stale analyzer version marker should be written");
}

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use komga_application::task_processing::TaskQueueRecord;
use komga_config::profile::RuntimeMode;
use komga_config::writer_ownership::WriterOwnershipPolicy;
use komga_infrastructure::database_handle::DatabaseHandle;
use komga_infrastructure::search::analyzer_profiles::search_analyzer_version;
use komga_infrastructure::search::index_lifecycle::{SearchEntityType, SearchIndexLifecycle};
use komga_infrastructure::sqlite::{
    connect_task_pool, connect_task_write_pool, connect_test_pool, default_read_max_connections,
};
use komga_infrastructure::task_queue::TaskRuntimeContext;
use komga_infrastructure::task_queue::queue_scheduler::TaskQueueScheduler;
use komga_server::app::{
    build_router_with_config, build_router_without_runtime_workers_for_contract,
};
use serde_json::{Value, json};
use sqlx::Row;
use std::fs;
use tower::util::ServiceExt;

mod support;

use support::runtime_router_contract_support::{
    RuntimeDbPaths, contract_seed::*, fixture_bootstrap::*, log_capture::*, media_file_fixtures::*,
    response_helpers::*, user_auth::*,
};

mod task_runtime_contract_cases;

const ANALYZER_VERSION_MARKER_FILE: &str = ".komga-search-analyzer-version";

async fn runtime_task_context(paths: &RuntimeDbPaths) -> TaskRuntimeContext {
    let task_write_pool = connect_task_write_pool(&paths.main_db)
        .await
        .expect("test private write pool should open");
    let task_read_pool = connect_task_pool(&paths.main_db, default_read_max_connections())
        .await
        .expect("test private read pool should open");
    TaskRuntimeContext {
        main_db: DatabaseHandle::file_backed(paths.main_db.clone())
            .await
            .expect("test db should open"),
        tasks_db_file: paths.tasks_db.clone(),
        lucene_data_directory: paths.config_dir.join("lucene"),
        consumes_queue: true,
        owns_main_database: true,
        owns_filesystem_scan_output: true,
        owns_sidecar_output: true,
        owns_search_index: true,
        task_pool_size: 1,
        task_write_pool,
        task_read_pool,
    }
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
    TaskRuntimeContext {
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

fn write_stale_analyzer_version_marker(index_dir: &std::path::Path) {
    fs::write(
        index_dir.join(ANALYZER_VERSION_MARKER_FILE),
        search_analyzer_version().saturating_add(1).to_string(),
    )
    .expect("stale analyzer version marker should be written");
}

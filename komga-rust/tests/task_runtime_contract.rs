use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::application::task_processing::TaskRuntimeContext;
use komga_rust::config::{RuntimeMode, WriterOwnershipPolicy};
use komga_rust::infrastructure::search::search_analyzer_version;
use komga_rust::infrastructure::sqlite::connect_pool;
use komga_rust::{SearchEntityType, SearchIndexLifecycle, TaskQueueRecord, TaskQueueScheduler};
use komga_server::app::{
    build_router_with_config, build_router_without_runtime_workers_for_contract,
};
use serde_json::{Value, json};
use sqlx::Row;
use std::fs;
use tower::util::ServiceExt;

#[path = "support/runtime_router_contract_support.rs"]
pub mod runtime_router_contract_support;

use runtime_router_contract_support::*;

#[path = "task_runtime_contract/import_and_transient.rs"]
mod import_and_transient;
#[path = "task_runtime_contract/lifecycle_logging.rs"]
mod lifecycle_logging;
#[path = "task_runtime_contract/metadata_aggregation.rs"]
mod metadata_aggregation;
#[path = "task_runtime_contract/ownership_guards.rs"]
mod ownership_guards;
#[path = "task_runtime_contract/search_index_lifecycle.rs"]
mod search_index_lifecycle;
#[path = "task_runtime_contract/worker_lifecycle_logging.rs"]
mod worker_lifecycle_logging;

const ANALYZER_VERSION_MARKER_FILE: &str = ".komga-search-analyzer-version";

fn runtime_task_context(paths: &RuntimeDbPaths) -> TaskRuntimeContext {
    TaskRuntimeContext {
        database_file: paths.main_db.clone(),
        tasks_db_file: paths.tasks_db.clone(),
        lucene_data_directory: paths.config_dir.join("lucene"),
        consumes_queue: true,
        owns_main_database: true,
        owns_filesystem_scan_output: true,
        owns_sidecar_output: true,
        owns_search_index: true,
    }
}

fn write_stale_analyzer_version_marker(index_dir: &std::path::Path) {
    fs::write(
        index_dir.join(ANALYZER_VERSION_MARKER_FILE),
        search_analyzer_version().saturating_add(1).to_string(),
    )
    .expect("stale analyzer version marker should be written");
}

#[test]
fn task_runtime_contract_target_is_registered() {
    assert_required_target_declared("tasks/scanner", "task_runtime_contract");
}

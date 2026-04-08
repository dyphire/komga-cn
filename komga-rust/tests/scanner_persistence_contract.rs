use std::collections::BTreeMap;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::application::task_processing::TaskRuntimeContext;
use komga_rust::config::{RuntimeCli, RuntimeConfig};
use komga_rust::infrastructure::sqlite::connect_pool;
use komga_rust::scanner::{ScannerOptions, scan_root_folder};
use komga_rust::{SearchEntityType, SearchIndexLifecycle, TaskQueueRecord, TaskQueueScheduler};
use serde_json::{Value, json};
use sqlx::Row;
use zip::CompressionMethod;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

const MINIMAL_PNG_BYTES: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0,
    0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

#[path = "support/persistence_contract_fixture.rs"]
mod persistence_contract_fixture;
#[path = "scanner_persistence_contract/persistence_runtime_flow.rs"]
mod persistence_runtime_flow;
#[path = "scanner_persistence_contract/rescan_behaviors.rs"]
mod rescan_behaviors;
#[path = "scanner_persistence_contract/runtime_guards.rs"]
mod runtime_guards;
#[path = "scanner_persistence_contract/support.rs"]
mod support;
#[path = "scanner_persistence_contract/task_shape_contracts.rs"]
mod task_shape_contracts;

#[test]
fn scanner_persistence_contract_target_is_registered() {
    assert_required_target_declared("tasks/scanner", "scanner_persistence_contract");
}

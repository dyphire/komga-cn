use komga_rust::infrastructure::search::search_analyzer_version;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tantivy::Index;
use tantivy::schema::{STORED, STRING, Schema};

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

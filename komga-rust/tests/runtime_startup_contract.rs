use komga_rust::infrastructure::search::search_analyzer_version;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tantivy::Index;
use tantivy::schema::{STORED, STRING, Schema};

#[path = "runtime_startup_contract/config_resolution.rs"]
mod config_resolution;
#[path = "runtime_startup_contract/search_lifecycle.rs"]
mod search_lifecycle;
#[path = "runtime_startup_contract/support.rs"]
mod support;

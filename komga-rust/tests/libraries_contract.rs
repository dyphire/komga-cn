use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::infrastructure::sqlite::connect_pool;
use komga_server::app::build_router_with_config;
use serde_json::{Value, json};
use sha2::{Digest, Sha512};
use sqlx::Row;
use std::sync::{Mutex, OnceLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tower::util::ServiceExt;

#[path = "support/runtime_router_contract_support.rs"]
pub mod runtime_router_contract_support;

use runtime_router_contract_support::*;

#[path = "libraries_contract/kobo_book_metadata.rs"]
mod kobo_book_metadata;
#[path = "libraries_contract/kobo_library_sync.rs"]
mod kobo_library_sync;
#[path = "libraries_contract/kobo_misc.rs"]
mod kobo_misc;
#[path = "libraries_contract/libraries_api.rs"]
mod libraries_api;

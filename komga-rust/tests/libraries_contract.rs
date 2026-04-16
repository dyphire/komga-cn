use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_infrastructure::sqlite::connect_pool;
use komga_server::app::build_router_with_config;
use serde_json::{Value, json};
use sha2::{Digest, Sha512};
use sqlx::Row;
use std::sync::{Mutex, OnceLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tower::util::ServiceExt;

mod support;

use support::runtime_router_contract_support::{
    RuntimeDbPaths,
    contract_seed::*,
    fixture_bootstrap::*,
    log_capture::*,
    media_file_fixtures::*,
    metadata_series_seeding::*,
    response_helpers::*,
    user_auth::*,
};

mod libraries_contract_cases;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_infrastructure::sqlite::connect_pool;
use komga_server::app::build_router_with_config;
use serde_json::{Value, json};
use sqlx::Row;
use tower::util::ServiceExt;

mod support;

use support::runtime_router_contract_support::{
    RuntimeDbPaths, contract_seed::*, external_service_support::*, fixture_bootstrap::*,
    metadata_series_seeding::*, response_helpers::*, user_auth::*,
};

mod libraries_contract_cases;

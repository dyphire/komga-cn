use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_infrastructure::sqlite::connect_test_pool;
use komga_server::app::build_router_with_config;
use serde_json::{Value, json};
use sqlx::Row;
use tower::util::ServiceExt;
use zip::CompressionMethod;

mod support;

use support::runtime_router_contract_support::{
    RuntimeDbPaths, contract_seed::*, fixture_bootstrap::*, media_file_fixtures::*,
    response_helpers::*, user_auth::*,
};

mod readlists_collections_contract_cases;

#[test]
fn readlists_collections_contract_target_is_registered() {
    assert_required_target_declared("readlists/collections", "readlists_collections_contract");
}

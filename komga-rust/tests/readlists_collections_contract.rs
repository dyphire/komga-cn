use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_infrastructure::sqlite::connect_pool;
use komga_server::app::build_router_with_config;
use serde_json::{Value, json};
use sqlx::Row;
use tower::util::ServiceExt;
use zip::CompressionMethod;

mod support {
    pub mod persistence_contract_fixture;

    pub mod runtime_router_contract_support {
        use super::persistence_contract_fixture;

        pub(crate) use super::persistence_contract_fixture::RuntimeDbPaths;

        pub mod contract_seed;
        pub mod response_helpers;
        pub mod readlists_collections_fixture_bootstrap;
        pub mod readlists_collections_media_file_fixtures;
        pub mod readlists_collections_user_auth;
    }
}

use support::runtime_router_contract_support::{
    RuntimeDbPaths,
    contract_seed::*,
    readlists_collections_fixture_bootstrap::*,
    readlists_collections_media_file_fixtures::*,
    readlists_collections_user_auth::*,
    response_helpers::*,
};

mod readlists_collections_contract_cases;

#[test]
fn readlists_collections_contract_target_is_registered() {
    assert_required_target_declared("readlists/collections", "readlists_collections_contract");
}

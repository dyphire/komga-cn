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

mod support {
    pub mod persistence_contract_fixture;

    pub mod runtime_router_contract_support {
        use super::persistence_contract_fixture;

        pub(crate) use super::persistence_contract_fixture::RuntimeDbPaths;

        pub mod contract_seed;
        pub mod response_helpers;
        pub mod libraries_fixture_bootstrap;
        pub mod libraries_metadata_series_seeding;
        pub mod libraries_user_auth;
    }
}

use support::runtime_router_contract_support::{
    RuntimeDbPaths,
    contract_seed::*,
    libraries_fixture_bootstrap::*,
    libraries_metadata_series_seeding::*,
    libraries_user_auth::*,
    response_helpers::*,
};

mod libraries_contract_cases;

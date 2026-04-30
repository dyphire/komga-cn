use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
use komga_infrastructure::sqlite::connect_test_pool;
use serde_json::{Value, json};
use sqlx::Row;
use tower::util::ServiceExt;

mod support;

use support::fixture::TestFixture;
use support::runtime_router_contract_support::{
    RuntimeDbPaths, external_service_support::*, metadata_series_seeding::*, response_helpers::*,
    user_auth::*,
};

mod libraries_contract_cases;

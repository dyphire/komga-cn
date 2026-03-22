use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use komga_rust::application::discovery::{DiscoveryQueries, ReadListBooksQuery};
use komga_rust::domain::discovery::{DiscoveryError, DiscoveryQueryContext};
use komga_rust::persistence::discovery::SqliteDiscoveryAdapter;
use serde_json::Value;
use std::collections::BTreeSet;
use tower::util::ServiceExt;

mod compat;

const SEARCH_OWNERSHIP_HEADER: &str = "x-komga-compat-search-ownership";
const NATIVE_OWNERSHIP_MARKER: &str = "native-rust-owned";
const ADMIN_BASIC_AUTH: &str = "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=";
const USER_BASIC_AUTH: &str = "dXNlckBleGFtcGxlLm9yZzp1c2Vy";
const LIMITED_BASIC_AUTH: &str = "bGltaXRlZEBleGFtcGxlLm9yZzpsaW1pdGVk";
const RESTRICTED_BASIC_AUTH: &str = "cmVzdHJpY3RlZEBleGFtcGxlLm9yZzpyZXN0cmljdGVk";

use compat::http::{page_content_ids, response_json, session_token_for_basic_auth};

struct DirectBrowsePrincipalCase<'a> {
    name: &'a str,
    basic_auth: &'a str,
    expected_series_url: &'a str,
    expected_book_url: &'a str,
    expect_filtered_collection: bool,
    expect_filtered_readlist: bool,
}

struct DirectOneshotPrincipalCase<'a> {
    name: &'a str,
    basic_auth: &'a str,
    expected_series_url: &'a str,
    expected_book_url: &'a str,
}

#[path = "catalog_detail_shadow/detail.rs"]
mod detail;
#[path = "catalog_detail_shadow/helpers.rs"]
mod helpers;
#[path = "catalog_detail_shadow/ownership.rs"]
mod ownership;
#[path = "catalog_detail_shadow/readlist.rs"]
mod readlist;
#[path = "catalog_detail_shadow/readlist_detail.rs"]
mod readlist_detail;

#[tokio::test]
async fn phase6_readlist_detail_runtime_ownership_is_native() {
    readlist_detail::phase6_readlist_detail_runtime_ownership_is_native().await;
}

#[tokio::test]
async fn phase6_readlist_detail_404_and_filtered_semantics_match_contract() {
    readlist_detail::phase6_readlist_detail_404_and_filtered_semantics_match_contract().await;
}

#[tokio::test]
async fn phase6_regression_phase4_phase5_routes_remain_stable() {
    readlist::phase6_regression_phase4_phase5_routes_remain_stable().await;
}

#[tokio::test]
async fn phase6_adjacent_excluded_branches_still_emit_shadow_marker() {
    ownership::phase6_adjacent_excluded_branches_still_emit_shadow_marker().await;
}

#[tokio::test]
async fn series_detail_and_collections_are_native_owned() {
    detail::series_detail_and_collections_are_native_owned().await;
}

#[tokio::test]
async fn phase7_exact_oneshot_true_series_detail_is_native() {
    ownership::phase7_exact_oneshot_true_series_detail_is_native().await;
}

#[tokio::test]
async fn phase7_series_oneshot_query_variants_remain_non_native() {
    ownership::phase7_series_oneshot_query_variants_remain_non_native().await;
}

#[tokio::test]
async fn phase7_missing_and_restricted_series_oneshot_detail_matches_plain_detail_semantics() {
    detail::phase7_missing_and_restricted_series_oneshot_detail_matches_plain_detail_semantics()
        .await;
}

#[tokio::test]
async fn browse_oneshot_happy_path_uses_native_bootstrap_shape() {
    helpers::browse_oneshot_happy_path_uses_native_bootstrap_shape().await;
}

async fn get_response<S>(app: &S, token: &str, uri: &str) -> axum::response::Response
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    app.clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .header("X-Auth-Token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn post_response<S>(
    app: &S,
    token: &str,
    uri: &str,
    body: &str,
    ownership_header: Option<&str>,
) -> axum::response::Response
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let mut request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("X-Auth-Token", token)
        .header(header::CONTENT_TYPE, "application/json");

    if let Some(marker) = ownership_header {
        request = request.header(SEARCH_OWNERSHIP_HEADER, marker);
    }

    app.clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
}

fn assert_native_owned(response: &axum::response::Response, branch: &str) {
    assert!(
        response.headers().get(SEARCH_OWNERSHIP_HEADER).is_none(),
        "native-owned detail branch should not emit shadow marker: {branch}",
    );
}

fn array_ids(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .expect("payload should be an array")
        .iter()
        .map(|it| {
            it.get("id")
                .and_then(Value::as_str)
                .expect("payload id should be a string")
        })
        .collect()
}

fn string_array(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .expect("payload field should be an array")
        .iter()
        .map(|it| it.as_str().expect("array entry should be a string"))
        .collect()
}

fn assert_shadow_marker(response: &axum::response::Response, branch: &str) {
    let marker = response
        .headers()
        .get(SEARCH_OWNERSHIP_HEADER)
        .and_then(|value| value.to_str().ok());
    assert_eq!(
        marker,
        Some("shadow-java-writer"),
        "branch {branch} should emit explicit non-native marker, got {marker:?}",
    );
}

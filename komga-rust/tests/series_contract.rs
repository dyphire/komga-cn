use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_infrastructure::sqlite::connect_pool;
use komga_server::app::build_router_with_config;
use serde_json::{Value, json};
use sqlx::Row;
use std::io::Cursor;
use tokio::time::{Duration, sleep};
use tower::util::ServiceExt;
use zip::ZipArchive;

mod support {
    pub mod persistence_contract_fixture;

    pub mod runtime_router_contract_support {
        use super::persistence_contract_fixture;

        pub(crate) use super::persistence_contract_fixture::RuntimeDbPaths;

        pub mod contract_seed;
        pub mod response_helpers;
        pub mod series_fixture_bootstrap;
        pub mod series_media_file_fixtures;
        pub mod series_metadata_seeding;
        pub mod series_user_auth;
    }
}

use support::runtime_router_contract_support::{
    RuntimeDbPaths,
    contract_seed::*,
    response_helpers::*,
    series_fixture_bootstrap::*,
    series_media_file_fixtures::*,
    series_metadata_seeding::*,
    series_user_auth::*,
};

mod series_contract_cases;

#[test]
fn series_contract_target_is_registered() {
    assert_required_target_declared("series", "series_contract");
}

async fn update_series_search_fixture_title(paths: &RuntimeDbPaths, series_id: &str, title: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("series search parity db should open for title update");

    sqlx::query(
        "UPDATE SERIES_METADATA \
         SET TITLE = ?, TITLE_SORT = ? \
         WHERE SERIES_ID = ?",
    )
    .bind(title)
    .bind(title)
    .bind(series_id)
    .execute(&pool)
    .await
    .expect("series search parity title should update");

    pool.close().await;
}

async fn series_list_ids(
    app: &axum::Router,
    auth_token: &str,
    sort: Option<&str>,
    full_text_search: Option<&str>,
) -> Vec<String> {
    let mut uri = String::from("/api/v1/series/list?page=0&size=20");
    if let Some(sort) = sort {
        uri.push_str("&sort=");
        uri.push_str(sort);
    }

    let mut payload = json!({
        "condition": {
            "type": "Title",
            "operator": "contains",
            "value": "series"
        }
    });
    if let Some(search) = full_text_search {
        payload["fullTextSearch"] = Value::String(search.to_string());
    }

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("x-auth-token", auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(payload.to_string()))
                .expect("series search parity request should build"),
        )
        .await
        .expect("series search parity request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    payload
        .get("content")
        .and_then(Value::as_array)
        .expect("series search parity payload should expose content array")
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str).map(str::to_string))
        .collect()
}

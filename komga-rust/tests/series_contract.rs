use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::infrastructure::sqlite::connect_pool;
use komga_server::app::build_router_with_config;
use serde_json::{Value, json};
use sqlx::Row;
use std::io::Cursor;
use tokio::time::{Duration, sleep};
use tower::util::ServiceExt;
use zip::ZipArchive;

#[path = "support/runtime_router_contract_support.rs"]
pub mod runtime_router_contract_support;

use runtime_router_contract_support::*;

#[path = "series_contract/detail_and_metadata.rs"]
mod detail_and_metadata;
#[path = "series_contract/discovery_filters.rs"]
mod discovery_filters;
#[path = "series_contract/discovery_release_date_filters.rs"]
mod discovery_release_date_filters;
#[path = "series_contract/discovery_search_validation.rs"]
mod discovery_search_validation;
#[path = "series_contract/search_parity.rs"]
mod search_parity;
#[path = "series_contract/tasks_and_file_routes.rs"]
mod tasks_and_file_routes;
#[path = "series_contract/thumbnails_and_media_assets.rs"]
mod thumbnails_and_media_assets;

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

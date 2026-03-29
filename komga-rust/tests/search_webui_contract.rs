use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_server::app::build_router_with_config;
use serde_json::{Value, json};
use tower::util::ServiceExt;

#[path = "support/runtime_router_contract_support.rs"]
mod runtime_router_contract_support;

use runtime_router_contract_support::*;

#[test]
fn search_webui_contract_target_is_registered() {
    assert_required_target_declared("search/WebUI", "search_webui_contract");
}

#[tokio::test]
async fn router_discovery_books_list_accepts_webui_shape_with_auth_token() {
    let paths = new_router_fixture("router-discovery-books-list").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "seriesId": {
                                "operator": "is",
                                "value": "series-1"
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("books/list request should build"),
        )
        .await
        .expect("books/list request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("books/list payload should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    cleanup_router_fixture(paths);
}

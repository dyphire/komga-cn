use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::infrastructure::sqlite::connect_pool;
use komga_server::app::build_router_with_config;
use serde_json::{Value, json};
use tower::util::ServiceExt;

#[path = "support/runtime_router_contract_support.rs"]
pub mod runtime_router_contract_support;

use runtime_router_contract_support::*;

async fn update_book_search_fixture_title(paths: &RuntimeDbPaths, book_id: &str, title: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("search webui db should open for title update");

    sqlx::query(
        "UPDATE BOOK_METADATA \
         SET TITLE = ? \
         WHERE BOOK_ID = ?",
    )
    .bind(title)
    .bind(book_id)
    .execute(&pool)
    .await
    .expect("search webui title should update");

    pool.close().await;
}

#[test]
fn search_webui_contract_target_is_registered() {
    assert_required_target_declared("search/WebUI", "search_webui_contract");
}

#[tokio::test]
async fn router_discovery_books_list_locks_webui_retained_search_parity() {
    let paths = new_router_fixture("router-discovery-books-list-webui-main-search-parity").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let matrix = [
        (
            "plain relevance",
            Some("book"),
            Some("relevance,desc"),
            vec!["book-1", "book-2", "book-3"],
        ),
        (
            "fielded relevance",
            Some("title:book"),
            Some("relevance,desc"),
            vec!["book-1", "book-2", "book-3"],
        ),
        (
            "blank search",
            Some("   "),
            Some("metadata.title,asc"),
            vec!["book-1", "book-2", "book-3"],
        ),
        (
            "invalid query",
            Some("title:("),
            Some("relevance,desc"),
            vec![],
        ),
    ];

    for (label, full_text_search, sort, expected_ids) in matrix {
        let mut uri = String::from("/api/v1/books/list?page=0&size=20");
        if let Some(sort) = sort {
            uri.push_str("&sort=");
            uri.push_str(sort);
        }

        let mut body = json!({
            "condition": {
                "seriesId": {
                    "operator": "is",
                    "value": "series-1"
                }
            }
        });
        if let Some(full_text_search) = full_text_search {
            body["fullTextSearch"] = Value::String(full_text_search.to_string());
            body["condition"] = json!({
                "type": "Title",
                "operator": "contains",
                "value": "book"
            });
        }

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("books/list webui parity request should build"),
            )
            .await
            .expect("books/list webui parity request should complete");

        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        let ids = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("books/list webui parity payload should expose content array")
            .iter()
            .filter_map(|entry| entry.get("id").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(ids, expected_ids, "case: {label}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_webui_retains_accent_folded_and_cjk_recall() {
    let paths = new_router_fixture("router-discovery-books-list-webui-accent-cjk-recall").await;
    seed_router_contract_data(&paths).await;
    update_book_search_fixture_title(&paths, "book-1", "Café 東京 Book 1").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20&sort=relevance,desc")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "fullTextSearch": "cafe 東京",
                        "condition": {
                            "type": "Title",
                            "operator": "contains",
                            "value": "book"
                        }
                    })
                    .to_string(),
                ))
                .expect("books/list webui accent+cjk request should build"),
        )
        .await
        .expect("books/list webui accent+cjk request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let ids = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("books/list webui accent+cjk payload should expose content array")
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec!["book-1"],
        "books/list webui requests should retain accent-folded mixed CJK recall",
    );

    cleanup_router_fixture(paths);
}

use super::*;

#[tokio::test]
async fn router_discovery_books_latest_ignores_sort_query_and_stays_last_modified_desc() {
    let paths = new_router_fixture("router-discovery-books-latest-strict-sort").await;
    seed_router_contract_data(&paths).await;
    seed_router_primary_series_cbz_book(&paths, "book-2", "book-2.cbz", "Another Book").await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("books latest sort parity db should open");
    sqlx::query("UPDATE BOOK SET LAST_MODIFIED_DATE = ? WHERE ID = ?")
        .bind("2025-04-02 00:00:00")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book-1 lastModified should update for latest sort parity");
    sqlx::query("UPDATE BOOK SET LAST_MODIFIED_DATE = ? WHERE ID = ?")
        .bind("2025-04-01 00:00:00")
        .bind("book-2")
        .execute(&pool)
        .await
        .expect("book-2 lastModified should update for latest sort parity");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/latest?page=0&size=20&sort=metadata.title,asc")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .body(Body::empty())
                .expect("strict books/latest sort request should build"),
        )
        .await
        .expect("strict books/latest sort request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books/latest sort payload should expose content array");
    assert_eq!(content.len(), 2);
    assert_eq!(content[0].get("id"), Some(&json!("book-1")));
    assert_eq!(content[1].get("id"), Some(&json!("book-2")));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_latest_accepts_basic_auth_like_kotlin_clients() {
    let paths = new_router_fixture("router-discovery-books-latest-basic-auth-compat").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/latest?unpaged=true")
                .header(
                    header::AUTHORIZATION,
                    basic_authorization_header_value(
                        "admin@example.org",
                        "router-contract-admin-123",
                    ),
                )
                .header("x-auth-token", "")
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .body(Body::empty())
                .expect("books/latest basic-auth request should build"),
        )
        .await
        .expect("books/latest basic-auth request should complete");

    assert_eq!(response.status(), StatusCode::OK);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_latest_unpaged_keeps_kotlin_page_shape() {
    let paths = new_router_fixture("router-discovery-books-latest-unpaged-shape").await;
    seed_router_contract_data(&paths).await;
    seed_router_primary_series_cbz_book(&paths, "book-2", "book-2.cbz", "Another Book").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/latest?unpaged=true")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .body(Body::empty())
                .expect("strict books/latest unpaged request should build"),
        )
        .await
        .expect("strict books/latest unpaged request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books/latest unpaged payload should expose content array");
    assert_eq!(content.len(), 2);
    assert_eq!(payload.get("size"), Some(&json!(20)));
    assert_eq!(payload.get("number"), Some(&json!(0)));
    assert_eq!(payload.get("totalElements"), Some(&json!(2)));
    assert_eq!(payload.get("totalPages"), Some(&json!(1)));
    assert_eq!(payload.pointer("/pageable/pageSize"), Some(&json!(20)));
    assert_eq!(payload.pointer("/pageable/pageNumber"), Some(&json!(0)));
    assert_eq!(payload.pointer("/pageable/paged"), Some(&json!(true)));
    assert_eq!(payload.pointer("/pageable/unpaged"), Some(&json!(false)));

    cleanup_router_fixture(paths);
}

use super::*;

#[tokio::test]
async fn router_discovery_books_list_locks_main_search_parity_for_retained_inputs() {
    let paths = new_router_fixture("router-discovery-books-list-main-search-parity").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;
    update_book_search_fixture_title(&paths, "book-2", "Book Book 2").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let admin_token = login_with_basic_and_get_token(app.clone()).await;

    let blank_ids = books_list_ids(&app, &admin_token, Some("relevance,desc"), Some("   ")).await;
    assert_eq!(blank_ids, vec!["book-1", "book-2", "book-3"]);

    let relevance_desc_ids =
        books_list_ids(&app, &admin_token, Some("relevance,desc"), Some("book")).await;
    assert_eq!(relevance_desc_ids, vec!["book-2", "book-1", "book-3"]);

    let relevance_asc_ids =
        books_list_ids(&app, &admin_token, Some("relevance,asc"), Some("book")).await;
    assert_eq!(relevance_asc_ids, vec!["book-3", "book-1", "book-2"]);

    let fielded_ids = books_list_ids(
        &app,
        &admin_token,
        Some("relevance,desc"),
        Some("title:book"),
    )
    .await;
    assert_eq!(fielded_ids, vec!["book-2", "book-1", "book-3"]);

    let invalid_query_ids =
        books_list_ids(&app, &admin_token, Some("relevance,desc"), Some("title:(")).await;
    assert!(invalid_query_ids.is_empty());

    seed_router_age_exclude_user_with_roles(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        16,
        &["USER", "PAGE_STREAMING"],
    )
    .await;
    let restricted_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;
    let visible_ids = books_list_ids(
        &app,
        &restricted_token,
        Some("relevance,desc"),
        Some("book"),
    )
    .await;
    assert_eq!(visible_ids, vec!["book-3"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_retains_accent_folded_and_cjk_recall() {
    let paths = new_router_fixture("router-discovery-books-list-accent-cjk-recall").await;
    seed_router_contract_data(&paths).await;
    update_book_search_fixture_title(&paths, "book-1", "Café 東京 Book 1").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let admin_token = login_with_basic_and_get_token(app.clone()).await;

    let accent_cjk_ids = books_list_ids(
        &app,
        &admin_token,
        Some("relevance,desc"),
        Some("cafe 東京"),
    )
    .await;
    assert_eq!(
        accent_cjk_ids,
        vec!["book-1"],
        "books/list should retain accent-folded mixed CJK recall at the route boundary",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_rejects_legacy_regex_search_body_input() {
    let paths = new_router_fixture("router-discovery-books-list-legacy-regex-search").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Title",
                            "operator": "contains",
                            "value": "book"
                        },
                        "regexSearch": "book"
                    })
                    .to_string(),
                ))
                .expect("legacy books/list regexSearch request should build"),
        )
        .await
        .expect("legacy books/list regexSearch request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
}

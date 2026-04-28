use super::*;

async fn books_list_response(
    app: &axum::Router,
    auth_token: &str,
    runtime_owned: bool,
    body: Body,
) -> axum::response::Response {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/api/v1/books/list?page=0&size=20")
        .header("x-auth-token", auth_token)
        .header(header::CONTENT_TYPE, "application/json");
    if runtime_owned {
        builder = builder.header("x-komga-runtime-search-ownership", "runtime-rust-owned");
    }

    app.clone()
        .oneshot(builder.body(body).expect("books/list request should build"))
        .await
        .expect("books/list request should complete")
}

fn page_ids(payload: &Value) -> Vec<String> {
    payload
        .get("content")
        .and_then(Value::as_array)
        .expect("book page payload should expose content array")
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

#[tokio::test]
async fn router_discovery_books_get_route_matches_paperback_compatibility_shape() {
    let paths = new_router_fixture("router-discovery-books-get-paperback-compat").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&search_ready_runtime_config_for_paths(&paths).await).await;
    let authorization =
        basic_authorization_header_value("admin@example.org", "router-contract-admin-123");
    let route = "/api/v1/books?page=0&size=20&search=Book%201&tag=Favorite&media_status=READY&read_status=UNREAD&released_after=2023-01-01&library_id=library-1";
    let body = json!({
        "condition": {
            "type": "AllOfBook",
            "conditions": [
                { "type": "LibraryId", "operator": "is", "value": "library-1" },
                { "type": "Tag", "operator": "is", "value": "Favorite" },
                { "type": "MediaStatus", "operator": "is", "value": "READY" },
                { "type": "ReadStatus", "operator": "is", "value": "UNREAD" },
                { "type": "ReleaseDate", "operator": "after", "dateTime": "2023-01-01" }
            ]
        },
        "fullTextSearch": "Book 1"
    });

    let get_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(route)
                .header(header::AUTHORIZATION, authorization.as_str())
                .header("x-auth-token", "")
                .body(Body::empty())
                .expect("deprecated books GET request should build"),
        )
        .await
        .expect("deprecated books GET request should complete");

    let post_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header(header::AUTHORIZATION, authorization.as_str())
                .header("x-auth-token", "")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("books/list parity request should build"),
        )
        .await
        .expect("books/list parity request should complete");

    assert_eq!(get_response.status(), StatusCode::OK);
    assert_eq!(post_response.status(), StatusCode::OK);

    let get_payload = response_json(get_response).await;
    let post_payload = response_json(post_response).await;
    assert_eq!(page_ids(&get_payload), page_ids(&post_payload));
    assert_eq!(
        get_payload.get("totalElements"),
        post_payload.get("totalElements")
    );
    assert_eq!(get_payload.get("number"), post_payload.get("number"));
    assert_eq!(get_payload.get("size"), post_payload.get("size"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_locks_main_search_parity_for_retained_inputs() {
    let paths = new_router_fixture("router-discovery-books-list-main-search-parity").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;
    update_book_search_fixture_title(&paths, "book-2", "Book Book 2").await;

    let app = build_router_with_config(&search_ready_runtime_config_for_paths(&paths).await).await;
    let admin_token = login_with_basic_and_get_token(app.clone()).await;

    let blank_ids = books_list_ids(&app, &admin_token, Some("relevance,desc"), Some("   ")).await;
    assert_eq!(blank_ids, vec!["book-1", "book-2", "book-3"]);

    let relevance_desc_ids =
        books_list_ids(&app, &admin_token, Some("relevance,desc"), Some("book")).await;
    assert_eq!(relevance_desc_ids, vec!["book-2", "book-1", "book-3"]);

    let default_relevance_ids = books_list_ids(&app, &admin_token, None, Some("book")).await;
    assert_eq!(default_relevance_ids, vec!["book-2", "book-1", "book-3"]);

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

    let app = build_router_with_config(&search_ready_runtime_config_for_paths(&paths).await).await;
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
async fn router_discovery_books_list_ignores_legacy_regex_search_body_input() {
    let paths = new_router_fixture("router-discovery-books-list-legacy-regex-search").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let baseline_ids = books_list_ids(&app, &auth_token, None, None).await;

    for legacy_field in ["regexSearch", "searchRegex", "search_regex"] {
        let mut payload = json!({
            "condition": {
                "type": "Title",
                "operator": "contains",
                "value": "book"
            }
        });
        payload[legacy_field] = Value::String("(".to_string());

        let response =
            books_list_response(&app, &auth_token, true, Body::from(payload.to_string())).await;

        assert_eq!(response.status(), StatusCode::OK, "field={legacy_field}");

        let payload = response_json(response).await;
        let ids = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("books/list payload should expose content array")
            .iter()
            .filter_map(|entry| entry.get("id").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(ids, baseline_ids, "field={legacy_field}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_rejects_invalid_request_bodies() {
    let paths = new_router_fixture("router-discovery-books-list-invalid-bodies").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for (case, body) in [
        ("empty", Body::empty()),
        ("invalid-json", Body::from("{")),
        ("array-body", Body::from("[]")),
    ] {
        let response = books_list_response(&app, &auth_token, false, body).await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "case={case}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_blank_full_text_search_does_not_report_relevance_sort() {
    let paths = new_router_fixture("router-discovery-books-list-blank-search-unsorted-meta").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
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
                        "condition": {
                            "type": "Title",
                            "operator": "contains",
                            "value": "book"
                        },
                        "fullTextSearch": "   "
                    })
                    .to_string(),
                ))
                .expect("blank-search books/list request should build"),
        )
        .await
        .expect("blank-search books/list request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload.pointer("/sort/sorted"), Some(&json!(false)));
    assert_eq!(payload.pointer("/sort/unsorted"), Some(&json!(true)));
    assert_eq!(
        payload.pointer("/pageable/sort/sorted"),
        Some(&json!(false))
    );
    assert_eq!(
        payload.pointer("/pageable/sort/unsorted"),
        Some(&json!(true))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_marks_unsorted_page_shape_without_full_text_search() {
    let paths = new_router_fixture("router-discovery-books-list-unsorted-page-shape").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Title",
                            "operator": "contains",
                            "value": "book"
                        }
                    })
                    .to_string(),
                ))
                .expect("unsorted books/list request should build"),
        )
        .await
        .expect("unsorted books/list request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload.pointer("/sort/sorted"), Some(&json!(false)));
    assert_eq!(payload.pointer("/sort/unsorted"), Some(&json!(true)));
    assert_eq!(
        payload.pointer("/pageable/sort/sorted"),
        Some(&json!(false))
    );
    assert_eq!(
        payload.pointer("/pageable/sort/unsorted"),
        Some(&json!(true))
    );

    cleanup_router_fixture(paths);
}

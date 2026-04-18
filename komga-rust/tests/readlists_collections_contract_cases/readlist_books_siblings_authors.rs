use super::*;

#[tokio::test]
async fn router_readlist_books_returns_paginated_content_and_library_filter() {
    let paths = new_router_fixture("router-readlist-books-paging-filter").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let paged_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books?page=1&size=1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist books paged request should build"),
        )
        .await
        .expect("readlist books paged request should complete");

    assert_eq!(paged_response.status(), StatusCode::OK);
    let paged_payload = response_json(paged_response).await;
    let paged_content = paged_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlist books paged payload should expose content array");
    assert_eq!(paged_content.len(), 1);
    assert_eq!(
        paged_content[0].get("id").and_then(Value::as_str),
        Some("book-2")
    );
    assert_eq!(
        paged_content[0].get("seriesTitle").and_then(Value::as_str),
        Some("Series 2"),
    );
    assert_eq!(
        paged_content[0].get("libraryId").and_then(Value::as_str),
        Some("library-1"),
    );
    assert_eq!(
        paged_content[0]
            .get("metadata")
            .and_then(|metadata| metadata.get("title"))
            .and_then(Value::as_str),
        Some("Book 2"),
    );
    assert_eq!(
        paged_payload.get("totalElements").and_then(Value::as_u64),
        Some(3)
    );

    let filtered_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books?library_id=library-1&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist books filtered request should build"),
        )
        .await
        .expect("readlist books filtered request should complete");

    assert_eq!(filtered_response.status(), StatusCode::OK);
    let filtered_payload = response_json(filtered_response).await;
    let filtered_content = filtered_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlist books filtered payload should expose content array");
    assert_eq!(filtered_content.len(), 2);
    assert_eq!(
        filtered_content[0].get("id").and_then(Value::as_str),
        Some("book-1")
    );
    assert_eq!(
        filtered_content[1].get("id").and_then(Value::as_str),
        Some("book-2")
    );
    assert_eq!(
        filtered_payload
            .get("pageable")
            .and_then(|pageable| pageable.get("unpaged"))
            .and_then(Value::as_bool),
        Some(true),
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_detail_books_siblings_and_book_tags_accept_basic_auth_like_kotlin_clients()
{
    let paths = new_router_fixture("router-readlist-detail-books-basic-auth-compat").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let authorization =
        basic_authorization_header_value("admin@example.org", "router-contract-admin-123");

    for route in [
        "/api/v1/readlists/readlist-1",
        "/api/v1/readlists/readlist-1/books?unpaged=true",
        "/api/v1/readlists/readlist-1/books/book-2/previous",
        "/api/v1/readlists/readlist-1/books/book-2/next",
        "/api/v1/tags/book?readlist_id=readlist-1",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header(header::AUTHORIZATION, authorization.as_str())
                    .header("x-auth-token", "")
                    .body(Body::empty())
                    .expect("readlist detail basic-auth request should build"),
            )
            .await
            .expect("readlist detail basic-auth request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_books_returns_empty_page_when_library_id_filter_excludes_visible_books_like_kotlin()
 {
    let paths = new_router_fixture("router-readlist-books-library-filter-empty-page").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-restricted-user",
        "library-restricted@example.org",
        "router-contract-library-restricted-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library-restricted@example.org",
        "router-contract-library-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books?library_id=library-2&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist books library-filter-empty request should build"),
        )
        .await
        .expect("readlist books library-filter-empty request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlist books empty payload should expose content array");
    assert!(content.is_empty());
    assert_eq!(
        payload.get("totalElements").and_then(Value::as_u64),
        Some(0)
    );
    assert_eq!(
        payload
            .get("pageable")
            .and_then(|pageable| pageable.get("unpaged"))
            .and_then(Value::as_bool),
        Some(true),
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_tags_supports_readlist_scope() {
    let paths = new_router_fixture("router-book-tags-readlist-scope").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/tags/book?readlist_id=readlist-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book tags readlist scope request should build"),
        )
        .await
        .expect("book tags readlist scope request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload,
        json!(["favorite-tag", "library-one-tag", "library-two-tag"])
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_tags_supports_repeated_library_id_query() {
    let paths = new_router_fixture("router-book-tags-repeated-library-id").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/tags/book?library_id=library-1&library_id=library-2")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book tags repeated library_id request should build"),
        )
        .await
        .expect("book tags repeated library_id request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload,
        json!(["favorite-tag", "library-one-tag", "library-two-tag"])
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_books_and_siblings_follow_release_date_when_unordered() {
    let paths = new_router_fixture("router-readlist-unordered-release-date").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;
    mark_readlist_unordered(&paths, "readlist-1").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let books = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books?unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("unordered readlist books request should build"),
        )
        .await
        .expect("unordered readlist books request should complete");
    assert_eq!(books.status(), StatusCode::OK);
    let books_payload = response_json(books).await;
    let book_ids = books_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("unordered readlist books payload should expose content array")
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert_eq!(book_ids, vec!["book-1", "book-2", "book-3"]);

    let previous = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books/book-2/previous")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("unordered previous request should build"),
        )
        .await
        .expect("unordered previous request should complete");
    assert_eq!(previous.status(), StatusCode::OK);
    let previous_payload = response_json(previous).await;
    assert_eq!(
        previous_payload.get("id").and_then(Value::as_str),
        Some("book-1")
    );

    let next = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books/book-2/next")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("unordered next request should build"),
        )
        .await
        .expect("unordered next request should complete");
    assert_eq!(next.status(), StatusCode::OK);
    let next_payload = response_json(next).await;
    assert_eq!(
        next_payload.get("id").and_then(Value::as_str),
        Some("book-3")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_books_filters_content_restricted_books_like_kotlin() {
    let paths = new_router_fixture("router-readlist-books-content-restrictions").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        18,
    )
    .await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("readlist books content restriction db should open");
    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-2")
    .bind(0_i64)
    .bind("Series 2")
    .bind("series/series-2")
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("restricted series row should be inserted");
    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind("Series 2")
    .bind("Series 2")
    .bind("PubHouse")
    .bind("EN")
    .bind(21_i64)
    .bind("series-2")
    .execute(&pool)
    .await
    .expect("restricted series metadata row should be inserted");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-2")
    .bind(0_i64)
    .bind("book-2.epub")
    .bind("books/book-2.epub")
    .bind("series-2")
    .bind(2_048_i64)
    .bind(2_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("restricted book row should be inserted");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/epub+zip")
        .bind("READY")
        .bind("book-2")
        .bind(10_i64)
        .execute(&pool)
        .await
        .expect("restricted media row should be inserted");
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("2")
    .bind(2.0_f64)
    .bind("Book 2")
    .bind("2024-01-16")
    .bind("book-2")
    .execute(&pool)
    .await
    .expect("restricted book metadata row should be inserted");
    sqlx::query("INSERT INTO READLIST (ID, NAME, BOOK_COUNT, ORDERED) VALUES (?, ?, ?, ?)")
        .bind("readlist-2")
        .bind("Filtered ReadList")
        .bind(2_i64)
        .bind(true)
        .execute(&pool)
        .await
        .expect("filtered readlist row should be inserted");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-2")
        .bind("book-1")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("visible readlist book row should be inserted");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-2")
        .bind("book-2")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("restricted readlist book row should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-2/books?unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("restricted readlist books request should build"),
        )
        .await
        .expect("restricted readlist books request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("restricted readlist books payload should expose content array");
    let ids = content
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["book-1"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_books_returns_not_found_for_library_hidden_readlist_like_kotlin() {
    let paths = new_router_fixture("router-readlist-books-library-hidden-not-found").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-restricted-user",
        "library-restricted@example.org",
        "router-contract-library-restricted-123",
        &["library-1"],
    )
    .await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("readlist books library-hidden db should open");
    sqlx::query("INSERT INTO READLIST (ID, NAME, BOOK_COUNT, ORDERED) VALUES (?, ?, ?, ?)")
        .bind("readlist-2")
        .bind("Library Hidden ReadList")
        .bind(1_i64)
        .bind(true)
        .execute(&pool)
        .await
        .expect("library-hidden readlist row should be inserted");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-2")
        .bind("book-3")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("library-hidden readlist book row should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library-restricted@example.org",
        "router-contract-library-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-2/books?unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("library-hidden readlist books request should build"),
        )
        .await
        .expect("library-hidden readlist books request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_books_author_filter_requires_matching_role() {
    let paths = new_router_fixture("router-readlist-author-role-filter").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let matching = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books?author=Jane+Writer,writer&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("author filter request should build"),
        )
        .await
        .expect("author filter request should complete");
    assert_eq!(matching.status(), StatusCode::OK);
    let matching_payload = response_json(matching).await;
    let matching_ids = matching_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("matching author payload should expose content array")
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert_eq!(matching_ids, vec!["book-1", "book-2"]);

    let mismatching = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books?author=Jane+Writer,penciller&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("mismatching author filter request should build"),
        )
        .await
        .expect("mismatching author filter request should complete");
    assert_eq!(mismatching.status(), StatusCode::OK);
    let mismatching_payload = response_json(mismatching).await;
    let mismatching_ids = mismatching_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("mismatching author payload should expose content array")
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert!(mismatching_ids.is_empty());

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_books_author_filter_accepts_empty_role_like_kotlin() {
    let paths = new_router_fixture("router-readlist-author-empty-role-filter").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;
    seed_readlist_author_edge_case(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books?author=Doe,+John,&library_id=library-2&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("empty-role author filter request should build"),
        )
        .await
        .expect("empty-role author filter request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("empty-role author filter payload should expose content array");
    let ids = content
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();

    assert_eq!(ids, vec!["book-3"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_books_author_filter_ignores_bare_name_like_kotlin_http_query() {
    let paths = new_router_fixture("router-readlist-author-bare-name-ignored").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;
    seed_readlist_author_edge_case(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books?author=Jane+Writer&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("bare-name author request should build"),
        )
        .await
        .expect("bare-name author request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let ids = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("bare-name author payload should expose content array")
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();

    assert_eq!(ids, vec!["book-1", "book-2", "book-3"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_books_preserves_blank_and_comma_author_roles() {
    let paths = new_router_fixture("router-readlist-author-payload-fidelity").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;
    seed_readlist_author_edge_case(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books?library_id=library-2&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist author fidelity request should build"),
        )
        .await
        .expect("readlist author fidelity request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let authors = payload
        .get("content")
        .and_then(Value::as_array)
        .and_then(|content| content.first())
        .and_then(|book| book.get("metadata"))
        .and_then(|metadata| metadata.get("authors"))
        .and_then(Value::as_array)
        .expect("readlist author fidelity payload should expose authors array");

    assert!(authors.iter().any(|author| {
        author.get("name").and_then(Value::as_str) == Some("Doe, John")
            && author.get("role").and_then(Value::as_str) == Some("")
    }));
    assert!(authors.iter().any(|author| {
        author.get("name").and_then(Value::as_str) == Some("Casey Role")
            && author.get("role").and_then(Value::as_str) == Some("CoWriter")
    }));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_book_siblings_follow_readlist_order() {
    let paths = new_router_fixture("router-readlist-book-siblings").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let previous = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books/book-2/previous")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist previous request should build"),
        )
        .await
        .expect("readlist previous request should complete");
    assert_eq!(previous.status(), StatusCode::OK);
    let previous_payload = response_json(previous).await;
    assert_eq!(
        previous_payload.get("id").and_then(Value::as_str),
        Some("book-1")
    );

    let next = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books/book-2/next")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist next request should build"),
        )
        .await
        .expect("readlist next request should complete");
    assert_eq!(next.status(), StatusCode::OK);
    let next_payload = response_json(next).await;
    assert_eq!(
        next_payload.get("id").and_then(Value::as_str),
        Some("book-3")
    );

    let missing = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books/book-1/previous")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist missing previous request should build"),
        )
        .await
        .expect("readlist missing previous request should complete");
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

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

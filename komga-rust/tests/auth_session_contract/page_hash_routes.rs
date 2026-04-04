use super::*;

#[tokio::test]
async fn router_put_page_hash_normalizes_negative_size_to_null() {
    let paths = new_router_fixture("router-put-page-hash-negative-size-null").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"hash":"negative-size-hash","size":-1,"action":"IGNORE"}"#,
                ))
                .expect("page hash put request should build"),
        )
        .await
        .expect("page hash put request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        load_page_hash_size(&paths, "negative-size-hash").await,
        None
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_put_page_hash_preserves_whitespace_padded_hash() {
    let paths = new_router_fixture("router-put-page-hash-whitespace-hash").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"hash":" negative-size-hash ","size":1,"action":"IGNORE"}"#,
                ))
                .expect("page hash put request with padded hash should build"),
        )
        .await
        .expect("page hash put request with padded hash should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        load_page_hash_size(&paths, " negative-size-hash ").await,
        Some(1)
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_put_page_hash_rejects_blank_only_hash() {
    let paths = new_router_fixture("router-put-page-hash-blank-only-hash").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"hash":"   ","size":1,"action":"IGNORE"}"#))
                .expect("page hash put request with blank-only hash should build"),
        )
        .await
        .expect("page hash put request with blank-only hash should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_put_page_hash_rejects_whitespace_padded_action() {
    let paths = new_router_fixture("router-put-page-hash-whitespace-action").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"hash":"negative-size-hash","size":1,"action":" IGNORE "}"#,
                ))
                .expect("page hash put request with whitespace action should build"),
        )
        .await
        .expect("page hash put request with whitespace action should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_put_page_hash_rejects_non_integer_size_values() {
    let paths = new_router_fixture("router-put-page-hash-non-integer-size").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"hash":"typed-size-hash","size":true,"action":"IGNORE"}"#,
                ))
                .expect("page hash put request with non-integer size should build"),
        )
        .await
        .expect("page hash put request with non-integer size should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        load_page_hash_record(&paths, "typed-size-hash")
            .await
            .is_none()
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_put_page_hash_preserves_existing_size_on_update() {
    let paths = new_router_fixture("router-put-page-hash-preserve-existing-size").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_row(&paths, "existing-size-hash", Some(5), "IGNORE").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"hash":"existing-size-hash","size":99,"action":"DELETE_AUTO"}"#,
                ))
                .expect("page hash update request should build"),
        )
        .await
        .expect("page hash update request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        load_page_hash_record(&paths, "existing-size-hash").await,
        Some((Some(5), "DELETE_AUTO".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_put_page_hash_persists_known_thumbnail_so_it_survives_source_removal() {
    let paths = new_router_fixture("router-put-page-hash-persists-thumbnail").await;
    seed_router_contract_data(&paths).await;
    let source_path = seed_page_hash_image_source(
        &paths,
        "book-page-hash-thumb",
        "known-thumb-hash",
        "images/known-thumb-source.png",
        "known-thumb-source.png",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"hash":"known-thumb-hash","size":64,"action":"IGNORE"}"#,
                ))
                .expect("page hash put request for thumbnail persistence should build"),
        )
        .await
        .expect("page hash put request for thumbnail persistence should complete");

    assert_eq!(put_response.status(), StatusCode::ACCEPTED);
    std::fs::remove_file(&source_path).expect("source image should be removable after put");

    let thumbnail_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/known-thumb-hash/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("known page hash thumbnail request should build"),
        )
        .await
        .expect("known page hash thumbnail request should complete");

    assert_eq!(thumbnail_response.status(), StatusCode::OK);
    assert_eq!(
        thumbnail_response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    let body = to_bytes(thumbnail_response.into_body(), usize::MAX)
        .await
        .expect("known page hash thumbnail response body should be readable");
    assert!(body.starts_with(&[0xFF, 0xD8]));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hashes_unknown_marks_unsorted_when_no_sort_query_is_present() {
    let paths = new_router_fixture("router-page-hashes-unknown-unsorted-flag").await;
    seed_router_contract_data(&paths).await;
    seed_unknown_page_hash_samples(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/unknown")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("unknown page hashes request should build"),
        )
        .await
        .expect("unknown page hashes request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload["sort"]["sorted"], false);
    assert_eq!(payload["sort"]["unsorted"], true);
    assert_eq!(payload["pageable"]["sort"]["sorted"], false);
    assert_eq!(payload["pageable"]["sort"]["unsorted"], true);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hashes_unknown_honors_hash_desc_sort_query() {
    let paths = new_router_fixture("router-page-hashes-unknown-hash-desc-sort").await;
    seed_router_contract_data(&paths).await;
    seed_unknown_page_hash_samples(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/unknown?sort=hash,desc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("sorted unknown page hashes request should build"),
        )
        .await
        .expect("sorted unknown page hashes request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("unknown page hashes content should be an array");
    let hashes = content
        .iter()
        .map(|entry| {
            entry["hash"]
                .as_str()
                .expect("page hash unknown entry should contain hash")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(hashes, vec!["z-hash".to_string(), "a-hash".to_string()]);
    assert_eq!(payload["sort"]["sorted"], true);
    assert_eq!(payload["sort"]["unsorted"], false);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hashes_unknown_honors_kotlin_legacy_sort_keys() {
    let paths = new_router_fixture("router-page-hashes-unknown-legacy-sort-keys").await;
    seed_router_contract_data(&paths).await;
    seed_unknown_page_hash_samples(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for sort in ["url,desc", "bookId,desc", "pageNumber,desc"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/page-hashes/unknown?sort={sort}"))
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("legacy-sorted unknown page hashes request should build"),
            )
            .await
            .expect("legacy-sorted unknown page hashes request should complete");

        assert_eq!(response.status(), StatusCode::OK, "sort={sort}");
        let payload = response_json(response).await;
        let content = payload["content"]
            .as_array()
            .expect("unknown page hashes content should be an array");
        let hashes = content
            .iter()
            .map(|entry| {
                entry["hash"]
                    .as_str()
                    .expect("page hash unknown entry should contain hash")
                    .to_string()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            hashes,
            vec!["z-hash".to_string(), "a-hash".to_string()],
            "sort={sort}"
        );
        assert_eq!(payload["sort"]["sorted"], true, "sort={sort}");
        assert_eq!(payload["sort"]["unsorted"], false, "sort={sort}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hashes_unknown_groups_same_hash_even_when_file_sizes_differ() {
    let paths = new_router_fixture("router-page-hashes-unknown-groups-by-hash-only").await;
    seed_router_contract_data(&paths).await;
    seed_unknown_page_hash_samples_with_mixed_sizes(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/unknown")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("mixed-size unknown page hashes request should build"),
        )
        .await
        .expect("mixed-size unknown page hashes request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("unknown page hashes content should be an array");
    assert_eq!(payload["totalElements"], json!(1));
    assert_eq!(content.len(), 1);
    assert_eq!(content[0]["hash"], json!("mixed-size-hash"));
    assert_eq!(content[0]["matchCount"], json!(2));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_matches_marks_unsorted_when_no_sort_query_is_present() {
    let paths = new_router_fixture("router-page-hash-matches-unsorted-flag").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_match_samples(&paths, "match-sort-hash").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("page hash matches request should build"),
        )
        .await
        .expect("page hash matches request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload["sort"]["sorted"], false);
    assert_eq!(payload["sort"]["unsorted"], true);
    assert_eq!(payload["pageable"]["sort"]["sorted"], false);
    assert_eq!(payload["pageable"]["sort"]["unsorted"], true);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_matches_honors_page_number_desc_sort_query() {
    let paths = new_router_fixture("router-page-hash-matches-page-number-desc").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_match_samples(&paths, "match-sort-hash").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash?sort=pageNumber,desc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("sorted page hash matches request should build"),
        )
        .await
        .expect("sorted page hash matches request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("page hash matches content should be an array");
    let page_numbers = content
        .iter()
        .map(|entry| {
            entry["pageNumber"]
                .as_i64()
                .expect("page hash match entry should contain page number")
        })
        .collect::<Vec<_>>();
    assert_eq!(page_numbers, vec![5, 3, 1]);
    assert_eq!(payload["sort"]["sorted"], true);
    assert_eq!(payload["sort"]["unsorted"], false);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_matches_rejects_match_count_and_total_size_sort_keys() {
    let paths = new_router_fixture("router-page-hash-matches-unsupported-aggregate-sort").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_match_samples(&paths, "match-sort-hash").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for sort in ["matchCount,desc", "totalSize,desc"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/page-hashes/match-sort-hash?sort={sort}"))
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("page hash matches aggregate sort request should build"),
            )
            .await
            .expect("page hash matches aggregate sort request should complete");

        assert_eq!(
            response.status(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "sort={sort}"
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_matches_converts_file_url_to_path_string() {
    let paths = new_router_fixture("router-page-hash-matches-url-to-path").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_match_samples(&paths, "match-sort-hash").await;
    update_book_url(
        &paths,
        "book-match-1",
        "file:/library-root/books/book-match-1.cbz",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash?sort=bookId,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("page hash matches url path request should build"),
        )
        .await
        .expect("page hash matches url path request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("page hash matches content should be an array");
    assert_eq!(content[0]["url"], "/library-root/books/book-match-1.cbz");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_matches_returns_internal_error_for_unparseable_book_url() {
    let paths = new_router_fixture("router-page-hash-matches-invalid-url").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_match_samples(&paths, "match-sort-hash").await;
    update_book_url(&paths, "book-match-1", "::not-a-valid-url::").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash?sort=bookId,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("page hash matches invalid url request should build"),
        )
        .await
        .expect("page hash matches invalid url request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_matches_decodes_percent_encoded_file_url_path() {
    let paths = new_router_fixture("router-page-hash-matches-decodes-file-url-path").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_match_samples(&paths, "match-sort-hash").await;
    update_book_url(
        &paths,
        "book-match-1",
        "file:/library%20root/books/book%20match%201.cbz",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash?sort=bookId,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("page hash matches encoded file url request should build"),
        )
        .await
        .expect("page hash matches encoded file url request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("page hash matches content should be an array");
    assert_eq!(content[0]["url"], "/library root/books/book match 1.cbz");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_matches_returns_internal_error_for_null_file_size() {
    let paths = new_router_fixture("router-page-hash-matches-null-file-size").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_match_samples(&paths, "match-sort-hash").await;
    update_media_page_file_size_to_null(&paths, "book-match-1", 0).await;
    assert_eq!(
        load_media_page_file_size(&paths, "book-match-1", 0).await,
        None
    );

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash?sort=bookId,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("page hash matches null file size request should build"),
        )
        .await
        .expect("page hash matches null file size request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_matches_returns_internal_error_for_non_file_url() {
    let paths = new_router_fixture("router-page-hash-matches-http-url").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_match_samples(&paths, "match-sort-hash").await;
    update_book_url(
        &paths,
        "book-match-1",
        "https://example.com/books/book-match-1.cbz",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash?sort=bookId,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("page hash matches non-file url request should build"),
        )
        .await
        .expect("page hash matches non-file url request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
}

use super::*;

#[tokio::test]
async fn router_kobo_book_file_epub_allows_path_token_user_with_file_download_role() {
    let paths = new_router_fixture("router-kobo-book-file-path-token-success").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-file-user",
        "kobo-file-user@example.org",
        "router-contract-kobo-file-123",
        18,
        &["USER", "KOBO_SYNC", "FILE_DOWNLOAD"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobooktoken", "kobo-file-user").await;
    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for kobo file path token test");
    let expected_body = b"router-kobo-file-content";
    std::fs::write(books_dir.join("book-1.epub"), expected_body)
        .expect("kobo file path token fixture should be written");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/validkobooktoken/v1/books/book-1/file/epub")
                .body(Body::empty())
                .expect("kobo file path token request should build"),
        )
        .await
        .expect("kobo file path token request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/epub+zip")
    );
    let content_disposition = response
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .expect("kobo file path token response should include content disposition");
    assert!(content_disposition.starts_with("attachment;"));
    assert!(content_disposition.contains("book-1.epub"));
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("kobo file path token body should be readable");
    assert_eq!(body.as_ref(), expected_body);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_book_file_epub_forbids_path_token_user_without_file_download_role() {
    let paths = new_router_fixture("router-kobo-book-file-path-token-forbidden").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-file-no-download-user",
        "kobo-file-no-download@example.org",
        "router-contract-kobo-file-no-download-123",
        18,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "nodownloadtoken", "kobo-file-no-download-user").await;
    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for forbidden kobo file test");
    std::fs::write(books_dir.join("book-1.epub"), b"router-kobo-file-content")
        .expect("forbidden kobo file fixture should be written");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/nodownloadtoken/v1/books/book-1/file/epub")
                .body(Body::empty())
                .expect("forbidden kobo file request should build"),
        )
        .await
        .expect("forbidden kobo file request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_book_file_epub_returns_forbidden_for_restricted_user() {
    let paths = new_router_fixture("router-kobo-book-file-restricted-user").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "restricted-kobo-file-user",
        "restricted-kobo-file@example.org",
        "router-contract-restricted-kobo-file-123",
        16,
        &["USER", "FILE_DOWNLOAD", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "any-token", "restricted-kobo-file-user").await;
    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for restricted kobo file test");
    std::fs::write(books_dir.join("book-1.epub"), b"router-kobo-file-content")
        .expect("restricted kobo file fixture should be written");

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("restricted kobo file db should open");
    sqlx::query("UPDATE SERIES_METADATA SET AGE_RATING = ? WHERE SERIES_ID = ?")
        .bind(18_i64)
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series age rating should be updated for restricted kobo file test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted-kobo-file@example.org",
        "router-contract-restricted-kobo-file-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/books/book-1/file/epub")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("restricted kobo file request should build"),
        )
        .await
        .expect("restricted kobo file request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_book_file_epub_returns_not_found_with_message_when_file_is_missing() {
    let paths = new_router_fixture("router-kobo-book-file-missing-file").await;
    seed_router_contract_data(&paths).await;
    seed_admin_kobo_path_token(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/books/book-1/file/epub")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing kobo file request should build"),
        )
        .await
        .expect("missing kobo file request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "File not found, it may have moved".to_string()
        ))
    );

    cleanup_router_fixture(paths);
}

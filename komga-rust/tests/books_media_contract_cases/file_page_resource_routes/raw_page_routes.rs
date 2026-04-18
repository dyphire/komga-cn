use super::*;

#[tokio::test]
async fn router_book_raw_page_does_not_return_not_modified_before_page_streaming_check() {
    let paths = new_router_fixture("router-book-raw-page-no-304-before-role-check").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "admin-access-no-page-streaming",
        "admin-access-no-page-streaming@example.org",
        "router-contract-admin-access-123",
        0,
        &["USER", "ADMIN"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "admin-access-no-page-streaming@example.org",
        "router-contract-admin-access-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1/raw")
                .header("x-auth-token", &auth_token)
                .header(header::IF_MODIFIED_SINCE, "Wed, 01 Jan 2099 00:00:00 +0000")
                .body(Body::empty())
                .expect("book raw page role-order request should build"),
        )
        .await
        .expect("book raw page role-order request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_returns_bad_request_for_negative_page_number() {
    let paths = new_router_fixture("router-book-raw-page-negative-page-number").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/-1/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("negative raw page request should build"),
        )
        .await
        .expect("negative raw page request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Page number does not exist".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_accepts_basic_auth_like_kotlin_clients() {
    let paths = new_router_fixture("router-book-raw-page-basic-auth-compat").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1/raw")
                .header(
                    header::AUTHORIZATION,
                    basic_authorization_header_value(
                        "admin@example.org",
                        "router-contract-admin-123",
                    ),
                )
                .header("x-auth-token", "")
                .body(Body::empty())
                .expect("raw page basic-auth request should build"),
        )
        .await
        .expect("raw page basic-auth request should complete");

    assert_eq!(response.status(), StatusCode::OK);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_returns_bad_request_for_non_integer_page_number() {
    let paths = new_router_fixture("router-book-raw-page-non-integer-page-number").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/abc/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("non-integer raw page request should build"),
        )
        .await
        .expect("non-integer raw page request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_content_disposition_uses_book_name_not_metadata_title() {
    let paths = new_router_fixture("router-book-raw-page-uses-book-name").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Metadata Title",
    )
    .await;
    update_router_book_name(&paths, "book-pdf-1", "Filesystem Shelf Name").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book raw page request should build"),
        )
        .await
        .expect("book raw page request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let disposition = response
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .expect("raw page response should expose content-disposition");
    assert!(
        disposition.contains("Filesystem Shelf Name-1"),
        "disposition was: {disposition}"
    );
    assert!(
        !disposition.contains("Metadata Title-1"),
        "disposition was: {disposition}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_returns_bad_request_for_missing_pdf_page_number() {
    let paths = new_router_fixture("router-book-page-missing-pdf-page").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/2/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing raw pdf page request should build"),
        )
        .await
        .expect("missing raw pdf page request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Page number does not exist".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_returns_not_found_with_message_when_media_not_ready() {
    let paths = new_router_fixture("router-book-raw-page-media-not-ready").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book raw page not-ready db should open");
    sqlx::query("UPDATE MEDIA SET STATUS = ? WHERE BOOK_ID = ?")
        .bind("OUTDATED")
        .bind("book-pdf-1")
        .execute(&pool)
        .await
        .expect("media status should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("not-ready raw pdf page request should build"),
        )
        .await
        .expect("not-ready raw pdf page request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Book analysis failed".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_forbidden_before_not_ready_for_restricted_user() {
    let paths = new_router_fixture("router-book-raw-page-restricted-before-not-ready").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "restricted-page-user",
        "restricted-page-user@example.org",
        "router-contract-restricted-page-123",
        16,
        &["USER", "PAGE_STREAMING"],
    )
    .await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book raw page restricted db should open");
    sqlx::query("UPDATE MEDIA SET STATUS = ? WHERE BOOK_ID = ?")
        .bind("OUTDATED")
        .bind("book-pdf-1")
        .execute(&pool)
        .await
        .expect("media status should update");
    sqlx::query("UPDATE SERIES_METADATA SET AGE_RATING = ? WHERE SERIES_ID = ?")
        .bind(18_i64)
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series age rating should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted-page-user@example.org",
        "router-contract-restricted-page-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("restricted raw pdf page request should build"),
        )
        .await
        .expect("restricted raw pdf page request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_returns_not_found_with_message_when_file_is_missing() {
    let paths = new_router_fixture("router-book-raw-page-file-missing").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let pdf_path = paths.config_dir.join("books/fixture-page.pdf");
    std::fs::remove_file(&pdf_path).expect("pdf fixture should be removable");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("file-missing raw pdf page request should build"),
        )
        .await
        .expect("file-missing raw pdf page request should complete");

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

#[tokio::test]
async fn router_book_raw_page_returns_not_modified_before_not_ready_checks() {
    let paths = new_router_fixture("router-book-raw-page-not-modified-before-not-ready").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book raw page not-ready db should open");
    sqlx::query("UPDATE MEDIA SET STATUS = ? WHERE BOOK_ID = ?")
        .bind("OUTDATED")
        .bind("book-pdf-1")
        .execute(&pool)
        .await
        .expect("media status should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1/raw")
                .header("x-auth-token", &auth_token)
                .header(header::IF_MODIFIED_SINCE, "Wed, 01 Jan 2099 00:00:00 +0000")
                .body(Body::empty())
                .expect("not-modified raw pdf page request should build"),
        )
        .await
        .expect("not-modified raw pdf page request should complete");

    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);

    cleanup_router_fixture(paths);
}

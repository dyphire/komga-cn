use super::*;

#[tokio::test]
async fn router_book_file_direct_route_returns_attachment_headers_and_body() {
    let ctx = TestFixture::new("router-book-file-direct-route").await;
    let books_dir = ctx.paths().config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for direct file route test");
    let expected_body = b"router-book-file-direct-content";
    std::fs::write(books_dir.join("book-1.epub"), expected_body)
        .expect("book fixture file should be written for direct file route test");

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/file")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("direct book file request should build"),
        )
        .await
        .expect("direct book file request should complete");

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
        .expect("direct book file response should expose content disposition");
    assert!(content_disposition.contains("attachment"));
    assert!(content_disposition.contains("book-1.epub"));
    let content_length = response
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .expect("direct book file response should expose content length");
    assert_eq!(content_length, expected_body.len().to_string());
    assert!(
        !response.headers().contains_key(header::ACCEPT_RANGES),
        "full book download should not advertise range headers on 200 responses"
    );
    assert!(
        !response.headers().contains_key(header::CONTENT_RANGE),
        "full book download should not expose content-range on 200 responses"
    );

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("direct book file response body should be readable");
    assert_eq!(body.as_ref(), expected_body);
}

#[tokio::test]
async fn router_book_file_direct_route_accepts_basic_auth_like_kotlin_clients() {
    let ctx = TestFixture::new("router-book-file-direct-basic-auth-compat").await;
    let books_dir = ctx.paths().config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for basic-auth direct file route test");
    let expected_body = b"router-book-file-basic-auth-content";
    std::fs::write(books_dir.join("book-1.epub"), expected_body)
        .expect("book fixture file should be written for basic-auth direct file route test");

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/file")
                .header(
                    header::AUTHORIZATION,
                    basic_authorization_header_value(
                        "admin@example.org",
                        "router-contract-admin-123",
                    ),
                )
                .header("x-auth-token", "")
                .body(Body::empty())
                .expect("basic-auth direct book file request should build"),
        )
        .await
        .expect("basic-auth direct book file request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("basic-auth direct book file response body should be readable");
    assert_eq!(body.as_ref(), expected_body);
}

#[tokio::test]
async fn router_book_file_direct_route_ignores_range_and_returns_full_body() {
    let ctx = TestFixture::new("router-book-file-direct-route-ignores-range").await;
    let books_dir = ctx.paths().config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for range-ignoring file route test");
    let expected_body = b"router-book-file-range-ignored";
    std::fs::write(books_dir.join("book-1.epub"), expected_body)
        .expect("book fixture file should be written for range-ignoring file route test");

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/file")
                .header("x-auth-token", &auth_token)
                .header(header::RANGE, "bytes=0-3")
                .body(Body::empty())
                .expect("range direct book file request should build"),
        )
        .await
        .expect("range direct book file request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        !response.headers().contains_key(header::ACCEPT_RANGES),
        "range request should not expose accept-ranges on Kotlin-aligned /file responses"
    );
    assert!(
        !response.headers().contains_key(header::CONTENT_RANGE),
        "range request should not expose content-range on Kotlin-aligned /file responses"
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("range direct book file response body should be readable");
    assert_eq!(body.as_ref(), expected_body);
}

#[tokio::test]
async fn router_book_file_direct_route_returns_not_found_with_message_when_file_is_missing() {
    let ctx = TestFixture::new("router-book-file-direct-route-missing-file").await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/file")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing direct book file request should build"),
        )
        .await
        .expect("missing direct book file request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "File not found, it may have moved".to_string()
        ))
    );
}

#[tokio::test]
async fn router_book_file_direct_route_uses_persisted_media_type_for_comic_archives() {
    let ctx = TestFixture::new("router-book-file-direct-route-comic-archive-type").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for direct comic archive file route fixture");
    sqlx::query("UPDATE BOOK SET NAME = ?, URL = ? WHERE ID = ?")
        .bind("book-1.cbz")
        .bind("books/book-1.cbz")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book file metadata should update for direct comic archive file route fixture");
    sqlx::query("UPDATE MEDIA SET MEDIA_TYPE = ? WHERE BOOK_ID = ?")
        .bind("application/zip")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("media type should update for direct comic archive file route fixture");
    pool.close().await;

    let books_dir = ctx.paths().config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for direct comic archive file route test");
    let expected_body = b"router-book-file-comic-archive-content";
    std::fs::write(books_dir.join("book-1.cbz"), expected_body)
        .expect("comic archive fixture file should be written for direct file route test");

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/file")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("direct comic archive book file request should build"),
        )
        .await
        .expect("direct comic archive book file request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/zip")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("direct comic archive book file response body should be readable");
    assert_eq!(body.as_ref(), expected_body);
}

#[tokio::test]
async fn router_book_file_direct_route_percent_encodes_unicode_attachment_name() {
    let ctx = TestFixture::new("router-book-file-direct-route-unicode-name").await;
    let unicode_file_name = "アキラ.epub";

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for unicode direct file route fixture");
    sqlx::query("UPDATE BOOK SET NAME = ?, URL = ? WHERE ID = ?")
        .bind(unicode_file_name)
        .bind(format!("books/{unicode_file_name}"))
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book file name should update for unicode direct file route fixture");
    pool.close().await;

    let books_dir = ctx.paths().config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for unicode direct file route test");
    std::fs::write(books_dir.join(unicode_file_name), b"unicode-book-file")
        .expect("unicode book fixture file should be written");

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/file")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("unicode direct book file request should build"),
        )
        .await
        .expect("unicode direct book file request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let content_disposition = response
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .expect("unicode direct book file response should expose content disposition");
    assert!(
        content_disposition.contains("filename*=UTF-8''%E3%82%A2%E3%82%AD%E3%83%A9.epub"),
        "unicode attachment header should percent-encode filename*: {content_disposition}"
    );
}

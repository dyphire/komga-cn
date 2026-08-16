use super::*;

#[tokio::test]
async fn router_book_resource_supports_not_modified_and_inline_content_disposition() {
    let ctx = TestFixture::new("router-book-resource-inline-not-modified").await;
    write_router_epub_resource(
        ctx.paths(),
        "books/book-1.epub",
        "OEBPS/chapter.xhtml",
        br#"<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><p>Hello</p></body></html>"#,
    );
    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for resource media type seed");
    sqlx::query(
        "INSERT INTO MEDIA_FILE (FILE_NAME, BOOK_ID, MEDIA_TYPE, SUB_TYPE, FILE_SIZE) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("OEBPS/chapter.xhtml")
    .bind("book-1")
    .bind("application/xhtml+xml")
    .bind("EPUB_PAGE")
    .bind(82_i64)
    .execute(&pool)
    .await
    .expect("resource media type should be seeded");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    for route in [
        "/api/v1/books/book-1/resource/OEBPS/chapter.xhtml",
        "/opds/v2/books/book-1/resource/OEBPS/chapter.xhtml",
    ] {
        let initial = ctx
            .app()
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("resource request should build"),
            )
            .await
            .expect("resource request should complete");

        assert_eq!(initial.status(), StatusCode::OK, "route: {route}");
        assert_eq!(
            initial
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/xhtml+xml"),
            "route: {route}"
        );
        let last_modified = initial
            .headers()
            .get(header::LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .expect("resource response should expose last-modified")
            .to_string();
        let disposition = initial
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .and_then(|value| value.to_str().ok())
            .expect("resource response should expose content-disposition");
        assert!(
            disposition.starts_with("inline;"),
            "route: {route}, disposition: {disposition}"
        );
        assert!(
            disposition.contains("chapter.xhtml"),
            "route: {route}, disposition: {disposition}"
        );
        assert_eq!(
            initial
                .headers()
                .get(header::CONTENT_SECURITY_POLICY)
                .and_then(|value| value.to_str().ok()),
            Some("script-src 'none'; object-src 'none';"),
            "route: {route}"
        );

        let not_modified = ctx
            .app()
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .header(header::IF_MODIFIED_SINCE, &last_modified)
                    .body(Body::empty())
                    .expect("conditional resource request should build"),
            )
            .await
            .expect("conditional resource request should complete");

        assert_eq!(
            not_modified.status(),
            StatusCode::NOT_MODIFIED,
            "route: {route}"
        );
        assert_eq!(
            not_modified
                .headers()
                .get(header::LAST_MODIFIED)
                .and_then(|value| value.to_str().ok()),
            Some(last_modified.as_str()),
            "route: {route}"
        );
    }
}

#[tokio::test]
async fn router_book_resource_routes_accept_basic_auth_like_kotlin_clients() {
    let ctx = TestFixture::new("router-book-resource-basic-auth-compat").await;
    write_router_epub_resource(
        ctx.paths(),
        "books/book-1.epub",
        "OEBPS/chapter.xhtml",
        br#"<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><p>Hello</p></body></html>"#,
    );

    let authorization =
        basic_authorization_header_value("admin@example.org", "router-contract-admin-123");

    for route in [
        "/api/v1/books/book-1/resource/OEBPS/chapter.xhtml",
        "/opds/v2/books/book-1/resource/OEBPS/chapter.xhtml",
    ] {
        let response = ctx
            .app()
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header(header::AUTHORIZATION, authorization.as_str())
                    .header("x-auth-token", "")
                    .body(Body::empty())
                    .expect("resource basic-auth request should build"),
            )
            .await
            .expect("resource basic-auth request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
    }
}

#[tokio::test]
async fn router_book_font_resource_requires_authentication() {
    let ctx = TestFixture::new("router-book-font-resource-auth").await;
    write_router_epub_resource(
        ctx.paths(),
        "books/book-1.epub",
        "OEBPS/fonts/fixture.woff",
        b"font-bytes",
    );

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/resource/OEBPS/fonts/fixture.woff")
                .body(Body::empty())
                .expect("font resource request should build"),
        )
        .await
        .expect("font resource request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn router_book_resource_requires_page_streaming_role_even_for_admins() {
    let ctx = TestFixture::new("router-book-resource-page-streaming-role").await;
    write_router_epub_resource(
        ctx.paths(),
        "books/book-1.epub",
        "OEBPS/chapter.xhtml",
        br#"<html xmlns="http://www.w3.org/1999/xhtml"><body>Chapter</body></html>"#,
    );

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for resource role seed");
    let password = "router-contract-resource-admin-only-123";
    sqlx::query(
        "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES) \
         VALUES (?, ?, ?, ?)",
    )
    .bind("resource-admin-only")
    .bind("resource-admin-only@example.org")
    .bind(hash_router_contract_password(password))
    .bind(true)
    .execute(&pool)
    .await
    .expect("resource admin-only user should be inserted");
    for role in ["USER", "ADMIN"] {
        sqlx::query("INSERT INTO USER_ROLE (USER_ID, ROLE) VALUES (?, ?)")
            .bind("resource-admin-only")
            .bind(role)
            .execute(&pool)
            .await
            .expect("resource admin-only role should be inserted");
    }
    pool.close().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/resource/OEBPS/chapter.xhtml")
                .header(
                    header::AUTHORIZATION,
                    basic_authorization_header_value("resource-admin-only@example.org", password),
                )
                .header("x-auth-token", "")
                .body(Body::empty())
                .expect("admin-only resource request should build"),
        )
        .await
        .expect("admin-only resource request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn router_book_resource_routes_report_invalid_epub_archive_errors() {
    let ctx = TestFixture::new("router-book-resource-invalid-epub").await;
    let epub_path = ctx.paths().config_dir.join("books/book-1.epub");
    std::fs::create_dir_all(
        epub_path
            .parent()
            .expect("invalid epub fixture should have a parent directory"),
    )
    .expect("invalid epub parent directory should be created");
    std::fs::write(&epub_path, b"not a zip").expect("invalid epub fixture should be written");

    let auth_token = ctx.login_admin().await;

    for route in [
        "/api/v1/books/book-1/resource/OEBPS/chapter.xhtml",
        "/opds/v2/books/book-1/resource/OEBPS/chapter.xhtml",
    ] {
        let response = ctx
            .app()
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("invalid epub resource request should build"),
            )
            .await
            .expect("invalid epub resource request should complete");

        assert_eq!(
            response.status(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "route: {route}"
        );
    }
}

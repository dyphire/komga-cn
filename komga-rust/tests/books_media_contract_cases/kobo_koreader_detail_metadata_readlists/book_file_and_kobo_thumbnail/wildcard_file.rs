use super::*;

#[tokio::test]
async fn router_book_file_wildcard_routes_match_api_v1_and_opds_v2() {
    let ctx = TestFixture::new("router-book-file-wildcard-routes").await;
    let books_dir = ctx.paths().config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for file route test");
    let expected_body = b"router-book-file-content";
    std::fs::write(books_dir.join("book-1.epub"), expected_body)
        .expect("book fixture file should be written");

    let auth_token = ctx.login_admin().await;

    for route in [
        "/api/v1/books/book-1/file/book-1.epub",
        "/opds/v2/books/book-1/file/book-1.epub",
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
                    .expect("book file wildcard request should build"),
            )
            .await
            .expect("book file wildcard request should complete");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("book file wildcard response body should be readable");
        assert_eq!(body.as_ref(), expected_body);
    }
}

#[tokio::test]
async fn router_book_file_wildcard_returns_not_found_with_message_when_file_is_missing() {
    let ctx = TestFixture::new("router-book-file-wildcard-missing-file").await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/file/book-1.epub")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing wildcard book file request should build"),
        )
        .await
        .expect("missing wildcard book file request should complete");

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
async fn router_book_file_wildcard_returns_forbidden_for_restricted_user_even_when_file_is_missing()
{
    let ctx = TestFixture::new("router-book-file-wildcard-restricted-missing-file").await;
    seed_router_library_restricted_user(
        ctx.paths(),
        "member-user",
        "member@example.org",
        "router-contract-member-123",
        &[],
    )
    .await;

    let member_token = ctx
        .login_with_credentials("member@example.org", "router-contract-member-123")
        .await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/file/book-1.epub")
                .header("x-auth-token", &member_token)
                .body(Body::empty())
                .expect("restricted missing wildcard book file request should build"),
        )
        .await
        .expect("restricted missing wildcard book file request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

use super::*;

#[tokio::test]
async fn router_opds_v1_book_page_uses_zero_based_external_page_numbers() {
    let ctx = TestFixture::new("router-opds-v1-book-page-zero-based").await;
    seed_router_pdf_book(
        ctx.paths(),
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let auth_token = ctx.login_admin().await;

    let zero_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/books/book-pdf-1/pages/0")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 zero-based page request should build"),
        )
        .await
        .expect("opds v1 zero-based page request should complete");

    assert_eq!(zero_response.status(), StatusCode::OK);

    let one_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/books/book-pdf-1/pages/1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 one-based page request should build"),
        )
        .await
        .expect("opds v1 one-based page request should complete");

    assert_eq!(one_response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(one_response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Page number does not exist".to_string()))
    );
}

#[tokio::test]
async fn router_opds_book_page_routes_do_not_negotiate_pdf_for_pdf_books() {
    let ctx = TestFixture::new("router-opds-book-page-no-pdf-negotiation").await;
    seed_router_pdf_book(
        ctx.paths(),
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let auth_token = ctx.login_admin().await;

    for route in [
        "/opds/v1.2/books/book-pdf-1/pages/0",
        "/opds/v2/books/book-pdf-1/pages/1",
    ] {
        let response = ctx
            .app()
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .header(header::ACCEPT, "application/pdf")
                    .body(Body::empty())
                    .expect("opds pdf negotiation request should build"),
            )
            .await
            .expect("opds pdf negotiation request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .expect("opds page should expose content-type");
        assert!(
            content_type.starts_with("image/"),
            "route: {route}, content-type was: {content_type}"
        );
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("opds page body should be readable");
        image::load_from_memory(&body).expect("opds page body should decode as image");
    }
}

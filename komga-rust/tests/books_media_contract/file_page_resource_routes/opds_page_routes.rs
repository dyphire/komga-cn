use super::*;

#[tokio::test]
async fn router_opds_v1_book_page_uses_zero_based_external_page_numbers() {
    let paths = new_router_fixture("router-opds-v1-book-page-zero-based").await;
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

    let zero_response = app
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

    let one_response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_book_page_routes_do_not_negotiate_pdf_for_pdf_books() {
    let paths = new_router_fixture("router-opds-book-page-no-pdf-negotiation").await;
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

    for route in [
        "/opds/v1.2/books/book-pdf-1/pages/0",
        "/opds/v2/books/book-pdf-1/pages/1",
    ] {
        let response = app
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
    }

    cleanup_router_fixture(paths);
}

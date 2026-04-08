use super::*;

#[tokio::test]
async fn router_book_pages_and_raw_pages_include_inline_content_disposition() {
    let paths = new_router_fixture("router-book-pages-inline-disposition").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;
    update_router_book_name(&paths, "book-pdf-1", "Readable Page Title").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for route in [
        "/api/v1/books/book-pdf-1/pages/1",
        "/api/v1/books/book-pdf-1/pages/1/raw",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("page request should build"),
            )
            .await
            .expect("page request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
        let disposition = response
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .and_then(|value| value.to_str().ok())
            .expect("page response should expose content-disposition");
        assert!(
            disposition.starts_with("inline;"),
            "route: {route}, disposition: {disposition}"
        );
        assert!(
            disposition.contains("Readable Page Title-1"),
            "route: {route}, disposition: {disposition}"
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_page_thumbnail_resizes_largest_dimension_to_300px() {
    let paths = new_router_fixture("router-book-page-thumbnail-300px").await;
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

    let pages_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book pages request should build"),
        )
        .await
        .expect("book pages request should complete");
    assert_eq!(pages_response.status(), StatusCode::OK);
    let pages_payload = response_json(pages_response).await;
    let rows = pages_payload
        .as_array()
        .expect("book pages payload should be an array");
    let page_max_dimension = rows[0]
        .get("width")
        .and_then(Value::as_u64)
        .expect("book page metadata should expose width")
        .max(
            rows[0]
                .get("height")
                .and_then(Value::as_u64)
                .expect("book page metadata should expose height"),
        );

    let thumbnail_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book page thumbnail request should build"),
        )
        .await
        .expect("book page thumbnail request should complete");
    assert_eq!(thumbnail_response.status(), StatusCode::OK);
    let thumbnail_bytes = axum::body::to_bytes(thumbnail_response.into_body(), usize::MAX)
        .await
        .expect("book page thumbnail body should read");
    let thumbnail_image = image::load_from_memory(&thumbnail_bytes)
        .expect("book page thumbnail should decode as image");
    let thumbnail_max_dimension = thumbnail_image.width().max(thumbnail_image.height());

    assert!(
        page_max_dimension > 300,
        "page max dimension was {page_max_dimension}"
    );
    assert_eq!(thumbnail_max_dimension, 300);
    assert!(u64::from(thumbnail_image.width()) <= page_max_dimension);
    assert!(u64::from(thumbnail_image.height()) <= page_max_dimension);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_page_thumbnail_returns_bad_request_for_missing_page_number() {
    let paths = new_router_fixture("router-book-page-thumbnail-missing-page-number").await;
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
        "/api/v1/books/book-pdf-1/pages/0/thumbnail",
        "/api/v1/books/book-pdf-1/pages/2/thumbnail",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("book page thumbnail missing-page request should build"),
            )
            .await
            .expect("book page thumbnail missing-page request should complete");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "route: {route}");
        let payload = response_json(response).await;
        assert_eq!(
            payload.get("error"),
            Some(&Value::String("Page number does not exist".to_string())),
            "route: {route}"
        );
    }

    cleanup_router_fixture(paths);
}

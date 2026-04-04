use super::*;

#[tokio::test]
async fn router_opds_v2_divina_manifest_uses_page_media_type_in_reading_order() {
    let paths = new_router_fixture("router-opds-v2-divina-manifest-page-media-type").await;
    seed_router_contract_data(&paths).await;
    seed_router_cbz_book(&paths, "book-3", "book-3.cbz", "Book 3").await;

    let mut config = runtime_config_for_paths(&paths);
    config.mode = RuntimeMode::Isolated;
    let app = build_router_with_config(&config);
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/books/book-3/manifest/divina")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 divina manifest request should build"),
        )
        .await
        .expect("opds v2 divina manifest request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("readingOrder")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("type"))
            .and_then(Value::as_str),
        Some("image/png")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_manifest_divina_uses_page_media_type_in_reading_order() {
    let paths = new_router_fixture("router-book-manifest-divina-page-media-type").await;
    seed_router_contract_data(&paths).await;
    seed_router_cbz_book(&paths, "book-3", "book-3.cbz", "Book 3").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-3/manifest/divina")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("v1 divina manifest request should build"),
        )
        .await
        .expect("v1 divina manifest request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("readingOrder")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("href"))
            .and_then(Value::as_str),
        Some("http://localhost/api/v1/books/book-3/pages/1?contentNegotiation=false")
    );
    assert_eq!(
        payload
            .get("readingOrder")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("type"))
            .and_then(Value::as_str),
        Some("image/png")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_manifest_pdf_uses_raw_pdf_pages_in_reading_order() {
    let paths = new_router_fixture("router-book-manifest-pdf-reading-order").await;
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
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/manifest/pdf")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("pdf manifest request should build"),
        )
        .await
        .expect("pdf manifest request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let reading_order = payload
        .get("readingOrder")
        .and_then(Value::as_array)
        .expect("pdf manifest should expose readingOrder array");
    assert_eq!(reading_order.len(), 1);
    assert_eq!(
        reading_order[0].get("href").and_then(Value::as_str),
        Some("http://localhost/api/v1/books/book-pdf-1/pages/1/raw")
    );
    assert_eq!(
        reading_order[0].get("type").and_then(Value::as_str),
        Some("application/pdf")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_manifest_pdf_returns_bad_request_with_message_for_non_pdf_media() {
    let paths = new_router_fixture("router-book-manifest-pdf-profile-mismatch").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/manifest/pdf")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("pdf manifest profile mismatch request should build"),
        )
        .await
        .expect("pdf manifest profile mismatch request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "Book media type 'application/epub+zip' not compatible with requested profile"
                .to_string()
        ))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_manifest_divina_accepts_pdf_books() {
    let paths = new_router_fixture("router-book-manifest-divina-pdf-book").await;
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
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/manifest/divina")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("divina manifest pdf request should build"),
        )
        .await
        .expect("divina manifest pdf request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/divina+json")
    );
    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("readingOrder")
            .and_then(Value::as_array)
            .map(|v| v.len()),
        Some(1)
    );
    assert_eq!(
        payload
            .get("readingOrder")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("href"))
            .and_then(Value::as_str),
        Some("http://localhost/api/v1/books/book-pdf-1/pages/1?contentNegotiation=false")
    );
    assert_eq!(
        payload
            .get("readingOrder")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("type"))
            .and_then(Value::as_str),
        Some("image/jpeg")
    );

    cleanup_router_fixture(paths);
}

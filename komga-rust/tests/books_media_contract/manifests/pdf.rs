use super::*;

#[tokio::test]
async fn router_book_manifest_dispatches_to_pdf_profile_payload() {
    let paths = new_router_fixture("router-book-manifest-default-uses-pdf-profile").await;
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
                .uri("/api/v1/books/book-pdf-1/manifest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("generic pdf manifest request should build"),
        )
        .await
        .expect("generic pdf manifest request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|value| value.get("conformsTo"))
            .and_then(Value::as_str),
        Some("https://readium.org/webpub-manifest/profiles/pdf")
    );
    assert!(
        payload
            .get("links")
            .and_then(Value::as_array)
            .is_some_and(|entries| entries.iter().any(|entry| {
                entry.get("href").and_then(Value::as_str)
                    == Some("http://localhost/api/v1/books/book-pdf-1/manifest/divina")
                    && entry.get("type").and_then(Value::as_str) == Some("application/divina+json")
            })),
        "generic pdf manifest should expose divina alternate link: {payload:?}"
    );
    assert_eq!(
        payload
            .get("readingOrder")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("type"))
            .and_then(Value::as_str),
        Some("application/pdf")
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

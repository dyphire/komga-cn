use super::*;

fn overwrite_cbz_with_single_page(
    paths: &RuntimeDbPaths,
    relative_book_path: &str,
    page_name: &str,
    page_bytes: &[u8],
) {
    let archive_path = paths.config_dir.join(relative_book_path);
    let file = File::create(&archive_path).expect("cbz fixture should be recreated");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o644);
    zip.start_file(page_name, options)
        .expect("cbz replacement page entry should be created");
    zip.write_all(page_bytes)
        .expect("cbz replacement page payload should be written");
    zip.finish()
        .expect("cbz replacement fixture should finish successfully");
}

#[tokio::test]
async fn router_book_manifest_dispatches_to_divina_profile_payload() {
    let ctx = TestFixture::builder("router-book-manifest-default-uses-divina-profile")
        .with_seed(|paths| async move {
            seed_router_primary_series_cbz_book(&paths, "book-3", "book-3.cbz", "Book 3").await;
        })
        .build()
        .await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-3/manifest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("generic divina manifest request should build"),
        )
        .await
        .expect("generic divina manifest request should complete");

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
            .get("metadata")
            .and_then(|value| value.get("conformsTo"))
            .and_then(Value::as_str),
        Some("https://readium.org/webpub-manifest/profiles/divina")
    );
}

#[tokio::test]
async fn router_opds_v2_divina_manifest_uses_page_media_type_in_reading_order() {
    let ctx = TestFixture::builder("router-opds-v2-divina-manifest-page-media-type")
        .with_seed(|paths| async move {
            seed_router_primary_series_cbz_book(&paths, "book-3", "book-3.cbz", "Book 3").await;
        })
        .with_config(|config| {
            config.mode = RuntimeMode::Isolated;
        })
        .build()
        .await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
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
}

#[tokio::test]
async fn router_opds_v2_divina_manifest_exposes_jpeg_alternate_for_webp_pages() {
    let ctx = TestFixture::builder("router-opds-v2-divina-manifest-webp-alternate")
        .with_config(|config| {
            config.mode = RuntimeMode::Isolated;
        })
        .build()
        .await;
    seed_router_primary_series_cbz_book(ctx.paths(), "book-webp-1", "book-webp-1.cbz", "Book WEBP")
        .await;
    overwrite_cbz_with_single_page(
        ctx.paths(),
        "books/book-webp-1.cbz",
        "page-1.webp",
        &fixture_png_bytes(),
    );

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/books/book-webp-1/manifest/divina")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 webp divina manifest request should build"),
        )
        .await
        .expect("opds v2 webp divina manifest request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("readingOrder")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("type"))
            .and_then(Value::as_str),
        Some("image/webp")
    );
    assert_eq!(
        payload
            .get("readingOrder")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("alternate"))
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("href"))
            .and_then(Value::as_str),
        Some(
            "http://localhost/opds/v2/books/book-webp-1/pages/1?contentNegotiation=false&convert=jpeg"
        )
    );
    assert_eq!(
        payload
            .get("readingOrder")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("alternate"))
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("type"))
            .and_then(Value::as_str),
        Some("image/jpeg")
    );
}

#[tokio::test]
async fn router_book_manifest_divina_uses_page_media_type_in_reading_order() {
    let ctx = TestFixture::builder("router-book-manifest-divina-page-media-type")
        .with_seed(|paths| async move {
            seed_router_primary_series_cbz_book(&paths, "book-3", "book-3.cbz", "Book 3").await;
        })
        .build()
        .await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
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
}

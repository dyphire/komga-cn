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
async fn router_book_manifest_epub_exposes_epub_specific_shape() {
    let paths = new_router_fixture("router-book-manifest-epub-specific-shape").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_with_cover(&paths, "books/book-1.epub");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for epub manifest seed");
    for (file_name, media_type, sub_type, file_size) in [
        (
            "OEBPS/chapter.xhtml",
            "application/xhtml+xml",
            "EPUB_PAGE",
            128_i64,
        ),
        ("OEBPS/images/cover.png", "image/png", "EPUB_ASSET", 67_i64),
    ] {
        sqlx::query(
            "INSERT INTO MEDIA_FILE (FILE_NAME, BOOK_ID, MEDIA_TYPE, SUB_TYPE, FILE_SIZE) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(file_name)
        .bind("book-1")
        .bind(media_type)
        .bind(sub_type)
        .bind(file_size)
        .execute(&pool)
        .await
        .expect("epub media file row should be inserted");
    }
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_manifest_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension blob should be seeded");
    sqlx::query("UPDATE BOOK_METADATA SET SUMMARY = ?, ISBN = ? WHERE BOOK_ID = ?")
        .bind("Fixture summary")
        .bind("9781234567890")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub manifest metadata should be seeded");
    sqlx::query("UPDATE BOOK SET LAST_MODIFIED_DATE = ? WHERE ID = ?")
        .bind("2024-02-03 04:05:06")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub manifest modified timestamp should be seeded");
    sqlx::query("UPDATE SERIES_METADATA SET READING_DIRECTION = ? WHERE SERIES_ID = ?")
        .bind("RIGHT_TO_LEFT")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("epub manifest reading direction should be seeded");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/manifest/epub")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("epub manifest request should build"),
        )
        .await
        .expect("epub manifest request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;

    assert_eq!(
        payload
            .get("metadata")
            .and_then(|value| value.get("description"))
            .and_then(Value::as_str),
        Some("Fixture summary")
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|value| value.get("identifier"))
            .and_then(Value::as_str),
        Some("urn:isbn:9781234567890")
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|value| value.get("numberOfPages"))
            .and_then(Value::as_i64),
        Some(10)
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|value| value.get("published"))
            .and_then(Value::as_str),
        Some("2024-01-15")
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|value| value.get("modified"))
            .and_then(Value::as_str),
        Some("2024-02-03T04:05:06Z")
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|value| value.get("subject"))
            .and_then(Value::as_array),
        Some(&vec![Value::String("favorite-tag".to_string())])
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|value| value.get("conformsTo"))
            .and_then(Value::as_str),
        Some("https://readium.org/webpub-manifest/profiles/epub")
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|value| value.get("rendition"))
            .and_then(|value| value.get("layout"))
            .and_then(Value::as_str),
        Some("reflowable")
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|value| value.get("language"))
            .and_then(Value::as_str),
        Some("EN")
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|value| value.get("readingProgression"))
            .and_then(Value::as_str),
        Some("rtl")
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|value| value.get("contributor"))
            .and_then(Value::as_array),
        Some(&vec![Value::String("Jane Writer".to_string())])
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|value| value.get("belongsTo"))
            .and_then(|value| value.get("series"))
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("name"))
            .and_then(Value::as_str),
        Some("Series 1")
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|value| value.get("belongsTo"))
            .and_then(|value| value.get("series"))
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("position"))
            .and_then(Value::as_f64),
        Some(1.0)
    );
    assert_eq!(
        payload
            .get("readingOrder")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("href"))
            .and_then(Value::as_str),
        Some("http://localhost/api/v1/books/book-1/resource/OEBPS/chapter.xhtml")
    );
    assert_eq!(
        payload
            .get("readingOrder")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("type"))
            .and_then(Value::as_str),
        Some("application/xhtml+xml")
    );
    assert!(
        payload
            .get("resources")
            .and_then(Value::as_array)
            .is_some_and(|entries| entries.iter().any(|entry| {
                entry.get("href").and_then(Value::as_str)
                    == Some("http://localhost/api/v1/books/book-1/resource/OEBPS/images/cover.png")
                    && entry.get("type").and_then(Value::as_str) == Some("image/png")
            })),
        "epub manifest should expose epub asset resource: {payload:?}"
    );
    assert_eq!(
        payload
            .get("toc")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("href"))
            .and_then(Value::as_str),
        Some("http://localhost/api/v1/books/book-1/resource/OEBPS/chapter.xhtml#part-1")
    );
    assert_eq!(
        payload
            .get("landmarks")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("href"))
            .and_then(Value::as_str),
        Some("http://localhost/api/v1/books/book-1/resource/OEBPS/images/cover.png")
    );
    assert_eq!(
        payload
            .get("pageList")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("href"))
            .and_then(Value::as_str),
        Some("http://localhost/api/v1/books/book-1/resource/OEBPS/chapter.xhtml#page-1")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_manifest_dispatches_to_epub_profile_payload() {
    let paths = new_router_fixture("router-book-manifest-default-uses-epub-profile").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_with_cover(&paths, "books/book-1.epub");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for generic epub manifest seed");
    for (file_name, media_type, sub_type, file_size) in [
        (
            "OEBPS/chapter.xhtml",
            "application/xhtml+xml",
            "EPUB_PAGE",
            128_i64,
        ),
        ("OEBPS/images/cover.png", "image/png", "EPUB_ASSET", 67_i64),
    ] {
        sqlx::query(
            "INSERT INTO MEDIA_FILE (FILE_NAME, BOOK_ID, MEDIA_TYPE, SUB_TYPE, FILE_SIZE) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(file_name)
        .bind("book-1")
        .bind(media_type)
        .bind(sub_type)
        .bind(file_size)
        .execute(&pool)
        .await
        .expect("generic epub manifest media file row should insert");
    }
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_manifest_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("generic epub manifest extension should seed");
    sqlx::query("UPDATE MEDIA SET EPUB_DIVINA_COMPATIBLE = ? WHERE BOOK_ID = ?")
        .bind(true)
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("generic epub manifest divina compatibility should seed");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/manifest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("generic epub manifest request should build"),
        )
        .await
        .expect("generic epub manifest request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|value| value.get("conformsTo"))
            .and_then(Value::as_str),
        Some("https://readium.org/webpub-manifest/profiles/epub")
    );
    assert!(
        payload
            .get("links")
            .and_then(Value::as_array)
            .is_some_and(|entries| entries.iter().any(|entry| {
                entry.get("href").and_then(Value::as_str)
                    == Some("http://localhost/api/v1/books/book-1/manifest/divina")
                    && entry.get("type").and_then(Value::as_str) == Some("application/divina+json")
            })),
        "generic epub manifest should expose divina alternate link when epub is compatible: {payload:?}"
    );

    cleanup_router_fixture(paths);
}

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
async fn router_book_manifest_dispatches_to_divina_profile_payload() {
    let paths = new_router_fixture("router-book-manifest-default-uses-divina-profile").await;
    seed_router_contract_data(&paths).await;
    seed_router_cbz_book(&paths, "book-3", "book-3.cbz", "Book 3").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

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
async fn router_opds_v2_divina_manifest_exposes_jpeg_alternate_for_webp_pages() {
    let paths = new_router_fixture("router-opds-v2-divina-manifest-webp-alternate").await;
    seed_router_contract_data(&paths).await;
    seed_router_cbz_book(&paths, "book-webp-1", "book-webp-1.cbz", "Book WEBP").await;
    overwrite_cbz_with_single_page(
        &paths,
        "books/book-webp-1.cbz",
        "page-1.webp",
        &fixture_png_bytes(),
    );

    let mut config = runtime_config_for_paths(&paths);
    config.mode = RuntimeMode::Isolated;
    let app = build_router_with_config(&config);
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_divina_manifest_accepts_divina_compatible_epub() {
    let paths = new_router_fixture("router-opds-v2-divina-manifest-compatible-epub").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_with_cover(&paths, "books/book-1.epub");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for opds epub divina manifest seed");
    for (file_name, media_type, sub_type, file_size) in [
        (
            "OEBPS/chapter.xhtml",
            "application/xhtml+xml",
            "EPUB_PAGE",
            128_i64,
        ),
        ("OEBPS/images/cover.png", "image/png", "EPUB_ASSET", 67_i64),
    ] {
        sqlx::query(
            "INSERT INTO MEDIA_FILE (FILE_NAME, BOOK_ID, MEDIA_TYPE, SUB_TYPE, FILE_SIZE) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(file_name)
        .bind("book-1")
        .bind(media_type)
        .bind(sub_type)
        .bind(file_size)
        .execute(&pool)
        .await
        .expect("opds epub divina manifest media file row should insert");
    }
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_manifest_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("opds epub divina manifest extension should seed");
    sqlx::query("UPDATE MEDIA SET EPUB_DIVINA_COMPATIBLE = ? WHERE BOOK_ID = ?")
        .bind(true)
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("opds epub divina compatibility should seed");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/books/book-1/manifest/divina")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds epub divina manifest request should build"),
        )
        .await
        .expect("opds epub divina manifest request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/opds-publication+json")
    );
    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|value| value.get("conformsTo"))
            .and_then(Value::as_str),
        Some("https://readium.org/webpub-manifest/profiles/divina")
    );
    assert_eq!(
        payload
            .get("readingOrder")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("href"))
            .and_then(Value::as_str),
        Some("http://localhost/opds/v2/books/book-1/pages/1?contentNegotiation=false")
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

use super::*;

async fn seed_epub_manifest_media(
    paths: &RuntimeDbPaths,
    context: &str,
    set_divina_compatible: bool,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect(context);
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
        .expect("epub manifest media file row should be inserted");
    }
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_manifest_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub manifest extension blob should be seeded");
    if set_divina_compatible {
        sqlx::query("UPDATE MEDIA SET EPUB_DIVINA_COMPATIBLE = ? WHERE BOOK_ID = ?")
            .bind(true)
            .bind("book-1")
            .execute(&pool)
            .await
            .expect("epub divina compatibility should be seeded");
    }
    pool.close().await;
}

#[tokio::test]
async fn router_book_manifest_epub_exposes_epub_specific_shape() {
    let paths = new_router_fixture("router-book-manifest-epub-specific-shape").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_with_cover(&paths, "books/book-1.epub");
    seed_epub_manifest_media(&paths, "main db should open for epub manifest seed", false).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for epub manifest metadata seed");
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
    seed_epub_manifest_media(
        &paths,
        "main db should open for generic epub manifest seed",
        true,
    )
    .await;

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
async fn router_opds_v2_divina_manifest_accepts_divina_compatible_epub() {
    let paths = new_router_fixture("router-opds-v2-divina-manifest-compatible-epub").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_with_cover(&paths, "books/book-1.epub");
    seed_epub_manifest_media(
        &paths,
        "main db should open for opds epub divina manifest seed",
        true,
    )
    .await;

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

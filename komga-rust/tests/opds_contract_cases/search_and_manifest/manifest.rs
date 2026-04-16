use super::*;

#[tokio::test]
async fn router_opds_v2_manifest_sets_private_cache_and_supports_if_none_match() {
    let paths = new_router_fixture("router-opds-v2-manifest-cache-headers").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let first_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/books/book-1/manifest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 manifest conditional request should build"),
        )
        .await
        .expect("opds v2 manifest conditional request should complete");

    assert_eq!(first_response.status(), StatusCode::OK);
    assert_eq!(
        first_response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("max-age=0, must-revalidate, private")
    );

    let etag = first_response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("opds v2 manifest response should include etag");

    let second_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/books/book-1/manifest")
                .header("x-auth-token", &auth_token)
                .header(header::IF_NONE_MATCH, etag)
                .body(Body::empty())
                .expect("opds v2 manifest conditional follow-up request should build"),
        )
        .await
        .expect("opds v2 manifest conditional follow-up request should complete");

    assert_eq!(second_response.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(
        second_response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("max-age=0, must-revalidate, private")
    );
    assert!(second_response.headers().contains_key(header::ETAG));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_manifest_dispatches_to_epub_profile_payload() {
    let paths = new_router_fixture("router-opds-v2-manifest-default-uses-epub-profile").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_resource(
        &paths,
        "books/book-1.epub",
        "OEBPS/chapter.xhtml",
        br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>Hello</p></body></html>"#,
    );

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds manifest epub profile db should open");
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
        .expect("opds manifest epub media file row should insert");
    }
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_manifest_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("opds manifest epub extension should seed");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/books/book-1/manifest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 generic manifest request should build"),
        )
        .await
        .expect("opds v2 generic manifest request should complete");

    if response.status() != StatusCode::OK {
        let status = response.status();
        let body = response_text(response).await;
        panic!("unexpected search status {status}: {body}");
    }
    let payload = response_json(response).await;

    assert_eq!(
        payload
            .get("metadata")
            .and_then(|value| value.get("conformsTo"))
            .and_then(Value::as_str),
        Some("https://readium.org/webpub-manifest/profiles/epub")
    );
    assert_eq!(
        payload
            .get("links")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("href"))
            .and_then(Value::as_str),
        Some("http://localhost/opds/v2/books/book-1/manifest")
    );
    assert_eq!(
        payload
            .get("links")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("type"))
            .and_then(Value::as_str),
        Some("application/webpub+json")
    );
    assert_eq!(
        payload
            .get("links")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("properties"))
            .and_then(|properties| properties.get("authenticate"))
            .and_then(|authenticate| authenticate.get("href"))
            .and_then(Value::as_str),
        Some("http://localhost/opds/v2/auth")
    );

    cleanup_router_fixture(paths);
}

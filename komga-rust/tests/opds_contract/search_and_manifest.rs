use super::*;

fn fixture_epub_manifest_extension_blob() -> Vec<u8> {
    vec![
        31, 139, 8, 0, 156, 175, 212, 105, 2, 255, 133, 142, 193, 10, 131, 48, 16, 68, 127, 165,
        108, 175, 86, 241, 154, 99, 75, 123, 18, 42, 120, 44, 30, 150, 184, 38, 161, 209, 132, 100,
        45, 22, 241, 223, 155, 30, 45, 66, 143, 51, 243, 120, 204, 2, 38, 222, 204, 76, 93, 133,
        111, 55, 49, 136, 30, 109, 164, 12, 216, 73, 16, 143, 5, 216, 176, 37, 16, 208, 48, 6, 134,
        12, 116, 160, 62, 197, 251, 245, 92, 55, 133, 212, 232, 153, 66, 62, 107, 30, 236, 209, 39,
        226, 84, 38, 70, 106, 99, 187, 64, 99, 18, 180, 107, 155, 129, 197, 177, 27, 48, 60, 227,
        198, 120, 113, 47, 10, 191, 70, 51, 160, 162, 88, 200, 239, 150, 251, 81, 237, 216, 124,
        34, 42, 19, 121, 35, 171, 83, 121, 40, 255, 253, 83, 180, 243, 111, 253, 0, 129, 229, 31,
        54, 3, 1, 0, 0,
    ]
}

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
    sqlx::query("UPDATE BOOK SET LAST_MODIFIED_DATE = ? WHERE ID = ?")
        .bind("2024-02-03 04:05:06")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("opds manifest book last modified should seed");
    sqlx::query("UPDATE BOOK_METADATA SET SUMMARY = ?, ISBN = ? WHERE BOOK_ID = ?")
        .bind("Fixture summary")
        .bind("9781234567890")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("opds manifest book metadata should seed");
    sqlx::query("UPDATE SERIES_METADATA SET READING_DIRECTION = ? WHERE SERIES_ID = ?")
        .bind("RIGHT_TO_LEFT")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("opds manifest series reading direction should seed");
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
            .and_then(|value| value.get("description"))
            .and_then(Value::as_str),
        Some("Fixture summary")
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
    assert_eq!(
        payload
            .get("readingOrder")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("href"))
            .and_then(Value::as_str),
        Some("http://localhost/opds/v2/books/book-1/resource/OEBPS/chapter.xhtml")
    );
    assert!(
        payload
            .get("resources")
            .and_then(Value::as_array)
            .is_some_and(|entries| entries.iter().any(|entry| {
                entry.get("href").and_then(Value::as_str)
                    == Some("http://localhost/opds/v2/books/book-1/resource/OEBPS/images/cover.png")
                    && entry.get("type").and_then(Value::as_str) == Some("image/png")
            })),
        "opds v2 epub manifest should expose epub asset resource: {payload:?}"
    );
    assert_eq!(
        payload
            .get("toc")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("href"))
            .and_then(Value::as_str),
        Some("http://localhost/opds/v2/books/book-1/resource/OEBPS/chapter.xhtml#part-1")
    );
    assert_eq!(
        payload
            .get("pageList")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("href"))
            .and_then(Value::as_str),
        Some("http://localhost/opds/v2/books/book-1/resource/OEBPS/chapter.xhtml#page-1")
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|value| value.get("belongsTo"))
            .and_then(|value| value.get("series"))
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("links"))
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("href"))
            .and_then(Value::as_str),
        Some("http://localhost/opds/v2/series/series-1")
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|value| value.get("belongsTo"))
            .and_then(|value| value.get("series"))
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("links"))
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("type"))
            .and_then(Value::as_str),
        Some("application/opds+json")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_latest_books_feed_hides_books_for_age_exclude_restricted_user() {
    let paths = new_router_fixture("router-opds-v2-latest-books-age-restricted").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/library-1/books/latest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 latest-books request should build"),
        )
        .await
        .expect("opds v2 latest-books request should complete");

    let status = response.status();
    let payload = response_json(response).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "unexpected OPDS latest-books response payload: {payload}",
    );
    assert_eq!(
        payload
            .get("publications")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(0),
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_search_feed_uses_search_title_for_non_blank_search() {
    let paths = new_router_fixture("router-opds-v1-series-search-title").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series?search=Series")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 series search request should build"),
        )
        .await
        .expect("opds v1 series search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains("Series search for: Series"),
        "OPDS v1 non-blank search must expose Kotlin-compatible feed title, body={body}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_unauthorized_includes_basic_challenge() {
    let paths = new_router_fixture("router-opds-v1-series-basic-challenge").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series")
                .body(Body::empty())
                .expect("opds v1 series unauthorized request should build"),
        )
        .await
        .expect("opds v1 series unauthorized request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"Realm\"")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_preserves_active_query_params_in_self_prev_next_links() {
    let paths = new_router_fixture("router-opds-v1-series-query-links").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series?page=1&size=1&search=Series&publisher=PubHouse&publisher=AltPub")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 series query-links request should build"),
        )
        .await
        .expect("opds v1 series query-links request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains("rel=\"self\" href=\"http://localhost/opds/v1.2/series?search=Series&amp;publisher=PubHouse&amp;publisher=AltPub\""),
        "body={body}"
    );
    assert!(
        body.contains("rel=\"previous\" href=\"http://localhost/opds/v1.2/series?search=Series&amp;publisher=PubHouse&amp;publisher=AltPub&amp;page=0\""),
        "body={body}"
    );
    assert!(
        body.contains("rel=\"next\" href=\"http://localhost/opds/v1.2/series?search=Series&amp;publisher=PubHouse&amp;publisher=AltPub&amp;page=2\""),
        "body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_feed_uses_series_last_modified_for_entry_updated() {
    let paths = new_router_fixture("router-opds-v1-series-entry-updated").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 series entry-updated db should open");
    sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ?, CREATED_DATE = ? WHERE ID = ?")
        .bind("2024-03-03 00:00:00")
        .bind("2024-03-03 00:00:00")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series last modified should update for entry-updated test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 series entry-updated request should build"),
        )
        .await
        .expect("opds v1 series entry-updated request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains("<entry><title>Series 1</title><updated>2024-03-03T00:00:00Z</updated><id>series-1</id><content></content>"),
        "body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_search_feed_uses_series_last_modified_for_entry_updated() {
    let paths = new_router_fixture("router-opds-v1-series-search-entry-updated").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 series search entry-updated db should open");
    sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ?, CREATED_DATE = ? WHERE ID = ?")
        .bind("2024-03-04 00:00:00")
        .bind("2024-03-04 00:00:00")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series last modified should update for search entry-updated test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series?search=Series%201")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 series search entry-updated request should build"),
        )
        .await
        .expect("opds v1 series search entry-updated request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains("<entry><title>Series 1</title><updated>2024-03-04T00:00:00Z</updated><id>series-1</id><content></content>"),
        "body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_search_uses_acquisition_type_and_utf8_encodings() {
    let paths = new_router_fixture("router-opds-v1-search-opensearch-shape").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/search")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 search request should build"),
        )
        .await
        .expect("opds v1 search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains("application/atom+xml;profile=opds-catalog;kind=acquisition"),
        "OPDS v1 search must use acquisition kind in OpenSearch Url, body={body}"
    );
    assert!(
        body.contains("<InputEncoding>UTF-8</InputEncoding>"),
        "OPDS v1 search must include InputEncoding UTF-8, body={body}"
    );
    assert!(
        body.contains("<OutputEncoding>UTF-8</OutputEncoding>"),
        "OPDS v1 search must include OutputEncoding UTF-8, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_blank_search_behaves_as_unfiltered_series_feed() {
    let paths = new_router_fixture("router-opds-v1-series-blank-search").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-alpha", "Alpha Series", "library-1").await;
    seed_router_custom_series(&paths, "series-zeta", "Zeta Series", "library-1").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series?search=%20%20%20")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 blank-search request should build"),
        )
        .await
        .expect("opds v1 blank-search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains("<title>All series</title>"),
        "OPDS v1 blank search must fall back to unfiltered All series feed, body={body}",
    );
    assert!(
        body.contains("/opds/v1.2/series/series-alpha")
            && body.contains("/opds/v1.2/series/series-zeta"),
        "OPDS v1 blank search must not filter out matching libraries, body={body}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_search_hides_unauthorized_library_series() {
    let paths = new_router_fixture("router-opds-v1-series-library-visibility").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-restricted-user",
        "library.restricted@example.org",
        "router-contract-library-restricted-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library.restricted@example.org",
        "router-contract-library-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series?search=Series")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 restricted search request should build"),
        )
        .await
        .expect("opds v1 restricted search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        !body.contains("/opds/v1.2/series/series-3"),
        "OPDS v1 search must hide series from unauthorized libraries, body={body}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_latest_series_feed_hides_series_for_age_exclude_restricted_user() {
    let paths = new_router_fixture("router-opds-v1-latest-series-age-restricted").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series/latest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 latest-series request should build"),
        )
        .await
        .expect("opds v1 latest-series request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        !body.contains("/opds/v1.2/series/series-1"),
        "OPDS v1 latest-series feed must hide restricted series, body={body}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_latest_series_feed_paginates_after_restriction_filtering() {
    let paths = new_router_fixture("router-opds-v1-latest-series-restricted-pagination").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-2", "Series 2", "library-1").await;
    seed_router_custom_series(&paths, "series-0", "Series 0", "library-1").await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds latest-series pagination db should open");
    sqlx::query("UPDATE SERIES_METADATA SET AGE_RATING = ? WHERE SERIES_ID = ?")
        .bind(0_i64)
        .bind("series-0")
        .execute(&pool)
        .await
        .expect("visible latest series age rating should update");
    for (series_id, last_modified) in [
        ("series-2", "2024-03-03T00:00:00"),
        ("series-1", "2024-03-02T00:00:00"),
        ("series-0", "2024-03-01T00:00:00"),
    ] {
        sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ?, CREATED_DATE = ? WHERE ID = ?")
            .bind(last_modified)
            .bind(last_modified)
            .bind(series_id)
            .execute(&pool)
            .await
            .expect("series latest ordering should update");
    }
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series/latest?page=0&size=1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 latest-series paged request should build"),
        )
        .await
        .expect("opds v1 latest-series paged request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("/opds/v1.2/series/series-0"));
    assert!(!body.contains("/opds/v1.2/series/series-2"));
    assert!(!body.contains("/opds/v1.2/series/series-1"));
    assert!(
        !body.contains("rel=\"next\""),
        "OPDS v1 latest-series must compute pagination after restrictions filtering, body={body}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_latest_series_feed_normalizes_entry_updated_to_utc_z() {
    let paths = new_router_fixture("router-opds-v1-latest-series-updated-format").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds latest-series updated db should open");
    sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ?, CREATED_DATE = ? WHERE ID = ?")
        .bind("2024-03-03 00:00:00")
        .bind("2024-03-03 00:00:00")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("latest series updated timestamp should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series/latest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 latest-series updated request should build"),
        )
        .await
        .expect("opds v1 latest-series updated request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains("<updated>2024-03-03T00:00:00Z</updated>"),
        "OPDS v1 latest-series entry updated must be normalized to UTC/Z, body={body}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_search_supports_fielded_query_candidate_lookup() {
    let paths = new_router_fixture("router-opds-v1-series-fielded-query").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series?search=publisher:AltPub")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 fielded search request should build"),
        )
        .await
        .expect("opds v1 fielded search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains("Series search for: publisher:AltPub"),
        "OPDS v1 fielded search should preserve feed title semantics, body={body}",
    );
    assert!(
        body.contains("/opds/v1.2/series/series-3"),
        "OPDS v1 fielded search should surface unified-search candidate matches, body={body}",
    );
    assert!(
        !body.contains("/opds/v1.2/series/series-1")
            && !body.contains("/opds/v1.2/series/series-2"),
        "OPDS v1 fielded search should keep result set narrowed to matching candidates, body={body}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_search_query_contract_covers_group_presence_and_order() {
    let paths = new_router_fixture("router-opds-v2-search-group-contract").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let expectations = [
        (
            "/opds/v2/search?query=Series%201",
            vec!["Series"],
            "single-group search should only retain non-empty groups",
        ),
        (
            "/opds/v2/search?query=1",
            vec!["Series", "Books", "Read Lists"],
            "multi-group search should preserve Kotlin group ordering",
        ),
    ];

    for (uri, expected_group_titles, context) in expectations {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(uri)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("opds v2 search request should build"),
            )
            .await
            .expect("opds v2 search request should complete");

        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        let groups = payload
            .get("groups")
            .and_then(Value::as_array)
            .expect("opds v2 search payload should expose groups array");
        let group_titles = groups
            .iter()
            .filter_map(|group| {
                group
                    .get("metadata")
                    .and_then(|value| value.get("title"))
                    .and_then(Value::as_str)
            })
            .collect::<Vec<_>>();

        assert_eq!(group_titles, expected_group_titles, "{context}: {payload}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_search_supports_fielded_query_candidate_lookup() {
    let paths = new_router_fixture("router-opds-v2-search-fielded-query").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/search?query=title:1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 fielded search request should build"),
        )
        .await
        .expect("opds v2 fielded search request should complete");

    if response.status() != StatusCode::OK {
        let status = response.status();
        let body = response_text(response).await;
        panic!("unexpected search status {status}: {body}");
    }
    let payload = response_json(response).await;
    let groups = payload
        .get("groups")
        .and_then(Value::as_array)
        .expect("opds v2 fielded search payload should expose groups array");
    let group_titles = groups
        .iter()
        .filter_map(|group| {
            group
                .get("metadata")
                .and_then(|value| value.get("title"))
                .and_then(Value::as_str)
        })
        .collect::<Vec<_>>();

    assert_eq!(
        group_titles,
        vec!["Series", "Books", "Read Lists"],
        "{payload}"
    );

    let rendered = payload.to_string();
    assert!(
        rendered.contains("/opds/v2/series/series-1")
            && rendered.contains("book-1/manifest")
            && rendered.contains("/opds/v2/readlists/readlist-1"),
        "OPDS v2 fielded search should include unified-search candidate matches: {payload}",
    );
    assert!(
        !rendered.contains("/opds/v2/series/series-2")
            && !rendered.contains("/opds/v2/series/series-3")
            && !rendered.contains("book-2/manifest")
            && !rendered.contains("book-3/manifest"),
        "OPDS v2 fielded search should keep non-matching entities out of groups: {payload}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_search_excludes_one_shot_series_for_blank_and_ranked_queries() {
    let paths = new_router_fixture("router-opds-v2-search-excludes-one-shots").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(
        &paths,
        "series-oneshot-search",
        "One Shot Search",
        "library-1",
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v2 search one-shot db should open");
    sqlx::query("UPDATE SERIES SET ONESHOT = ? WHERE ID = ?")
        .bind(true)
        .bind("series-oneshot-search")
        .execute(&pool)
        .await
        .expect("opds v2 search one-shot series should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let expectations = [
        (
            "/opds/v2/search",
            "blank search should omit one-shot series from default results",
        ),
        (
            "/opds/v2/search?query=One%20Shot%20Search",
            "fielded search should omit one-shot series from ranked results",
        ),
    ];

    for (uri, context) in expectations {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(uri)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("opds v2 one-shot search request should build"),
            )
            .await
            .expect("opds v2 one-shot search request should complete");

        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        let rendered = payload.to_string();
        assert!(
            !rendered.contains("/opds/v2/series/series-oneshot-search"),
            "{context}: {payload}",
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_search_books_group_uses_shared_publication_shape() {
    let paths = new_router_fixture("router-opds-v2-search-book-publication-shape").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v2 search publication db should open");
    sqlx::query(
        "UPDATE BOOK_METADATA SET SUMMARY = ?, ISBN = ?, RELEASE_DATE = ? WHERE BOOK_ID = ?",
    )
    .bind("Search fixture summary")
    .bind("9781234567890")
    .bind("2024-02-03T04:05:06Z")
    .bind("book-1")
    .execute(&pool)
    .await
    .expect("opds v2 search book metadata should seed");
    sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
        .bind("book-1")
        .bind("Search Author")
        .bind("author")
        .execute(&pool)
        .await
        .expect("opds v2 search author should seed");
    sqlx::query("INSERT INTO BOOK_METADATA_TAG (BOOK_ID, TAG) VALUES (?, ?)")
        .bind("book-1")
        .bind("SearchTag")
        .execute(&pool)
        .await
        .expect("opds v2 search tag should seed");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/search?query=title:1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 search book publication request should build"),
        )
        .await
        .expect("opds v2 search book publication request should complete");

    if response.status() != StatusCode::OK {
        let status = response.status();
        let body = response_text(response).await;
        panic!("unexpected search status {status}: {body}");
    }
    let payload = response_json(response).await;
    let books_group = payload
        .get("groups")
        .and_then(Value::as_array)
        .and_then(|groups| {
            groups.iter().find(|group| {
                group
                    .get("metadata")
                    .and_then(|value| value.get("title"))
                    .and_then(Value::as_str)
                    == Some("Books")
            })
        })
        .expect("opds v2 search should expose Books group");
    let publication = books_group
        .get("publications")
        .and_then(Value::as_array)
        .and_then(|publications| publications.first())
        .expect("opds v2 search Books group should expose publications");

    assert!(
        payload
            .get("metadata")
            .and_then(|value| value.get("modified"))
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        "search feed should expose modified metadata: {payload:?}"
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|value| value.get("identifier"))
            .and_then(Value::as_str),
        Some("urn:isbn:9781234567890")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|value| value.get("description"))
            .and_then(Value::as_str),
        Some("Search fixture summary")
    );
    assert!(
        publication
            .get("metadata")
            .and_then(|value| value.get("author"))
            .and_then(Value::as_array)
            .is_some_and(|authors| authors
                .iter()
                .any(|author| author.as_str() == Some("Search Author"))),
        "search publication should expose author metadata: {publication:?}"
    );
    assert!(
        publication
            .get("metadata")
            .and_then(|value| value.get("subject"))
            .and_then(Value::as_array)
            .is_some_and(|subjects| subjects
                .iter()
                .any(|subject| subject.as_str() == Some("SearchTag"))),
        "search publication should expose subject metadata: {publication:?}"
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|value| value.get("belongsTo"))
            .and_then(|value| value.get("series"))
            .and_then(Value::as_array)
            .and_then(|series| series.first())
            .and_then(|series| series.get("links"))
            .and_then(Value::as_array)
            .and_then(|links| links.first())
            .and_then(|link| link.get("href"))
            .and_then(Value::as_str),
        Some("http://localhost/opds/v2/series/series-1")
    );
    assert!(
        publication
            .get("links")
            .and_then(Value::as_array)
            .is_some_and(|links| {
                links.iter().any(|link| {
                    link.get("rel").and_then(Value::as_str)
                        == Some("http://opds-spec.org/acquisition")
                        && link.get("href").and_then(Value::as_str)
                            == Some("http://localhost/opds/v2/books/book-1/file")
                })
            }),
        "search publication should expose acquisition link: {publication:?}"
    );
    assert!(
        publication
            .get("links")
            .and_then(Value::as_array)
            .is_some_and(|links| {
                links.iter().any(|link| {
                    link.get("rel").and_then(Value::as_str)
                        == Some("http://www.cantook.com/api/progression")
                        && link.get("href").and_then(Value::as_str)
                            == Some("http://localhost/opds/v2/books/book-1/progression")
                })
            }),
        "search publication should expose progression link: {publication:?}"
    );
    assert_eq!(
        publication
            .get("images")
            .and_then(Value::as_array)
            .and_then(|images| images.first())
            .and_then(|image| image.get("href"))
            .and_then(Value::as_str),
        Some("http://localhost/opds/v2/books/book-1/thumbnail")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_search_hides_unauthorized_library_results() {
    let paths = new_router_fixture("router-opds-v2-search-library-visibility").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-restricted-user-v2",
        "library.restricted.v2@example.org",
        "router-contract-library-restricted-v2-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library.restricted.v2@example.org",
        "router-contract-library-restricted-v2-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/search?query=Series%203")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 restricted search request should build"),
        )
        .await
        .expect("opds v2 restricted search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let groups = payload
        .get("groups")
        .and_then(Value::as_array)
        .expect("opds v2 restricted search payload should expose groups array");
    assert!(
        groups.is_empty(),
        "OPDS v2 search must omit unauthorized-only results instead of returning empty groups: {payload}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_search_hides_results_for_age_exclude_restricted_user() {
    let paths = new_router_fixture("router-opds-v2-search-age-restricted").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user(
        &paths,
        "search-restricted-user",
        "search.restricted@example.org",
        "router-contract-search-restricted-123",
        12,
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let admin_auth_token = login_with_basic_and_get_token(app.clone()).await;
    let restricted_auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "search.restricted@example.org",
        "router-contract-search-restricted-123",
    )
    .await;

    let expectations = [
        (
            "/opds/v2/search?query=1",
            vec!["Series", "Books", "Read Lists"],
        ),
        ("/opds/v2/search", vec!["Series", "Books", "Read Lists"]),
    ];

    for (uri, expected_admin_group_titles) in expectations {
        let admin_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(uri)
                    .header("x-auth-token", &admin_auth_token)
                    .body(Body::empty())
                    .expect("opds v2 search admin request should build"),
            )
            .await
            .expect("opds v2 search admin request should complete");

        assert_eq!(admin_response.status(), StatusCode::OK);
        let admin_payload = response_json(admin_response).await;
        let admin_group_titles = admin_payload
            .get("groups")
            .and_then(Value::as_array)
            .expect("opds v2 search admin payload should expose groups array")
            .iter()
            .filter_map(|group| {
                group
                    .get("metadata")
                    .and_then(|value| value.get("title"))
                    .and_then(Value::as_str)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            admin_group_titles, expected_admin_group_titles,
            "{admin_payload}"
        );

        let restricted_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(uri)
                    .header("x-auth-token", &restricted_auth_token)
                    .body(Body::empty())
                    .expect("opds v2 search restricted request should build"),
            )
            .await
            .expect("opds v2 search restricted request should complete");

        assert_eq!(restricted_response.status(), StatusCode::OK);
        let restricted_payload = response_json(restricted_response).await;
        let restricted_groups = restricted_payload
            .get("groups")
            .and_then(Value::as_array)
            .expect("opds v2 search restricted payload should expose groups array");
        assert!(
            restricted_groups.is_empty(),
            "age-exclude restricted OPDS search should hide restricted groups for {uri}: {restricted_payload}",
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_search_supports_accent_folded_and_cjk_series_queries() {
    let paths = new_router_fixture("router-opds-search-accent-cjk-recall").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-cafe", "Café 東京 Series", "library-1").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let v1_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series?search=Cafe%20東京")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 accent+cjk search request should build"),
        )
        .await
        .expect("opds v1 accent+cjk search request should complete");
    assert_eq!(v1_response.status(), StatusCode::OK);
    let v1_body = response_text(v1_response).await;
    assert!(
        v1_body.contains("/opds/v1.2/series/series-cafe"),
        "OPDS v1 search should retain accent-folded mixed CJK recall: {v1_body}",
    );

    let v2_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/search?query=Cafe%20東京")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 accent+cjk search request should build"),
        )
        .await
        .expect("opds v2 accent+cjk search request should complete");
    assert_eq!(v2_response.status(), StatusCode::OK);
    let v2_payload = response_json(v2_response).await;
    let rendered = v2_payload.to_string();
    assert!(
        rendered.contains("/opds/v2/series/series-cafe"),
        "OPDS v2 search should retain accent-folded mixed CJK recall: {v2_payload}",
    );

    cleanup_router_fixture(paths);
}

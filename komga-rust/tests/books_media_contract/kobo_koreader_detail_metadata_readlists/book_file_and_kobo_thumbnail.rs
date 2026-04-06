use super::*;

async fn seed_kobo_thumbnail_bytes(
    paths: &RuntimeDbPaths,
    thumbnail_id: &str,
    media_type: &str,
    bytes: &[u8],
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for kobo thumbnail seed");
    sqlx::query("UPDATE THUMBNAIL_BOOK SET MEDIA_TYPE = ?, THUMBNAIL = ? WHERE ID = ?")
        .bind(media_type)
        .bind(bytes)
        .bind(thumbnail_id)
        .execute(&pool)
        .await
        .expect("kobo thumbnail row should be updated");
    pool.close().await;
}

async fn seed_kobo_thumbnail_sidecar_url(
    paths: &RuntimeDbPaths,
    thumbnail_id: &str,
    media_type: &str,
    relative_path: &str,
    bytes: &[u8],
) {
    let sidecar_path = paths.config_dir.join(relative_path);
    if let Some(parent) = sidecar_path.parent() {
        std::fs::create_dir_all(parent)
            .expect("kobo thumbnail sidecar parent directory should be created");
    }
    std::fs::write(&sidecar_path, bytes).expect("kobo thumbnail sidecar file should be written");
    let sidecar_url = reqwest::Url::from_file_path(sidecar_path.as_path())
        .expect("kobo thumbnail sidecar path should convert to file url")
        .to_string();

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for kobo thumbnail sidecar seed");
    sqlx::query("UPDATE THUMBNAIL_BOOK SET MEDIA_TYPE = ?, THUMBNAIL = NULL, URL = ? WHERE ID = ?")
        .bind(media_type)
        .bind(sidecar_url)
        .bind(thumbnail_id)
        .execute(&pool)
        .await
        .expect("kobo thumbnail sidecar row should be updated");
    pool.close().await;
}

#[tokio::test]
async fn router_book_file_direct_route_returns_attachment_headers_and_body() {
    let paths = new_router_fixture("router-book-file-direct-route").await;
    seed_router_contract_data(&paths).await;
    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for direct file route test");
    let expected_body = b"router-book-file-direct-content";
    std::fs::write(books_dir.join("book-1.epub"), expected_body)
        .expect("book fixture file should be written for direct file route test");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/file")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("direct book file request should build"),
        )
        .await
        .expect("direct book file request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/epub+zip")
    );
    let content_disposition = response
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .expect("direct book file response should expose content disposition");
    assert!(content_disposition.contains("attachment"));
    assert!(content_disposition.contains("book-1.epub"));
    let content_length = response
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .expect("direct book file response should expose content length");
    assert_eq!(content_length, expected_body.len().to_string());
    assert!(
        !response.headers().contains_key(header::ACCEPT_RANGES),
        "full book download should not advertise range headers on 200 responses"
    );
    assert!(
        !response.headers().contains_key(header::CONTENT_RANGE),
        "full book download should not expose content-range on 200 responses"
    );

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("direct book file response body should be readable");
    assert_eq!(body.as_ref(), expected_body);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_file_direct_route_ignores_range_and_returns_full_body() {
    let paths = new_router_fixture("router-book-file-direct-route-ignores-range").await;
    seed_router_contract_data(&paths).await;
    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for range-ignoring file route test");
    let expected_body = b"router-book-file-range-ignored";
    std::fs::write(books_dir.join("book-1.epub"), expected_body)
        .expect("book fixture file should be written for range-ignoring file route test");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/file")
                .header("x-auth-token", &auth_token)
                .header(header::RANGE, "bytes=0-3")
                .body(Body::empty())
                .expect("range direct book file request should build"),
        )
        .await
        .expect("range direct book file request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        !response.headers().contains_key(header::ACCEPT_RANGES),
        "range request should not expose accept-ranges on Kotlin-aligned /file responses"
    );
    assert!(
        !response.headers().contains_key(header::CONTENT_RANGE),
        "range request should not expose content-range on Kotlin-aligned /file responses"
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("range direct book file response body should be readable");
    assert_eq!(body.as_ref(), expected_body);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_file_direct_route_returns_not_found_with_message_when_file_is_missing() {
    let paths = new_router_fixture("router-book-file-direct-route-missing-file").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/file")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing direct book file request should build"),
        )
        .await
        .expect("missing direct book file request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "File not found, it may have moved".to_string()
        ))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_file_direct_route_uses_persisted_media_type_for_comic_archives() {
    let paths = new_router_fixture("router-book-file-direct-route-comic-archive-type").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for direct comic archive file route fixture");
    sqlx::query("UPDATE BOOK SET NAME = ?, URL = ? WHERE ID = ?")
        .bind("book-1.cbz")
        .bind("books/book-1.cbz")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book file metadata should update for direct comic archive file route fixture");
    sqlx::query("UPDATE MEDIA SET MEDIA_TYPE = ? WHERE BOOK_ID = ?")
        .bind("application/zip")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("media type should update for direct comic archive file route fixture");
    pool.close().await;

    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for direct comic archive file route test");
    let expected_body = b"router-book-file-comic-archive-content";
    std::fs::write(books_dir.join("book-1.cbz"), expected_body)
        .expect("comic archive fixture file should be written for direct file route test");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/file")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("direct comic archive book file request should build"),
        )
        .await
        .expect("direct comic archive book file request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/zip")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("direct comic archive book file response body should be readable");
    assert_eq!(body.as_ref(), expected_body);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_file_direct_route_percent_encodes_unicode_attachment_name() {
    let paths = new_router_fixture("router-book-file-direct-route-unicode-name").await;
    seed_router_contract_data(&paths).await;
    let unicode_file_name = "アキラ.epub";

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for unicode direct file route fixture");
    sqlx::query("UPDATE BOOK SET NAME = ?, URL = ? WHERE ID = ?")
        .bind(unicode_file_name)
        .bind(format!("books/{unicode_file_name}"))
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book file name should update for unicode direct file route fixture");
    pool.close().await;

    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for unicode direct file route test");
    std::fs::write(books_dir.join(unicode_file_name), b"unicode-book-file")
        .expect("unicode book fixture file should be written");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/file")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("unicode direct book file request should build"),
        )
        .await
        .expect("unicode direct book file request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let content_disposition = response
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .expect("unicode direct book file response should expose content disposition");
    assert!(
        content_disposition.contains("filename*=UTF-8''%E3%82%A2%E3%82%AD%E3%83%A9.epub"),
        "unicode attachment header should percent-encode filename*: {content_disposition}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_file_wildcard_routes_match_api_v1_and_opds_v2() {
    let paths = new_router_fixture("router-book-file-wildcard-routes").await;
    seed_router_contract_data(&paths).await;
    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for file route test");
    let expected_body = b"router-book-file-content";
    std::fs::write(books_dir.join("book-1.epub"), expected_body)
        .expect("book fixture file should be written");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for route in [
        "/api/v1/books/book-1/file/book-1.epub",
        "/opds/v2/books/book-1/file/book-1.epub",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("book file wildcard request should build"),
            )
            .await
            .expect("book file wildcard request should complete");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("book file wildcard response body should be readable");
        assert_eq!(body.as_ref(), expected_body);
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_file_wildcard_returns_not_found_with_message_when_file_is_missing() {
    let paths = new_router_fixture("router-book-file-wildcard-missing-file").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/file/book-1.epub")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing wildcard book file request should build"),
        )
        .await
        .expect("missing wildcard book file request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "File not found, it may have moved".to_string()
        ))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_file_wildcard_returns_forbidden_for_restricted_user_even_when_file_is_missing()
{
    let paths = new_router_fixture("router-book-file-wildcard-restricted-missing-file").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "member-user",
        "member@example.org",
        "router-contract-member-123",
        &[],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let member_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "member@example.org",
        "router-contract-member-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/file/book-1.epub")
                .header("x-auth-token", &member_token)
                .body(Body::empty())
                .expect("restricted missing wildcard book file request should build"),
        )
        .await
        .expect("restricted missing wildcard book file request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_thumbnail_exact_id_local_response_is_jpeg() {
    let paths = new_router_fixture("router-kobo-thumbnail-local-jpeg").await;
    seed_router_contract_data(&paths).await;
    seed_kobo_thumbnail_bytes(&paths, "thumb-book-1", "image/png", &fixture_png_bytes()).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/books/thumb-book-1/thumbnail/800/800/false/image.jpg")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo thumbnail local request should build"),
        )
        .await
        .expect("kobo thumbnail local request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("kobo thumbnail local response body should be readable");
    assert_eq!(
        image::guess_format(body.as_ref()).expect("kobo thumbnail local body should decode"),
        image::ImageFormat::Jpeg
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_thumbnail_redirects_to_kobo_cdn_when_exact_thumbnail_is_missing_and_proxy_enabled()
 {
    let paths = new_router_fixture("router-kobo-thumbnail-redirects-to-cdn").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/books/book-1/thumbnail/800/800/90/true/image.jpg")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo thumbnail redirect request should build"),
        )
        .await
        .expect("kobo thumbnail redirect request should complete");

    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("https://cdn.kobo.com/book-images/book-1/800/800/false/image.jpg")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_thumbnail_returns_not_found_when_exact_thumbnail_is_missing_and_proxy_disabled()
 {
    let paths = new_router_fixture("router-kobo-thumbnail-missing-local").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/books/book-1/thumbnail/800/800/false/image.jpg")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo thumbnail missing local request should build"),
        )
        .await
        .expect("kobo thumbnail missing local request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_thumbnail_exact_id_sidecar_stays_local_when_proxy_enabled() {
    let paths = new_router_fixture("router-kobo-thumbnail-sidecar-local").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;
    seed_kobo_thumbnail_sidecar_url(
        &paths,
        "thumb-book-1",
        "image/png",
        "covers/thumb-book-1.png",
        &fixture_png_bytes(),
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/books/thumb-book-1/thumbnail/800/800/90/true/image.jpg")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo thumbnail sidecar request should build"),
        )
        .await
        .expect("kobo thumbnail sidecar request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    assert!(
        !response.headers().contains_key(header::LOCATION),
        "exact thumbnail id should stay local even when Kobo proxy is enabled"
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("kobo thumbnail sidecar response body should be readable");
    assert_eq!(
        image::guess_format(body.as_ref()).expect("kobo thumbnail sidecar body should decode"),
        image::ImageFormat::Jpeg
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_book_file_epub_allows_path_token_user_with_file_download_role() {
    let paths = new_router_fixture("router-kobo-book-file-path-token-success").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-file-user",
        "kobo-file-user@example.org",
        "router-contract-kobo-file-123",
        18,
        &["USER", "KOBO_SYNC", "FILE_DOWNLOAD"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobooktoken", "kobo-file-user").await;
    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for kobo file path token test");
    let expected_body = b"router-kobo-file-content";
    std::fs::write(books_dir.join("book-1.epub"), expected_body)
        .expect("kobo file path token fixture should be written");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/validkobooktoken/v1/books/book-1/file/epub")
                .body(Body::empty())
                .expect("kobo file path token request should build"),
        )
        .await
        .expect("kobo file path token request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/epub+zip")
    );
    let content_disposition = response
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .expect("kobo file path token response should include content disposition");
    assert!(content_disposition.starts_with("attachment;"));
    assert!(content_disposition.contains("book-1.epub"));
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("kobo file path token body should be readable");
    assert_eq!(body.as_ref(), expected_body);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_book_file_epub_forbids_path_token_user_without_file_download_role() {
    let paths = new_router_fixture("router-kobo-book-file-path-token-forbidden").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-file-no-download-user",
        "kobo-file-no-download@example.org",
        "router-contract-kobo-file-no-download-123",
        18,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "nodownloadtoken", "kobo-file-no-download-user").await;
    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for forbidden kobo file test");
    std::fs::write(books_dir.join("book-1.epub"), b"router-kobo-file-content")
        .expect("forbidden kobo file fixture should be written");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/nodownloadtoken/v1/books/book-1/file/epub")
                .body(Body::empty())
                .expect("forbidden kobo file request should build"),
        )
        .await
        .expect("forbidden kobo file request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_book_file_epub_returns_forbidden_for_restricted_user() {
    let paths = new_router_fixture("router-kobo-book-file-restricted-user").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "restricted-kobo-file-user",
        "restricted-kobo-file@example.org",
        "router-contract-restricted-kobo-file-123",
        16,
        &["USER", "FILE_DOWNLOAD"],
    )
    .await;
    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for restricted kobo file test");
    std::fs::write(books_dir.join("book-1.epub"), b"router-kobo-file-content")
        .expect("restricted kobo file fixture should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("restricted kobo file db should open");
    sqlx::query("UPDATE SERIES_METADATA SET AGE_RATING = ? WHERE SERIES_ID = ?")
        .bind(18_i64)
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series age rating should be updated for restricted kobo file test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted-kobo-file@example.org",
        "router-contract-restricted-kobo-file-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/books/book-1/file/epub")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("restricted kobo file request should build"),
        )
        .await
        .expect("restricted kobo file request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_book_file_epub_returns_not_found_with_message_when_file_is_missing() {
    let paths = new_router_fixture("router-kobo-book-file-missing-file").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/books/book-1/file/epub")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing kobo file request should build"),
        )
        .await
        .expect("missing kobo file request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "File not found, it may have moved".to_string()
        ))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_book_file_epub_convert_kepub_uses_kepub_attachment_name() {
    let paths = new_router_fixture("router-kobo-book-file-convert-kepub").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_with_cover(&paths, "books/book-1.epub");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/books/book-1/file/epub?convert_kepub=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("convert kepub kobo file request should build"),
        )
        .await
        .expect("convert kepub kobo file request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/epub+zip")
    );
    let content_disposition = response
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .expect("convert kepub response should include content disposition");
    assert!(content_disposition.contains("book-1.kepub.epub"));
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("convert kepub response body should be readable");
    assert!(!body.is_empty());
    assert_eq!(&body.as_ref()[..2], b"PK");

    cleanup_router_fixture(paths);
}

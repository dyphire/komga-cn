use super::*;

#[tokio::test]
async fn router_readlist_and_collection_media_assets_hide_age_restricted_content() {
    let paths = new_router_fixture("router-readlist-collection-media-assets-restricted").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
        &["USER", "FILE_DOWNLOAD"],
    )
    .await;
    write_router_epub_resource(
        &paths,
        "books/book-1.epub",
        "OEBPS/chapter.xhtml",
        br#"<html xmlns='http://www.w3.org/1999/xhtml'><body>Restricted</body></html>"#,
    );

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let admin_token = login_with_basic_and_get_token(app.clone()).await;
    let restricted_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;
    let image_bytes = fixture_png_bytes();

    let (readlist_content_type, readlist_body) =
        multipart_image_upload_body("file", "readlist.png", "image/png", true, &image_bytes);
    let readlist_upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/readlists/readlist-1/thumbnails")
                .header("x-auth-token", &admin_token)
                .header(header::CONTENT_TYPE, readlist_content_type)
                .body(Body::from(readlist_body))
                .expect("restricted readlist thumbnail upload request should build"),
        )
        .await
        .expect("restricted readlist thumbnail upload request should complete");
    assert_eq!(readlist_upload.status(), StatusCode::OK);
    let readlist_thumbnail_id = response_json(readlist_upload)
        .await
        .get("id")
        .and_then(Value::as_str)
        .expect("restricted readlist upload should return thumbnail id")
        .to_string();

    let (collection_content_type, collection_body) =
        multipart_image_upload_body("file", "collection.png", "image/png", true, &image_bytes);
    let collection_upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections/collection-1/thumbnails")
                .header("x-auth-token", &admin_token)
                .header(header::CONTENT_TYPE, collection_content_type)
                .body(Body::from(collection_body))
                .expect("restricted collection thumbnail upload request should build"),
        )
        .await
        .expect("restricted collection thumbnail upload request should complete");
    assert_eq!(collection_upload.status(), StatusCode::OK);
    let collection_thumbnail_id = response_json(collection_upload)
        .await
        .get("id")
        .and_then(Value::as_str)
        .expect("restricted collection upload should return thumbnail id")
        .to_string();

    for route in [
        "/api/v1/readlists/readlist-1/thumbnails",
        "/api/v1/readlists/readlist-1/file",
        "/api/v1/collections/collection-1/thumbnails",
        "/api/v1/collections/collection-1/thumbnail",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &restricted_token)
                    .body(Body::empty())
                    .expect("restricted readlist/collection get request should build"),
            )
            .await
            .expect("restricted readlist/collection get request should complete");

        assert_eq!(response.status(), StatusCode::NOT_FOUND, "route: {route}");
    }

    for route in [
        format!("/api/v1/readlists/readlist-1/thumbnails/{readlist_thumbnail_id}"),
        format!("/api/v1/collections/collection-1/thumbnails/{collection_thumbnail_id}"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route.as_str())
                    .header("x-auth-token", &restricted_token)
                    .body(Body::empty())
                    .expect("restricted readlist/collection by-id request should build"),
            )
            .await
            .expect("restricted readlist/collection by-id request should complete");

        assert_eq!(response.status(), StatusCode::NOT_FOUND, "route: {}", route);
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_tachiyomi_progress_ignores_content_restrictions_like_kotlin() {
    let paths = new_router_fixture("router-readlist-tachiyomi-content-restrictions").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
        &["USER", "FILE_DOWNLOAD"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let restricted_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/read-progress/tachiyomi")
                .header("x-auth-token", &restricted_token)
                .body(Body::empty())
                .expect("restricted readlist tachiyomi get request should build"),
        )
        .await
        .expect("restricted readlist tachiyomi get request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "booksCount": 1,
            "booksReadCount": 0,
            "booksUnreadCount": 1,
            "booksInProgressCount": 0,
            "lastReadContinuousIndex": 0,
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_detail_filters_books_for_partially_restricted_user() {
    let paths = new_router_fixture("router-readlist-detail-partially-restricted").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "partially-restricted-user",
        "partial@example.org",
        "router-contract-partial-123",
        15,
        &["USER", "FILE_DOWNLOAD"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let restricted_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "partial@example.org",
        "router-contract-partial-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1")
                .header("x-auth-token", &restricted_token)
                .body(Body::empty())
                .expect("partially restricted readlist detail request should build"),
        )
        .await
        .expect("partially restricted readlist detail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload.get("filtered"), Some(&Value::Bool(true)));
    assert_eq!(payload.get("bookIds"), Some(&json!(["book-3"])));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_tachiyomi_progress_ignores_content_restriction_subsets_like_kotlin() {
    let paths = new_router_fixture("router-readlist-tachiyomi-partially-restricted").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "partially-restricted-user",
        "partial@example.org",
        "router-contract-partial-123",
        15,
        &["USER", "FILE_DOWNLOAD"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let restricted_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "partial@example.org",
        "router-contract-partial-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/read-progress/tachiyomi")
                .header("x-auth-token", &restricted_token)
                .body(Body::empty())
                .expect("partially restricted readlist tachiyomi get request should build"),
        )
        .await
        .expect("partially restricted readlist tachiyomi get request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "booksCount": 3,
            "booksReadCount": 0,
            "booksUnreadCount": 3,
            "booksInProgressCount": 0,
            "lastReadContinuousIndex": 0,
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_tachiyomi_progress_counts_full_readlist_for_library_restricted_user() {
    let paths = new_router_fixture("router-readlist-tachiyomi-library-restricted").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-1-user",
        "library1@example.org",
        "router-contract-library1-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let restricted_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library1@example.org",
        "router-contract-library1-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/read-progress/tachiyomi")
                .header("x-auth-token", &restricted_token)
                .body(Body::empty())
                .expect("library-restricted readlist tachiyomi get request should build"),
        )
        .await
        .expect("library-restricted readlist tachiyomi get request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "booksCount": 3,
            "booksReadCount": 0,
            "booksUnreadCount": 3,
            "booksInProgressCount": 0,
            "lastReadContinuousIndex": 0,
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_tachiyomi_progress_returns_not_found_when_library_sharing_has_no_matches()
{
    let paths = new_router_fixture("router-readlist-tachiyomi-no-shared-libraries").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "no-library-user",
        "nolib@example.org",
        "router-contract-nolib-123",
        &[],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let restricted_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "nolib@example.org",
        "router-contract-nolib-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/read-progress/tachiyomi")
                .header("x-auth-token", &restricted_token)
                .body(Body::empty())
                .expect("hidden-library readlist tachiyomi get request should build"),
        )
        .await
        .expect("hidden-library readlist tachiyomi get request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_tachiyomi_progress_put_returns_not_found_when_library_sharing_has_no_matches()
 {
    let paths = new_router_fixture("router-readlist-tachiyomi-put-no-shared-libraries").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "no-library-user",
        "nolib@example.org",
        "router-contract-nolib-123",
        &[],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let restricted_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "nolib@example.org",
        "router-contract-nolib-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/readlists/readlist-1/read-progress/tachiyomi")
                .header("x-auth-token", &restricted_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "lastBookRead": 1 }).to_string()))
                .expect("hidden-library readlist tachiyomi put request should build"),
        )
        .await
        .expect("hidden-library readlist tachiyomi put request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for no-shared-libraries verification");
    let writes = sqlx::query("SELECT BOOK_ID FROM READ_PROGRESS WHERE USER_ID = ?")
        .bind("no-library-user")
        .fetch_all(&pool)
        .await
        .expect("no-shared-libraries read progress rows should be queryable");
    pool.close().await;
    assert!(writes.is_empty());

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_tachiyomi_progress_put_ignores_fully_hidden_content_like_kotlin() {
    let paths = new_router_fixture("router-readlist-tachiyomi-put-content-restrictions").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
        &["USER", "FILE_DOWNLOAD"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let restricted_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/readlists/readlist-1/read-progress/tachiyomi")
                .header("x-auth-token", &restricted_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "lastBookRead": 1 }).to_string()))
                .expect("content-restricted readlist tachiyomi put request should build"),
        )
        .await
        .expect("content-restricted readlist tachiyomi put request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for content-restricted verification");
    let writes = sqlx::query("SELECT BOOK_ID FROM READ_PROGRESS WHERE USER_ID = ?")
        .bind("restricted-user")
        .fetch_all(&pool)
        .await
        .expect("content-restricted read progress rows should be queryable");
    pool.close().await;
    assert!(writes.is_empty());

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_tachiyomi_progress_put_marks_only_visible_books_for_restricted_user() {
    let paths = new_router_fixture("router-readlist-tachiyomi-put-partially-restricted").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "partially-restricted-user",
        "partial@example.org",
        "router-contract-partial-123",
        15,
        &["USER", "FILE_DOWNLOAD"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let restricted_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "partial@example.org",
        "router-contract-partial-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/readlists/readlist-1/read-progress/tachiyomi")
                .header("x-auth-token", &restricted_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "lastBookRead": 1 }).to_string()))
                .expect("partially restricted readlist tachiyomi put request should build"),
        )
        .await
        .expect("partially restricted readlist tachiyomi put request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for partially restricted put verification");
    let rows = sqlx::query(
        "SELECT BOOK_ID, PAGE, COMPLETED FROM READ_PROGRESS WHERE USER_ID = ? ORDER BY BOOK_ID ASC",
    )
    .bind("partially-restricted-user")
    .fetch_all(&pool)
    .await
    .expect("partially restricted read progress rows should be queryable");
    pool.close().await;

    let persisted = rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("BOOK_ID"),
                row.get::<i64, _>("PAGE"),
                row.get::<i64, _>("COMPLETED"),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(persisted, vec![("book-3".to_string(), 12, 1)]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_media_assets_allow_partially_visible_restricted_readlist() {
    let paths = new_router_fixture("router-readlist-media-assets-partially-restricted").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "partially-restricted-user",
        "partial@example.org",
        "router-contract-partial-123",
        15,
        &["USER", "FILE_DOWNLOAD"],
    )
    .await;
    for (relative_path, chapter) in [
        ("books/book-1.epub", "book-1"),
        ("books/book-2.epub", "book-2"),
        ("library-2/books/book-3.epub", "book-3"),
    ] {
        write_router_epub_resource(
            &paths,
            relative_path,
            "OEBPS/chapter.xhtml",
            format!("<html xmlns='http://www.w3.org/1999/xhtml'><body>{chapter}</body></html>")
                .as_bytes(),
        );
    }

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let admin_token = login_with_basic_and_get_token(app.clone()).await;
    let restricted_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "partial@example.org",
        "router-contract-partial-123",
    )
    .await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "readlist.png", "image/png", true, &image_bytes);

    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/readlists/readlist-1/thumbnails")
                .header("x-auth-token", &admin_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("partially restricted readlist thumbnail upload request should build"),
        )
        .await
        .expect("partially restricted readlist thumbnail upload request should complete");
    assert_eq!(upload.status(), StatusCode::OK);

    let thumbnails = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/thumbnails")
                .header("x-auth-token", &restricted_token)
                .body(Body::empty())
                .expect("partially restricted readlist thumbnails request should build"),
        )
        .await
        .expect("partially restricted readlist thumbnails request should complete");

    assert_eq!(thumbnails.status(), StatusCode::OK);
    let thumbnails_payload = response_json(thumbnails).await;
    assert_eq!(
        thumbnails_payload.as_array().map(Vec::len),
        Some(1),
        "partially visible readlist should still expose its thumbnail list"
    );

    let archive = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/file")
                .header("x-auth-token", &restricted_token)
                .body(Body::empty())
                .expect("partially restricted readlist file request should build"),
        )
        .await
        .expect("partially restricted readlist file request should complete");

    assert_eq!(archive.status(), StatusCode::OK);
    let body = to_bytes(archive.into_body(), usize::MAX)
        .await
        .expect("partially restricted readlist archive body should be readable");
    let cursor = std::io::Cursor::new(body.to_vec());
    let mut zip = zip::ZipArchive::new(cursor)
        .expect("partially restricted readlist archive should parse as zip");
    let names = (0..zip.len())
        .map(|index| {
            zip.by_index(index)
                .expect("zip entry should open")
                .name()
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "1 - book-1.epub".to_string(),
            "2 - book-2.epub".to_string(),
            "3 - book-3.epub".to_string(),
        ]
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_file_uses_deflated_zip_entries() {
    let paths = new_router_fixture("router-readlist-file-deflated-zip").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;
    for (relative_path, chapter) in [
        ("books/book-1.epub", "book-1"),
        ("books/book-2.epub", "book-2"),
        ("library-2/books/book-3.epub", "book-3"),
    ] {
        write_router_epub_resource(
            &paths,
            relative_path,
            "OEBPS/chapter.xhtml",
            format!("<html xmlns='http://www.w3.org/1999/xhtml'><body>{chapter}</body></html>")
                .as_bytes(),
        );
    }

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let archive = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/file")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist file request should build"),
        )
        .await
        .expect("readlist file request should complete");

    assert_eq!(archive.status(), StatusCode::OK);
    let body = to_bytes(archive.into_body(), usize::MAX)
        .await
        .expect("readlist archive body should be readable");
    let cursor = std::io::Cursor::new(body.to_vec());
    let mut zip = zip::ZipArchive::new(cursor).expect("readlist archive should parse as zip");
    let entry = zip.by_index(0).expect("zip entry should open");
    assert_eq!(entry.compression(), CompressionMethod::Deflated);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_file_emits_zip64_records() {
    let paths = new_router_fixture("router-readlist-file-zip64-records").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;
    for (relative_path, chapter) in [
        ("books/book-1.epub", "book-1"),
        ("books/book-2.epub", "book-2"),
        ("library-2/books/book-3.epub", "book-3"),
    ] {
        write_router_epub_resource(
            &paths,
            relative_path,
            "OEBPS/chapter.xhtml",
            format!("<html xmlns='http://www.w3.org/1999/xhtml'><body>{chapter}</body></html>")
                .as_bytes(),
        );
    }

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let archive = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/file")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist file request should build"),
        )
        .await
        .expect("readlist file request should complete");

    assert_eq!(archive.status(), StatusCode::OK);
    let body = to_bytes(archive.into_body(), usize::MAX)
        .await
        .expect("readlist archive body should be readable");
    assert!(
        body.windows(4)
            .any(|window| window == [0x50, 0x4b, 0x06, 0x06]),
        "readlist file should include zip64 EOCD signature"
    );
    assert!(
        body.windows(4)
            .any(|window| window == [0x50, 0x4b, 0x06, 0x07]),
        "readlist file should include zip64 locator signature"
    );

    cleanup_router_fixture(paths);
}

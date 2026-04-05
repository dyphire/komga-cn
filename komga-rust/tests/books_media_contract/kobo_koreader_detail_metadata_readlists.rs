use super::*;

#[tokio::test]
async fn router_kobo_state_update_roundtrip_persists_progress() {
    let paths = new_router_fixture("router-kobo-state-roundtrip").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/kobo/any-token/v1/library/book-1/state")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "ReadingStates": [{
                            "EntitlementId": "book-1",
                            "LastModified": "2026-03-27T10:00:00Z",
                            "Statistics": {
                                "LastModified": "2026-03-27T10:00:00Z"
                            },
                            "StatusInfo": {
                                "LastModified": "2026-03-27T10:00:00Z",
                                "Status": "Reading"
                            },
                            "CurrentBookmark": {
                                "LastModified": "2026-03-27T10:00:00Z",
                                "ProgressPercent": 47.0,
                                "ContentSourceProgressPercent": 23.0,
                                "Location": {
                                    "Source": "/book-1/manifest#position=5",
                                    "Value": "kobo.5.1"
                                }
                            }
                        }]
                    })
                    .to_string(),
                ))
                .expect("kobo state update request should build"),
        )
        .await
        .expect("kobo state update request should complete");
    assert_eq!(put_response.status(), StatusCode::OK);

    let get_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/book-1/state")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo state get request should build"),
        )
        .await
        .expect("kobo state get request should complete");
    assert_eq!(get_response.status(), StatusCode::OK);

    let payload = response_json(get_response).await;
    let state = payload
        .as_array()
        .and_then(|values| values.first())
        .expect("kobo state response should contain one reading state object");
    assert_eq!(
        state
            .get("StatusInfo")
            .and_then(|value| value.get("Status")),
        Some(&Value::String("Reading".to_string())),
    );
    assert_eq!(
        state
            .get("CurrentBookmark")
            .and_then(|value| value.get("ProgressPercent")),
        Some(&json!(47.0)),
    );
    assert_eq!(
        state
            .get("CurrentBookmark")
            .and_then(|value| value.get("ContentSourceProgressPercent")),
        Some(&json!(23.0)),
    );
    assert_eq!(
        state
            .get("CurrentBookmark")
            .and_then(|value| value.get("Location"))
            .and_then(|value| value.get("Source")),
        Some(&Value::String("/book-1/manifest#position=5".to_string())),
    );
    assert_eq!(
        state
            .get("CurrentBookmark")
            .and_then(|value| value.get("Location"))
            .and_then(|value| value.get("Value")),
        Some(&Value::String("kobo.5.1".to_string())),
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
async fn router_koreader_progress_put_then_get_roundtrip() {
    let paths = new_router_fixture("router-koreader-progress-roundtrip").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for koreader roundtrip epub seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for koreader roundtrip test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/koreader/syncs/progress")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "document": "hash-book-1",
                        "percentage": 0.33,
                        "progress": "/body/DocFragment[2]/body/div/p[1]/text().0",
                        "device": "KOReader",
                        "device_id": "reader-1"
                    })
                    .to_string(),
                ))
                .expect("koreader progress put request should build"),
        )
        .await
        .expect("koreader progress put request should complete");
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);

    let get_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/koreader/syncs/progress/hash-book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("koreader progress get request should build"),
        )
        .await
        .expect("koreader progress get request should complete");
    assert_eq!(get_response.status(), StatusCode::OK);

    let payload = response_json(get_response).await;
    assert_eq!(
        payload.get("document"),
        Some(&Value::String("hash-book-1".to_string())),
    );
    assert_eq!(
        payload.get("progress"),
        Some(&Value::String("/body/DocFragment[2].0".to_string()))
    );
    assert_eq!(payload.get("percentage"), Some(&json!(0.33)));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_koreader_progress_put_rejects_invalid_epub_progress_string() {
    let paths = new_router_fixture("router-koreader-progress-invalid-epub-progress").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for koreader invalid epub seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for koreader invalid epub test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/koreader/syncs/progress")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "document": "hash-book-1",
                        "percentage": 0.33,
                        "progress": "7",
                        "device": "KOReader",
                        "device_id": "reader-1"
                    })
                    .to_string(),
                ))
                .expect("koreader invalid epub progress put request should build"),
        )
        .await
        .expect("koreader invalid epub progress put request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_koreader_progress_put_rejects_invalid_non_epub_progress_string() {
    let paths = new_router_fixture("router-koreader-progress-invalid-pdf-progress").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "book-pdf-1.pdf",
        "PDF Book 1",
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for koreader invalid pdf seed");
    sqlx::query("UPDATE BOOK SET FILE_HASH_KOREADER = ? WHERE ID = ?")
        .bind("hash-book-pdf-1")
        .bind("book-pdf-1")
        .execute(&pool)
        .await
        .expect("pdf book koreader hash should be set");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/koreader/syncs/progress")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "document": "hash-book-pdf-1",
                        "percentage": 0.33,
                        "progress": "chapter_3",
                        "device": "KOReader",
                        "device_id": "reader-1"
                    })
                    .to_string(),
                ))
                .expect("koreader invalid pdf progress put request should build"),
        )
        .await
        .expect("koreader invalid pdf progress put request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_koreader_progress_put_rejects_out_of_range_non_epub_progress() {
    let paths = new_router_fixture("router-koreader-progress-out-of-range-pdf-progress").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-2",
        "series-1",
        "book-pdf-2.pdf",
        "PDF Book 2",
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for koreader out-of-range pdf seed");
    sqlx::query("UPDATE BOOK SET FILE_HASH_KOREADER = ? WHERE ID = ?")
        .bind("hash-book-pdf-2")
        .bind("book-pdf-2")
        .execute(&pool)
        .await
        .expect("pdf book koreader hash should be set for out-of-range test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/koreader/syncs/progress")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "document": "hash-book-pdf-2",
                        "percentage": 0.33,
                        "progress": "42",
                        "device": "KOReader",
                        "device_id": "reader-1"
                    })
                    .to_string(),
                ))
                .expect("koreader out-of-range pdf progress put request should build"),
        )
        .await
        .expect("koreader out-of-range pdf progress put request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_koreader_progress_get_preserves_empty_device_fields() {
    let paths = new_router_fixture("router-koreader-progress-empty-device").await;
    seed_router_contract_data(&paths).await;
    seed_router_read_progress(&paths, false).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/koreader/syncs/progress/hash-book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("koreader progress get request should build"),
        )
        .await
        .expect("koreader progress get request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(payload.get("device"), Some(&Value::String(String::new())));
    assert_eq!(
        payload.get("device_id"),
        Some(&Value::String(String::new()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_book_detail_includes_persisted_authors_tags_and_read_progress() {
    let paths = new_router_fixture("router-discovery-book-detail-persisted-metadata").await;
    seed_router_contract_data(&paths).await;
    seed_router_read_progress(&paths, true).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book detail request should build"),
        )
        .await
        .expect("book detail request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("authors"))
            .and_then(Value::as_array)
            .and_then(|authors| authors.first())
            .and_then(|author| author.get("name")),
        Some(&Value::String("Jane Writer".to_string())),
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("authors"))
            .and_then(Value::as_array)
            .and_then(|authors| authors.first())
            .and_then(|author| author.get("role")),
        Some(&Value::String("writer".to_string())),
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("tags"))
            .and_then(Value::as_array)
            .and_then(|tags| tags.first()),
        Some(&Value::String("favorite-tag".to_string())),
    );
    assert_eq!(
        payload
            .get("readProgress")
            .and_then(|progress| progress.get("page")),
        Some(&json!(10)),
    );
    assert_eq!(
        payload
            .get("readProgress")
            .and_then(|progress| progress.get("completed")),
        Some(&Value::Bool(true)),
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_book_detail_preserves_empty_read_progress_device_fields() {
    let paths = new_router_fixture("router-discovery-book-detail-empty-read-progress-device").await;
    seed_router_contract_data(&paths).await;
    seed_router_read_progress(&paths, false).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book detail device parity db should open");
    sqlx::query(
        "UPDATE READ_PROGRESS SET DEVICE_ID = '', DEVICE_NAME = '' WHERE BOOK_ID = ? AND USER_ID = ?",
    )
    .bind("book-1")
    .bind("admin-user")
    .execute(&pool)
    .await
    .expect("read progress device fields should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book detail request should build"),
        )
        .await
        .expect("book detail request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("readProgress")
            .and_then(|progress| progress.get("deviceId")),
        Some(&Value::String(String::new()))
    );
    assert_eq!(
        payload
            .get("readProgress")
            .and_then(|progress| progress.get("deviceName")),
        Some(&Value::String(String::new()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_book_detail_converts_admin_url_to_file_path() {
    let paths = new_router_fixture("router-discovery-book-detail-admin-url-path").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book detail url parity db should open");
    sqlx::query("UPDATE BOOK SET URL = ? WHERE ID = ?")
        .bind("file:/library%20root/books/book%201.cbz")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book url should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book detail request should build"),
        )
        .await
        .expect("book detail request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(
        payload.get("url"),
        Some(&Value::String("/library root/books/book 1.cbz".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_book_detail_formats_file_last_modified_as_utc_timestamp() {
    let paths = new_router_fixture("router-discovery-book-detail-file-last-modified-utc").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book detail request should build"),
        )
        .await
        .expect("book detail request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(
        payload.get("fileLastModified"),
        Some(&Value::String("1970-01-01T00:00:00Z".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_book_detail_does_not_bridge_missing_book_n_ids() {
    let paths = new_router_fixture("router-discovery-book-detail-no-bridge-id").await;
    seed_router_contract_data(&paths).await;
    seed_router_cbz_book(&paths, "book-z-2", "book-z-2.cbz", "Second Real Book").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-2")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book detail bridge-id request should build"),
        )
        .await
        .expect("book detail bridge-id request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_metadata_batch_update_persists_title_and_updates_book_snapshot() {
    let paths =
        new_router_fixture("router-book-metadata-batch-update-persists-and-touches-book").await;
    seed_router_contract_data(&paths).await;

    let pool_before = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open before metadata batch update");
    let last_modified_before = sqlx::query(
        "SELECT COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED FROM BOOK WHERE ID = ? LIMIT 1",
    )
    .bind("book-1")
    .fetch_one(&pool_before)
    .await
    .expect("book last modified should be queryable before metadata batch update")
    .get::<String, _>("LAST_MODIFIED");
    pool_before.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let patch = json!({
        "book-1": {
            "title": "Updated Batch Title"
        }
    });

    let update = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/metadata")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(patch.to_string()))
                .expect("book metadata batch update request should build"),
        )
        .await
        .expect("book metadata batch update request should complete");

    assert_eq!(update.status(), StatusCode::NO_CONTENT);

    let detail = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book detail after metadata batch update request should build"),
        )
        .await
        .expect("book detail after metadata batch update request should complete");

    assert_eq!(detail.status(), StatusCode::OK);
    let payload = response_json(detail).await;
    assert_eq!(
        payload.get("metadata").and_then(|value| value.get("title")),
        Some(&Value::String("Updated Batch Title".to_string()))
    );

    let pool_after = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open after metadata batch update");
    let last_modified_after = sqlx::query(
        "SELECT COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED FROM BOOK WHERE ID = ? LIMIT 1",
    )
    .bind("book-1")
    .fetch_one(&pool_after)
    .await
    .expect("book last modified should be queryable after metadata batch update")
    .get::<String, _>("LAST_MODIFIED");
    pool_after.close().await;
    assert_ne!(last_modified_after, last_modified_before);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_metadata_batch_update_refreshes_book_search_results() {
    let paths = new_router_fixture("router-book-metadata-batch-update-refreshes-search").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let initial_search = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Title",
                            "operator": "is",
                            "value": "Book 1"
                        }
                    })
                    .to_string(),
                ))
                .expect("initial books/list title search request should build"),
        )
        .await
        .expect("initial books/list title search request should complete");
    assert_eq!(initial_search.status(), StatusCode::OK);
    let initial_payload = response_json(initial_search).await;
    let initial_content = initial_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("initial books/list title search should expose content array");
    assert_eq!(initial_content.len(), 1);
    assert_eq!(
        initial_content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    let patch = json!({
        "book-1": {
            "title": "Updated Batch Title"
        }
    });
    let update = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/metadata")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(patch.to_string()))
                .expect("book metadata batch update request should build"),
        )
        .await
        .expect("book metadata batch update request should complete");
    assert_eq!(update.status(), StatusCode::NO_CONTENT);

    let updated_search = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Title",
                            "operator": "is",
                            "value": "Updated Batch Title"
                        }
                    })
                    .to_string(),
                ))
                .expect("updated books/list title search request should build"),
        )
        .await
        .expect("updated books/list title search request should complete");
    assert_eq!(updated_search.status(), StatusCode::OK);
    let updated_payload = response_json(updated_search).await;
    let updated_content = updated_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("updated books/list title search should expose content array");
    assert_eq!(updated_content.len(), 1);
    assert_eq!(
        updated_content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_book_readlists_returns_existing_persisted_readlists() {
    let paths = new_router_fixture("router-discovery-book-readlists-persisted").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/readlists")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book readlists request should build"),
        )
        .await
        .expect("book readlists request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    let content = payload
        .as_array()
        .expect("book readlists payload should be an array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id"),
        Some(&Value::String("readlist-1".to_string())),
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_previous_uses_metadata_number_sort_instead_of_book_number() {
    let paths = new_router_fixture("router-book-previous-number-sort").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book previous number-sort db should open");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-prev-1")
    .bind(0_i64)
    .bind("book-prev-1.cbz")
    .bind("books/book-prev-1.cbz")
    .bind("series-1")
    .bind(4_096_i64)
    .bind(99_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("previous sibling book row should be inserted");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/vnd.comicbook+zip")
        .bind("READY")
        .bind("book-prev-1")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("previous sibling media row should be inserted");
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("99")
    .bind(0.5_f64)
    .bind("Previous by Number Sort")
    .bind("2024-01-01")
    .bind("book-prev-1")
    .execute(&pool)
    .await
    .expect("previous sibling metadata row should be inserted");
    pool.close().await;

    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for previous sibling fixture");
    let file = File::create(books_dir.join("book-prev-1.cbz"))
        .expect("previous sibling cbz fixture should be created");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o644);
    zip.start_file("page-1.png", options)
        .expect("previous sibling page entry should be created");
    zip.write_all(&fixture_png_bytes())
        .expect("previous sibling page payload should be written");
    zip.finish()
        .expect("previous sibling cbz fixture should finish successfully");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/previous")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book previous request should build"),
        )
        .await
        .expect("book previous request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("id"),
        Some(&Value::String("book-prev-1".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_previous_excludes_deleted_books_even_when_they_sort_closer() {
    let paths = new_router_fixture("router-book-previous-excludes-deleted").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book previous deleted db should open");
    sqlx::query("UPDATE BOOK_METADATA SET NUMBER_SORT = ? WHERE BOOK_ID = ?")
        .bind(1.0_f64)
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book-1 number_sort should update");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, DELETED_DATE) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-prev-active")
    .bind(0_i64)
    .bind("book-prev-active.cbz")
    .bind("books/book-prev-active.cbz")
    .bind("series-1")
    .bind(4_096_i64)
    .bind(98_i64)
    .bind("library-1")
    .bind(Option::<String>::None)
    .execute(&pool)
    .await
    .expect("active previous sibling book row should be inserted");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, DELETED_DATE) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-prev-deleted")
    .bind(0_i64)
    .bind("book-prev-deleted.cbz")
    .bind("books/book-prev-deleted.cbz")
    .bind("series-1")
    .bind(4_096_i64)
    .bind(97_i64)
    .bind("library-1")
    .bind("2025-04-01 00:00:00")
    .execute(&pool)
    .await
    .expect("deleted previous sibling book row should be inserted");
    for book_id in ["book-prev-active", "book-prev-deleted"] {
        sqlx::query(
            "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)",
        )
        .bind("application/vnd.comicbook+zip")
        .bind("READY")
        .bind(book_id)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("previous sibling media row should be inserted");
    }
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("98")
    .bind(0.5_f64)
    .bind("Active Previous")
    .bind("2024-01-01")
    .bind("book-prev-active")
    .execute(&pool)
    .await
    .expect("active previous sibling metadata row should be inserted");
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("97")
    .bind(0.75_f64)
    .bind("Deleted Previous")
    .bind("2024-01-01")
    .bind("book-prev-deleted")
    .execute(&pool)
    .await
    .expect("deleted previous sibling metadata row should be inserted");
    pool.close().await;

    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for deleted previous fixture");
    for file_name in ["book-prev-active.cbz", "book-prev-deleted.cbz"] {
        let file = File::create(books_dir.join(file_name))
            .expect("previous sibling cbz fixture should be created");
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o644);
        zip.start_file("page-1.png", options)
            .expect("previous sibling page entry should be created");
        zip.write_all(&fixture_png_bytes())
            .expect("previous sibling page payload should be written");
        zip.finish()
            .expect("previous sibling cbz fixture should finish successfully");
    }

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/previous")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book previous deleted-filter request should build"),
        )
        .await
        .expect("book previous deleted-filter request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("id"),
        Some(&Value::String("book-prev-active".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_previous_breaks_number_sort_ties_by_book_id() {
    let paths = new_router_fixture("router-book-previous-number-sort-tie").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book previous tie db should open");
    sqlx::query("UPDATE BOOK_METADATA SET NUMBER_SORT = ? WHERE BOOK_ID = ?")
        .bind(1.0_f64)
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book-1 number_sort should update for tie test");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-0a")
    .bind(0_i64)
    .bind("book-0a.cbz")
    .bind("books/book-0a.cbz")
    .bind("series-1")
    .bind(4_096_i64)
    .bind(50_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("tie previous sibling book row should be inserted");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/vnd.comicbook+zip")
        .bind("READY")
        .bind("book-0a")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("tie previous sibling media row should be inserted");
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("50")
    .bind(1.0_f64)
    .bind("Previous Tie")
    .bind("2024-01-01")
    .bind("book-0a")
    .execute(&pool)
    .await
    .expect("tie previous sibling metadata row should be inserted");
    pool.close().await;

    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for tie previous fixture");
    let file = File::create(books_dir.join("book-0a.cbz"))
        .expect("tie previous sibling cbz fixture should be created");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o644);
    zip.start_file("page-1.png", options)
        .expect("tie previous sibling page entry should be created");
    zip.write_all(&fixture_png_bytes())
        .expect("tie previous sibling page payload should be written");
    zip.finish()
        .expect("tie previous sibling cbz fixture should finish successfully");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/previous")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book previous tie request should build"),
        )
        .await
        .expect("book previous tie request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("id"),
        Some(&Value::String("book-0a".to_string()))
    );

    cleanup_router_fixture(paths);
}

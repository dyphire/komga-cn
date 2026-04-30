use super::*;

#[tokio::test]
async fn router_koreader_progress_get_returns_forbidden_for_session_user_without_koreader_sync_role()
 {
    let ctx = TestFixture::new("router-koreader-progress-missing-sync-role").await;
    seed_router_age_exclude_user_with_roles(
        ctx.paths(),
        "member-no-koreader-sync",
        "member-no-koreader-sync@example.org",
        "member-no-koreader-sync-123",
        99,
        &["USER", "PAGE_STREAMING"],
    )
    .await;

    let auth_token = ctx
        .login_with_credentials(
            "member-no-koreader-sync@example.org",
            "member-no-koreader-sync-123",
        )
        .await;

    let response = ctx
        .app()
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

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn router_koreader_progress_put_then_get_roundtrip() {
    let ctx = TestFixture::new("router-koreader-progress-roundtrip").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
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

    let auth_token = ctx.login_admin().await;

    let put_response = ctx
        .app()
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
    assert_eq!(put_response.status(), StatusCode::OK);

    let get_response = ctx
        .app()
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
    assert_eq!(payload.get("percentage"), Some(&json!(0.2)));
}

#[tokio::test]
async fn router_koreader_progress_put_persists_kotlin_style_epub_locator() {
    let ctx = TestFixture::new("router-koreader-progress-persists-kotlin-epub-locator").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for koreader locator seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for koreader locator test");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
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
                .expect("koreader locator put request should build"),
        )
        .await
        .expect("koreader locator put request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for koreader locator verification");
    let row = sqlx::query(
        "SELECT PAGE, COMPLETED, LOCATOR FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ? LIMIT 1",
    )
    .bind("book-1")
    .bind("admin-user")
    .fetch_one(&verify_pool)
    .await
    .expect("koreader locator progression row should be queryable");
    let locator_blob = row
        .try_get::<Option<Vec<u8>>, _>("LOCATOR")
        .or_else(|_| row.try_get::<Option<Vec<u8>>, _>("locator"))
        .expect("koreader locator should load locator blob")
        .expect("koreader locator should persist locator blob");
    let locator: Value =
        serde_json::from_slice(&locator_blob).expect("koreader locator should parse as json");
    verify_pool.close().await;

    assert_eq!(row.get::<i64, _>("PAGE"), 2);
    assert!(!row.get::<bool, _>("COMPLETED"));
    assert_eq!(
        locator.pointer("/href"),
        Some(&Value::String("/book-1.xhtml#kobo.2.1".to_string()))
    );
    assert_eq!(locator.pointer("/locations/progression"), Some(&json!(0.0)));
    assert_eq!(
        locator.pointer("/locations/totalProgression"),
        Some(&json!(0.2))
    );
}

#[tokio::test]
async fn router_koreader_progress_put_treats_cbz_as_visual_and_marks_last_page_completed() {
    let ctx = TestFixture::builder("router-koreader-progress-cbz-visual-branch")
        .with_seed(|paths| async move {
            seed_router_primary_series_cbz_book(
                &paths,
                "book-cbz-1",
                "book-cbz-1.cbz",
                "CBZ Book 1",
            )
            .await;
        })
        .build()
        .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for koreader cbz seed");
    sqlx::query("UPDATE BOOK SET FILE_HASH_KOREADER = ? WHERE ID = ?")
        .bind("hash-book-cbz-1")
        .bind("book-cbz-1")
        .execute(&pool)
        .await
        .expect("cbz book koreader hash should be set");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/koreader/syncs/progress")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "document": "hash-book-cbz-1",
                        "percentage": 0.33,
                        "progress": "1",
                        "device": "KOReader",
                        "device_id": "reader-cbz"
                    })
                    .to_string(),
                ))
                .expect("koreader cbz progress put request should build"),
        )
        .await
        .expect("koreader cbz progress put request should complete");

    assert_eq!(response.status(), StatusCode::OK);

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for koreader cbz verification");
    let progression_row = sqlx::query(
        "SELECT PAGE, COMPLETED FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ? LIMIT 1",
    )
    .bind("book-cbz-1")
    .bind("admin-user")
    .fetch_one(&verify_pool)
    .await
    .expect("koreader cbz progression row should be queryable");
    verify_pool.close().await;

    assert_eq!(progression_row.get::<i64, _>("PAGE"), 1);
    assert!(progression_row.get::<bool, _>("COMPLETED"));
}

#[tokio::test]
async fn router_koreader_progress_put_marks_epub_completed_from_matched_total_progression() {
    let ctx = TestFixture::new("router-koreader-progress-epub-completed-threshold").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for koreader epub completion seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob_total_progression_0995())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for koreader completion test");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
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
                        "percentage": 0.1,
                        "progress": "/body/DocFragment[1]/body/div/p[1]/text().0",
                        "device": "KOReader",
                        "device_id": "reader-epub"
                    })
                    .to_string(),
                ))
                .expect("koreader epub completion put request should build"),
        )
        .await
        .expect("koreader epub completion put request should complete");

    assert_eq!(response.status(), StatusCode::OK);

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for koreader epub completion verification");
    let progression_row = sqlx::query(
        "SELECT PAGE, COMPLETED FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ? LIMIT 1",
    )
    .bind("book-1")
    .bind("admin-user")
    .fetch_one(&verify_pool)
    .await
    .expect("koreader epub completion progression row should be queryable");
    verify_pool.close().await;

    assert_eq!(progression_row.get::<i64, _>("PAGE"), 10);
    assert!(progression_row.get::<bool, _>("COMPLETED"));
}

#[tokio::test]
async fn router_koreader_progress_put_keeps_page_zero_when_epub_match_lacks_total_progression() {
    let ctx = TestFixture::new("router-koreader-progress-epub-without-total-progression").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for koreader missing-total seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob_without_total_progression())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for koreader missing-total test");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
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
                        "percentage": 0.9,
                        "progress": "/body/DocFragment[1]/body/div/p[1]/text().0",
                        "device": "KOReader",
                        "device_id": "reader-epub"
                    })
                    .to_string(),
                ))
                .expect("koreader missing-total put request should build"),
        )
        .await
        .expect("koreader missing-total put request should complete");

    assert_eq!(response.status(), StatusCode::OK);

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for koreader missing-total verification");
    let row = sqlx::query(
        "SELECT PAGE, COMPLETED, LOCATOR FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ? LIMIT 1",
    )
    .bind("book-1")
    .bind("admin-user")
    .fetch_one(&verify_pool)
    .await
    .expect("koreader missing-total row should be queryable");
    let locator_blob = row
        .try_get::<Option<Vec<u8>>, _>("LOCATOR")
        .or_else(|_| row.try_get::<Option<Vec<u8>>, _>("locator"))
        .expect("koreader missing-total should load locator blob")
        .expect("koreader missing-total should persist locator blob");
    let locator: Value = serde_json::from_slice(&locator_blob)
        .expect("koreader missing-total locator should parse as json");
    verify_pool.close().await;

    assert_eq!(row.get::<i64, _>("PAGE"), 0);
    assert!(!row.get::<bool, _>("COMPLETED"));
    assert_eq!(locator.pointer("/locations/progression"), Some(&json!(0.0)));
    assert_eq!(
        locator.pointer("/locations/totalProgression"),
        Some(&Value::Null)
    );
}

#[tokio::test]
async fn router_koreader_progress_put_returns_forbidden_without_header_or_session_like_kotlin() {
    let ctx = TestFixture::new("router-koreader-progress-put-anonymous-forbidden").await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/koreader/syncs/progress")
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
                .expect("anonymous koreader progress put request should build"),
        )
        .await
        .expect("anonymous koreader progress put request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

async fn assert_koreader_progress_envelope(
    response: axum::response::Response,
    expected_status: StatusCode,
    expected_error: &str,
    expected_message: &str,
    expected_path: &str,
) {
    assert_eq!(response.status(), expected_status);

    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(expected_error.to_string()))
    );
    assert_eq!(
        payload.get("message"),
        Some(&Value::String(expected_message.to_string()))
    );
    assert_eq!(
        payload.get("path"),
        Some(&Value::String(expected_path.to_string()))
    );
    assert_eq!(
        payload.get("status"),
        Some(&Value::from(expected_status.as_u16()))
    );
    assert!(payload.get("timestamp").and_then(Value::as_u64).is_some());
}

#[tokio::test]
async fn router_koreader_progress_get_returns_kotlin_error_envelopes() {
    let ctx = TestFixture::new("router-koreader-progress-error-envelopes").await;

    let auth_token = ctx.login_admin().await;

    let empty_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/koreader/syncs/progress/hash-book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("koreader empty progress request should build"),
        )
        .await
        .expect("koreader empty progress request should complete");

    assert_koreader_progress_envelope(
        empty_response,
        StatusCode::OK,
        "OK",
        "No progress found for this book",
        "/koreader/syncs/progress/hash-book-1",
    )
    .await;

    let not_found_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/koreader/syncs/progress/missing-book-hash")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("koreader missing progress request should build"),
        )
        .await
        .expect("koreader missing progress request should complete");

    assert_koreader_progress_envelope(
        not_found_response,
        StatusCode::NOT_FOUND,
        "Not Found",
        "Book not found",
        "/koreader/syncs/progress/missing-book-hash",
    )
    .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for koreader conflict seed");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, FILE_HASH_KOREADER) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-duplicate-hash")
    .bind(0_i64)
    .bind("duplicate-hash-book.cbz")
    .bind("books/duplicate-hash-book.cbz")
    .bind("series-1")
    .bind(2_048_i64)
    .bind(2_i64)
    .bind("library-1")
    .bind("hash-book-1")
    .execute(&pool)
    .await
    .expect("duplicate hash book should be inserted");
    pool.close().await;

    let conflict_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/koreader/syncs/progress/hash-book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("koreader conflict progress request should build"),
        )
        .await
        .expect("koreader conflict progress request should complete");

    assert_koreader_progress_envelope(
        conflict_response,
        StatusCode::CONFLICT,
        "Conflict",
        "More than 1 book found with the same hash",
        "/koreader/syncs/progress/hash-book-1",
    )
    .await;
}

#[tokio::test]
async fn router_koreader_progress_put_returns_kotlin_error_envelopes_for_lookup_failures() {
    let ctx = TestFixture::new("router-koreader-progress-put-error-envelopes").await;

    let auth_token = ctx.login_admin().await;

    let not_found_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/koreader/syncs/progress")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "document": "missing-book-hash",
                        "percentage": 0.33,
                        "progress": "7",
                        "device": "KOReader",
                        "device_id": "reader-1"
                    })
                    .to_string(),
                ))
                .expect("koreader missing progress put request should build"),
        )
        .await
        .expect("koreader missing progress put request should complete");

    assert_koreader_progress_envelope(
        not_found_response,
        StatusCode::NOT_FOUND,
        "Not Found",
        "Book not found",
        "/koreader/syncs/progress",
    )
    .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for koreader put conflict seed");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, FILE_HASH_KOREADER) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-duplicate-put-hash")
    .bind(0_i64)
    .bind("duplicate-put-hash-book.cbz")
    .bind("books/duplicate-put-hash-book.cbz")
    .bind("series-1")
    .bind(2_048_i64)
    .bind(2_i64)
    .bind("library-1")
    .bind("hash-book-1")
    .execute(&pool)
    .await
    .expect("duplicate hash book should be inserted for put envelope test");
    pool.close().await;

    let conflict_response = ctx
        .app()
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
                        "progress": "7",
                        "device": "KOReader",
                        "device_id": "reader-1"
                    })
                    .to_string(),
                ))
                .expect("koreader conflict progress put request should build"),
        )
        .await
        .expect("koreader conflict progress put request should complete");

    assert_koreader_progress_envelope(
        conflict_response,
        StatusCode::CONFLICT,
        "Conflict",
        "More than 1 book found with the same hash",
        "/koreader/syncs/progress",
    )
    .await;
}

#[tokio::test]
async fn router_koreader_progress_put_rejects_invalid_epub_progress_string() {
    let ctx = TestFixture::new("router-koreader-progress-invalid-epub-progress").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
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

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
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

    assert_koreader_progress_envelope(
        response,
        StatusCode::BAD_REQUEST,
        "Bad Request",
        "Could not get Epub resource index from progress: 7",
        "/koreader/syncs/progress",
    )
    .await;
}

#[tokio::test]
async fn router_koreader_progress_put_returns_internal_error_for_out_of_range_epub_resource_index()
{
    let ctx = TestFixture::new("router-koreader-progress-out-of-range-epub-progress").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for koreader out-of-range epub seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for koreader out-of-range epub test");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
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
                        "progress": "/body/DocFragment[99]/body/div/p[1]/text().0",
                        "device": "KOReader",
                        "device_id": "reader-1"
                    })
                    .to_string(),
                ))
                .expect("koreader out-of-range epub progress put request should build"),
        )
        .await
        .expect("koreader out-of-range epub progress put request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn router_koreader_progress_put_rejects_missing_epub_extension_like_kotlin() {
    let ctx = TestFixture::new("router-koreader-progress-missing-epub-extension").await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
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
                .expect("koreader missing epub extension put request should build"),
        )
        .await
        .expect("koreader missing epub extension put request should complete");

    assert_koreader_progress_envelope(
        response,
        StatusCode::BAD_REQUEST,
        "Bad Request",
        "Epub extension not found",
        "/koreader/syncs/progress",
    )
    .await;
}

#[tokio::test]
async fn router_koreader_progress_put_rejects_invalid_non_epub_progress_string() {
    let ctx = TestFixture::new("router-koreader-progress-invalid-pdf-progress").await;
    seed_router_pdf_book(
        ctx.paths(),
        "book-pdf-1",
        "series-1",
        "book-pdf-1.pdf",
        "PDF Book 1",
    )
    .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for koreader invalid pdf seed");
    sqlx::query("UPDATE BOOK SET FILE_HASH_KOREADER = ? WHERE ID = ?")
        .bind("hash-book-pdf-1")
        .bind("book-pdf-1")
        .execute(&pool)
        .await
        .expect("pdf book koreader hash should be set");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_koreader_progress_put_rejects_out_of_range_non_epub_progress() {
    let ctx = TestFixture::new("router-koreader-progress-out-of-range-pdf-progress").await;
    seed_router_pdf_book(
        ctx.paths(),
        "book-pdf-2",
        "series-1",
        "book-pdf-2.pdf",
        "PDF Book 2",
    )
    .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for koreader out-of-range pdf seed");
    sqlx::query("UPDATE BOOK SET FILE_HASH_KOREADER = ? WHERE ID = ?")
        .bind("hash-book-pdf-2")
        .bind("book-pdf-2")
        .execute(&pool)
        .await
        .expect("pdf book koreader hash should be set for out-of-range test");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_koreader_progress_get_preserves_empty_device_fields() {
    let ctx = TestFixture::builder("router-koreader-progress-empty-device")
        .with_seed(|paths| async move {
            seed_router_read_progress(&paths, false).await;
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
}

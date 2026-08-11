use super::*;

#[tokio::test]
async fn router_book_progression_put_returns_conflict_for_older_progression() {
    let ctx = TestFixture::new("router-book-progression-put-conflict").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for progression conflict seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for progression conflict test");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED, READ_DATE, DEVICE_ID, DEVICE_NAME, LOCATOR) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(5_i64)
    .bind(false)
    .bind("2024-01-03 00:00:00")
    .bind("reader-1")
    .bind("KOReader")
    .bind(serde_json::to_vec(&json!({
        "href": "/book-1.xhtml#kobo.5.1",
        "type": "application/xhtml+xml",
        "locations": {
            "progression": 0.5,
            "position": 5,
            "totalProgression": 0.5
        }
    }))
    .expect("progression conflict locator should serialize"))
    .execute(&pool)
    .await
    .expect("existing read progress row for progression conflict should insert");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "modified": "2024-01-02T00:00:00Z",
                        "device": {
                            "id": "reader-2",
                            "name": "Another device"
                        },
                        "locator": {
                            "href": "/book-1.xhtml#kobo.4.1",
                            "type": "application/xhtml+xml",
                            "locations": {
                                "progression": 0.4,
                                "position": 4,
                                "totalProgression": 0.4
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("older progression put request should build"),
        )
        .await
        .expect("older progression put request should complete");

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "Progression is older than existing".to_string()
        ))
    );
}

#[tokio::test]
async fn router_book_progression_put_returns_conflict_for_same_modified_retry() {
    let ctx = TestFixture::new("router-book-progression-put-same-modified-conflict").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for same-modified retry seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for same-modified retry test");
    pool.close().await;

    let auth_token = ctx.login_admin().await;
    let progression = json!({
        "modified": "2024-01-04T05:06:07Z",
        "device": {
            "id": "reader-9",
            "name": "Kobo Libra"
        },
        "locator": {
            "href": "/book-1.xhtml#kobo.2.1",
            "type": "application/xhtml+xml",
            "locations": {
                "position": 2,
                "progression": 0.5,
                "totalProgression": 0.2
            },
            "koboSpan": "kobo-span-2"
        }
    });

    let first_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(progression.to_string()))
                .expect("same-modified first request should build"),
        )
        .await
        .expect("same-modified first request should complete");
    assert_eq!(first_response.status(), StatusCode::NO_CONTENT);

    let retry_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(progression.to_string()))
                .expect("same-modified retry request should build"),
        )
        .await
        .expect("same-modified retry request should complete");

    assert_eq!(retry_response.status(), StatusCode::CONFLICT);
    let payload = response_json(retry_response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "Progression is older than existing".to_string()
        ))
    );
}

#[tokio::test]
async fn router_book_progression_put_persists_modified_device_and_locator() {
    let ctx = TestFixture::new("router-book-progression-put-persists-full-payload").await;

    let extension_blob = fixture_epub_positions_extension_blob();

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for progression full-payload seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(extension_blob)
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for progression full-payload test");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let progression = json!({
        "modified": "2024-01-04T05:06:07Z",
        "device": {
            "id": "reader-9",
            "name": "Kobo Libra"
        },
        "locator": {
            "href": "/book-1.xhtml#kobo.2.1",
            "type": "application/xhtml+xml",
            "locations": {
                "position": 2,
                "progression": 0.5,
                "totalProgression": 0.2
            },
            "koboSpan": "kobo-span-2"
        }
    });

    let put_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(progression.to_string()))
                .expect("book progression full-payload put request should build"),
        )
        .await
        .expect("book progression full-payload put request should complete");
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);

    let get_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book progression full-payload get request should build"),
        )
        .await
        .expect("book progression full-payload get request should complete");
    assert_eq!(get_response.status(), StatusCode::OK);
    let payload = response_json(get_response).await;
    assert_eq!(payload.get("modified"), progression.get("modified"));
    assert_eq!(payload.get("device"), progression.get("device"));
    assert_eq!(payload.get("locator"), progression.get("locator"));
}

#[tokio::test]
async fn router_book_progression_get_rejects_invalid_persisted_locator_shape() {
    let ctx = TestFixture::new("router-book-progression-get-invalid-locator-shape").await;
    let auth_token = ctx.login_admin().await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for invalid progression locator seed");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED, READ_DATE, DEVICE_ID, DEVICE_NAME, LOCATOR) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(5_i64)
    .bind(false)
    .bind("2024-01-03 00:00:00")
    .bind("reader-1")
    .bind("KOReader")
    .bind(serde_json::to_vec(&json!([])).expect("invalid locator fixture should serialize"))
    .execute(&pool)
    .await
    .expect("invalid read progress locator row should insert");
    pool.close().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book progression invalid-locator get request should build"),
        )
        .await
        .expect("book progression invalid-locator get request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn router_book_progression_get_marks_offsetless_iso_read_date_as_utc() {
    let ctx = TestFixture::new("router-book-progression-get-offsetless-utc").await;
    let auth_token = ctx.login_admin().await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for offsetless progression seed");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED, READ_DATE, DEVICE_ID, DEVICE_NAME, LOCATOR) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(1_i64)
    .bind(false)
    .bind("2024-01-03T04:05:06")
    .bind("reader-1")
    .bind("KOReader")
    .bind(serde_json::to_vec(&json!({})).expect("empty locator should serialize"))
    .execute(&pool)
    .await
    .expect("offsetless read progress row should insert");
    pool.close().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("offsetless progression get request should build"),
        )
        .await
        .expect("offsetless progression get request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("modified"),
        Some(&json!("2024-01-03T04:05:06Z"))
    );
}

#[tokio::test]
async fn router_book_progression_put_roundtrips_on_opds_v2_route() {
    let ctx = TestFixture::new("router-book-progression-opds-v2-roundtrip").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for opds progression seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for opds progression test");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let progression = json!({
        "modified": "2024-01-04T05:06:07Z",
        "device": {
            "id": "reader-9",
            "name": "Kobo Libra"
        },
        "locator": {
            "href": "/book-1.xhtml#kobo.2.1",
            "type": "application/xhtml+xml",
            "locations": {
                "position": 2,
                "progression": 0.5,
                "totalProgression": 0.2
            },
            "koboSpan": "kobo-span-2"
        }
    });

    let put_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/opds/v2/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(progression.to_string()))
                .expect("opds progression put request should build"),
        )
        .await
        .expect("opds progression put request should complete");
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);

    let get_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds progression get request should build"),
        )
        .await
        .expect("opds progression get request should complete");
    assert_eq!(get_response.status(), StatusCode::OK);
    let payload = response_json(get_response).await;
    assert_eq!(payload.get("modified"), progression.get("modified"));
    assert_eq!(payload.get("device"), progression.get("device"));
    assert_eq!(payload.get("locator"), progression.get("locator"));
}

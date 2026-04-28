use super::*;

#[tokio::test]
async fn router_book_progression_put_returns_conflict_for_older_progression() {
    let paths = new_router_fixture("router-book-progression-put-conflict").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_returns_conflict_for_same_modified_retry() {
    let paths = new_router_fixture("router-book-progression-put-same-modified-conflict").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
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

    let first_response = app
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

    let retry_response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_persists_modified_device_and_locator() {
    let paths = new_router_fixture("router-book-progression-put-persists-full-payload").await;
    seed_router_contract_data(&paths).await;

    let extension_blob = fixture_epub_positions_extension_blob();

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

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

    let put_response = app
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

    let get_response = app
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
    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_roundtrips_on_opds_v2_route() {
    let paths = new_router_fixture("router-book-progression-opds-v2-roundtrip").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

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

    let put_response = app
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

    let get_response = app
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

    cleanup_router_fixture(paths);
}

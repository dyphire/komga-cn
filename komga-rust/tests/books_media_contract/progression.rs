use super::*;

#[tokio::test]
async fn router_book_progression_put_returns_conflict_for_older_progression() {
    let paths = new_router_fixture("router-book-progression-put-conflict").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
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
async fn router_book_progression_put_allows_same_modified_retry() {
    let paths = new_router_fixture("router-book-progression-put-same-modified-retry").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
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

    for attempt in 0..2 {
        let response = app
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

        assert_eq!(
            response.status(),
            StatusCode::NO_CONTENT,
            "retry attempt {attempt} should stay idempotent"
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_persists_modified_device_and_locator() {
    let paths = new_router_fixture("router-book-progression-put-persists-full-payload").await;
    seed_router_contract_data(&paths).await;

    let extension_blob = fixture_epub_positions_extension_blob();

    let pool = connect_pool(paths.main_db.as_path(), 1)
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
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

    let pool = connect_pool(paths.main_db.as_path(), 1)
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
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

#[tokio::test]
async fn router_book_progression_put_rejects_epub_locator_without_progression() {
    let paths = new_router_fixture("router-book-progression-put-epub-missing-progression").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
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
                        "modified": "2024-01-04T05:06:07Z",
                        "device": { "id": "reader-1", "name": "KOReader" },
                        "locator": {
                            "href": "chapter.xhtml#frag",
                            "type": "application/xhtml+xml",
                            "locations": { "position": 15 }
                        }
                    })
                    .to_string(),
                ))
                .expect("epub progression without locator progression request should build"),
        )
        .await
        .expect("epub progression without locator progression request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "location.progression is required".to_string()
        ))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_rejects_epub_locator_when_extension_is_missing() {
    let paths = new_router_fixture("router-book-progression-put-epub-missing-extension").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
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
                        "modified": "2024-01-04T05:06:07Z",
                        "device": { "id": "reader-1", "name": "KOReader" },
                        "locator": {
                            "href": "chapter.xhtml#frag",
                            "type": "application/xhtml+xml",
                            "locations": { "progression": 0.3 }
                        }
                    })
                    .to_string(),
                ))
                .expect("epub progression without extension request should build"),
        )
        .await
        .expect("epub progression without extension request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Epub extension not found".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_rejects_epub_locator_with_non_existing_href() {
    let paths = new_router_fixture("router-book-progression-put-epub-bad-href").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for progression bad-href seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for progression bad-href test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
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
                        "modified": "2024-01-04T05:06:07Z",
                        "device": { "id": "reader-1", "name": "KOReader" },
                        "locator": {
                            "href": "ch5.xhtml#frag",
                            "type": "application/xhtml+xml",
                            "locations": { "progression": 0.3 }
                        }
                    })
                    .to_string(),
                ))
                .expect("epub progression bad href request should build"),
        )
        .await
        .expect("epub progression bad href request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "Resource does not exist in book: ch5.xhtml".to_string()
        ))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_accepts_pdf_position_payload() {
    let paths = new_router_fixture("router-book-progression-put-pdf-position").await;
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
    let progression = json!({
        "modified": "2024-01-04T05:06:07Z",
        "device": { "id": "reader-9", "name": "Kobo Libra" },
        "locator": {
            "href": "",
            "type": "",
            "locations": { "position": 1 }
        }
    });

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/books/book-pdf-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(progression.to_string()))
                .expect("pdf progression put request should build"),
        )
        .await
        .expect("pdf progression put request should complete");
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);

    let get_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/progression")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("pdf progression get request should build"),
        )
        .await
        .expect("pdf progression get request should complete");
    assert_eq!(get_response.status(), StatusCode::OK);
    let payload = response_json(get_response).await;
    assert_eq!(payload.get("modified"), progression.get("modified"));
    assert_eq!(payload.get("device"), progression.get("device"));
    assert_eq!(payload.get("locator"), progression.get("locator"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_rejects_pdf_position_beyond_page_count() {
    let paths = new_router_fixture("router-book-progression-put-pdf-out-of-range").await;
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
                .method("PUT")
                .uri("/api/v1/books/book-pdf-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "modified": "2024-01-04T05:06:07Z",
                        "device": { "id": "reader-9", "name": "Kobo Libra" },
                        "locator": {
                            "href": "",
                            "type": "",
                            "locations": { "position": 2 }
                        }
                    })
                    .to_string(),
                ))
                .expect("pdf progression out-of-range request should build"),
        )
        .await
        .expect("pdf progression out-of-range request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "Page argument (2) must be within 1 and book page count (1)".to_string()
        ))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_normalizes_epub_locator_from_matching_position() {
    let paths = new_router_fixture("router-book-progression-put-epub-normalizes-locator").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for epub progression normalization seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for progression normalization test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let progression = json!({
        "modified": "2024-01-04T05:06:07Z",
        "device": { "id": "reader-9", "name": "Kobo Libra" },
        "locator": {
            "href": "/book-1.xhtml#custom-fragment",
            "type": "",
            "locations": {
                "progression": 0.5,
                "totalProgression": 0.9
            }
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
                .expect("epub progression normalization put request should build"),
        )
        .await
        .expect("epub progression normalization put request should complete");
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);

    let get_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("epub progression normalization get request should build"),
        )
        .await
        .expect("epub progression normalization get request should complete");
    assert_eq!(get_response.status(), StatusCode::OK);
    let payload = response_json(get_response).await;
    assert_eq!(payload.get("device"), progression.get("device"));
    assert_eq!(
        payload.get("locator"),
        Some(&json!({
            "href": "/book-1.xhtml#custom-fragment",
            "type": "application/xhtml+xml",
            "locations": {
                "progression": 0.5,
                "totalProgression": 0.2
            }
        }))
    );

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for normalized progression verification");
    let progression_row = sqlx::query(
        "SELECT PAGE, COMPLETED FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ? LIMIT 1",
    )
    .bind("book-1")
    .bind("admin-user")
    .fetch_one(&verify_pool)
    .await
    .expect("normalized progression row should be queryable");
    verify_pool.close().await;
    assert_eq!(progression_row.get::<i64, _>("PAGE"), 2);
    assert!(!progression_row.get::<bool, _>("COMPLETED"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_rejects_invalid_epub_progression_between_positions() {
    let paths = new_router_fixture("router-book-progression-put-epub-invalid-progression").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for invalid epub progression seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for invalid progression test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
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
                        "modified": "2024-01-04T05:06:07Z",
                        "device": { "id": "reader-9", "name": "Kobo Libra" },
                        "locator": {
                            "href": "/book-1.xhtml#custom-fragment",
                            "type": "application/xhtml+xml",
                            "locations": {
                                "progression": 0.9
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("invalid epub progression request should build"),
        )
        .await
        .expect("invalid epub progression request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Invalid progression".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_accepts_fixed_layout_epub_single_position() {
    let paths =
        new_router_fixture("router-book-progression-put-epub-fixed-layout-single-position").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for fixed-layout progression seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob_fixed_layout_single_position())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("fixed-layout epub extension should be seeded");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let progression = json!({
        "modified": "2024-01-04T05:06:07Z",
        "device": { "id": "reader-9", "name": "Kobo Libra" },
        "locator": {
            "href": "/book-1.xhtml#other-fragment",
            "type": "",
            "locations": { "progression": 0.9 }
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
                .expect("fixed-layout progression put request should build"),
        )
        .await
        .expect("fixed-layout progression put request should complete");
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);

    let get_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("fixed-layout progression get request should build"),
        )
        .await
        .expect("fixed-layout progression get request should complete");
    assert_eq!(get_response.status(), StatusCode::OK);
    let payload = response_json(get_response).await;
    assert_eq!(
        payload.get("locator"),
        Some(&json!({
            "href": "/book-1.xhtml#other-fragment",
            "type": "application/xhtml+xml",
            "locations": {
                "progression": 0.9,
                "totalProgression": 0.2
            },
            "koboSpan": "fixed-span"
        }))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_uses_total_progression_to_round_epub_page() {
    let paths =
        new_router_fixture("router-book-progression-put-epub-rounds-total-progression").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for epub page-rounding seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob_total_progression_021())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for page-rounding test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let progression = json!({
        "modified": "2024-01-04T05:06:07Z",
        "device": { "id": "reader-9", "name": "Kobo Libra" },
        "locator": {
            "href": "/book-1.xhtml#frag",
            "type": "",
            "locations": { "progression": 0.5 }
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
                .expect("epub page-rounding put request should build"),
        )
        .await
        .expect("epub page-rounding put request should complete");
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for page-rounding verification");
    let progression_row = sqlx::query(
        "SELECT PAGE, COMPLETED FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ? LIMIT 1",
    )
    .bind("book-1")
    .bind("admin-user")
    .fetch_one(&verify_pool)
    .await
    .expect("page-rounding progression row should be queryable");
    verify_pool.close().await;
    assert_eq!(progression_row.get::<i64, _>("PAGE"), 2);
    assert!(!progression_row.get::<bool, _>("COMPLETED"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_marks_completed_when_total_progression_is_above_threshold() {
    let paths = new_router_fixture("router-book-progression-put-epub-completed-threshold").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for epub completion-threshold seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob_total_progression_0995())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for completion-threshold test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let progression = json!({
        "modified": "2024-01-04T05:06:07Z",
        "device": { "id": "reader-9", "name": "Kobo Libra" },
        "locator": {
            "href": "/book-1.xhtml#frag",
            "type": "",
            "locations": { "progression": 0.5 }
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
                .expect("epub completion-threshold put request should build"),
        )
        .await
        .expect("epub completion-threshold put request should complete");
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for completion-threshold verification");
    let progression_row = sqlx::query(
        "SELECT PAGE, COMPLETED FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ? LIMIT 1",
    )
    .bind("book-1")
    .bind("admin-user")
    .fetch_one(&verify_pool)
    .await
    .expect("completion-threshold progression row should be queryable");
    verify_pool.close().await;
    assert_eq!(progression_row.get::<i64, _>("PAGE"), 10);
    assert!(progression_row.get::<bool, _>("COMPLETED"));

    cleanup_router_fixture(paths);
}

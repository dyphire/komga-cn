use super::*;

#[tokio::test]
async fn router_book_progression_put_accepts_url_encoded_epub_href() {
    let paths = new_router_fixture("router-book-progression-put-epub-url-encoded-href").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for url-encoded href seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for url-encoded href test");
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
                            "href": "/book%2D1.xhtml#frag",
                            "type": "application/xhtml+xml",
                            "locations": { "progression": 0.5 }
                        }
                    })
                    .to_string(),
                ))
                .expect("url-encoded href progression request should build"),
        )
        .await
        .expect("url-encoded href progression request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_routes_accept_basic_auth_like_kotlin_clients() {
    let paths = new_router_fixture("router-book-progression-basic-auth-compat").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for basic-auth progression seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for basic-auth progression test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let authorization =
        basic_authorization_header_value("admin@example.org", "router-contract-admin-123");
    let progression = json!({
        "modified": "2024-01-04T05:06:07Z",
        "device": { "id": "reader-9", "name": "Kobo Libra" },
        "locator": {
            "href": "/book-1.xhtml#custom-fragment",
            "type": "application/xhtml+xml",
            "locations": { "progression": 0.5 }
        }
    });

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/books/book-1/progression")
                .header(header::AUTHORIZATION, authorization.as_str())
                .header("x-auth-token", "")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(progression.to_string()))
                .expect("basic-auth progression put request should build"),
        )
        .await
        .expect("basic-auth progression put request should complete");
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);

    let get_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/progression")
                .header(header::AUTHORIZATION, authorization.as_str())
                .header("x-auth-token", "")
                .body(Body::empty())
                .expect("basic-auth progression get request should build"),
        )
        .await
        .expect("basic-auth progression get request should complete");
    assert_eq!(get_response.status(), StatusCode::OK);

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
async fn router_book_progression_put_ignores_epub_locator_position_when_persisting_page() {
    let paths =
        new_router_fixture("router-book-progression-put-epub-ignores-locator-position").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for epub conflicting-position seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob_total_progression_021())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for conflicting-position test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let progression = json!({
        "modified": "2024-01-04T05:06:07Z",
        "device": { "id": "reader-9", "name": "Kobo Libra" },
        "locator": {
            "href": "/book-1.xhtml#frag",
            "type": "",
            "locations": {
                "position": 9,
                "progression": 0.5
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
                .expect("epub conflicting-position put request should build"),
        )
        .await
        .expect("epub conflicting-position put request should complete");
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for conflicting-position verification");
    let progression_row = sqlx::query(
        "SELECT PAGE, COMPLETED FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ? LIMIT 1",
    )
    .bind("book-1")
    .bind("admin-user")
    .fetch_one(&verify_pool)
    .await
    .expect("conflicting-position progression row should be queryable");
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

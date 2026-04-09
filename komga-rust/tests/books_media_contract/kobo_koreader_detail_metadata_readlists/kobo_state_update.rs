#![allow(clippy::await_holding_lock)]

use super::*;

async fn seed_kobo_state_epub_extension(paths: &RuntimeDbPaths, blob: Vec<u8>) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for kobo state epub extension seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(blob)
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("kobo state epub extension should be seeded");
    pool.close().await;
}

#[tokio::test]
async fn router_kobo_state_update_roundtrip_persists_progress() {
    let paths = new_router_fixture("router-kobo-state-roundtrip").await;
    seed_router_contract_data(&paths).await;
    seed_kobo_state_epub_extension(
        &paths,
        fixture_epub_positions_extension_blob_total_progression_021(),
    )
    .await;

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
                                "ProgressPercent": 99.0,
                                "ContentSourceProgressPercent": 50.0,
                                "Location": {
                                    "Source": "/book-1.xhtml#frag",
                                    "Value": "kobo.2.1"
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
        Some(&json!(21.0)),
    );
    assert_eq!(
        state
            .get("CurrentBookmark")
            .and_then(|value| value.get("ContentSourceProgressPercent")),
        Some(&json!(50.0)),
    );
    assert_eq!(
        state
            .get("CurrentBookmark")
            .and_then(|value| value.get("Location"))
            .and_then(|value| value.get("Type")),
        Some(&Value::String("KoboSpan".to_string())),
    );
    assert_eq!(
        state
            .get("CurrentBookmark")
            .and_then(|value| value.get("Location"))
            .and_then(|value| value.get("Source")),
        Some(&Value::String("/book-1.xhtml#frag".to_string())),
    );
    assert_eq!(
        state
            .get("CurrentBookmark")
            .and_then(|value| value.get("Location"))
            .and_then(|value| value.get("Value")),
        Some(&Value::String("kobo.2.1".to_string())),
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_state_update_proxies_missing_book_when_kobo_proxy_enabled() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();

    let server = spawn_single_response_server(
        200,
        "application/json",
        r#"{"RequestResult":"Success","UpdateResults":[{"EntitlementId":"missing-book","CurrentBookmarkResult":{"Result":"Success"},"StatisticsResult":{"Result":"Ignored"},"StatusInfoResult":{"Result":"Success"}}]}"#,
    )
    .await;
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-kobo-state-update-proxy-missing-book").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/kobo/any-token/v1/library/missing-book/state")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "ReadingStates": [{
                            "EntitlementId": "missing-book",
                            "LastModified": "2026-03-27T10:00:00Z",
                            "Statistics": {"LastModified": "2026-03-27T10:00:00Z"},
                            "StatusInfo": {"LastModified": "2026-03-27T10:00:00Z", "Status": "Reading"},
                            "CurrentBookmark": {
                                "LastModified": "2026-03-27T10:00:00Z",
                                "ContentSourceProgressPercent": 23.0,
                                "Location": {"Source": "/missing/manifest#position=5", "Value": "kobo.5.1"}
                            }
                        }]
                    })
                    .to_string(),
                ))
                .expect("kobo state update proxy request should build"),
        )
        .await
        .expect("kobo state update proxy request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "RequestResult":"Success",
            "UpdateResults":[{
                "EntitlementId":"missing-book",
                "CurrentBookmarkResult":{"Result":"Success"},
                "StatisticsResult":{"Result":"Ignored"},
                "StatusInfoResult":{"Result":"Success"}
            }]
        })
    );

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server
        .join
        .await
        .expect("kobo state update proxy server should finish");
}

#[tokio::test]
async fn router_kobo_state_update_returns_not_found_for_missing_book_when_proxy_disabled() {
    let paths = new_router_fixture("router-kobo-state-update-missing-book-local").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/kobo/any-token/v1/library/missing-book/state")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "ReadingStates": [{
                            "EntitlementId": "missing-book",
                            "LastModified": "2026-03-27T10:00:00Z",
                            "Statistics": {"LastModified": "2026-03-27T10:00:00Z"},
                            "StatusInfo": {"LastModified": "2026-03-27T10:00:00Z", "Status": "Reading"},
                            "CurrentBookmark": {
                                "LastModified": "2026-03-27T10:00:00Z",
                                "ContentSourceProgressPercent": 23.0,
                                "Location": {"Source": "/missing/manifest#position=5", "Value": "kobo.5.1"}
                            }
                        }]
                    })
                    .to_string(),
                ))
                .expect("kobo state update missing-book request should build"),
        )
        .await
        .expect("kobo state update missing-book request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_state_update_uses_path_api_key_identity_for_valid_epub_update() {
    let paths = new_router_fixture("router-kobo-state-update-api-key-identity").await;
    seed_router_contract_data(&paths).await;
    seed_kobo_state_epub_extension(
        &paths,
        fixture_epub_positions_extension_blob_total_progression_021(),
    )
    .await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-state-user",
        "kobo-state@example.org",
        "router-contract-kobo-state-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-state-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/kobo/validkobotoken/v1/library/book-1/state")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "ReadingStates": [{
                            "EntitlementId": "book-1",
                            "LastModified": "2026-03-27T10:00:00Z",
                            "Statistics": {"LastModified": "2026-03-27T10:00:00Z"},
                            "StatusInfo": {"LastModified": "2026-03-27T10:00:00Z", "Status": "Reading"},
                            "CurrentBookmark": {
                                "LastModified": "2026-03-27T10:00:00Z",
                                "ContentSourceProgressPercent": 50.0,
                                "Location": {"Source": "/book-1.xhtml#frag", "Type": "KoboSpan", "Value": "kobo.5.1"}
                            }
                        }]
                    })
                    .to_string(),
                ))
                .expect("kobo state update path token request should build"),
        )
        .await
        .expect("kobo state update path token request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await.pointer("/RequestResult"),
        Some(&Value::String("Success".to_string()))
    );

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract db should open for path token state update assertion");
    let row = sqlx::query(
        "SELECT DEVICE_ID, DEVICE_NAME, LOCATOR FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ? LIMIT 1",
    )
    .bind("book-1")
    .bind("kobo-state-user")
    .fetch_one(&pool)
    .await
    .expect("path token state update should persist read progress row");
    let locator_blob = row
        .try_get::<Option<Vec<u8>>, _>("LOCATOR")
        .or_else(|_| row.try_get::<Option<Vec<u8>>, _>("locator"))
        .expect("path token state update should load locator blob")
        .expect("path token state update should persist locator blob");
    let locator: Value = serde_json::from_slice(&locator_blob)
        .expect("path token state update locator should parse as json");
    let device_id = row
        .try_get::<String, _>("DEVICE_ID")
        .or_else(|_| row.try_get::<String, _>("device_id"))
        .expect("path token state update should load device id");
    let device_name = row
        .try_get::<String, _>("DEVICE_NAME")
        .or_else(|_| row.try_get::<String, _>("device_name"))
        .expect("path token state update should load device name");
    assert_eq!(device_id, "api-key-validkobotoken".to_string());
    assert_eq!(device_name, "kobo sync".to_string());
    assert_eq!(
        locator.get("href"),
        Some(&Value::String("/book-1.xhtml#frag".to_string()))
    );
    assert_eq!(
        locator.get("koboSpan"),
        Some(&Value::String("kobo.5.1".to_string()))
    );
    pool.close().await;

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_state_update_returns_failure_for_invalid_epub_progression() {
    let paths = new_router_fixture("router-kobo-state-update-invalid-epub-progression").await;
    seed_router_contract_data(&paths).await;
    seed_kobo_state_epub_extension(&paths, fixture_epub_positions_extension_blob()).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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
                            "Statistics": {"LastModified": "2026-03-27T10:00:00Z"},
                            "StatusInfo": {"LastModified": "2026-03-27T10:00:00Z", "Status": "Reading"},
                            "CurrentBookmark": {
                                "LastModified": "2026-03-27T10:00:00Z",
                                "ProgressPercent": 90.0,
                                "ContentSourceProgressPercent": 90.0,
                                "Location": {"Source": "/book-1.xhtml#custom-fragment", "Type": "KoboSpan", "Value": "kobo.9.1"}
                            }
                        }]
                    })
                    .to_string(),
                ))
                .expect("invalid epub progression kobo state update request should build"),
        )
        .await
        .expect("invalid epub progression kobo state update request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "RequestResult":"Failure",
            "UpdateResults":[{
                "EntitlementId":"book-1",
                "CurrentBookmarkResult":{"Result":"Failure"},
                "StatisticsResult":{"Result":"Failure"},
                "StatusInfoResult":{"Result":"Failure"}
            }]
        })
    );

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for invalid epub progression verification");
    let row = sqlx::query("SELECT 1 FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ? LIMIT 1")
        .bind("book-1")
        .bind("admin-user")
        .fetch_optional(&pool)
        .await
        .expect("invalid epub progression verification query should succeed");
    assert!(row.is_none());
    pool.close().await;

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_state_update_uses_matched_epub_total_progression_for_page_and_completion() {
    let paths = new_router_fixture("router-kobo-state-update-matched-total-progression").await;
    seed_router_contract_data(&paths).await;
    seed_kobo_state_epub_extension(
        &paths,
        fixture_epub_positions_extension_blob_total_progression_021(),
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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
                            "Statistics": {"LastModified": "2026-03-27T10:00:00Z"},
                            "StatusInfo": {"LastModified": "2026-03-27T10:00:00Z", "Status": "Reading"},
                            "CurrentBookmark": {
                                "LastModified": "2026-03-27T10:00:00Z",
                                "ProgressPercent": 99.0,
                                "ContentSourceProgressPercent": 50.0,
                                "Location": {"Source": "/book-1.xhtml#frag", "Type": "KoboSpan", "Value": "kobo.2.1"}
                            }
                        }]
                    })
                    .to_string(),
                ))
                .expect("matched total progression kobo state update request should build"),
        )
        .await
        .expect("matched total progression kobo state update request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await.pointer("/RequestResult"),
        Some(&Value::String("Success".to_string()))
    );

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for matched total progression verification");
    let row = sqlx::query(
        "SELECT PAGE, COMPLETED, LOCATOR FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ? LIMIT 1",
    )
    .bind("book-1")
    .bind("admin-user")
    .fetch_one(&pool)
    .await
    .expect("matched total progression row should exist");
    let locator_blob = row
        .try_get::<Option<Vec<u8>>, _>("LOCATOR")
        .or_else(|_| row.try_get::<Option<Vec<u8>>, _>("locator"))
        .expect("matched total progression should load locator blob")
        .expect("matched total progression should persist locator blob");
    let locator: Value = serde_json::from_slice(&locator_blob)
        .expect("matched total progression locator should parse as json");
    assert_eq!(row.get::<i64, _>("PAGE"), 2);
    assert!(!row.get::<bool, _>("COMPLETED"));
    assert_eq!(
        locator.pointer("/locations/totalProgression"),
        Some(&json!(0.21))
    );
    assert_eq!(locator.pointer("/locations/position"), None);
    pool.close().await;

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_state_update_finished_uses_last_epub_position_semantics() {
    let paths = new_router_fixture("router-kobo-state-update-finished-last-position").await;
    seed_router_contract_data(&paths).await;
    seed_kobo_state_epub_extension(
        &paths,
        fixture_epub_positions_extension_blob_fixed_layout_single_position(),
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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
                            "Statistics": {"LastModified": "2026-03-27T10:00:00Z"},
                            "StatusInfo": {"LastModified": "2026-03-27T10:00:00Z", "Status": "Finished"},
                            "CurrentBookmark": {
                                "LastModified": "2026-03-27T10:00:00Z",
                                "ProgressPercent": 100.0,
                                "ContentSourceProgressPercent": 100.0,
                                "Location": {"Source": "/wrong.xhtml#start", "Type": "KoboSpan", "Value": "kobo.1.1"}
                            }
                        }]
                    })
                    .to_string(),
                ))
                .expect("finished kobo state update request should build"),
        )
        .await
        .expect("finished kobo state update request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await.pointer("/RequestResult"),
        Some(&Value::String("Success".to_string()))
    );

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for finished state verification");
    let row = sqlx::query(
        "SELECT PAGE, COMPLETED, LOCATOR FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ? LIMIT 1",
    )
    .bind("book-1")
    .bind("admin-user")
    .fetch_one(&pool)
    .await
    .expect("finished state row should exist");
    let locator_blob = row
        .try_get::<Option<Vec<u8>>, _>("LOCATOR")
        .or_else(|_| row.try_get::<Option<Vec<u8>>, _>("locator"))
        .expect("finished state should load locator blob")
        .expect("finished state should persist locator blob");
    let locator: Value =
        serde_json::from_slice(&locator_blob).expect("finished state locator should parse as json");
    assert_eq!(row.get::<i64, _>("PAGE"), 2);
    assert!(!row.get::<bool, _>("COMPLETED"));
    assert_eq!(
        locator.pointer("/locations/totalProgression"),
        Some(&json!(0.2))
    );
    pool.close().await;

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_state_update_returns_failure_for_older_progression() {
    let paths = new_router_fixture("router-kobo-state-update-older-progression").await;
    seed_router_contract_data(&paths).await;
    seed_kobo_state_epub_extension(&paths, fixture_epub_positions_extension_blob()).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for older progression seed");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED, READ_DATE, DEVICE_ID, DEVICE_NAME, LOCATOR) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(5_i64)
    .bind(false)
    .bind("2026-03-28 00:00:00")
    .bind("reader-1")
    .bind("Kobo Libra")
    .bind(
        serde_json::to_vec(&json!({
            "href": "/book-1.xhtml#kobo.5.1",
            "type": "application/xhtml+xml",
            "locations": {
                "progression": 0.5,
                "totalProgression": 0.5
            }
        }))
        .expect("older progression locator should serialize"),
    )
    .execute(&pool)
    .await
    .expect("older progression row should insert");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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
                            "Statistics": {"LastModified": "2026-03-27T10:00:00Z"},
                            "StatusInfo": {"LastModified": "2026-03-27T10:00:00Z", "Status": "Reading"},
                            "CurrentBookmark": {
                                "LastModified": "2026-03-27T10:00:00Z",
                                "ProgressPercent": 50.0,
                                "ContentSourceProgressPercent": 50.0,
                                "Location": {"Source": "/book-1.xhtml#frag", "Type": "KoboSpan", "Value": "kobo.5.1"}
                            }
                        }]
                    })
                    .to_string(),
                ))
                .expect("older progression kobo state update request should build"),
        )
        .await
        .expect("older progression kobo state update request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "RequestResult":"Failure",
            "UpdateResults":[{
                "EntitlementId":"book-1",
                "CurrentBookmarkResult":{"Result":"Failure"},
                "StatisticsResult":{"Result":"Failure"},
                "StatusInfoResult":{"Result":"Failure"}
            }]
        })
    );

    cleanup_router_fixture(paths);
}

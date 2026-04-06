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

#[tokio::test]
async fn router_kobo_state_empty_payload_omits_progress_fields_and_location() {
    let paths = new_router_fixture("router-kobo-state-empty-shape").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/book-1/state")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo empty state request should build"),
        )
        .await
        .expect("kobo empty state request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let state = payload
        .as_array()
        .and_then(|values| values.first())
        .expect("kobo empty state response should contain one reading state object");
    let current_bookmark = state
        .get("CurrentBookmark")
        .and_then(Value::as_object)
        .expect("kobo empty state should include current bookmark");
    assert!(current_bookmark.contains_key("LastModified"));
    assert!(!current_bookmark.contains_key("ProgressPercent"));
    assert!(!current_bookmark.contains_key("ContentSourceProgressPercent"));
    assert!(!current_bookmark.contains_key("Location"));
    assert_eq!(
        state
            .get("StatusInfo")
            .and_then(|value| value.get("Status")),
        Some(&Value::String("ReadyToRead".to_string())),
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_state_existing_progress_without_locator_omits_progress_fields_and_location() {
    let paths = new_router_fixture("router-kobo-state-missing-locator").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract db should open for missing locator state test");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED, DEVICE_ID, DEVICE_NAME, CREATED_DATE, LAST_MODIFIED_DATE, LOCATOR) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(5_i64)
    .bind(false)
    .bind("")
    .bind("")
    .bind("2026-03-27T09:00:00Z")
    .bind("2026-03-27T10:00:00Z")
    .bind(Option::<Vec<u8>>::None)
    .execute(&pool)
    .await
    .expect("read progress row without locator should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/book-1/state")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo missing locator state request should build"),
        )
        .await
        .expect("kobo missing locator state request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let state = payload
        .as_array()
        .and_then(|values| values.first())
        .expect("kobo missing locator state response should contain one reading state object");
    let current_bookmark = state
        .get("CurrentBookmark")
        .and_then(Value::as_object)
        .expect("kobo missing locator state should include current bookmark");
    assert_eq!(
        current_bookmark.get("LastModified"),
        Some(&Value::String("2026-03-27T10:00:00Z".to_string())),
    );
    assert!(!current_bookmark.contains_key("ProgressPercent"));
    assert!(!current_bookmark.contains_key("ContentSourceProgressPercent"));
    assert!(!current_bookmark.contains_key("Location"));
    assert_eq!(
        state
            .get("StatusInfo")
            .and_then(|value| value.get("Status")),
        Some(&Value::String("Reading".to_string())),
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_state_existing_progress_preserves_empty_string_locator_fields() {
    let paths = new_router_fixture("router-kobo-state-empty-string-locator").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract db should open for empty-string locator state test");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED, DEVICE_ID, DEVICE_NAME, CREATED_DATE, LAST_MODIFIED_DATE, LOCATOR) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(5_i64)
    .bind(false)
    .bind("")
    .bind("")
    .bind("2026-03-27T09:00:00Z")
    .bind("2026-03-27T10:00:00Z")
    .bind(serde_json::to_vec(&json!({
        "href": "",
        "koboSpan": "",
        "locations": {}
    }))
    .expect("empty-string locator payload should serialize"))
    .execute(&pool)
    .await
    .expect("read progress row with empty-string locator should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/book-1/state")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo empty-string locator state request should build"),
        )
        .await
        .expect("kobo empty-string locator state request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let state = payload
        .as_array()
        .and_then(|values| values.first())
        .expect("kobo empty-string locator state response should contain one reading state object");
    let current_bookmark = state
        .get("CurrentBookmark")
        .and_then(Value::as_object)
        .expect("kobo empty-string locator state should include current bookmark");
    assert!(!current_bookmark.contains_key("ProgressPercent"));
    assert!(!current_bookmark.contains_key("ContentSourceProgressPercent"));
    let location = current_bookmark
        .get("Location")
        .and_then(Value::as_object)
        .expect("kobo empty-string locator state should include location object");
    assert_eq!(location.get("Source"), Some(&Value::String(String::new())));
    assert_eq!(
        location.get("Type"),
        Some(&Value::String("KoboSpan".to_string()))
    );
    assert_eq!(location.get("Value"), Some(&Value::String(String::new())));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_state_proxies_missing_book_when_kobo_proxy_enabled() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();

    let server = spawn_single_response_server(
        200,
        "application/json",
        r#"[{"StatusInfo":{"Status":"Reading"},"CurrentBookmark":{"ProgressPercent":12.5}}]"#,
    )
    .await;
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-kobo-state-proxy-missing-book").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/missing-book/state")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo proxied missing book state request should build"),
        )
        .await
        .expect("kobo proxied missing book state request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!([{"StatusInfo":{"Status":"Reading"},"CurrentBookmark":{"ProgressPercent":12.5}}])
    );

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server
        .join
        .await
        .expect("kobo proxied missing book state server should finish");
}

#[tokio::test]
async fn router_kobo_state_returns_not_found_for_missing_book_when_proxy_disabled() {
    let paths = new_router_fixture("router-kobo-state-missing-book-local").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/missing-book/state")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo missing book state request should build"),
        )
        .await
        .expect("kobo missing book state request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

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

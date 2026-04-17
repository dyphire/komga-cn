#![allow(clippy::await_holding_lock)]

use super::*;

#[tokio::test]
async fn router_kobo_state_empty_payload_omits_progress_fields_and_location() {
    let paths = new_router_fixture("router-kobo-state-empty-shape").await;
    seed_router_contract_data(&paths).await;
    seed_admin_kobo_path_token(&paths).await;

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
    seed_admin_kobo_path_token(&paths).await;

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
    seed_admin_kobo_path_token(&paths).await;

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
    seed_admin_kobo_path_token(&paths).await;
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
    seed_admin_kobo_path_token(&paths).await;

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

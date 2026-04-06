use super::*;

async fn seed_kobo_sync_api_key(paths: &RuntimeDbPaths, api_key: &str, user_id: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("user api key seed db should open");

    let api_key_hash = {
        let mut hasher = Sha512::new();
        hasher.update(api_key.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };

    sqlx::query("INSERT INTO USER_API_KEY (ID, USER_ID, API_KEY, COMMENT) VALUES (?, ?, ?, ?)")
        .bind(format!("api-key-{api_key}"))
        .bind(user_id)
        .bind(api_key_hash)
        .bind("kobo sync")
        .execute(&pool)
        .await
        .expect("user api key row should be inserted");

    pool.close().await;
}

async fn load_first_kobo_sync_point_state_json(paths: &RuntimeDbPaths) -> Value {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("kobo sync point state db should open");

    let row =
        sqlx::query("SELECT STATE_JSON FROM KOBO_SYNC_POINT_STATE ORDER BY SYNC_POINT_ID LIMIT 1")
            .fetch_one(&pool)
            .await
            .expect("kobo sync point state row should load");

    pool.close().await;
    serde_json::from_str(row.get::<String, _>("STATE_JSON").as_str())
        .expect("kobo sync point state json should parse")
}

async fn load_kobo_sync_point_state_json(paths: &RuntimeDbPaths, sync_point_id: &str) -> Value {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("kobo sync point state db should open");

    let row =
        sqlx::query("SELECT STATE_JSON FROM KOBO_SYNC_POINT_STATE WHERE SYNC_POINT_ID = ? LIMIT 1")
            .bind(sync_point_id)
            .fetch_one(&pool)
            .await
            .expect("kobo sync point state row should load");

    pool.close().await;
    serde_json::from_str(row.get::<String, _>("STATE_JSON").as_str())
        .expect("kobo sync point state json should parse")
}

async fn seed_kobo_sync_point_state(
    paths: &RuntimeDbPaths,
    sync_point_id: &str,
    state_json: &Value,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("kobo sync point state seed db should open");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS KOBO_SYNC_POINT_STATE ( SYNC_POINT_ID TEXT NOT NULL, USER_ID TEXT NOT NULL, STATE_JSON TEXT NOT NULL, PRIMARY KEY (SYNC_POINT_ID, USER_ID) )",
    )
    .execute(&pool)
    .await
    .expect("kobo sync point state table should exist");

    sqlx::query(
        "INSERT INTO KOBO_SYNC_POINT_STATE (SYNC_POINT_ID, USER_ID, STATE_JSON) VALUES (?, ?, ?)",
    )
    .bind(sync_point_id)
    .bind(
        state_json
            .get("user_id")
            .and_then(Value::as_str)
            .expect("seed sync point state should include user_id"),
    )
    .bind(state_json.to_string())
    .execute(&pool)
    .await
    .expect("kobo sync point state row should be inserted");

    pool.close().await;
}

#[tokio::test]
async fn router_kobo_library_sync_returns_nested_dto_shape_and_sync_token() {
    let paths = new_router_fixture("router-kobo-library-sync-shape").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let first_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/sync")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo library sync request should build"),
        )
        .await
        .expect("kobo library sync request should complete");
    assert_eq!(first_response.status(), StatusCode::OK);

    let sync_token_header = first_response
        .headers()
        .get("x-kobo-synctoken")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("kobo sync response should include x-kobo-synctoken header");
    assert!(sync_token_header.starts_with("KOMGA."));

    let first_payload = response_json(first_response).await;
    let first_events = first_payload
        .as_array()
        .expect("kobo sync response should be a JSON array");
    assert!(!first_events.is_empty());

    let entitlement = first_events
        .iter()
        .find_map(|event| event.get("NewEntitlement"))
        .expect("kobo sync payload should contain a NewEntitlement event");
    assert!(entitlement.get("BookEntitlement").is_some());
    assert!(entitlement.get("BookMetadata").is_some());
    assert!(entitlement.get("ReadingState").is_some());

    let second_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/sync")
                .header("x-auth-token", &auth_token)
                .header("x-kobo-synctoken", sync_token_header)
                .body(Body::empty())
                .expect("kobo library sync continuation request should build"),
        )
        .await
        .expect("kobo library sync continuation request should complete");
    assert_eq!(second_response.status(), StatusCode::OK);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_library_sync_persists_api_key_id_in_sync_point_state() {
    let paths = new_router_fixture("router-kobo-library-sync-api-key-ownership").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-sync-user",
        "kobo-sync@example.org",
        "router-contract-kobo-sync-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-sync-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/validkobotoken/v1/library/sync")
                .body(Body::empty())
                .expect("kobo library sync path-token request should build"),
        )
        .await
        .expect("kobo library sync path-token request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let state_json = load_first_kobo_sync_point_state_json(&paths).await;
    assert_eq!(
        state_json.get("user_id"),
        Some(&Value::String("kobo-sync-user".to_string()))
    );
    assert_eq!(
        state_json.get("api_key_id"),
        Some(&Value::String("api-key-validkobotoken".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_library_sync_rejects_bare_base64_komga_tokens_as_invalid() {
    let paths = new_router_fixture("router-kobo-library-sync-bare-base64-komga-token").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let first_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/sync")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("initial kobo library sync request should build"),
        )
        .await
        .expect("initial kobo library sync request should complete");

    assert_eq!(first_response.status(), StatusCode::OK);
    let sync_token_header = first_response
        .headers()
        .get("x-kobo-synctoken")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("initial sync response should include x-kobo-synctoken");
    let first_payload = response_json(first_response).await;
    assert!(
        first_payload
            .as_array()
            .expect("initial sync response should be a JSON array")
            .iter()
            .any(|event| event.get("NewEntitlement").is_some())
    );
    let bare_sync_token = sync_token_header
        .strip_prefix("KOMGA.")
        .expect("initial sync token should use KOMGA prefix");

    let second_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/sync")
                .header("x-auth-token", &auth_token)
                .header("x-kobo-synctoken", bare_sync_token)
                .body(Body::empty())
                .expect("bare-base64 kobo library sync request should build"),
        )
        .await
        .expect("bare-base64 kobo library sync request should complete");

    assert_eq!(second_response.status(), StatusCode::OK);
    let second_payload = response_json(second_response).await;
    assert!(
        second_payload
            .as_array()
            .expect("bare-base64 sync response should be a JSON array")
            .iter()
            .any(|event| event.get("NewEntitlement").is_some())
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_library_sync_does_not_backfill_missing_api_key_id_on_existing_state() {
    let paths = new_router_fixture("router-kobo-library-sync-no-api-key-backfill").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-sync-user",
        "kobo-sync@example.org",
        "router-contract-kobo-sync-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-sync-user").await;

    let ongoing_sync_point_id = "existing-sync-point";
    seed_kobo_sync_point_state(
        &paths,
        ongoing_sync_point_id,
        &json!({
            "user_id": "kobo-sync-user",
            "api_key_id": null,
            "marker": "2026-01-01T00:00:00Z",
            "cursor": 0,
            "from_marker": null,
            "snapshot": null
        }),
    )
    .await;
    let sync_token_payload = json!({
        "version": 1,
        "rawKoboSyncToken": "",
        "ongoingSyncPointId": ongoing_sync_point_id,
        "lastSuccessfulSyncPointId": null
    })
    .to_string();
    let sync_token_header = format!(
        "KOMGA.{}",
        STANDARD_NO_PAD.encode(sync_token_payload.as_bytes())
    );

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/validkobotoken/v1/library/sync")
                .header("x-kobo-synctoken", sync_token_header)
                .body(Body::empty())
                .expect("existing-state kobo library sync request should build"),
        )
        .await
        .expect("existing-state kobo library sync request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let state_json = load_kobo_sync_point_state_json(&paths, ongoing_sync_point_id).await;
    assert_eq!(state_json.get("api_key_id"), Some(&Value::Null));

    cleanup_router_fixture(paths);
}

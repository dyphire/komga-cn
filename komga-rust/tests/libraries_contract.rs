use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::infrastructure::sqlite::connect_pool;
use komga_server::app::build_router_with_config;
use serde_json::{Value, json};
use sha2::{Digest, Sha512};
use sqlx::Row;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::sync::{Mutex, OnceLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tower::util::ServiceExt;

#[path = "support/runtime_router_contract_support.rs"]
mod runtime_router_contract_support;

use runtime_router_contract_support::*;

#[test]
fn libraries_contract_target_is_registered() {
    assert_required_target_declared("libraries", "libraries_contract");
}

struct SingleResponseServer {
    url: String,
    join: tokio::task::JoinHandle<()>,
}

fn kobo_proxy_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn restore_env_var(name: &str, previous: Option<String>) {
    match previous {
        Some(value) => unsafe { std::env::set_var(name, value) },
        None => unsafe { std::env::remove_var(name) },
    }
}

async fn spawn_single_response_server(
    status_code: u16,
    content_type: &str,
    body: &str,
) -> SingleResponseServer {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock response server should bind");
    let address = listener
        .local_addr()
        .expect("mock response server should have local addr");
    let body = body.to_string();
    let content_type = content_type.to_string();
    let join = tokio::spawn(async move {
        let (mut stream, _) = listener
            .accept()
            .await
            .expect("mock response server should accept one connection");
        let mut request = [0_u8; 2048];
        let _ = stream.read(&mut request).await;
        let status_text = match status_code {
            200 => "OK",
            404 => "Not Found",
            500 => "Internal Server Error",
            503 => "Service Unavailable",
            _ => "OK",
        };
        let response = format!(
            "HTTP/1.1 {status_code} {status_text}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .await
            .expect("mock response server should write response");
    });

    SingleResponseServer {
        url: format!("http://{address}/feed.json"),
        join,
    }
}

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

fn fixed_layout_extension_blob() -> Vec<u8> {
    vec![
        31, 139, 8, 0, 100, 225, 210, 105, 2, 255, 171, 86, 202, 44, 118, 203, 172, 72, 77, 241,
        73, 172, 204, 47, 45, 81, 178, 42, 41, 42, 77, 173, 5, 0, 254, 47, 201, 165, 22, 0, 0, 0,
    ]
}

async fn upsert_server_setting(paths: &RuntimeDbPaths, key: &str, value: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("server settings db should open");

    sqlx::query("INSERT OR REPLACE INTO SERVER_SETTINGS (KEY, VALUE) VALUES (?, ?)")
        .bind(key)
        .bind(value)
        .execute(&pool)
        .await
        .expect("server setting should upsert");

    pool.close().await;
}

async fn count_query_rows(paths: &RuntimeDbPaths, sql: &str, bind: &str) -> i64 {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("count query db should open");
    let count = sqlx::query(sql)
        .bind(bind)
        .fetch_one(&pool)
        .await
        .expect("count query should succeed")
        .get::<i64, _>("COUNT");
    pool.close().await;
    count
}

fn write_executable_fixture(paths: &RuntimeDbPaths, file_name: &str) -> String {
    let path = paths.config_dir.join(file_name);
    fs::write(&path, "#!/bin/sh\nexit 0\n").expect("kepubify fixture should be written");
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(&path)
            .expect("kepubify fixture metadata should load")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).expect("kepubify fixture should be executable");
    }
    path.to_string_lossy().to_string()
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

#[tokio::test]
async fn router_kobo_catch_all_returns_empty_json_when_proxy_disabled() {
    let paths = new_router_fixture("router-kobo-catch-all-disabled").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/unimplemented-resource")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo catch-all request should build"),
        )
        .await
        .expect("kobo catch-all request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload, serde_json::json!({}));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_libraries_route_matches_kotlin_etag_without_extra_cache_headers() {
    let paths = new_router_fixture("router-api-libraries-kotlin-cache-headers").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let first_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/libraries")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("libraries cache request should build"),
        )
        .await
        .expect("libraries cache request should complete");

    assert_eq!(first_response.status(), StatusCode::OK);
    assert!(
        first_response
            .headers()
            .get(header::CACHE_CONTROL)
            .is_none(),
        "Kotlin libraries list does not emit Cache-Control on 200"
    );

    let etag = first_response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("libraries response should include etag");

    let second_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/libraries")
                .header("x-auth-token", &auth_token)
                .header(header::IF_NONE_MATCH, etag)
                .body(Body::empty())
                .expect("conditional libraries request should build"),
        )
        .await
        .expect("conditional libraries request should complete");

    assert_eq!(second_response.status(), StatusCode::NOT_MODIFIED);
    assert!(
        second_response
            .headers()
            .get(header::CACHE_CONTROL)
            .is_none(),
        "Kotlin conditional libraries list does not emit Cache-Control on 304"
    );
    assert!(second_response.headers().contains_key(header::ETAG));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_patch_accepts_null_scan_directory_exclusions_as_clear() {
    let paths = new_router_fixture("router-api-library-patch-null-exclusions").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract db should open for library exclusions seed");
    sqlx::query("INSERT INTO LIBRARY_EXCLUSIONS (LIBRARY_ID, EXCLUSION) VALUES (?, ?), (?, ?)")
        .bind("library-1")
        .bind("folder-a")
        .bind("library-1")
        .bind("folder-b")
        .execute(&pool)
        .await
        .expect("library exclusions should be seeded");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let patch_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "scanDirectoryExclusions": null }).to_string(),
                ))
                .expect("library patch request should build"),
        )
        .await
        .expect("library patch request should complete");
    assert_eq!(patch_response.status(), StatusCode::NO_CONTENT);

    let get_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("library detail request should build"),
        )
        .await
        .expect("library detail request should complete");
    assert_eq!(get_response.status(), StatusCode::OK);
    let payload = response_json(get_response).await;
    assert_eq!(
        payload.get("scanDirectoryExclusions"),
        Some(&json!([])),
        "PATCH null scanDirectoryExclusions should clear exclusions like Kotlin"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_delete_requires_authentication() {
    let paths = new_router_fixture("router-api-library-delete-requires-auth").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/libraries/library-1")
                .body(Body::empty())
                .expect("library delete unauthenticated request should build"),
        )
        .await
        .expect("library delete unauthenticated request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_delete_forbids_non_admin_user() {
    let paths = new_router_fixture("router-api-library-delete-forbidden-non-admin").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "non-admin-user",
        "non-admin@example.org",
        "router-contract-non-admin-123",
        18,
        &["USER"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "non-admin@example.org",
        "router-contract-non-admin-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("library delete forbidden request should build"),
        )
        .await
        .expect("library delete forbidden request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_delete_returns_not_found_for_missing_library() {
    let paths = new_router_fixture("router-api-library-delete-missing").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/libraries/missing-library")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("library delete missing request should build"),
        )
        .await
        .expect("library delete missing request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_delete_cascades_library_rows_like_kotlin() {
    let paths = new_router_fixture("router-api-library-delete-cascade").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("library delete cascade seed db should open");
    sqlx::query("INSERT INTO LIBRARY_EXCLUSIONS (LIBRARY_ID, EXCLUSION) VALUES (?, ?)")
        .bind("library-1")
        .bind("excluded-dir")
        .execute(&pool)
        .await
        .expect("library exclusion should be seeded");
    sqlx::query(
        "INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)",
    )
    .bind("books/book-1.xml")
    .bind("books/book-1.epub")
    .bind(1_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("library sidecar should be seeded");
    sqlx::query("INSERT INTO USER_LIBRARY_SHARING (USER_ID, LIBRARY_ID) VALUES (?, ?)")
        .bind("admin-user")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library sharing should be seeded");
    sqlx::query("INSERT INTO BOOK_METADATA_AGGREGATION_TAG (TAG, SERIES_ID) VALUES (?, ?)")
        .bind("agg-tag")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("aggregation tag should be seeded");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("library delete cascade request should build"),
        )
        .await
        .expect("library delete cascade request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        count_query_rows(
            &paths,
            "SELECT COUNT(*) AS COUNT FROM LIBRARY WHERE ID = ?",
            "library-1",
        )
        .await,
        0
    );
    assert_eq!(
        count_query_rows(
            &paths,
            "SELECT COUNT(*) AS COUNT FROM SERIES WHERE LIBRARY_ID = ?",
            "library-1",
        )
        .await,
        0
    );
    assert_eq!(
        count_query_rows(
            &paths,
            "SELECT COUNT(*) AS COUNT FROM BOOK WHERE LIBRARY_ID = ?",
            "library-1",
        )
        .await,
        0
    );
    assert_eq!(
        count_query_rows(
            &paths,
            "SELECT COUNT(*) AS COUNT FROM LIBRARY_EXCLUSIONS WHERE LIBRARY_ID = ?",
            "library-1",
        )
        .await,
        0
    );
    assert_eq!(
        count_query_rows(
            &paths,
            "SELECT COUNT(*) AS COUNT FROM SIDECAR WHERE LIBRARY_ID = ?",
            "library-1",
        )
        .await,
        0
    );
    assert_eq!(
        count_query_rows(
            &paths,
            "SELECT COUNT(*) AS COUNT FROM USER_LIBRARY_SHARING WHERE LIBRARY_ID = ?",
            "library-1",
        )
        .await,
        0
    );
    assert_eq!(
        count_query_rows(
            &paths,
            "SELECT COUNT(*) AS COUNT FROM BOOK_METADATA_AGGREGATION WHERE SERIES_ID = ?",
            "series-1",
        )
        .await,
        0
    );
    assert_eq!(
        count_query_rows(
            &paths,
            "SELECT COUNT(*) AS COUNT FROM BOOK_METADATA_AGGREGATION_AUTHOR WHERE SERIES_ID = ?",
            "series-1",
        )
        .await,
        0
    );
    assert_eq!(
        count_query_rows(
            &paths,
            "SELECT COUNT(*) AS COUNT FROM BOOK_METADATA_AGGREGATION_TAG WHERE SERIES_ID = ?",
            "series-1",
        )
        .await,
        0
    );
    assert_eq!(
        count_query_rows(
            &paths,
            "SELECT COUNT(*) AS COUNT FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?",
            "book-1",
        )
        .await,
        0
    );
    assert_eq!(
        count_query_rows(
            &paths,
            "SELECT COUNT(*) AS COUNT FROM READLIST_BOOK WHERE BOOK_ID = ?",
            "book-1",
        )
        .await,
        0
    );
    assert_eq!(
        count_query_rows(
            &paths,
            "SELECT COUNT(*) AS COUNT FROM COLLECTION_SERIES WHERE SERIES_ID = ?",
            "series-1",
        )
        .await,
        0
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_patch_rejects_blank_name() {
    let paths = new_router_fixture("router-api-library-patch-blank-name").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let patch_response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "name": "   " }).to_string()))
                .expect("library patch blank-name request should build"),
        )
        .await
        .expect("library patch blank-name request should complete");

    assert_eq!(patch_response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(patch_response).await;
    assert_eq!(
        payload,
        json!({
            "violations": [
                {
                    "fieldName": "name",
                    "message": "must not be blank"
                }
            ]
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_patch_rejects_blank_root() {
    let paths = new_router_fixture("router-api-library-patch-blank-root").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let patch_response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "root": "   " }).to_string()))
                .expect("library patch blank-root request should build"),
        )
        .await
        .expect("library patch blank-root request should complete");

    assert_eq!(patch_response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(patch_response).await;
    assert_eq!(
        payload,
        json!({
            "violations": [
                {
                    "fieldName": "root",
                    "message": "must not be blank"
                }
            ]
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_patch_rejects_multiple_blank_fields_with_kotlin_validation_payload() {
    let paths = new_router_fixture("router-api-library-patch-multiple-blank-fields").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let patch_response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "name": "   ", "root": "   " }).to_string(),
                ))
                .expect("library patch multiple-blank-fields request should build"),
        )
        .await
        .expect("library patch multiple-blank-fields request should complete");

    assert_eq!(patch_response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(patch_response).await;
    assert_eq!(
        payload,
        json!({
            "violations": [
                {
                    "fieldName": "root",
                    "message": "must not be blank"
                },
                {
                    "fieldName": "name",
                    "message": "must not be blank"
                }
            ]
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_book_metadata_route_sets_etag_and_supports_if_none_match() {
    let paths = new_router_fixture("router-kobo-book-metadata-cache-headers").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let first_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/book-1/metadata")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo metadata request should build"),
        )
        .await
        .expect("kobo metadata request should complete");

    assert_eq!(first_response.status(), StatusCode::OK);
    let etag = first_response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("kobo metadata response should include etag");

    let second_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/book-1/metadata")
                .header("x-auth-token", &auth_token)
                .header(header::IF_NONE_MATCH, etag)
                .body(Body::empty())
                .expect("conditional kobo metadata request should build"),
        )
        .await
        .expect("conditional kobo metadata request should complete");

    assert_eq!(second_response.status(), StatusCode::NOT_MODIFIED);
    assert!(second_response.headers().contains_key(header::ETAG));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_book_metadata_uses_persisted_fields_instead_of_placeholders() {
    let paths = new_router_fixture("router-kobo-book-metadata-parity").await;
    seed_router_contract_data(&paths).await;
    let kepubify_path = write_executable_fixture(&paths, "kepubify-ok.sh");
    upsert_server_setting(&paths, "KEPUBIFY_PATH", &kepubify_path).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract db should open for kobo metadata parity");
    sqlx::query("UPDATE BOOK_METADATA SET ISBN = ? WHERE BOOK_ID = ?")
        .bind("9781234567890")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book metadata isbn should be updated");
    sqlx::query("UPDATE MEDIA SET EPUB_IS_KEPUB = ? WHERE BOOK_ID = ?")
        .bind(false)
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book metadata epub is kepub should be updated");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/book-1/metadata")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo metadata parity request should build"),
        )
        .await
        .expect("kobo metadata parity request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let metadata = payload
        .as_array()
        .and_then(|items| items.first())
        .expect("kobo metadata response should contain one item");

    assert_eq!(metadata.get("Description"), Some(&json!(" ")));
    assert_eq!(metadata.get("Language"), Some(&json!("en")));
    assert_eq!(metadata.get("CoverImageId"), Some(&json!("thumb-book-1")));
    assert_eq!(metadata.get("ISBN"), Some(&json!("9781234567890")));
    assert_eq!(
        metadata.pointer("/Publisher/Name"),
        Some(&json!("PubHouse"))
    );
    assert_eq!(metadata.pointer("/Publisher/Imprint"), Some(&json!("")));
    assert_eq!(metadata.pointer("/Series/Id"), Some(&json!("series-1")));
    assert_eq!(metadata.pointer("/Series/Name"), Some(&json!("Series 1")));
    assert_eq!(metadata.pointer("/Series/Number"), Some(&json!("1")));
    assert_eq!(metadata.pointer("/Series/NumberFloat"), Some(&json!(1.0)));
    assert_eq!(metadata.get("Contributors"), Some(&json!(["Jane Writer"])));
    assert_eq!(
        metadata.pointer("/DownloadUrls/0/Format"),
        Some(&json!("KEPUB"))
    );
    assert_eq!(
        metadata.pointer("/DownloadUrls/0/Url"),
        Some(&json!(format!(
            "http://localhost:{}/kobo/any-token/v1/books/book-1/file/epub?convert_kepub=true",
            runtime_config_for_paths(&paths).bind_address.port()
        )))
    );
    assert_eq!(
        metadata.pointer("/ContributorRoles/0/Name"),
        Some(&json!("Jane Writer"))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_book_metadata_uses_epub3fl_for_fixed_layout_books() {
    let paths = new_router_fixture("router-kobo-book-metadata-fixed-layout").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract db should open for fixed-layout metadata parity");
    sqlx::query("UPDATE MEDIA SET EPUB_IS_KEPUB = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind(false)
        .bind(fixed_layout_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book metadata media extension should be updated");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/book-1/metadata")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo fixed-layout metadata request should build"),
        )
        .await
        .expect("kobo fixed-layout metadata request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let metadata = payload
        .as_array()
        .and_then(|items| items.first())
        .expect("fixed-layout metadata response should contain one item");
    assert_eq!(
        metadata.pointer("/DownloadUrls/0/Format"),
        Some(&json!("EPUB3FL"))
    );
    assert_eq!(
        metadata.pointer("/DownloadUrls/0/Url"),
        Some(&json!(format!(
            "http://localhost:{}/kobo/any-token/v1/books/book-1/file/epub?convert_kepub=false",
            runtime_config_for_paths(&paths).bind_address.port()
        )))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_book_metadata_uses_epub3_when_kepub_conversion_is_not_available() {
    let paths = new_router_fixture("router-kobo-book-metadata-epub3-fallback").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KEPUBIFY_PATH", "/definitely/missing/kepubify").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/book-1/metadata")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo epub3 fallback metadata request should build"),
        )
        .await
        .expect("kobo epub3 fallback metadata request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let metadata = payload
        .as_array()
        .and_then(|items| items.first())
        .expect("epub3 fallback metadata response should contain one item");
    assert_eq!(
        metadata.pointer("/DownloadUrls/0/Format"),
        Some(&json!("EPUB3"))
    );
    assert_eq!(
        metadata.pointer("/DownloadUrls/0/Url"),
        Some(&json!(format!(
            "http://localhost:{}/kobo/any-token/v1/books/book-1/file/epub?convert_kepub=false",
            runtime_config_for_paths(&paths).bind_address.port()
        )))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_book_metadata_returns_empty_array_when_book_is_missing_and_proxy_disabled() {
    let paths = new_router_fixture("router-kobo-book-metadata-missing-local").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/missing-book/metadata")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo missing local metadata request should build"),
        )
        .await
        .expect("kobo missing local metadata request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_json(response).await, json!([]));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_book_metadata_returns_empty_array_when_book_exists_but_metadata_row_is_missing()
 {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();

    let server = spawn_single_response_server(
        200,
        "application/json",
        r#"[{"Title":"Proxy Title","DownloadUrls":[{"Format":"EPUB3","Url":"https://proxy.example/book.epub"}]}]"#,
    )
    .await;
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-kobo-book-metadata-missing-metadata-row").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract db should open for missing metadata row");
    sqlx::query("DELETE FROM BOOK_METADATA WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book metadata row should be deleted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/book-1/metadata")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo missing metadata row request should build"),
        )
        .await
        .expect("kobo missing metadata row request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_json(response).await, json!([]));

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server.join.abort();
}

#[tokio::test]
async fn router_kobo_book_metadata_proxies_missing_books_when_proxy_enabled() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();

    let server = spawn_single_response_server(
        200,
        "application/json",
        r#"[{"Title":"Proxy Title","DownloadUrls":[{"Format":"EPUB3","Url":"https://proxy.example/book.epub"}]}]"#,
    )
    .await;
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-kobo-book-metadata-proxy-missing").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/missing-book/metadata")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo missing proxied metadata request should build"),
        )
        .await
        .expect("kobo missing proxied metadata request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!([{"Title":"Proxy Title","DownloadUrls":[{"Format":"EPUB3","Url":"https://proxy.example/book.epub"}]}])
    );

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server
        .join
        .await
        .expect("kobo missing proxied metadata server should finish");
}

#[tokio::test]
async fn router_api_libraries_head_reuses_get_etag_for_conditional_requests() {
    let paths = new_router_fixture("router-api-libraries-head-etag").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let get_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/libraries")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("libraries get request should build"),
        )
        .await
        .expect("libraries get request should complete");
    assert_eq!(get_response.status(), StatusCode::OK);
    let etag = get_response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("libraries get response should include etag");

    let head_response = app
        .oneshot(
            Request::builder()
                .method("HEAD")
                .uri("/api/v1/libraries")
                .header("x-auth-token", &auth_token)
                .header(header::IF_NONE_MATCH, etag)
                .body(Body::empty())
                .expect("libraries head request should build"),
        )
        .await
        .expect("libraries head request should complete");

    assert_eq!(head_response.status(), StatusCode::NOT_MODIFIED);

    cleanup_router_fixture(paths);
}

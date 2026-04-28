use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_infrastructure::sqlite::connect_test_pool;
use komga_server::app::build_router_with_config;
use serde_json::{Value, json};
use sqlx::Row;
use std::sync::{Mutex, OnceLock};
use tokio::sync::Mutex as AsyncMutex;
use tower::util::ServiceExt;

mod support;

use support::runtime_router_contract_support::{
    RuntimeDbPaths, contract_seed::*, external_service_support::*, fixture_bootstrap::*,
    log_capture::*, media_file_fixtures::*, response_helpers::*, user_auth::*,
};

mod auth_session_contract_cases;
use auth_session_contract_cases::{kobo_and_session_basics, koreader_activity_syncpoints};

#[test]
fn auth_session_contract_target_is_registered() {
    assert_required_target_declared("auth/session", "auth_session_contract");
}

#[tokio::test]
async fn remember_me_reauthenticates_after_session_expiry() {
    kobo_and_session_basics::remember_me_and_logout::verify_remember_me_reauthenticates_after_session_expiry().await;
}

#[tokio::test]
async fn remember_me_duration_setting_requires_restart_before_cookie_ttl_changes() {
    kobo_and_session_basics::remember_me_and_logout::verify_remember_me_duration_setting_requires_restart_before_cookie_ttl_changes().await;
}

#[tokio::test]
async fn remember_me_cold_start_uses_persisted_runtime_settings() {
    kobo_and_session_basics::remember_me_and_logout::verify_remember_me_cold_start_uses_persisted_runtime_settings().await;
}

#[tokio::test]
async fn existing_session_when_exchanging_for_cookies_then_session_is_returned_in_cookies() {
    kobo_and_session_basics::claims_and_session::verify_login_set_cookie_returns_session_cookie_for_header_session().await;
}

#[tokio::test]
async fn api_key_login_records_apikey_source_after_auth_refactor() {
    koreader_activity_syncpoints::authentication_activity::verify_api_key_login_records_apikey_source_after_auth_refactor().await;
}

#[tokio::test]
async fn remember_me_auto_login_records_remember_me_source() {
    kobo_and_session_basics::remember_me_and_logout::verify_remember_me_auto_login_records_remember_me_source().await;
}

#[tokio::test]
async fn oauth2_callback_reuses_komga_session_cookie_after_in_memory_session_refactor() {
    kobo_and_session_basics::oauth2::verify_oauth2_callback_success_uses_session_cookie_without_auth_token_header().await;
}

#[tokio::test]
async fn admin_user_update_expires_sessions_and_emits_session_expired_event() {
    koreader_activity_syncpoints::remember_me_lifecycle::verify_admin_user_update_expires_sessions_and_emits_session_expired_event().await;
}

#[tokio::test]
async fn rotating_remember_me_key_requires_restart_before_existing_cookie_is_invalidated() {
    kobo_and_session_basics::remember_me_and_logout::verify_rotating_remember_me_key_requires_restart_before_existing_cookie_is_invalidated().await;
}

#[tokio::test]
async fn server_context_path_mounts_api_routes_under_runtime_prefix() {
    let paths = new_router_fixture("router-server-context-path-mounts-api").await;
    let mut config = runtime_config_for_paths(&paths);
    config.server_context_path = Some("/komga".to_string());

    let app = build_router_with_config(&config).await;

    let prefixed_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/komga/api/v1/settings")
                .body(Body::empty())
                .expect("prefixed settings request should build"),
        )
        .await
        .expect("prefixed settings request should complete");
    assert_eq!(prefixed_response.status(), StatusCode::UNAUTHORIZED);

    let bare_response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/settings")
                .body(Body::empty())
                .expect("bare settings request should build"),
        )
        .await
        .expect("bare settings request should complete");
    assert_eq!(bare_response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn password_change_invalidates_existing_remember_me_cookie() {
    koreader_activity_syncpoints::remember_me_lifecycle::verify_password_change_invalidates_existing_remember_me_cookie().await;
}

#[tokio::test]
async fn self_password_change_keeps_session_but_invalidates_old_remember_me() {
    koreader_activity_syncpoints::remember_me_lifecycle::verify_self_password_change_keeps_session_but_invalidates_old_remember_me().await;
}

async fn seed_syncpoint_user(paths: &RuntimeDbPaths, user_id: &str, email: &str) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("syncpoint user db should open");

    sqlx::query(
        "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES) \
         VALUES (?, ?, '', ?)",
    )
    .bind(user_id)
    .bind(email)
    .bind(true)
    .execute(&pool)
    .await
    .expect("syncpoint test user should be inserted");

    pool.close().await;
}

async fn seed_global_client_setting(
    paths: &RuntimeDbPaths,
    key: &str,
    value: &str,
    allow_unauthorized: bool,
) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("global client settings db should open");

    sqlx::query(
        "INSERT INTO CLIENT_SETTINGS_GLOBAL (KEY, VALUE, ALLOW_UNAUTHORIZED) VALUES (?, ?, ?)",
    )
    .bind(key)
    .bind(value)
    .bind(allow_unauthorized)
    .execute(&pool)
    .await
    .expect("global client setting row should be inserted");

    pool.close().await;
}

async fn load_announcement_read_ids_for_user(paths: &RuntimeDbPaths, user_id: &str) -> Vec<String> {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("announcements read query db should open");

    let rows = sqlx::query(
        "SELECT ANNOUNCEMENT_ID FROM ANNOUNCEMENTS_READ WHERE USER_ID = ? ORDER BY ANNOUNCEMENT_ID",
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .expect("announcement read ids should load");
    pool.close().await;

    rows.into_iter()
        .map(|row| row.get::<String, _>("ANNOUNCEMENT_ID"))
        .collect()
}

fn releases_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn announcements_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn auth_session_runtime_env_lock() -> &'static AsyncMutex<()> {
    static LOCK: OnceLock<AsyncMutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| AsyncMutex::new(()))
}

async fn seed_announcement_read_ids(paths: &RuntimeDbPaths, user_id: &str, ids: &[&str]) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("announcements seed db should open");

    for id in ids {
        sqlx::query("INSERT INTO ANNOUNCEMENTS_READ (USER_ID, ANNOUNCEMENT_ID) VALUES (?, ?)")
            .bind(user_id)
            .bind(id)
            .execute(&pool)
            .await
            .expect("announcement read row should be inserted");
    }

    pool.close().await;
}

async fn seed_syncpoints(paths: &RuntimeDbPaths, rows: &[(&str, &str, Option<&str>)]) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("syncpoint db should open");

    for (id, user_id, key_id) in rows {
        sqlx::query("INSERT INTO SYNC_POINT (ID, USER_ID, API_KEY_ID) VALUES (?, ?, ?)")
            .bind(id)
            .bind(user_id)
            .bind(key_id)
            .execute(&pool)
            .await
            .expect("syncpoint row should be inserted");
    }

    pool.close().await;
}

async fn seed_syncpoint_children(paths: &RuntimeDbPaths, sync_point_id: &str) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("syncpoint child db should open");

    sqlx::query(
        "INSERT INTO SYNC_POINT_BOOK (SYNC_POINT_ID, BOOK_ID, BOOK_CREATED_DATE, BOOK_LAST_MODIFIED_DATE, BOOK_FILE_LAST_MODIFIED, BOOK_FILE_SIZE, BOOK_FILE_HASH, BOOK_METADATA_LAST_MODIFIED_DATE, SYNCED) \
         VALUES (?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 1, ?, CURRENT_TIMESTAMP, 0)",
    )
    .bind(sync_point_id)
    .bind(format!("book-{sync_point_id}"))
    .bind(format!("hash-{sync_point_id}"))
    .execute(&pool)
    .await
    .expect("syncpoint book row should be inserted");

    sqlx::query(
        "INSERT INTO SYNC_POINT_BOOK_REMOVED_SYNCED (SYNC_POINT_ID, BOOK_ID) VALUES (?, ?)",
    )
    .bind(sync_point_id)
    .bind(format!("removed-book-{sync_point_id}"))
    .execute(&pool)
    .await
    .expect("syncpoint removed-book row should be inserted");

    sqlx::query(
        "INSERT INTO SYNC_POINT_READLIST (SYNC_POINT_ID, READLIST_ID, READLIST_NAME, READLIST_CREATED_DATE, READLIST_LAST_MODIFIED_DATE, SYNCED) \
         VALUES (?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 0)",
    )
    .bind(sync_point_id)
    .bind(format!("readlist-{sync_point_id}"))
    .bind(format!("Readlist {sync_point_id}"))
    .execute(&pool)
    .await
    .expect("syncpoint readlist row should be inserted");

    sqlx::query(
        "INSERT INTO SYNC_POINT_READLIST_BOOK (SYNC_POINT_ID, READLIST_ID, BOOK_ID) VALUES (?, ?, ?)",
    )
    .bind(sync_point_id)
    .bind(format!("readlist-{sync_point_id}"))
    .bind(format!("book-{sync_point_id}"))
    .execute(&pool)
    .await
    .expect("syncpoint readlist book row should be inserted");

    sqlx::query(
        "INSERT INTO SYNC_POINT_READLIST_REMOVED_SYNCED (SYNC_POINT_ID, READLIST_ID) VALUES (?, ?)",
    )
    .bind(sync_point_id)
    .bind(format!("removed-readlist-{sync_point_id}"))
    .execute(&pool)
    .await
    .expect("syncpoint removed-readlist row should be inserted");

    pool.close().await;
}

async fn load_syncpoint_child_counts(paths: &RuntimeDbPaths, sync_point_id: &str) -> [i64; 5] {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("syncpoint child count db should open");

    let counts = [
        load_syncpoint_child_count(&pool, "SYNC_POINT_BOOK", sync_point_id).await,
        load_syncpoint_child_count(&pool, "SYNC_POINT_BOOK_REMOVED_SYNCED", sync_point_id).await,
        load_syncpoint_child_count(&pool, "SYNC_POINT_READLIST", sync_point_id).await,
        load_syncpoint_child_count(&pool, "SYNC_POINT_READLIST_BOOK", sync_point_id).await,
        load_syncpoint_child_count(&pool, "SYNC_POINT_READLIST_REMOVED_SYNCED", sync_point_id)
            .await,
    ];

    pool.close().await;
    counts
}

async fn load_syncpoint_child_count(
    pool: &sqlx::SqlitePool,
    table: &str,
    sync_point_id: &str,
) -> i64 {
    let sql = format!("SELECT COUNT(*) AS COUNT FROM {table} WHERE SYNC_POINT_ID = ?");
    sqlx::query(&sql)
        .bind(sync_point_id)
        .fetch_one(pool)
        .await
        .expect("syncpoint child count should load")
        .get::<i64, _>("COUNT")
}

async fn load_syncpoint_ids(paths: &RuntimeDbPaths) -> Vec<String> {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("syncpoint query db should open");

    let rows = sqlx::query("SELECT ID FROM SYNC_POINT ORDER BY ID")
        .fetch_all(&pool)
        .await
        .expect("syncpoint rows should load");
    pool.close().await;

    rows.into_iter()
        .map(|row| row.get::<String, _>("ID"))
        .collect()
}

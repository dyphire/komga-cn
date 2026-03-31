use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::infrastructure::sqlite::connect_pool;
use komga_server::app::build_router_with_config;
use serde_json::Value;
use sqlx::Row;
use tower::util::ServiceExt;

#[path = "support/runtime_router_contract_support.rs"]
mod runtime_router_contract_support;

use runtime_router_contract_support::*;

#[test]
fn auth_session_contract_target_is_registered() {
    assert_required_target_declared("auth/session", "auth_session_contract");
}

#[tokio::test]
async fn router_kobo_initialization_returns_scoped_api_token_header() {
    let paths = new_router_fixture("router-kobo-initialization-api-token").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/initialization")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo initialization request should build"),
        )
        .await
        .expect("kobo initialization request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let api_token = response
        .headers()
        .get("x-kobo-apitoken")
        .and_then(|value| value.to_str().ok())
        .expect("kobo initialization response should include x-kobo-apitoken");
    assert!(api_token.starts_with("KOMGA."));
    assert_ne!(api_token, "e30=");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_root_exposed_by_default_without_beans_link() {
    let paths = new_router_fixture("router-actuator-root-omits-beans-link").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("actuator root request should build"),
        )
        .await
        .expect("actuator root request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let links = payload
        .get("_links")
        .and_then(Value::as_object)
        .expect("actuator root should include links object");
    assert!(links.get("beans").is_none());

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_shutdown_requires_admin_authentication() {
    let paths = new_router_fixture("router-actuator-shutdown-auth").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/actuator/shutdown")
                .body(Body::empty())
                .expect("actuator shutdown request should build"),
        )
        .await
        .expect("actuator shutdown request should complete");

    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("actuator shutdown response body should be readable");
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "unexpected actuator shutdown status={status}, body={}",
        String::from_utf8_lossy(&body),
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_delete_syncpoints_me_without_key_id_deletes_all_syncpoints_for_current_user() {
    let paths = new_router_fixture("router-delete-syncpoints-me-all").await;
    seed_router_contract_data(&paths).await;
    seed_syncpoint_user(&paths, "other-user", "other@example.org").await;
    seed_syncpoints(
        &paths,
        &[
            ("sp-1", "admin-user", Some("key-1")),
            ("sp-2", "admin-user", Some("key-2")),
            ("sp-3", "admin-user", None),
            ("sp-4", "other-user", Some("key-1")),
        ],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/syncpoints/me")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("syncpoints delete-all request should build"),
        )
        .await
        .expect("syncpoints delete-all request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(load_syncpoint_ids(&paths).await, vec!["sp-4".to_string()]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_delete_syncpoints_me_with_repeated_key_id_deletes_only_matching_keys() {
    let paths = new_router_fixture("router-delete-syncpoints-me-many-keys").await;
    seed_router_contract_data(&paths).await;
    seed_syncpoint_user(&paths, "other-user", "other@example.org").await;
    seed_syncpoints(
        &paths,
        &[
            ("sp-1", "admin-user", Some("key-1")),
            ("sp-2", "admin-user", Some("key-2")),
            ("sp-3", "admin-user", Some("key-3")),
            ("sp-4", "other-user", Some("key-1")),
        ],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/syncpoints/me?key_id=key-1&key_id=key-3")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("syncpoints delete-many request should build"),
        )
        .await
        .expect("syncpoints delete-many request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        load_syncpoint_ids(&paths).await,
        vec!["sp-2".to_string(), "sp-4".to_string()],
    );

    cleanup_router_fixture(paths);
}

async fn seed_syncpoint_user(paths: &RuntimeDbPaths, user_id: &str, email: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
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

async fn seed_syncpoints(paths: &RuntimeDbPaths, rows: &[(&str, &str, Option<&str>)]) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
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

async fn load_syncpoint_ids(paths: &RuntimeDbPaths) -> Vec<String> {
    let pool = connect_pool(paths.main_db.as_path(), 1)
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

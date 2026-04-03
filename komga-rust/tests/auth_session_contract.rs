use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::infrastructure::sqlite::connect_pool;
use komga_server::app::build_router_with_config;
use serde_json::{Value, json};
use sha2::{Digest, Sha512};
use sqlx::Row;
use std::sync::{Mutex, OnceLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
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
async fn router_kobo_ping_does_not_accept_web_auth_fallback_when_path_token_is_invalid() {
    let paths = new_router_fixture("router-kobo-ping-path-token-only-auth").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/invalid-token/ping")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo ping request should build"),
        )
        .await
        .expect("kobo ping request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_ping_rejects_path_tokens_with_characters_outside_kotlin_regex() {
    let paths = new_router_fixture("router-kobo-ping-token-char-constraint").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-user",
        "kobo@example.org",
        "router-contract-kobo-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "bad.token", "kobo-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/bad.token/ping")
                .body(Body::empty())
                .expect("kobo ping constrained token request should build"),
        )
        .await
        .expect("kobo ping constrained token request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_ping_returns_forbidden_for_valid_token_without_kobo_sync_role() {
    let paths = new_router_fixture("router-kobo-ping-forbidden-without-kobo-role").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "plain-user",
        "plain@example.org",
        "router-contract-plain-123",
        0,
        &["USER"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "plain-kobo-token", "plain-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/plain-kobo-token/ping")
                .body(Body::empty())
                .expect("kobo ping forbidden request should build"),
        )
        .await
        .expect("kobo ping forbidden request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_catch_all_returns_internal_error_for_non_json_upstream_body() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();

    let server = spawn_single_response_server(200, "text/plain", "plain-text-body").await;
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-kobo-catch-all-non-json-body").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-proxy-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/validkobotoken/v1/test")
                .body(Body::empty())
                .expect("kobo catch-all request should build"),
        )
        .await
        .expect("kobo catch-all request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server
        .join
        .await
        .expect("kobo proxy mock server should finish");
}

#[tokio::test]
async fn router_kobo_catch_all_preserves_non_success_status_for_non_json_upstream_body() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();

    let server = spawn_single_response_server(503, "text/plain", "upstream error text").await;
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-kobo-catch-all-non-json-error-body").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-proxy-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/validkobotoken/v1/test")
                .body(Body::empty())
                .expect("kobo catch-all non-success request should build"),
        )
        .await
        .expect("kobo catch-all non-success request should complete");

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server
        .join
        .await
        .expect("kobo proxy non-success mock server should finish");
}

#[tokio::test]
async fn router_kobo_catch_all_does_not_passthrough_error_body_or_kobo_headers() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();

    let server = spawn_single_response_server_with_headers(
        503,
        "text/plain",
        "upstream error text",
        &[("x-kobo-test", "1")],
    )
    .await;
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-kobo-catch-all-no-error-body-passthrough").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-proxy-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/validkobotoken/v1/test")
                .body(Body::empty())
                .expect("kobo catch-all no passthrough request should build"),
        )
        .await
        .expect("kobo catch-all no passthrough request should complete");

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(response.headers().get("x-kobo-test").is_none());
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("kobo catch-all error body should be readable");
    assert!(body.is_empty());

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server
        .join
        .await
        .expect("kobo proxy no-passthrough mock server should finish");
}

#[tokio::test]
async fn router_kobo_catch_all_does_not_passthrough_json_error_body_or_kobo_headers() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();

    let server = spawn_single_response_server_with_headers(
        503,
        "application/json",
        r#"{"error":"upstream-failure"}"#,
        &[("x-kobo-test", "1")],
    )
    .await;
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-kobo-catch-all-no-json-error-passthrough").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-proxy-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/validkobotoken/v1/test")
                .body(Body::empty())
                .expect("kobo catch-all json error request should build"),
        )
        .await
        .expect("kobo catch-all json error request should complete");

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(response.headers().get("x-kobo-test").is_none());
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("kobo catch-all json error body should be readable");
    assert!(body.is_empty());

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server
        .join
        .await
        .expect("kobo proxy json-error mock server should finish");
}

#[tokio::test]
async fn router_kobo_catch_all_returns_internal_error_for_transport_failure() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", "http://127.0.0.1:1");
    }

    let paths = new_router_fixture("router-kobo-catch-all-transport-failure").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-proxy-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/validkobotoken/v1/test")
                .body(Body::empty())
                .expect("kobo catch-all transport failure request should build"),
        )
        .await
        .expect("kobo catch-all transport failure request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
}

#[tokio::test]
async fn router_kobo_catch_all_preserves_success_status_for_empty_body() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();

    let server = spawn_single_response_server(204, "application/json", "").await;
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-kobo-catch-all-empty-success-body").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-proxy-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/validkobotoken/v1/test")
                .body(Body::empty())
                .expect("kobo catch-all empty success request should build"),
        )
        .await
        .expect("kobo catch-all empty success request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server
        .join
        .await
        .expect("kobo proxy empty-success mock server should finish");
}

#[tokio::test]
async fn router_kobo_catch_all_put_returns_bad_request_for_invalid_json_body() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", "http://127.0.0.1:1");
    }

    let paths = new_router_fixture("router-kobo-catch-all-put-invalid-json-body").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-proxy-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/kobo/validkobotoken/v1/test")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"broken": }"#))
                .expect("kobo catch-all invalid json put request should build"),
        )
        .await
        .expect("kobo catch-all invalid json put request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
}

#[tokio::test]
async fn router_kobo_catch_all_put_returns_unsupported_media_type_for_text_plain_body() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", "http://127.0.0.1:1");
    }

    let paths = new_router_fixture("router-kobo-catch-all-put-text-plain-body").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-proxy-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/kobo/validkobotoken/v1/test")
                .header(header::CONTENT_TYPE, "text/plain")
                .body(Body::from("plain-text-body"))
                .expect("kobo catch-all text/plain put request should build"),
        )
        .await
        .expect("kobo catch-all text/plain put request should complete");

    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
}

#[tokio::test]
async fn router_kobo_catch_all_put_returns_bad_request_for_malformed_xml_body() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", "http://127.0.0.1:1");
    }

    let paths = new_router_fixture("router-kobo-catch-all-put-malformed-xml-body").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-proxy-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/kobo/validkobotoken/v1/test")
                .header(header::CONTENT_TYPE, "application/xml")
                .body(Body::from("<root><broken></root>"))
                .expect("kobo catch-all malformed xml put request should build"),
        )
        .await
        .expect("kobo catch-all malformed xml put request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
}

#[tokio::test]
async fn router_kobo_catch_all_put_reserializes_json_request_body_before_proxying() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();

    let server = spawn_request_body_echo_server().await;
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-kobo-catch-all-put-json-reserialize").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-proxy-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/kobo/validkobotoken/v1/test")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{\n  \"key\" : 1,\n  \"items\" : [ 2, 3 ]\n}"))
                .expect("kobo catch-all json reserialize put request should build"),
        )
        .await
        .expect("kobo catch-all json reserialize put request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let received = payload
        .get("received")
        .and_then(Value::as_str)
        .expect("kobo catch-all echo response should include received body");
    assert!(!received.contains(' '));
    assert!(!received.contains('\n'));
    let reparsed: Value = serde_json::from_str(received)
        .expect("kobo catch-all echoed request body should remain valid json");
    assert_eq!(reparsed, json!({"key":1,"items":[2,3]}));

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server
        .join
        .await
        .expect("kobo proxy request-body echo server should finish");
}

#[tokio::test]
async fn router_koreader_user_create_returns_unauthorized_for_invalid_x_auth_user() {
    let paths = new_router_fixture("router-koreader-user-create-invalid-auth-header").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/koreader/users/create")
                .header("x-auth-user", "invalid-api-key")
                .body(Body::empty())
                .expect("koreader users create request should build"),
        )
        .await
        .expect("koreader users create request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_koreader_user_create_ignores_invalid_x_api_key_for_koreader_auth() {
    let paths = new_router_fixture("router-koreader-user-create-invalid-x-api-key").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/koreader/users/create")
                .header("x-api-key", "invalid-api-key")
                .body(Body::empty())
                .expect("koreader users create x-api-key request should build"),
        )
        .await
        .expect("koreader users create x-api-key request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

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

#[tokio::test]
async fn router_put_announcements_deduplicates_duplicate_ids() {
    let paths = new_router_fixture("router-put-announcements-deduplicates-ids").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/announcements")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"["announcement-1","announcement-1","announcement-2"]"#,
                ))
                .expect("put announcements duplicate ids request should build"),
        )
        .await
        .expect("put announcements duplicate ids request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        load_announcement_read_ids_for_user(&paths, "admin-user").await,
        vec!["announcement-1".to_string(), "announcement-2".to_string()]
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_releases_returns_internal_error_when_upstream_fetch_fails() {
    let _guard = releases_env_lock()
        .lock()
        .expect("releases env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_RELEASES_URL").ok();
    unsafe {
        std::env::set_var("KOMGA_RUST_RELEASES_URL", "http://127.0.0.1:1/releases");
    }

    let paths = new_router_fixture("router-get-releases-upstream-failure").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/releases")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("get releases request should build"),
        )
        .await
        .expect("get releases request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_RELEASES_URL", previous);
}

#[tokio::test]
async fn router_get_releases_returns_internal_error_for_non_array_payload() {
    let _guard = releases_env_lock()
        .lock()
        .expect("releases env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_RELEASES_URL").ok();

    let server =
        spawn_single_response_server(200, "application/json", r#"{"tag_name":"v1.0.0"}"#).await;
    unsafe {
        std::env::set_var("KOMGA_RUST_RELEASES_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-get-releases-non-array-payload").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/releases")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("get releases non-array request should build"),
        )
        .await
        .expect("get releases non-array request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_RELEASES_URL", previous);
    server
        .join
        .await
        .expect("releases non-array mock server should finish");
}

#[tokio::test]
async fn router_get_releases_returns_internal_error_for_non_success_status_with_valid_array_body() {
    let _guard = releases_env_lock()
        .lock()
        .expect("releases env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_RELEASES_URL").ok();

    let server = spawn_single_response_server(
        503,
        "application/json",
        r#"[{"html_url":"https://example.com/release/1","tag_name":"v1.0.0","published_at":"2024-01-01T00:00:00Z","body":"desc","prerelease":false}]"#,
    )
    .await;
    unsafe {
        std::env::set_var("KOMGA_RUST_RELEASES_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-get-releases-non-success-valid-array").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/releases")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("get releases non-success valid array request should build"),
        )
        .await
        .expect("get releases non-success valid array request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_RELEASES_URL", previous);
    server
        .join
        .await
        .expect("releases non-success valid-array mock server should finish");
}

#[tokio::test]
async fn router_get_announcements_returns_internal_error_when_upstream_fetch_fails() {
    let _guard = announcements_env_lock()
        .lock()
        .expect("announcements env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_ANNOUNCEMENTS_URL").ok();
    unsafe {
        std::env::set_var(
            "KOMGA_RUST_ANNOUNCEMENTS_URL",
            "http://127.0.0.1:1/feed.json",
        );
    }

    let paths = new_router_fixture("router-get-announcements-upstream-failure").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/announcements")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("get announcements request should build"),
        )
        .await
        .expect("get announcements request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_ANNOUNCEMENTS_URL", previous);
}

#[tokio::test]
async fn router_get_announcements_does_not_passthrough_unknown_feed_fields() {
    let _guard = announcements_env_lock()
        .lock()
        .expect("announcements env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_ANNOUNCEMENTS_URL").ok();

    let server = spawn_single_response_server(
        200,
        "application/json",
        r#"{"version":"https://jsonfeed.org/version/1.1","title":"Komga News","home_page_url":"https://komga.org","description":"News","unexpected":"keep-me-out","items":[{"id":"announcement-1","url":"https://komga.org/post/1","title":"Hello","summary":"Summary","content_html":"<p>Hi</p>","date_modified":"2024-01-01T00:00:00Z","author":{"name":"Komga","url":"https://komga.org"},"tags":["news"],"unexpectedItemField":true}]}"#,
    )
    .await;
    unsafe {
        std::env::set_var("KOMGA_RUST_ANNOUNCEMENTS_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-get-announcements-known-dto-fields-only").await;
    seed_router_contract_data(&paths).await;
    seed_announcement_read_ids(&paths, "admin-user", &["announcement-1"]).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/announcements")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("get announcements dto request should build"),
        )
        .await
        .expect("get announcements dto request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert!(payload.get("unexpected").is_none());
    let items = payload["items"]
        .as_array()
        .expect("announcements items should be an array");
    assert!(items[0].get("unexpectedItemField").is_none());
    assert_eq!(items[0]["date_modified"], "2024-01-01T00:00:00Z");
    assert_eq!(items[0]["_komga"]["read"], true);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_ANNOUNCEMENTS_URL", previous);
    server
        .join
        .await
        .expect("announcement mock server should finish");
}

#[tokio::test]
async fn router_get_announcements_returns_not_found_for_null_body_payload() {
    let _guard = announcements_env_lock()
        .lock()
        .expect("announcements env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_ANNOUNCEMENTS_URL").ok();

    let server = spawn_single_response_server(200, "application/json", "null").await;
    unsafe {
        std::env::set_var("KOMGA_RUST_ANNOUNCEMENTS_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-get-announcements-null-body").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/announcements")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("get announcements null body request should build"),
        )
        .await
        .expect("get announcements null body request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_ANNOUNCEMENTS_URL", previous);
    server
        .join
        .await
        .expect("announcement null-body mock server should finish");
}

#[tokio::test]
async fn router_get_announcements_returns_internal_error_for_invalid_date_modified() {
    let _guard = announcements_env_lock()
        .lock()
        .expect("announcements env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_ANNOUNCEMENTS_URL").ok();

    let server = spawn_single_response_server(
        200,
        "application/json",
        r#"{"version":"https://jsonfeed.org/version/1.1","title":"Komga News","items":[{"id":"announcement-1","date_modified":"not-a-date"}]}"#,
    )
    .await;
    unsafe {
        std::env::set_var("KOMGA_RUST_ANNOUNCEMENTS_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-get-announcements-invalid-date-modified").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/announcements")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("get announcements invalid date request should build"),
        )
        .await
        .expect("get announcements invalid date request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_ANNOUNCEMENTS_URL", previous);
    server
        .join
        .await
        .expect("announcement invalid-date mock server should finish");
}

#[tokio::test]
async fn router_get_announcements_returns_internal_error_for_non_success_upstream_status() {
    let _guard = announcements_env_lock()
        .lock()
        .expect("announcements env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_ANNOUNCEMENTS_URL").ok();

    let server = spawn_single_response_server(
        503,
        "application/json",
        r#"{"version":"https://jsonfeed.org/version/1.1","title":"Komga News","items":[]}"#,
    )
    .await;
    unsafe {
        std::env::set_var("KOMGA_RUST_ANNOUNCEMENTS_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-get-announcements-non-success-status").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/announcements")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("get announcements non-success request should build"),
        )
        .await
        .expect("get announcements non-success request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_ANNOUNCEMENTS_URL", previous);
    server
        .join
        .await
        .expect("announcement non-success mock server should finish");
}

#[tokio::test]
async fn router_client_settings_global_list_does_not_inject_missing_oauth_hide_login_default() {
    let paths = new_router_fixture("router-client-settings-global-list-no-synthetic-default").await;
    seed_router_contract_data(&paths).await;
    seed_global_client_setting(&paths, "public.setting", "public-value", true).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/client-settings/global/list")
                .body(Body::empty())
                .expect("client settings global list request should build"),
        )
        .await
        .expect("client settings global list request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let settings = payload
        .as_object()
        .expect("global client settings response should be an object");
    assert_eq!(settings["public.setting"]["value"], "public-value");
    assert!(settings.get("webui.oauth2.hide_login").is_none());

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_put_page_hash_normalizes_negative_size_to_null() {
    let paths = new_router_fixture("router-put-page-hash-negative-size-null").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"hash":"negative-size-hash","size":-1,"action":"IGNORE"}"#,
                ))
                .expect("page hash put request should build"),
        )
        .await
        .expect("page hash put request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        load_page_hash_size(&paths, "negative-size-hash").await,
        None
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_put_page_hash_preserves_whitespace_padded_hash() {
    let paths = new_router_fixture("router-put-page-hash-whitespace-hash").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"hash":" negative-size-hash ","size":1,"action":"IGNORE"}"#,
                ))
                .expect("page hash put request with padded hash should build"),
        )
        .await
        .expect("page hash put request with padded hash should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        load_page_hash_size(&paths, " negative-size-hash ").await,
        Some(1)
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_put_page_hash_rejects_blank_only_hash() {
    let paths = new_router_fixture("router-put-page-hash-blank-only-hash").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"hash":"   ","size":1,"action":"IGNORE"}"#))
                .expect("page hash put request with blank-only hash should build"),
        )
        .await
        .expect("page hash put request with blank-only hash should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_put_page_hash_rejects_whitespace_padded_action() {
    let paths = new_router_fixture("router-put-page-hash-whitespace-action").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"hash":"negative-size-hash","size":1,"action":" IGNORE "}"#,
                ))
                .expect("page hash put request with whitespace action should build"),
        )
        .await
        .expect("page hash put request with whitespace action should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_put_page_hash_rejects_non_integer_size_values() {
    let paths = new_router_fixture("router-put-page-hash-non-integer-size").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"hash":"typed-size-hash","size":true,"action":"IGNORE"}"#,
                ))
                .expect("page hash put request with non-integer size should build"),
        )
        .await
        .expect("page hash put request with non-integer size should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        load_page_hash_record(&paths, "typed-size-hash")
            .await
            .is_none()
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_put_page_hash_preserves_existing_size_on_update() {
    let paths = new_router_fixture("router-put-page-hash-preserve-existing-size").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_row(&paths, "existing-size-hash", Some(5), "IGNORE").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"hash":"existing-size-hash","size":99,"action":"DELETE_AUTO"}"#,
                ))
                .expect("page hash update request should build"),
        )
        .await
        .expect("page hash update request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        load_page_hash_record(&paths, "existing-size-hash").await,
        Some((Some(5), "DELETE_AUTO".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_put_page_hash_persists_known_thumbnail_so_it_survives_source_removal() {
    let paths = new_router_fixture("router-put-page-hash-persists-thumbnail").await;
    seed_router_contract_data(&paths).await;
    let source_path = seed_page_hash_image_source(
        &paths,
        "book-page-hash-thumb",
        "known-thumb-hash",
        "images/known-thumb-source.png",
        "known-thumb-source.png",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"hash":"known-thumb-hash","size":64,"action":"IGNORE"}"#,
                ))
                .expect("page hash put request for thumbnail persistence should build"),
        )
        .await
        .expect("page hash put request for thumbnail persistence should complete");

    assert_eq!(put_response.status(), StatusCode::ACCEPTED);
    std::fs::remove_file(&source_path).expect("source image should be removable after put");

    let thumbnail_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/known-thumb-hash/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("known page hash thumbnail request should build"),
        )
        .await
        .expect("known page hash thumbnail request should complete");

    assert_eq!(thumbnail_response.status(), StatusCode::OK);
    assert_eq!(
        thumbnail_response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    let body = to_bytes(thumbnail_response.into_body(), usize::MAX)
        .await
        .expect("known page hash thumbnail response body should be readable");
    assert!(body.starts_with(&[0xFF, 0xD8]));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hashes_unknown_marks_unsorted_when_no_sort_query_is_present() {
    let paths = new_router_fixture("router-page-hashes-unknown-unsorted-flag").await;
    seed_router_contract_data(&paths).await;
    seed_unknown_page_hash_samples(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/unknown")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("unknown page hashes request should build"),
        )
        .await
        .expect("unknown page hashes request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload["sort"]["sorted"], false);
    assert_eq!(payload["sort"]["unsorted"], true);
    assert_eq!(payload["pageable"]["sort"]["sorted"], false);
    assert_eq!(payload["pageable"]["sort"]["unsorted"], true);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hashes_unknown_honors_hash_desc_sort_query() {
    let paths = new_router_fixture("router-page-hashes-unknown-hash-desc-sort").await;
    seed_router_contract_data(&paths).await;
    seed_unknown_page_hash_samples(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/unknown?sort=hash,desc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("sorted unknown page hashes request should build"),
        )
        .await
        .expect("sorted unknown page hashes request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("unknown page hashes content should be an array");
    let hashes = content
        .iter()
        .map(|entry| {
            entry["hash"]
                .as_str()
                .expect("page hash unknown entry should contain hash")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(hashes, vec!["z-hash".to_string(), "a-hash".to_string()]);
    assert_eq!(payload["sort"]["sorted"], true);
    assert_eq!(payload["sort"]["unsorted"], false);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_matches_marks_unsorted_when_no_sort_query_is_present() {
    let paths = new_router_fixture("router-page-hash-matches-unsorted-flag").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_match_samples(&paths, "match-sort-hash").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("page hash matches request should build"),
        )
        .await
        .expect("page hash matches request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload["sort"]["sorted"], false);
    assert_eq!(payload["sort"]["unsorted"], true);
    assert_eq!(payload["pageable"]["sort"]["sorted"], false);
    assert_eq!(payload["pageable"]["sort"]["unsorted"], true);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_matches_honors_page_number_desc_sort_query() {
    let paths = new_router_fixture("router-page-hash-matches-page-number-desc").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_match_samples(&paths, "match-sort-hash").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash?sort=pageNumber,desc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("sorted page hash matches request should build"),
        )
        .await
        .expect("sorted page hash matches request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("page hash matches content should be an array");
    let page_numbers = content
        .iter()
        .map(|entry| {
            entry["pageNumber"]
                .as_i64()
                .expect("page hash match entry should contain page number")
        })
        .collect::<Vec<_>>();
    assert_eq!(page_numbers, vec![5, 3, 1]);
    assert_eq!(payload["sort"]["sorted"], true);
    assert_eq!(payload["sort"]["unsorted"], false);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_matches_converts_file_url_to_path_string() {
    let paths = new_router_fixture("router-page-hash-matches-url-to-path").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_match_samples(&paths, "match-sort-hash").await;
    update_book_url(
        &paths,
        "book-match-1",
        "file:/library-root/books/book-match-1.cbz",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash?sort=bookId,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("page hash matches url path request should build"),
        )
        .await
        .expect("page hash matches url path request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("page hash matches content should be an array");
    assert_eq!(content[0]["url"], "/library-root/books/book-match-1.cbz");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_matches_returns_internal_error_for_unparseable_book_url() {
    let paths = new_router_fixture("router-page-hash-matches-invalid-url").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_match_samples(&paths, "match-sort-hash").await;
    update_book_url(&paths, "book-match-1", "::not-a-valid-url::").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash?sort=bookId,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("page hash matches invalid url request should build"),
        )
        .await
        .expect("page hash matches invalid url request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_matches_decodes_percent_encoded_file_url_path() {
    let paths = new_router_fixture("router-page-hash-matches-decodes-file-url-path").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_match_samples(&paths, "match-sort-hash").await;
    update_book_url(
        &paths,
        "book-match-1",
        "file:/library%20root/books/book%20match%201.cbz",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash?sort=bookId,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("page hash matches encoded file url request should build"),
        )
        .await
        .expect("page hash matches encoded file url request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("page hash matches content should be an array");
    assert_eq!(content[0]["url"], "/library root/books/book match 1.cbz");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_matches_returns_internal_error_for_null_file_size() {
    let paths = new_router_fixture("router-page-hash-matches-null-file-size").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_match_samples(&paths, "match-sort-hash").await;
    update_media_page_file_size_to_null(&paths, "book-match-1", 0).await;
    assert_eq!(
        load_media_page_file_size(&paths, "book-match-1", 0).await,
        None
    );

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash?sort=bookId,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("page hash matches null file size request should build"),
        )
        .await
        .expect("page hash matches null file size request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_matches_returns_internal_error_for_non_file_url() {
    let paths = new_router_fixture("router-page-hash-matches-http-url").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_match_samples(&paths, "match-sort-hash").await;
    update_book_url(
        &paths,
        "book-match-1",
        "https://example.com/books/book-match-1.cbz",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash?sort=bookId,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("page hash matches non-file url request should build"),
        )
        .await
        .expect("page hash matches non-file url request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

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

async fn seed_global_client_setting(
    paths: &RuntimeDbPaths,
    key: &str,
    value: &str,
    allow_unauthorized: bool,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
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
    let pool = connect_pool(paths.main_db.as_path(), 1)
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

fn kobo_proxy_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn restore_env_var(key: &str, value: Option<String>) {
    if let Some(value) = value {
        unsafe {
            std::env::set_var(key, value);
        }
    } else {
        unsafe {
            std::env::remove_var(key);
        }
    }
}

struct SingleResponseServer {
    url: String,
    join: tokio::task::JoinHandle<()>,
}

async fn spawn_single_response_server(
    status_code: u16,
    content_type: &str,
    body: &str,
) -> SingleResponseServer {
    spawn_single_response_server_with_headers(status_code, content_type, body, &[]).await
}

async fn spawn_single_response_server_with_headers(
    status_code: u16,
    content_type: &str,
    body: &str,
    headers: &[(&str, &str)],
) -> SingleResponseServer {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock response server should bind");
    let address = listener
        .local_addr()
        .expect("mock response server should have local addr");
    let body = body.to_string();
    let content_type = content_type.to_string();
    let headers = headers
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect::<Vec<_>>();
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
        let extra_headers = headers
            .iter()
            .map(|(name, value)| format!("{name}: {value}\r\n"))
            .collect::<String>();
        let response = format!(
            "HTTP/1.1 {status_code} {status_text}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n{}",
            body.len(),
            extra_headers,
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

async fn spawn_request_body_echo_server() -> SingleResponseServer {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock request echo server should bind");
    let address = listener
        .local_addr()
        .expect("mock request echo server should have local addr");
    let join = tokio::spawn(async move {
        let (mut stream, _) = listener
            .accept()
            .await
            .expect("mock request echo server should accept one connection");
        let mut request = Vec::new();
        let mut chunk = [0_u8; 1024];

        loop {
            let read = stream
                .read(&mut chunk)
                .await
                .expect("mock request echo server should read request bytes");
            if read == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..read]);

            let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
            else {
                continue;
            };
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.trim()
                            .eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                })
                .unwrap_or(0);
            let body_start = header_end + 4;
            if request.len() >= body_start + content_length {
                break;
            }
        }

        let header_end = request
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .expect("mock request echo server should receive complete headers");
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.trim()
                        .eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
            })
            .unwrap_or(0);
        let body_start = header_end + 4;
        let body_end = body_start + content_length;
        let body = String::from_utf8_lossy(&request[body_start..body_end]).to_string();
        let response_body = serde_json::to_string(&json!({ "received": body }))
            .expect("mock request echo payload should serialize");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        stream
            .write_all(response.as_bytes())
            .await
            .expect("mock request echo server should write response");
    });

    SingleResponseServer {
        url: format!("http://{address}/echo.json"),
        join,
    }
}

async fn seed_announcement_read_ids(paths: &RuntimeDbPaths, user_id: &str, ids: &[&str]) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
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

async fn load_page_hash_size(paths: &RuntimeDbPaths, hash: &str) -> Option<i64> {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("page hash query db should open");

    let row = sqlx::query("SELECT SIZE FROM PAGE_HASH WHERE HASH = ?")
        .bind(hash)
        .fetch_one(&pool)
        .await
        .expect("page hash row should load");
    pool.close().await;

    row.get::<Option<i64>, _>("SIZE")
}

async fn load_page_hash_record(
    paths: &RuntimeDbPaths,
    hash: &str,
) -> Option<(Option<i64>, String)> {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("page hash record query db should open");

    let row = sqlx::query("SELECT SIZE, ACTION FROM PAGE_HASH WHERE HASH = ?")
        .bind(hash)
        .fetch_optional(&pool)
        .await
        .expect("page hash record should be queryable");
    pool.close().await;

    row.map(|row| {
        (
            row.get::<Option<i64>, _>("SIZE"),
            row.get::<String, _>("ACTION"),
        )
    })
}

async fn seed_page_hash_row(paths: &RuntimeDbPaths, hash: &str, size: Option<i64>, action: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("page hash seed db should open");

    sqlx::query("INSERT INTO PAGE_HASH (HASH, SIZE, ACTION, DELETE_COUNT, CREATED_DATE, LAST_MODIFIED_DATE) VALUES (?, ?, ?, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)")
        .bind(hash)
        .bind(size)
        .bind(action)
        .execute(&pool)
        .await
        .expect("page hash row should be inserted");

    pool.close().await;
}

async fn seed_page_hash_image_source(
    paths: &RuntimeDbPaths,
    book_id: &str,
    hash: &str,
    relative_book_path: &str,
    file_name: &str,
) -> std::path::PathBuf {
    let source_path = paths.config_dir.join(relative_book_path);
    if let Some(parent) = source_path.parent() {
        std::fs::create_dir_all(parent).expect("page hash source parent should be created");
    }

    let image_bytes = fixture_png_bytes();
    std::fs::write(&source_path, &image_bytes).expect("page hash source image should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("page hash image source db should open");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind(0_i64)
    .bind(file_name)
    .bind(relative_book_path)
    .bind("series-1")
    .bind(i64::try_from(image_bytes.len()).expect("image bytes length should fit i64"))
    .bind(2_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("page hash source book row should be inserted");

    sqlx::query(
        "INSERT INTO MEDIA_PAGE (BOOK_ID, NUMBER, FILE_HASH, FILE_NAME, MEDIA_TYPE, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind(0_i64)
    .bind(hash)
    .bind(file_name)
    .bind("image/png")
    .bind(i64::try_from(image_bytes.len()).expect("image bytes length should fit i64"))
    .execute(&pool)
    .await
    .expect("page hash source media page row should be inserted");

    pool.close().await;
    source_path
}

async fn seed_unknown_page_hash_samples(paths: &RuntimeDbPaths) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("unknown page hash sample db should open");

    for (book_id, name, url, number) in [
        (
            "book-unknown-1",
            "book-unknown-1.epub",
            "books/book-unknown-1.epub",
            10_i64,
        ),
        (
            "book-unknown-2",
            "book-unknown-2.epub",
            "books/book-unknown-2.epub",
            11_i64,
        ),
        (
            "book-unknown-3",
            "book-unknown-3.epub",
            "books/book-unknown-3.epub",
            12_i64,
        ),
        (
            "book-unknown-4",
            "book-unknown-4.epub",
            "books/book-unknown-4.epub",
            13_i64,
        ),
    ] {
        sqlx::query(
            "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(book_id)
        .bind(0_i64)
        .bind(name)
        .bind(url)
        .bind("series-1")
        .bind(2_048_i64)
        .bind(number)
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("unknown page hash sample book row should be inserted");
    }

    for (book_id, page_hash, file_size) in [
        ("book-unknown-1", "a-hash", 111_i64),
        ("book-unknown-2", "a-hash", 111_i64),
        ("book-unknown-3", "z-hash", 222_i64),
        ("book-unknown-4", "z-hash", 222_i64),
    ] {
        sqlx::query(
            "INSERT INTO MEDIA_PAGE (BOOK_ID, NUMBER, FILE_HASH, FILE_NAME, MEDIA_TYPE, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(book_id)
        .bind(0_i64)
        .bind(page_hash)
        .bind(format!("{book_id}.png"))
        .bind("image/png")
        .bind(file_size)
        .execute(&pool)
        .await
        .expect("unknown page hash sample media page row should be inserted");
    }

    pool.close().await;
}

async fn seed_page_hash_match_samples(paths: &RuntimeDbPaths, hash: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("page hash match sample db should open");

    for (book_id, name, url, number) in [
        (
            "book-match-1",
            "book-match-1.epub",
            "file:/library-root/books/book-match-1.epub",
            20_i64,
        ),
        (
            "book-match-2",
            "book-match-2.epub",
            "file:/library-root/books/book-match-2.epub",
            21_i64,
        ),
        (
            "book-match-3",
            "book-match-3.epub",
            "file:/library-root/books/book-match-3.epub",
            22_i64,
        ),
    ] {
        sqlx::query(
            "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(book_id)
        .bind(0_i64)
        .bind(name)
        .bind(url)
        .bind("series-1")
        .bind(2_048_i64)
        .bind(number)
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("page hash match sample book row should be inserted");
    }

    for (book_id, page_number) in [
        ("book-match-1", 0_i64),
        ("book-match-2", 2_i64),
        ("book-match-3", 4_i64),
    ] {
        sqlx::query(
            "INSERT INTO MEDIA_PAGE (BOOK_ID, NUMBER, FILE_HASH, FILE_NAME, MEDIA_TYPE, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(book_id)
        .bind(page_number)
        .bind(hash)
        .bind(format!("{book_id}-{page_number}.png"))
        .bind("image/png")
        .bind(100_i64 + page_number)
        .execute(&pool)
        .await
        .expect("page hash match sample media page row should be inserted");
    }

    pool.close().await;
}

async fn update_book_url(paths: &RuntimeDbPaths, book_id: &str, url: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book url update db should open");

    sqlx::query("UPDATE BOOK SET URL = ? WHERE ID = ?")
        .bind(url)
        .bind(book_id)
        .execute(&pool)
        .await
        .expect("book url should be updated");

    pool.close().await;
}

async fn update_media_page_file_size_to_null(paths: &RuntimeDbPaths, book_id: &str, number: i64) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("media page update db should open");

    sqlx::query("UPDATE MEDIA_PAGE SET FILE_SIZE = NULL WHERE BOOK_ID = ? AND NUMBER = ?")
        .bind(book_id)
        .bind(number)
        .execute(&pool)
        .await
        .expect("media page file size should be updated to null");

    pool.close().await;
}

async fn load_media_page_file_size(
    paths: &RuntimeDbPaths,
    book_id: &str,
    number: i64,
) -> Option<i64> {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("media page query db should open");

    let row = sqlx::query("SELECT FILE_SIZE FROM MEDIA_PAGE WHERE BOOK_ID = ? AND NUMBER = ?")
        .bind(book_id)
        .bind(number)
        .fetch_one(&pool)
        .await
        .expect("media page row should load");

    pool.close().await;
    row.get::<Option<i64>, _>("FILE_SIZE")
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

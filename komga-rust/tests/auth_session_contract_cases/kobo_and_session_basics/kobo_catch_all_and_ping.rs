#![allow(clippy::await_holding_lock)]

use super::*;

#[tokio::test]
async fn router_kobo_ping_does_not_accept_web_auth_fallback_when_path_token_is_invalid() {
    let ctx = TestFixture::new("router-kobo-ping-path-token-only-auth").await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_kobo_ping_rejects_path_tokens_with_characters_outside_kotlin_regex() {
    let ctx = TestFixture::new("router-kobo-ping-token-char-constraint").await;
    seed_router_age_exclude_user_with_roles(
        ctx.paths(),
        "kobo-user",
        "kobo@example.org",
        "router-contract-kobo-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(ctx.paths(), "bad.token", "kobo-user").await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_kobo_ping_returns_forbidden_for_valid_token_without_kobo_sync_role() {
    let ctx = TestFixture::new("router-kobo-ping-forbidden-without-kobo-role").await;
    seed_router_age_exclude_user_with_roles(
        ctx.paths(),
        "plain-user",
        "plain@example.org",
        "router-contract-plain-123",
        0,
        &["USER"],
    )
    .await;
    seed_kobo_sync_api_key(ctx.paths(), "plain-kobo-token", "plain-user").await;

    let response = ctx
        .app()
        .clone()
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

    let ctx = TestFixture::new("router-kobo-catch-all-non-json-body").await;
    upsert_server_setting(ctx.paths(), "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        ctx.paths(),
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(ctx.paths(), "validkobotoken", "kobo-proxy-user").await;

    let response = ctx
        .app()
        .clone()
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

    let ctx = TestFixture::new("router-kobo-catch-all-non-json-error-body").await;
    upsert_server_setting(ctx.paths(), "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        ctx.paths(),
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(ctx.paths(), "validkobotoken", "kobo-proxy-user").await;

    let response = ctx
        .app()
        .clone()
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

    let ctx = TestFixture::new("router-kobo-catch-all-no-error-body-passthrough").await;
    upsert_server_setting(ctx.paths(), "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        ctx.paths(),
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(ctx.paths(), "validkobotoken", "kobo-proxy-user").await;

    let response = ctx
        .app()
        .clone()
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

    let ctx = TestFixture::new("router-kobo-catch-all-no-json-error-passthrough").await;
    upsert_server_setting(ctx.paths(), "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        ctx.paths(),
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(ctx.paths(), "validkobotoken", "kobo-proxy-user").await;

    let response = ctx
        .app()
        .clone()
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

    let ctx = TestFixture::new("router-kobo-catch-all-transport-failure").await;
    upsert_server_setting(ctx.paths(), "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        ctx.paths(),
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(ctx.paths(), "validkobotoken", "kobo-proxy-user").await;

    let response = ctx
        .app()
        .clone()
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

    let ctx = TestFixture::new("router-kobo-catch-all-empty-success-body").await;
    upsert_server_setting(ctx.paths(), "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        ctx.paths(),
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(ctx.paths(), "validkobotoken", "kobo-proxy-user").await;

    let response = ctx
        .app()
        .clone()
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

    let ctx = TestFixture::new("router-kobo-catch-all-put-invalid-json-body").await;
    upsert_server_setting(ctx.paths(), "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        ctx.paths(),
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(ctx.paths(), "validkobotoken", "kobo-proxy-user").await;

    let response = ctx
        .app()
        .clone()
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

    let ctx = TestFixture::new("router-kobo-catch-all-put-text-plain-body").await;
    upsert_server_setting(ctx.paths(), "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        ctx.paths(),
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(ctx.paths(), "validkobotoken", "kobo-proxy-user").await;

    let response = ctx
        .app()
        .clone()
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

    let ctx = TestFixture::new("router-kobo-catch-all-put-malformed-xml-body").await;
    upsert_server_setting(ctx.paths(), "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        ctx.paths(),
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(ctx.paths(), "validkobotoken", "kobo-proxy-user").await;

    let response = ctx
        .app()
        .clone()
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

    let ctx = TestFixture::new("router-kobo-catch-all-put-json-reserialize").await;
    upsert_server_setting(ctx.paths(), "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        ctx.paths(),
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(ctx.paths(), "validkobotoken", "kobo-proxy-user").await;

    let response = ctx
        .app()
        .clone()
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
    assert_eq!(received, "{\n  \"key\" : 1,\n  \"items\" : [ 2, 3 ]\n}");

    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server
        .join
        .await
        .expect("kobo proxy request-body echo server should finish");
}

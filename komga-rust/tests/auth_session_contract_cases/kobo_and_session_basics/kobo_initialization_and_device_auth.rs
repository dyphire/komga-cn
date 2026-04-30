#![allow(clippy::await_holding_lock)]

use super::*;

#[tokio::test]
async fn router_kobo_initialization_returns_forbidden_for_session_user_without_kobo_sync_role() {
    let ctx = TestFixture::new("router-kobo-initialization-missing-kobo-sync-role").await;
    seed_router_age_exclude_user_with_roles(
        ctx.paths(),
        "member-no-kobo-sync",
        "member-no-kobo-sync@example.org",
        "member-no-kobo-sync-123",
        99,
        &["USER", "PAGE_STREAMING"],
    )
    .await;
    seed_kobo_sync_api_key(ctx.paths(), "membernokobosync", "member-no-kobo-sync").await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/membernokobosync/v1/initialization")
                .header(header::HOST, "127.0.0.1")
                .body(Body::empty())
                .expect("kobo initialization request should build"),
        )
        .await
        .expect("kobo initialization request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn router_kobo_initialization_returns_unauthorized_for_invalid_path_token_even_with_kobo_sync_session()
 {
    let ctx = TestFixture::new("router-kobo-initialization-invalid-path-token").await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/initialization")
                .header(header::HOST, "127.0.0.1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo invalid path token request should build"),
        )
        .await
        .expect("kobo invalid path token request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn router_kobo_initialization_returns_fixed_api_token_header_and_absolute_local_urls() {
    let ctx = TestFixture::new("router-kobo-initialization-api-token").await;
    seed_admin_kobo_path_token(ctx.paths()).await;

    let expected_host = format!("http://127.0.0.1:{}", ctx.config().bind_address.port());
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/initialization")
                .header(header::HOST, "127.0.0.1")
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
    assert_eq!(api_token, "e30=");
    let payload = response_json(response).await;
    let resources = payload
        .get("Resources")
        .expect("kobo initialization should include Resources");
    assert_eq!(
        resources.get("account_page"),
        Some(&Value::String(
            "https://www.kobo.com/account/settings".to_string()
        ))
    );
    assert_eq!(
        resources.get("image_host"),
        Some(&Value::String(expected_host.clone()))
    );
    assert_eq!(
        resources.get("device_auth"),
        Some(&Value::String(
            "https://storeapi.kobo.com/v1/auth/device".to_string()
        ))
    );
    assert_eq!(
        resources.get("library_sync"),
        Some(&Value::String(
            "https://storeapi.kobo.com/v1/library/sync".to_string()
        ))
    );
    assert_eq!(
        resources.get("image_url_template"),
        Some(&Value::String(format!(
            "{expected_host}/kobo/any-token/v1/books/{{ImageId}}/thumbnail/{{Width}}/{{Height}}/false/image.jpg"
        )))
    );
    assert_eq!(
        resources.get("image_url_quality_template"),
        Some(&Value::String(format!(
            "{expected_host}/kobo/any-token/v1/books/{{ImageId}}/thumbnail/{{Width}}/{{Height}}/{{Quality}}/{{IsGreyscale}}/image.jpg"
        )))
    );
}

#[tokio::test]
async fn router_kobo_initialization_uses_kobo_port_when_host_omits_port() {
    let ctx = TestFixture::new("router-kobo-initialization-kobo-port").await;
    seed_admin_kobo_path_token(ctx.paths()).await;
    upsert_server_setting(ctx.paths(), "KOBO_PORT", "8085").await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/initialization")
                .header(header::HOST, "localhost")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo initialization koboPort request should build"),
        )
        .await
        .expect("kobo initialization koboPort request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.pointer("/Resources/image_host"),
        Some(&Value::String("http://localhost:8085".to_string()))
    );
}

#[tokio::test]
async fn router_kobo_initialization_prefers_forwarded_host_over_kobo_port() {
    let ctx = TestFixture::new("router-kobo-initialization-forwarded-host").await;
    seed_admin_kobo_path_token(ctx.paths()).await;
    upsert_server_setting(ctx.paths(), "KOBO_PORT", "8085").await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/initialization")
                .header(header::HOST, "127.0.0.1")
                .header("x-forwarded-proto", "https")
                .header("x-forwarded-host", "demo.komga.org")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo initialization forwarded host request should build"),
        )
        .await
        .expect("kobo initialization forwarded host request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.pointer("/Resources/image_host"),
        Some(&Value::String("https://demo.komga.org".to_string()))
    );
}

#[tokio::test]
async fn router_kobo_initialization_uses_proxied_resources_and_overrides_local_urls() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();

    let server = spawn_single_response_server(
        200,
        "application/json",
        r#"{"Resources":{"account_page":"https://proxy.example/account","feature_flag":"proxy-only","device_auth":"https://proxy.example/device","library_sync":"https://proxy.example/library"}}"#,
    )
    .await;
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", server.url.clone());
    }

    let ctx = TestFixture::new("router-kobo-initialization-proxy-success").await;
    seed_admin_kobo_path_token(ctx.paths()).await;
    upsert_server_setting(ctx.paths(), "KOBO_PROXY", "true").await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/initialization")
                .header(header::HOST, "komga.example")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo initialization proxy success request should build"),
        )
        .await
        .expect("kobo initialization proxy success request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let resources = payload
        .get("Resources")
        .expect("proxied initialization should include Resources");
    assert_eq!(
        resources.get("account_page"),
        Some(&Value::String("https://proxy.example/account".to_string()))
    );
    assert_eq!(
        resources.get("feature_flag"),
        Some(&Value::String("proxy-only".to_string()))
    );
    assert_eq!(
        resources.get("device_auth"),
        Some(&Value::String("https://proxy.example/device".to_string()))
    );
    assert_eq!(
        resources.get("library_sync"),
        Some(&Value::String("https://proxy.example/library".to_string()))
    );

    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server
        .join
        .await
        .expect("kobo initialization proxy success server should finish");
}

#[tokio::test]
async fn router_kobo_initialization_falls_back_to_native_resources_for_non_401_proxy_failure() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();

    let server =
        spawn_single_response_server(503, "application/json", r#"{"error":"upstream-failure"}"#)
            .await;
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", server.url.clone());
    }

    let ctx = TestFixture::new("router-kobo-initialization-proxy-fallback").await;
    seed_admin_kobo_path_token(ctx.paths()).await;
    upsert_server_setting(ctx.paths(), "KOBO_PROXY", "true").await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/initialization")
                .header(header::HOST, "komga.example")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo initialization proxy fallback request should build"),
        )
        .await
        .expect("kobo initialization proxy fallback request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.pointer("/Resources/account_page"),
        Some(&Value::String(
            "https://www.kobo.com/account/settings".to_string()
        ))
    );

    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server
        .join
        .await
        .expect("kobo initialization proxy fallback server should finish");
}

#[tokio::test]
async fn router_kobo_initialization_preserves_unauthorized_from_proxy() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();

    let server =
        spawn_single_response_server(401, "application/json", r#"{"error":"unauthorized"}"#).await;
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", server.url.clone());
    }

    let ctx = TestFixture::new("router-kobo-initialization-proxy-unauthorized").await;
    seed_admin_kobo_path_token(ctx.paths()).await;
    upsert_server_setting(ctx.paths(), "KOBO_PROXY", "true").await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/initialization")
                .header(header::HOST, "komga.example")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo initialization proxy unauthorized request should build"),
        )
        .await
        .expect("kobo initialization proxy unauthorized request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server
        .join
        .await
        .expect("kobo initialization proxy unauthorized server should finish");
}

#[tokio::test]
async fn router_kobo_auth_device_uses_proxied_response_when_proxy_enabled() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();

    let server = spawn_request_body_echo_server().await;
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", server.url.clone());
    }

    let ctx = TestFixture::new("router-kobo-auth-device-proxy-success").await;
    seed_admin_kobo_path_token(ctx.paths()).await;
    upsert_server_setting(ctx.paths(), "KOBO_PROXY", "true").await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/kobo/any-token/v1/auth/device?affiliate=rakuten&source=device")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-auth-token", &auth_token)
                .body(Body::from(
                    "{\n  \"UserKey\": \"Reader 1\",\n  \"Nested\": { \"value\": 7 }\n}",
                ))
                .expect("kobo auth device proxy success request should build"),
        )
        .await
        .expect("kobo auth device proxy success request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let received = payload
        .get("received")
        .and_then(Value::as_str)
        .expect("proxied auth device response should echo request body");
    assert_eq!(
        received,
        "{\n  \"UserKey\": \"Reader 1\",\n  \"Nested\": { \"value\": 7 }\n}"
    );
    assert_eq!(
        payload.get("query"),
        Some(&Value::String(
            "affiliate=rakuten&source=device".to_string()
        ))
    );

    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server
        .join
        .await
        .expect("kobo auth device proxy success server should finish");
}

#[tokio::test]
async fn router_kobo_auth_device_falls_back_to_dummy_payload_when_proxy_returns_unauthorized() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();

    let server =
        spawn_single_response_server(401, "application/json", r#"{"error":"unauthorized"}"#).await;
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", server.url.clone());
    }

    let ctx = TestFixture::new("router-kobo-auth-device-proxy-unauthorized").await;
    seed_admin_kobo_path_token(ctx.paths()).await;
    upsert_server_setting(ctx.paths(), "KOBO_PROXY", "true").await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/kobo/any-token/v1/auth/device")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-auth-token", &auth_token)
                .body(Body::from(r#"{"UserKey":"Reader-1"}"#))
                .expect("kobo auth device proxy unauthorized request should build"),
        )
        .await
        .expect("kobo auth device proxy unauthorized request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let access = payload
        .get("AccessToken")
        .and_then(Value::as_str)
        .expect("fallback auth device payload should include AccessToken");
    let refresh = payload
        .get("RefreshToken")
        .and_then(Value::as_str)
        .expect("fallback auth device payload should include RefreshToken");
    let tracking = payload
        .get("TrackingId")
        .and_then(Value::as_str)
        .expect("fallback auth device payload should include TrackingId");
    assert_eq!(
        payload.get("TokenType"),
        Some(&Value::String("Bearer".to_string()))
    );
    assert_eq!(
        payload.get("UserKey"),
        Some(&Value::String("Reader-1".to_string()))
    );
    assert_eq!(access.len(), 24);
    assert!(access.chars().all(|ch| ch.is_ascii_alphanumeric()));
    assert!(!access.starts_with("kobo-"));
    assert_eq!(refresh.len(), 24);
    assert!(refresh.chars().all(|ch| ch.is_ascii_alphanumeric()));
    assert!(!refresh.starts_with("kobo-"));
    assert_eq!(tracking.len(), 36);
    assert_eq!(tracking.chars().nth(8), Some('-'));
    assert_eq!(tracking.chars().nth(13), Some('-'));
    assert_eq!(tracking.chars().nth(18), Some('-'));
    assert_eq!(tracking.chars().nth(23), Some('-'));

    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server
        .join
        .await
        .expect("kobo auth device proxy unauthorized server should finish");
}

#[tokio::test]
async fn router_kobo_auth_device_falls_back_to_dummy_payload_when_proxy_is_disabled() {
    let ctx = TestFixture::new("router-kobo-auth-device-proxy-disabled").await;
    seed_admin_kobo_path_token(ctx.paths()).await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/kobo/any-token/v1/auth/device")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-auth-token", &auth_token)
                .body(Body::from(r#"{"UserKey":123}"#))
                .expect("kobo auth device proxy disabled request should build"),
        )
        .await
        .expect("kobo auth device proxy disabled request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("TokenType"),
        Some(&Value::String("Bearer".to_string()))
    );
    assert_eq!(
        payload.get("UserKey"),
        Some(&Value::String("123".to_string()))
    );
}

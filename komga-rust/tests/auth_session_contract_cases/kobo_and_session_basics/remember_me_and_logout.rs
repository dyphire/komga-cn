use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::json;
use std::time::Duration;

fn response_cookies(response: &axum::response::Response) -> Vec<String> {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .map(str::to_string)
        .collect()
}

fn cookie_value(cookies: &[String], name: &str) -> Option<String> {
    cookies.iter().find_map(|cookie| {
        cookie
            .split(';')
            .next()
            .and_then(|pair| pair.split_once('='))
            .filter(|(cookie_name, value)| *cookie_name == name && !value.is_empty())
            .map(|(_, value)| value.to_string())
    })
}

fn cookie_header<'a>(cookies: &'a [String], name: &str) -> Option<&'a str> {
    cookies.iter().find_map(|cookie| {
        cookie
            .split(';')
            .next()
            .and_then(|pair| pair.split_once('='))
            .filter(|(cookie_name, value)| *cookie_name == name && !value.is_empty())
            .map(|_| cookie.as_str())
    })
}

fn cookie_max_age_seconds(cookie: &str) -> Option<u64> {
    cookie
        .split(';')
        .map(str::trim)
        .find_map(|segment| segment.strip_prefix("Max-Age="))
        .and_then(|value| value.parse::<u64>().ok())
}

async fn login_with_basic_and_remember_me(
    app: axum::Router,
    email: &str,
    password: &str,
) -> axum::response::Response {
    let basic_token = STANDARD.encode(format!("{email}:{password}"));
    app.oneshot(
        Request::builder()
            .method("GET")
            .uri("/api/v2/users/me?remember-me=true")
            .header(header::AUTHORIZATION, format!("Basic {basic_token}"))
            .body(Body::empty())
            .expect("remember-me login request should build"),
    )
    .await
    .expect("remember-me login request should complete")
}

async fn users_me_with_cookie(app: axum::Router, cookie_header: &str) -> axum::response::Response {
    app.oneshot(
        Request::builder()
            .method("GET")
            .uri("/api/v2/users/me")
            .header(header::COOKIE, cookie_header)
            .body(Body::empty())
            .expect("users/me cookie request should build"),
    )
    .await
    .expect("users/me cookie request should complete")
}

async fn patch_server_settings(
    app: axum::Router,
    admin_token: &str,
    payload: Value,
) -> axum::response::Response {
    app.oneshot(
        Request::builder()
            .method("PATCH")
            .uri("/api/v1/settings")
            .header("x-auth-token", admin_token)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(payload.to_string()))
            .expect("settings patch request should build"),
    )
    .await
    .expect("settings patch request should complete")
}

async fn get_server_settings(app: axum::Router, admin_token: &str) -> axum::response::Response {
    app.oneshot(
        Request::builder()
            .method("GET")
            .uri("/api/v1/settings")
            .header("x-auth-token", admin_token)
            .body(Body::empty())
            .expect("settings get request should build"),
    )
    .await
    .expect("settings get request should complete")
}

#[tokio::test]
async fn basic_login_with_remember_me_true_issues_session_and_remember_me_cookies() {
    let ctx = TestFixture::new("router-remember-me-basic-login-cookies").await;
    let response = login_with_basic_and_remember_me(
        ctx.app().clone(),
        "admin@example.org",
        "router-contract-admin-123",
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let cookies = response_cookies(&response);
    assert!(
        cookie_value(&cookies, "KOMGA-SESSION").is_some(),
        "remember-me basic login should issue KOMGA-SESSION cookie: {cookies:?}"
    );
    assert!(
        cookie_value(&cookies, "komga-remember-me").is_some(),
        "remember-me basic login should issue komga-remember-me cookie: {cookies:?}"
    );
}

pub(crate) async fn verify_remember_me_reauthenticates_after_session_expiry() {
    let ctx = TestFixture::builder("router-remember-me-session-expiry-reauth")
        .with_config(|config| {
            config.session_max_inactive_seconds = 1;
        })
        .build()
        .await;
    let login_response = login_with_basic_and_remember_me(
        ctx.app().clone(),
        "admin@example.org",
        "router-contract-admin-123",
    )
    .await;
    assert_eq!(login_response.status(), StatusCode::OK);

    let login_cookies = response_cookies(&login_response);
    let session_cookie = cookie_value(&login_cookies, "KOMGA-SESSION")
        .expect("remember-me login should issue session cookie");
    let remember_me_cookie = cookie_value(&login_cookies, "komga-remember-me")
        .expect("remember-me login should issue remember-me cookie");

    tokio::time::sleep(Duration::from_secs(2)).await;

    let expired_session_response = users_me_with_cookie(
        ctx.app().clone(),
        &format!("KOMGA-SESSION={session_cookie}"),
    )
    .await;
    assert_eq!(expired_session_response.status(), StatusCode::UNAUTHORIZED);

    let remember_only_response = users_me_with_cookie(
        ctx.app().clone(),
        &format!("komga-remember-me={remember_me_cookie}"),
    )
    .await;

    assert_eq!(remember_only_response.status(), StatusCode::OK);
    let remember_payload = response_json(remember_only_response).await;
    assert_eq!(
        remember_payload.get("email"),
        Some(&json!("admin@example.org"))
    );

    let combined_expired_session_response = users_me_with_cookie(
        ctx.app().clone(),
        &format!("KOMGA-SESSION={session_cookie}; komga-remember-me={remember_me_cookie}"),
    )
    .await;

    assert_eq!(combined_expired_session_response.status(), StatusCode::OK);
    let refreshed_cookies = response_cookies(&combined_expired_session_response);
    let refreshed_session_cookie = cookie_value(&refreshed_cookies, "KOMGA-SESSION")
        .expect("remember-me reauthentication should issue a fresh session cookie");
    assert_ne!(refreshed_session_cookie, session_cookie);

    let refreshed_session_response = users_me_with_cookie(
        ctx.app().clone(),
        &format!("KOMGA-SESSION={refreshed_session_cookie}"),
    )
    .await;
    assert_eq!(refreshed_session_response.status(), StatusCode::OK);
}

pub(crate) async fn verify_remember_me_auto_login_records_remember_me_source() {
    let ctx = TestFixture::builder("router-remember-me-auto-login-records-source")
        .with_config(|config| {
            config.session_max_inactive_seconds = 1;
        })
        .build()
        .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for remember-me activity cleanup");
    sqlx::query("DELETE FROM AUTHENTICATION_ACTIVITY WHERE EMAIL = ?")
        .bind("admin@example.org")
        .execute(&pool)
        .await
        .expect("existing authentication activity rows should delete");
    pool.close().await;

    let login_response = login_with_basic_and_remember_me(
        ctx.app().clone(),
        "admin@example.org",
        "router-contract-admin-123",
    )
    .await;
    assert_eq!(login_response.status(), StatusCode::OK);

    let login_cookies = response_cookies(&login_response);
    let remember_me_cookie = cookie_value(&login_cookies, "komga-remember-me")
        .expect("remember-me login should issue remember-me cookie");

    tokio::time::sleep(Duration::from_secs(2)).await;

    let remember_only_response = users_me_with_cookie(
        ctx.app().clone(),
        &format!("komga-remember-me={remember_me_cookie}"),
    )
    .await;

    assert_eq!(remember_only_response.status(), StatusCode::OK);

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for remember-me activity assertion");
    let source = sqlx::query_scalar::<_, Option<String>>(
        "SELECT SOURCE FROM AUTHENTICATION_ACTIVITY WHERE EMAIL = ? ORDER BY DATE_TIME DESC LIMIT 1",
    )
    .bind("admin@example.org")
    .fetch_one(&pool)
    .await
    .expect("remember-me auto-login should record authentication activity");
    pool.close().await;

    assert_eq!(source.as_deref(), Some("RememberMe"));
}

#[tokio::test]
async fn logout_clears_session_and_remember_me_replay() {
    let ctx = TestFixture::new("router-logout-clears-session-and-remember-me-replay").await;
    let login_response = login_with_basic_and_remember_me(
        ctx.app().clone(),
        "admin@example.org",
        "router-contract-admin-123",
    )
    .await;
    assert_eq!(login_response.status(), StatusCode::OK);

    let login_cookies = response_cookies(&login_response);
    let session_cookie = cookie_value(&login_cookies, "KOMGA-SESSION")
        .expect("remember-me login should issue session cookie");
    let remember_me_cookie = cookie_value(&login_cookies, "komga-remember-me")
        .expect("remember-me login should issue remember-me cookie");

    let logout_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/logout")
                .header(
                    header::COOKIE,
                    format!(
                        "KOMGA-SESSION={session_cookie}; komga-remember-me={remember_me_cookie}"
                    ),
                )
                .body(Body::empty())
                .expect("logout replay request should build"),
        )
        .await
        .expect("logout replay request should complete");
    assert_eq!(logout_response.status(), StatusCode::NO_CONTENT);

    let logout_cookies = response_cookies(&logout_response);
    assert!(
        logout_cookies
            .iter()
            .any(|cookie| cookie.contains("KOMGA-SESSION=;") && cookie.contains("Max-Age=0")),
        "logout should expire the session cookie: {logout_cookies:?}"
    );
    assert!(
        logout_cookies
            .iter()
            .any(|cookie| cookie.contains("komga-remember-me=;") && cookie.contains("Max-Age=0")),
        "logout should expire the remember-me cookie: {logout_cookies:?}"
    );

    let replay_session_response = users_me_with_cookie(
        ctx.app().clone(),
        &format!("KOMGA-SESSION={session_cookie}"),
    )
    .await;
    assert_eq!(replay_session_response.status(), StatusCode::UNAUTHORIZED);

    let replay_remember_me_response = users_me_with_cookie(
        ctx.app().clone(),
        &format!("komga-remember-me={remember_me_cookie}"),
    )
    .await;
    assert_eq!(replay_remember_me_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn malformed_remember_me_cookie_is_rejected() {
    let ctx = TestFixture::new("router-malformed-remember-me-cookie-rejected").await;
    let response =
        users_me_with_cookie(ctx.app().clone(), "komga-remember-me=not-a-valid-token").await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_resolution_priority_prefers_header_then_session_cookie_then_remember_me() {
    let ctx = TestFixture::new("router-auth-resolution-priority").await;
    seed_router_library_restricted_user(
        ctx.paths(),
        "member-user",
        "member@example.org",
        "router-contract-member-123",
        &["library-1"],
    )
    .await;

    let admin_token = ctx.login_admin().await;
    let member_login_response = login_with_basic_and_remember_me(
        ctx.app().clone(),
        "member@example.org",
        "router-contract-member-123",
    )
    .await;
    assert_eq!(member_login_response.status(), StatusCode::OK);
    let member_cookies = response_cookies(&member_login_response);
    let member_session_cookie = cookie_value(&member_cookies, "KOMGA-SESSION")
        .expect("member remember-me login should issue session cookie");
    let member_remember_me_cookie = cookie_value(&member_cookies, "komga-remember-me")
        .expect("member remember-me login should issue remember-me cookie");

    let header_wins_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me")
                .header("x-auth-token", &admin_token)
                .header(
                    header::COOKIE,
                    format!(
                        "KOMGA-SESSION={member_session_cookie}; komga-remember-me={member_remember_me_cookie}"
                    ),
                )
                .body(Body::empty())
                .expect("header priority request should build"),
        )
        .await
        .expect("header priority request should complete");
    assert_eq!(header_wins_response.status(), StatusCode::OK);
    let header_payload = response_json(header_wins_response).await;
    assert_eq!(
        header_payload.get("email"),
        Some(&json!("admin@example.org"))
    );

    let session_wins_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me")
                .header(
                    header::COOKIE,
                    format!(
                        "KOMGA-SESSION={member_session_cookie}; komga-remember-me=bogus-remember-me-token"
                    ),
                )
                .body(Body::empty())
                .expect("session priority request should build"),
        )
        .await
        .expect("session priority request should complete");
    assert_eq!(session_wins_response.status(), StatusCode::OK);
    let session_payload = response_json(session_wins_response).await;
    assert_eq!(
        session_payload.get("email"),
        Some(&json!("member@example.org"))
    );

    let remember_only_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me")
                .header(
                    header::COOKIE,
                    format!("komga-remember-me={member_remember_me_cookie}"),
                )
                .body(Body::empty())
                .expect("remember-only priority request should build"),
        )
        .await
        .expect("remember-only priority request should complete");
    assert_eq!(remember_only_response.status(), StatusCode::OK);
    let remember_payload = response_json(remember_only_response).await;
    assert_eq!(
        remember_payload.get("email"),
        Some(&json!("member@example.org"))
    );
}

pub(crate) async fn verify_remember_me_duration_setting_requires_restart_before_cookie_ttl_changes()
{
    let ctx = TestFixture::new("router-remember-me-settings-restart-only").await;

    let admin_token = ctx.login_admin().await;

    let patch_response = patch_server_settings(
        ctx.app().clone(),
        &admin_token,
        json!({
            "rememberMeDurationDays": 15
        }),
    )
    .await;

    assert_eq!(patch_response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        load_server_setting(ctx.paths(), "REMEMBER_ME_DURATION").await,
        Some("15".to_string())
    );

    let settings_response = get_server_settings(ctx.app().clone(), &admin_token).await;
    assert_eq!(settings_response.status(), StatusCode::OK);
    let settings_payload = response_json(settings_response).await;
    assert_eq!(
        settings_payload.get("rememberMeDurationDays"),
        Some(&json!(15))
    );

    let login_response = login_with_basic_and_remember_me(
        ctx.app().clone(),
        "admin@example.org",
        "router-contract-admin-123",
    )
    .await;
    assert_eq!(login_response.status(), StatusCode::OK);
    let current_runtime_cookies = response_cookies(&login_response);
    let current_runtime_cookie = cookie_header(&current_runtime_cookies, "komga-remember-me")
        .expect("remember-me login should issue remember-me cookie header before restart");
    assert_eq!(
        cookie_max_age_seconds(current_runtime_cookie),
        Some(365 * 24 * 60 * 60),
        "remember-me duration should stay on the current runtime value until restart: {current_runtime_cookie}"
    );

    let restarted_app = komga_server::app::build_router_with_config(ctx.config()).await;
    let restarted_login_response = login_with_basic_and_remember_me(
        restarted_app,
        "admin@example.org",
        "router-contract-admin-123",
    )
    .await;
    assert_eq!(restarted_login_response.status(), StatusCode::OK);
    let restarted_cookies = response_cookies(&restarted_login_response);
    let restarted_cookie = cookie_header(&restarted_cookies, "komga-remember-me")
        .expect("remember-me login should issue remember-me cookie header after restart");
    assert_eq!(
        cookie_max_age_seconds(restarted_cookie),
        Some(15 * 24 * 60 * 60),
        "remember-me duration should switch to the persisted value after restart: {restarted_cookie}"
    );
}

#[tokio::test]
async fn remember_me_duration_setting_requires_restart_before_cookie_ttl_changes() {
    verify_remember_me_duration_setting_requires_restart_before_cookie_ttl_changes().await;
}

pub(crate) async fn verify_remember_me_cold_start_uses_persisted_runtime_settings() {
    let ctx = TestFixture::builder("router-remember-me-cold-start-runtime-settings")
        .with_seed(|paths| async move {
            upsert_server_setting(&paths, "REMEMBER_ME_KEY", "cold-start-remember-key").await;
            upsert_server_setting(&paths, "REMEMBER_ME_DURATION", "15").await;
        })
        .build()
        .await;

    let login_response = login_with_basic_and_remember_me(
        ctx.app().clone(),
        "admin@example.org",
        "router-contract-admin-123",
    )
    .await;
    assert_eq!(login_response.status(), StatusCode::OK);

    let cookies = response_cookies(&login_response);
    let remember_me_cookie = cookie_header(&cookies, "komga-remember-me")
        .expect("cold-start remember-me login should issue remember-me cookie header");
    assert_eq!(
        cookie_max_age_seconds(remember_me_cookie),
        Some(15 * 24 * 60 * 60),
        "cold-start login should preload persisted remember-me duration before any /api/v1/settings request: {remember_me_cookie}"
    );

    let remember_me_cookie_value = cookie_value(&cookies, "komga-remember-me")
        .expect("cold-start remember-me login should issue remember-me cookie value");
    let replay_response = users_me_with_cookie(
        ctx.app().clone(),
        &format!("komga-remember-me={remember_me_cookie_value}"),
    )
    .await;
    assert_eq!(
        replay_response.status(),
        StatusCode::OK,
        "cold-start remember-me replay should succeed without first touching /api/v1/settings"
    );
}

#[tokio::test]
async fn remember_me_cold_start_uses_persisted_runtime_settings() {
    verify_remember_me_cold_start_uses_persisted_runtime_settings().await;
}

pub(crate) async fn verify_rotating_remember_me_key_requires_restart_before_existing_cookie_is_invalidated()
 {
    let ctx = TestFixture::new("router-remember-me-key-rotation-restart-only").await;
    let admin_token = ctx.login_admin().await;
    let login_response = login_with_basic_and_remember_me(
        ctx.app().clone(),
        "admin@example.org",
        "router-contract-admin-123",
    )
    .await;
    assert_eq!(login_response.status(), StatusCode::OK);

    let login_cookies = response_cookies(&login_response);
    let remember_me_cookie = cookie_value(&login_cookies, "komga-remember-me")
        .expect("remember-me login should issue remember-me cookie before rotation");

    let patch_response = patch_server_settings(
        ctx.app().clone(),
        &admin_token,
        json!({
            "renewRememberMeKey": true
        }),
    )
    .await;
    assert_eq!(patch_response.status(), StatusCode::NO_CONTENT);

    let persisted_key = load_server_setting(ctx.paths(), "REMEMBER_ME_KEY")
        .await
        .expect("renewRememberMeKey should persist a new remember-me key");
    assert!(
        !persisted_key.is_empty(),
        "renewRememberMeKey should persist a non-empty remember-me key"
    );

    let settings_response = get_server_settings(ctx.app().clone(), &admin_token).await;
    assert_eq!(settings_response.status(), StatusCode::OK);

    let replay_response = users_me_with_cookie(
        ctx.app().clone(),
        &format!("komga-remember-me={remember_me_cookie}"),
    )
    .await;
    assert_eq!(
        replay_response.status(),
        StatusCode::OK,
        "renewRememberMeKey should not invalidate existing remember-me cookies until restart"
    );

    let restarted_app = komga_server::app::build_router_with_config(ctx.config()).await;
    let restarted_replay_response = users_me_with_cookie(
        restarted_app,
        &format!("komga-remember-me={remember_me_cookie}"),
    )
    .await;
    assert_eq!(
        restarted_replay_response.status(),
        StatusCode::UNAUTHORIZED,
        "renewRememberMeKey should invalidate existing remember-me cookies after restart"
    );
}

#[tokio::test]
async fn rotating_remember_me_key_requires_restart_before_existing_cookie_is_invalidated() {
    verify_rotating_remember_me_key_requires_restart_before_existing_cookie_is_invalidated().await;
}

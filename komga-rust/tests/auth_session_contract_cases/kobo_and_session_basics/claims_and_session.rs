use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};

#[tokio::test]
async fn router_claim_status_returns_internal_error_when_status_lookup_fails() {
    let ctx = TestFixture::builder("router-claim-status-lookup-error")
        .without_standard_seed()
        .build()
        .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for claim status failure setup");
    sqlx::query("ALTER TABLE USER RENAME TO USER_BROKEN")
        .execute(&pool)
        .await
        .expect("user table should be renamed for claim status failure setup");
    pool.close().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/claim")
                .body(Body::empty())
                .expect("claim status failure request should build"),
        )
        .await
        .expect("claim status failure request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn router_users_me_basic_auth_defaults_to_session_cookie_without_auth_token_header() {
    let ctx = TestFixture::new("router-users-me-basic-defaults-to-cookie").await;
    let basic_token = STANDARD.encode("admin@example.org:router-contract-admin-123");

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me")
                .header(header::AUTHORIZATION, format!("Basic {basic_token}"))
                .body(Body::empty())
                .expect("users/me basic request should build"),
        )
        .await
        .expect("users/me basic request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers().get("x-auth-token").is_none(),
        "plain basic auth should not emit x-auth-token unless requested"
    );
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .expect("users/me basic response should include session cookie");
    assert!(set_cookie.contains("KOMGA-SESSION="));

    let payload = response_json(response).await;
    assert_eq!(
        payload.get("email"),
        Some(&Value::String("admin@example.org".to_string()))
    );
}

#[tokio::test]
async fn router_claim_rejects_invalid_email_header() {
    let ctx = TestFixture::new("router-claim-invalid-email-header").await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/claim")
                .header("X-Komga-Email", "user@domain")
                .header("X-Komga-Password", "password")
                .body(Body::empty())
                .expect("claim request should build"),
        )
        .await
        .expect("claim request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn router_claim_returns_kotlin_already_claimed_message() {
    let ctx = TestFixture::builder("router-claim-already-claimed-message")
        .without_standard_seed()
        .build()
        .await;

    let first_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/claim")
                .header("X-Komga-Email", "first-claim@example.org")
                .header("X-Komga-Password", "password")
                .body(Body::empty())
                .expect("initial claim request should build"),
        )
        .await
        .expect("initial claim request should complete");
    assert_eq!(first_response.status(), StatusCode::OK);

    let second_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/claim")
                .header("X-Komga-Email", "second-claim@example.org")
                .header("X-Komga-Password", "password")
                .body(Body::empty())
                .expect("already-claimed request should build"),
        )
        .await
        .expect("already-claimed request should complete");

    assert_eq!(second_response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(second_response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Bad Request".to_string()))
    );
    assert_eq!(
        payload.get("message"),
        Some(&Value::String(
            "This server has already been claimed".to_string()
        ))
    );
    assert_eq!(payload.get("status"), Some(&Value::from(400)));
}

pub(crate) async fn verify_login_set_cookie_returns_session_cookie_for_header_session() {
    let ctx = TestFixture::new("router-login-set-cookie-session-header").await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/login/set-cookie")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("login/set-cookie request should build"),
        )
        .await
        .expect("login/set-cookie request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .expect("login/set-cookie should return set-cookie header");
    assert!(set_cookie.starts_with(&format!("KOMGA-SESSION={auth_token}")));
    assert!(set_cookie.contains("Path=/"));
}

#[tokio::test]
async fn router_login_set_cookie_returns_session_cookie_for_header_session() {
    verify_login_set_cookie_returns_session_cookie_for_header_session().await;
}

#[tokio::test]
async fn router_logout_post_clears_session_cookie() {
    let ctx = TestFixture::new("router-logout-post-clears-session-cookie").await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/logout")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("logout post request should build"),
        )
        .await
        .expect("logout post request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let cookies = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .collect::<Vec<_>>();
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.contains("KOMGA-SESSION=;"))
    );
}

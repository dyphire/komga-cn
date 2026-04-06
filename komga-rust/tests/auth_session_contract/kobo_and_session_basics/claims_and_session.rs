use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};

#[tokio::test]
async fn router_users_me_basic_auth_defaults_to_session_cookie_without_auth_token_header() {
    let paths = new_router_fixture("router-users-me-basic-defaults-to-cookie").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let basic_token = STANDARD.encode("admin@example.org:router-contract-admin-123");

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_claim_rejects_invalid_email_header() {
    let paths = new_router_fixture("router-claim-invalid-email-header").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_claim_returns_kotlin_already_claimed_message() {
    let paths = new_router_fixture("router-claim-already-claimed-message").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let first_response = app
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

    let second_response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_login_set_cookie_returns_session_cookie_for_header_session() {
    let paths = new_router_fixture("router-login-set-cookie-session-header").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_logout_get_clears_session_cookie() {
    let paths = new_router_fixture("router-logout-get-clears-session-cookie").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/logout")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("logout get request should build"),
        )
        .await
        .expect("logout get request should complete");

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

    cleanup_router_fixture(paths);
}

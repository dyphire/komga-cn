use super::sse_events::{parse_event_log, read_sse_until};
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::time::Duration;

const MEMBER_PASSWORD: &str = "router-contract-member-123";

async fn login_member_with_remember_me(
    app: axum::Router,
    email: &str,
    password: &str,
) -> (String, String, String) {
    let basic_token = STANDARD.encode(format!("{email}:{password}"));
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me?remember-me=true")
                .header(header::AUTHORIZATION, format!("Basic {basic_token}"))
                .body(Body::empty())
                .expect("member remember-me login request should build"),
        )
        .await
        .expect("member remember-me login request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let cookies = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok().map(str::to_string))
        .collect::<Vec<_>>();

    let session_cookie = cookies
        .iter()
        .find_map(|cookie| {
            cookie
                .strip_prefix("KOMGA-SESSION=")
                .and_then(|value| value.split(';').next())
        })
        .map(str::to_string)
        .expect("member remember-me login should issue session cookie");
    let remember_me_cookie = cookies
        .iter()
        .find_map(|cookie| {
            cookie
                .strip_prefix("komga-remember-me=")
                .and_then(|value| value.split(';').next())
        })
        .map(str::to_string)
        .expect("member remember-me login should issue remember-me cookie");

    (
        session_cookie,
        remember_me_cookie,
        response_json(response).await["id"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    )
}

pub(crate) async fn verify_password_change_invalidates_existing_remember_me_cookie() {
    let member_user_id = "member-password-reset-lifecycle";
    let member_email = "member-password-reset@example.org";
    let ctx = TestFixture::new("router-remember-me-password-reset-lifecycle").await;
    seed_router_library_restricted_user(
        ctx.paths(),
        member_user_id,
        member_email,
        MEMBER_PASSWORD,
        &["library-1"],
    )
    .await;

    let app = ctx.app().clone();
    let admin_token = ctx.login_admin().await;
    let (member_session_cookie, member_remember_me_cookie, logged_in_member_user_id) =
        login_member_with_remember_me(app.clone(), member_email, MEMBER_PASSWORD).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/sse/v1/events")
                .header(
                    "x-auth-token",
                    ctx.login_with_credentials(member_email, MEMBER_PASSWORD)
                        .await,
                )
                .body(Body::empty())
                .expect("member lifecycle sse request should build"),
        )
        .await
        .expect("member lifecycle sse request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let admin_app = app.clone();
    let password_update_uri = format!("/api/v2/users/{logged_in_member_user_id}/password");
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        let response = admin_app
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(password_update_uri)
                    .header("x-auth-token", &admin_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({ "password": "updated-password-123" }).to_string(),
                    ))
                    .expect("remember-me password reset request should build"),
            )
            .await
            .expect("remember-me password reset request should complete");
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    });

    let body = read_sse_until(
        response.into_body(),
        |raw| raw.contains("event: SessionExpired"),
        Duration::from_secs(3),
    )
    .await;
    let parsed = parse_event_log(&body).expect("remember-me lifecycle sse body should parse");
    assert!(
        parsed.events.iter().any(|event| {
            event.name == "SessionExpired"
                && event.payload.get("userId")
                    == Some(&Value::String(logged_in_member_user_id.clone()))
        }),
        "password reset should emit SessionExpired for the target user: {body}"
    );

    let session_replay_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me")
                .header(
                    header::COOKIE,
                    format!("KOMGA-SESSION={member_session_cookie}"),
                )
                .body(Body::empty())
                .expect("session replay after password reset should build"),
        )
        .await
        .expect("session replay after password reset should complete");
    assert_eq!(session_replay_response.status(), StatusCode::UNAUTHORIZED);

    let remember_replay_response = app
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
                .expect("remember-me replay after password reset should build"),
        )
        .await
        .expect("remember-me replay after password reset should complete");
    assert_eq!(remember_replay_response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn password_change_invalidates_existing_remember_me_cookie() {
    verify_password_change_invalidates_existing_remember_me_cookie().await;
}

pub(crate) async fn verify_self_password_change_keeps_session_but_invalidates_old_remember_me() {
    let member_user_id = "member-self-password-change";
    let member_email = "member-self-password@example.org";
    let ctx = TestFixture::new("router-remember-me-self-password-change").await;
    seed_router_library_restricted_user(
        ctx.paths(),
        member_user_id,
        member_email,
        MEMBER_PASSWORD,
        &["library-1"],
    )
    .await;

    let app = ctx.app().clone();
    let (member_session_cookie, member_remember_me_cookie, _) =
        login_member_with_remember_me(app.clone(), member_email, MEMBER_PASSWORD).await;

    let patch_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/v2/users/{member_user_id}/password"))
                .header(
                    header::COOKIE,
                    format!("KOMGA-SESSION={member_session_cookie}"),
                )
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "password": "router-contract-member-456" }).to_string(),
                ))
                .expect("self password remember-me patch request should build"),
        )
        .await
        .expect("self password remember-me patch request should complete");
    assert_eq!(patch_response.status(), StatusCode::NO_CONTENT);

    let existing_session_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me")
                .header(
                    header::COOKIE,
                    format!("KOMGA-SESSION={member_session_cookie}"),
                )
                .body(Body::empty())
                .expect("self password current session replay should build"),
        )
        .await
        .expect("self password current session replay should complete");
    assert_eq!(existing_session_response.status(), StatusCode::OK);

    let remember_replay_response = app
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
                .expect("self password old remember-me replay should build"),
        )
        .await
        .expect("self password old remember-me replay should complete");
    assert_eq!(remember_replay_response.status(), StatusCode::UNAUTHORIZED);

    let new_session_token = ctx
        .login_with_credentials(member_email, "router-contract-member-456")
        .await;
    assert!(!new_session_token.is_empty());
}

#[tokio::test]
async fn self_password_change_keeps_session_but_invalidates_old_remember_me() {
    verify_self_password_change_keeps_session_but_invalidates_old_remember_me().await;
}

pub(crate) async fn verify_admin_user_update_expires_sessions_and_emits_session_expired_event() {
    let member_user_id = "member-admin-update-session-expired";
    let member_email = "member-admin-update@example.org";
    let ctx = TestFixture::new("router-admin-user-update-session-expired-sse").await;
    seed_router_library_restricted_user(
        ctx.paths(),
        member_user_id,
        member_email,
        MEMBER_PASSWORD,
        &["library-1"],
    )
    .await;

    let app = ctx.app().clone();
    let admin_token = ctx.login_admin().await;
    let member_header_token = ctx
        .login_with_credentials(member_email, MEMBER_PASSWORD)
        .await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/sse/v1/events")
                .header("x-auth-token", &member_header_token)
                .body(Body::empty())
                .expect("admin user update lifecycle sse request should build"),
        )
        .await
        .expect("admin user update lifecycle sse request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let admin_app = app.clone();
    let user_update_uri = format!("/api/v2/users/{member_user_id}");
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        let response = admin_app
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(user_update_uri)
                    .header("x-auth-token", &admin_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "sharedLibraries": {
                                "all": false,
                                "libraryIds": ["library-2"]
                            }
                        })
                        .to_string(),
                    ))
                    .expect("admin user update request should build"),
            )
            .await
            .expect("admin user update request should complete");
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    });

    let body = read_sse_until(
        response.into_body(),
        |raw| raw.contains("event: SessionExpired"),
        Duration::from_secs(3),
    )
    .await;
    let parsed = parse_event_log(&body).expect("admin user update sse body should parse");
    assert!(
        parsed.events.iter().any(|event| {
            event.name == "SessionExpired"
                && event.payload.get("userId") == Some(&Value::String(member_user_id.to_string()))
        }),
        "admin user update should emit SessionExpired for the target user: {body}"
    );

    let me_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me")
                .header("x-auth-token", &member_header_token)
                .body(Body::empty())
                .expect("admin user update existing session request should build"),
        )
        .await
        .expect("admin user update existing session request should complete");
    assert_eq!(me_response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_user_update_expires_sessions_and_emits_session_expired_event() {
    verify_admin_user_update_expires_sessions_and_emits_session_expired_event().await;
}

#[tokio::test]
async fn user_deletion_invalidates_existing_session_and_remember_me_and_emits_session_expired() {
    let member_user_id = "member-delete-session-expired";
    let member_email = "member-delete@example.org";
    let ctx = TestFixture::new("router-remember-me-user-delete-lifecycle").await;

    seed_router_library_restricted_user(
        ctx.paths(),
        member_user_id,
        member_email,
        MEMBER_PASSWORD,
        &["library-1"],
    )
    .await;

    let app = ctx.app().clone();
    let admin_token = ctx.login_admin().await;
    let member_header_token = ctx
        .login_with_credentials(member_email, MEMBER_PASSWORD)
        .await;
    let (member_session_cookie, member_remember_me_cookie, logged_in_member_user_id) =
        login_member_with_remember_me(app.clone(), member_email, MEMBER_PASSWORD).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/sse/v1/events")
                .header("x-auth-token", &member_header_token)
                .body(Body::empty())
                .expect("member delete lifecycle sse request should build"),
        )
        .await
        .expect("member delete lifecycle sse request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let admin_app = app.clone();
    let user_delete_uri = format!("/api/v2/users/{logged_in_member_user_id}");
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        let response = admin_app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(user_delete_uri)
                    .header("x-auth-token", &admin_token)
                    .body(Body::empty())
                    .expect("remember-me user delete request should build"),
            )
            .await
            .expect("remember-me user delete request should complete");
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    });

    let body = read_sse_until(
        response.into_body(),
        |raw| raw.contains("event: SessionExpired"),
        Duration::from_secs(3),
    )
    .await;
    let parsed = parse_event_log(&body).expect("delete lifecycle sse body should parse");
    assert!(
        parsed.events.iter().any(|event| {
            event.name == "SessionExpired"
                && event.payload.get("userId")
                    == Some(&Value::String(logged_in_member_user_id.clone()))
        }),
        "user deletion should emit SessionExpired for the target user: {body}"
    );

    let session_replay_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me")
                .header(
                    header::COOKIE,
                    format!("KOMGA-SESSION={member_session_cookie}"),
                )
                .body(Body::empty())
                .expect("session replay after delete should build"),
        )
        .await
        .expect("session replay after delete should complete");
    assert_eq!(session_replay_response.status(), StatusCode::UNAUTHORIZED);

    let remember_replay_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me")
                .header(
                    header::COOKIE,
                    format!("komga-remember-me={member_remember_me_cookie}"),
                )
                .body(Body::empty())
                .expect("remember-me replay after delete should build"),
        )
        .await
        .expect("remember-me replay after delete should complete");
    assert_eq!(remember_replay_response.status(), StatusCode::UNAUTHORIZED);
}

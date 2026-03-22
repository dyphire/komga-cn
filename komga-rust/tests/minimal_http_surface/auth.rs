use super::*;

#[tokio::test]
async fn protected_routes_reject_missing_or_empty_auth_token() {
    let app = komga_rust::app::build_router();

    for (method, uri) in [
        ("GET", "/api/v1/libraries"),
        ("GET", "/api/v1/series"),
        ("GET", "/api/v1/books"),
        ("GET", "/api/v1/books/book-1/pages"),
        ("GET", "/api/v1/books/book-1/pages/1"),
        ("GET", "/api/v1/books/book-1/pages/1/thumbnail"),
        ("GET", "/api/v1/books/book-1/thumbnail"),
        ("GET", "/api/v1/books/book-1/file"),
        ("PATCH", "/api/v1/books/book-1/read-progress"),
        ("DELETE", "/api/v1/books/book-1/read-progress"),
        ("PATCH", "/api/v1/books/book-1/progression"),
        ("GET", "/opds/v1.2/series"),
        ("GET", "/opds/v2/books/book-1/manifest"),
        ("GET", "/api/v1/login/set-cookie"),
    ] {
        let missing = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            missing.status(),
            StatusCode::UNAUTHORIZED,
            "{uri} should reject missing token"
        );

        let empty = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .header("X-Auth-Token", "")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            empty.status(),
            StatusCode::UNAUTHORIZED,
            "{uri} should reject empty token"
        );
    }
}

#[tokio::test]
async fn login_set_cookie_returns_session_headers_when_authorized() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/login/set-cookie")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(response.headers().get("x-auth-token").is_none());

    let set_cookie = response.headers().get(header::SET_COOKIE).unwrap();
    let cookie = set_cookie.to_str().unwrap();
    assert!(cookie.contains("KOMGA-SESSION="));
    assert!(cookie.contains("Path=/"));
}

#[tokio::test]
async fn users_me_requires_credentials() {
    let app = komga_rust::app::build_router();

    let missing = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);

    let empty = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header("X-Auth-Token", "")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(empty.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn users_me_returns_auth_token_header_and_json_for_basic_auth() {
    let app = komga_rust::app::build_router();

    let admin = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header(
                    header::AUTHORIZATION,
                    "Basic YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(admin.status(), StatusCode::OK);
    assert!(
        !admin
            .headers()
            .get("x-auth-token")
            .unwrap()
            .to_str()
            .unwrap()
            .is_empty()
    );
    let admin_cookie = admin
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(admin_cookie.contains("KOMGA-SESSION="));

    let admin_body = axum::body::to_bytes(admin.into_body(), usize::MAX)
        .await
        .unwrap();
    let admin_json: Value = serde_json::from_slice(&admin_body).unwrap();
    assert_eq!(admin_json["id"], "admin");
    assert_eq!(admin_json["email"], "admin@example.org");

    let user = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header(header::AUTHORIZATION, "Basic dXNlckBleGFtcGxlLm9yZzp1c2Vy")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(user.status(), StatusCode::OK);
    assert!(
        !user
            .headers()
            .get("x-auth-token")
            .unwrap()
            .to_str()
            .unwrap()
            .is_empty()
    );
    let user_cookie = user
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(user_cookie.contains("KOMGA-SESSION="));

    let user_body = axum::body::to_bytes(user.into_body(), usize::MAX)
        .await
        .unwrap();
    let user_json: Value = serde_json::from_slice(&user_body).unwrap();
    assert_eq!(user_json["id"], "0PV32486S7X3J");
    assert_eq!(user_json["email"], "user@example.org");
    assert_eq!(
        user_json["roles"],
        serde_json::json!(["FILE_DOWNLOAD", "PAGE_STREAMING", "USER"])
    );
    assert_eq!(user_json["sharedAllLibraries"], true);
    assert_eq!(user_json["sharedLibrariesIds"], serde_json::json!([]));
    assert_eq!(user_json["labelsAllow"], serde_json::json!([]));
    assert_eq!(user_json["labelsExclude"], serde_json::json!([]));
    assert!(user_json["ageRestriction"].is_null());

    let limited = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header(
                    header::AUTHORIZATION,
                    "Basic bGltaXRlZEBleGFtcGxlLm9yZzpsaW1pdGVk",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(limited.status(), StatusCode::OK);
    assert!(
        !limited
            .headers()
            .get("x-auth-token")
            .unwrap()
            .to_str()
            .unwrap()
            .is_empty()
    );
    let limited_cookie = limited
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(limited_cookie.contains("KOMGA-SESSION="));

    let limited_body = axum::body::to_bytes(limited.into_body(), usize::MAX)
        .await
        .unwrap();
    let limited_json: Value = serde_json::from_slice(&limited_body).unwrap();
    assert_eq!(limited_json["id"], "1PXGX4XP02A26");
    assert_eq!(limited_json["email"], "limited@example.org");
    assert_eq!(
        limited_json["roles"],
        serde_json::json!(["FILE_DOWNLOAD", "PAGE_STREAMING", "USER"])
    );
    assert_eq!(limited_json["sharedAllLibraries"], false);
    assert_eq!(limited_json["sharedLibrariesIds"], serde_json::json!(["1"]));
    assert_eq!(limited_json["labelsAllow"], serde_json::json!([]));
    assert_eq!(limited_json["labelsExclude"], serde_json::json!([]));
    assert!(limited_json["ageRestriction"].is_null());

    let invalid = app
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header(header::AUTHORIZATION, "Basic aW52YWxpZDp0b2tlbg==")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(invalid.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn users_me_accepts_valid_api_key_with_uppercase_header_and_sets_session_cookie() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header("X-API-Key", "compat-api-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let session_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("valid api key should issue session cookie")
        .to_str()
        .unwrap();
    assert!(session_cookie.contains("KOMGA-SESSION="));
    assert!(session_cookie.contains("HttpOnly"));

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["email"], "user@example.org");
}

#[tokio::test]
async fn users_me_accepts_valid_api_key_with_lowercase_header_and_sets_session_cookie() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header("x-api-key", "compat-api-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let session_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("valid api key should issue session cookie")
        .to_str()
        .unwrap();
    assert!(session_cookie.contains("KOMGA-SESSION="));
    assert!(session_cookie.contains("HttpOnly"));

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["email"], "user@example.org");
}

#[tokio::test]
async fn users_me_rejects_invalid_api_key_with_unauthorized() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header("x-api-key", "invalid-api-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "Unauthorized");
    assert_eq!(json["message"], "Unauthorized");
    assert_eq!(json["path"], "/api/v2/users/me");
    assert_eq!(json["status"], 401);
    assert!(
        json["timestamp"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
}

#[tokio::test]
async fn users_me_with_remember_me_returns_session_and_remember_me_cookies() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me?remember-me=true")
                .header(header::AUTHORIZATION, "Basic dXNlckBleGFtcGxlLm9yZzp1c2Vy")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get("x-auth-token").is_none());

    let set_cookies: Vec<_> = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|value| value.to_str().unwrap())
        .collect();

    assert!(
        set_cookies
            .iter()
            .any(|cookie| cookie.contains("KOMGA-SESSION="))
    );
    assert!(
        set_cookies
            .iter()
            .any(|cookie| cookie.contains("komga-remember-me="))
    );
    assert_eq!(set_cookies.len(), 2);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["id"], "0PV32486S7X3J");
    assert_eq!(json["email"], "user@example.org");
    assert_eq!(
        json["roles"],
        serde_json::json!(["FILE_DOWNLOAD", "PAGE_STREAMING", "USER"])
    );
    assert_eq!(json["sharedAllLibraries"], true);
    assert_eq!(json["sharedLibrariesIds"], serde_json::json!([]));
    assert_eq!(json["labelsAllow"], serde_json::json!([]));
    assert_eq!(json["labelsExclude"], serde_json::json!([]));
    assert!(json["ageRestriction"].is_null());
}

#[tokio::test]
async fn users_me_with_remember_me_and_empty_auth_token_returns_exchange_header() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me?remember-me=true")
                .header(header::AUTHORIZATION, "Basic dXNlckBleGFtcGxlLm9yZzp1c2Vy")
                .header("X-Auth-Token", "")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let x_auth_token = response
        .headers()
        .get("x-auth-token")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(!x_auth_token.is_empty());

    let set_cookies: Vec<_> = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|value| value.to_str().unwrap())
        .collect();

    assert_eq!(set_cookies.len(), 1);
    assert!(set_cookies[0].contains("komga-remember-me="));
    assert!(set_cookies[0].contains("Max-Age="));
    assert!(set_cookies[0].contains("Expires="));
    assert!(response.headers().get("x-auth-token").is_some());

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["id"], "0PV32486S7X3J");
    assert_eq!(json["email"], "user@example.org");
}

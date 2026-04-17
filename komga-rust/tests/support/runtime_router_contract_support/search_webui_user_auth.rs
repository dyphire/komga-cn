use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use tower::util::ServiceExt;

pub async fn login_with_basic_and_get_token(app: axum::Router) -> String {
    let basic_token = STANDARD.encode("admin@example.org:router-contract-admin-123");
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me")
                .header(header::AUTHORIZATION, format!("Basic {basic_token}"))
                .header("x-auth-token", "")
                .body(Body::empty())
                .expect("users/me request should build"),
        )
        .await
        .expect("users/me request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    response
        .headers()
        .get("x-auth-token")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("users/me login should return x-auth-token")
}

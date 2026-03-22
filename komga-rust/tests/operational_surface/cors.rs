use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt;

#[tokio::test]
async fn dev_frontend_origin_gets_cors_headers_on_normal_requests() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/claim")
                .header(header::ORIGIN, "http://127.0.0.1:8081")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-origin")
            .unwrap(),
        "http://127.0.0.1:8081"
    );
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-credentials")
            .unwrap(),
        "true"
    );
    assert!(response.headers().get("vary").is_some());
}

#[tokio::test]
async fn dev_frontend_origin_gets_cors_headers_even_on_unauthorized_protected_requests() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header(header::ORIGIN, "http://127.0.0.1:8081")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-origin")
            .unwrap(),
        "http://127.0.0.1:8081"
    );
}

#[tokio::test]
async fn dev_frontend_preflight_request_succeeds_for_runtime_api() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/api/v2/users/me")
                .header(header::ORIGIN, "http://127.0.0.1:8081")
                .header("Access-Control-Request-Method", "GET")
                .header(
                    "Access-Control-Request-Headers",
                    "authorization,x-auth-token,content-type",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-origin")
            .unwrap(),
        "http://127.0.0.1:8081"
    );
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-credentials")
            .unwrap(),
        "true"
    );
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-methods")
            .unwrap(),
        "GET,POST,PATCH,DELETE,OPTIONS"
    );
    let allow_headers = response
        .headers()
        .get("access-control-allow-headers")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(allow_headers.contains("authorization"));
    assert!(allow_headers.contains("x-auth-token"));
    assert!(allow_headers.contains("content-type"));
}

#[tokio::test]
async fn non_dev_origin_does_not_get_dev_cors_headers() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/claim")
                .header(header::ORIGIN, "http://malicious.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response
            .headers()
            .get("access-control-allow-origin")
            .is_none()
    );
}

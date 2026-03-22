use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt;

#[tokio::test]
async fn sse_events_route_requires_auth_and_accepts_session_cookie() {
    let app = komga_rust::app::build_router();

    let unauthorized = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/sse/v1/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/sse/v1/events")
                .header(header::COOKIE, "KOMGA-SESSION=komga-admin-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/event-stream"
    );
}

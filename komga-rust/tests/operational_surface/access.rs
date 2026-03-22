use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt;

use super::basic_auth;

#[tokio::test]
async fn settings_and_actuator_routes_enforce_admin_access() {
    let app = komga_rust::app::build_router();

    for (method, uri) in [
        ("GET", "/api/v1/settings"),
        ("PATCH", "/api/v1/settings"),
        ("GET", "/actuator"),
        ("GET", "/actuator/info"),
        ("GET", "/actuator/metrics"),
        ("GET", "/actuator/metrics/komga.tasks.execution"),
        ("GET", "/actuator/logfile"),
        ("POST", "/actuator/shutdown"),
    ] {
        let unauthorized = app
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
            unauthorized.status(),
            StatusCode::UNAUTHORIZED,
            "{uri} should reject missing auth"
        );

        let forbidden = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .header(header::AUTHORIZATION, basic_auth("user@example.org:user"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            forbidden.status(),
            StatusCode::FORBIDDEN,
            "{uri} should reject non-admin auth"
        );
    }
}

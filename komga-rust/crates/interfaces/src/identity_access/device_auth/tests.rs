use super::*;
use axum::http::{HeaderMap, StatusCode};

#[tokio::test]
async fn kobo_ping_rejects_requests_without_valid_auth() {
    let identity = crate::state::tests::test_identity_state().await;
    let response = kobo_ping_for_tests(
        &identity,
        "invalid-token",
        RequestConnectionInfo::default(),
        HeaderMap::new(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

use super::*;

#[test]
fn libraries_contract_target_is_registered() {
    assert_required_target_declared("libraries", "libraries_contract");
}

#[tokio::test]
async fn router_kobo_catch_all_returns_empty_json_when_proxy_disabled() {
    let paths = new_router_fixture("router-kobo-catch-all-disabled").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/unimplemented-resource")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo catch-all request should build"),
        )
        .await
        .expect("kobo catch-all request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload, serde_json::json!({}));

    cleanup_router_fixture(paths);
}

use super::*;

#[tokio::test]
async fn router_kobo_catch_all_returns_empty_json_when_proxy_disabled() {
    let ctx = TestFixture::builder("router-kobo-catch-all-disabled")
        .with_seed(|paths| async move {
            seed_admin_kobo_path_token(&paths).await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
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
}

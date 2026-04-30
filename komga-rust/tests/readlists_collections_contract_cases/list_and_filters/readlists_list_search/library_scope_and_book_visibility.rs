use super::*;

#[tokio::test]
async fn router_readlists_library_id_does_not_filter_book_ids_for_all_library_user_like_kotlin() {
    let ctx = TestFixture::new("router-readlists-library-id-all-library-user").await;
    seed_readlist_endpoint_variants(ctx.paths()).await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists?library_id=library-1&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlists library-id all-library-user request should build"),
        )
        .await
        .expect("readlists library-id all-library-user request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlists library-id all-library-user payload should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(content[0].get("id"), Some(&json!("readlist-1")));
    assert_eq!(
        content[0].get("bookIds"),
        Some(&json!(["book-1", "book-2", "book-3"]))
    );
    assert_eq!(content[0].get("filtered"), Some(&Value::Bool(false)));
}

use super::*;

#[tokio::test]
async fn router_discovery_books_list_supports_tag_nullable_operators_with_null_rows_in_runtime_owned_mode()
 {
    let ctx = TestFixture::builder("router-discovery-books-list-strict-tag-nullable-positive")
        .with_seed(|paths| async move {
            seed_router_contract_nullable_samples(&paths).await;
        })
        .build()
        .await;

    let auth_token = ctx.login_admin().await;

    for (operator, expected_id) in [
        ("is", "book-1"),
        ("isNot", "book-2"),
        ("isNull", "book-2"),
        ("isNotNull", "book-1"),
    ] {
        let body = if operator == "is" || operator == "isNot" {
            json!({
                "condition": {
                    "type": "Tag",
                    "operator": operator,
                    "value": "favorite-tag",
                }
            })
            .to_string()
        } else {
            json!({
                "condition": {
                    "type": "Tag",
                    "operator": operator,
                }
            })
            .to_string()
        };

        let response = ctx
            .app()
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/books/list?page=0&size=20")
                    .header("x-auth-token", &auth_token)
                    .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .expect("strict books/list nullable tag request should build"),
            )
            .await
            .expect("strict books/list nullable tag request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        let content = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("strict books nullable tag payload should expose content array");
        assert_eq!(
            content.len(),
            1,
            "unexpected books nullable tag count for operator={operator}",
        );
        assert_eq!(
            content[0].get("id"),
            Some(&Value::String(expected_id.to_string())),
            "unexpected books nullable tag id for operator={operator}",
        );
    }
}

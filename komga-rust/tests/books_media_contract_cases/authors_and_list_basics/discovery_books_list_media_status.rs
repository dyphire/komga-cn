use super::*;

#[tokio::test]
async fn router_discovery_books_list_supports_media_status_begins_with_in_runtime_owned_mode() {
    let ctx = TestFixture::new("router-discovery-books-list-strict-operator").await;

    let auth_token = ctx.login_admin().await;

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
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "MediaStatus",
                            "operator": "beginsWith",
                            "value": "READY"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list request should build"),
        )
        .await
        .expect("strict books/list beginsWith request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict media-status beginsWith payload should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );
}

#[tokio::test]
async fn router_discovery_books_list_supports_media_status_is_not_in_runtime_owned_mode() {
    let ctx = TestFixture::new("router-discovery-books-list-strict-media-status-is-not").await;

    let auth_token = ctx.login_admin().await;

    let excluded_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "MediaStatus",
                            "operator": "isNot",
                            "value": "READY"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list media-status isNot excluded request should build"),
        )
        .await
        .expect("strict books/list media-status isNot excluded request should complete");
    assert_eq!(excluded_response.status(), StatusCode::OK);
    let excluded_payload = response_json(excluded_response).await;
    let excluded_content = excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict media-status isNot excluded payload should expose content array");
    assert_eq!(excluded_content.len(), 0);

    let kept_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "MediaStatus",
                            "operator": "isNot",
                            "value": "UNKNOWN"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list media-status isNot kept request should build"),
        )
        .await
        .expect("strict books/list media-status isNot kept request should complete");
    assert_eq!(kept_response.status(), StatusCode::OK);
    let kept_payload = response_json(kept_response).await;
    let kept_content = kept_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict media-status isNot kept payload should expose content array");
    assert_eq!(kept_content.len(), 1);
}

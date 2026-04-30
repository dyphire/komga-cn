use super::*;

#[tokio::test]
async fn router_discovery_books_list_supports_read_status_is_and_is_not_in_runtime_owned_mode() {
    let ctx = TestFixture::builder("router-discovery-books-list-strict-read-status")
        .with_seed(|paths| async move {
            seed_router_read_progress(&paths, true).await;
        })
        .build()
        .await;

    let auth_token = ctx.login_admin().await;

    let unread_response = ctx
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
                            "type": "ReadStatus",
                            "operator": "is",
                            "value": "UNREAD"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list unread request should build"),
        )
        .await
        .expect("strict books/list unread request should complete");
    assert_eq!(unread_response.status(), StatusCode::OK);
    let unread_payload = response_json(unread_response).await;
    let unread_content = unread_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict unread payload should expose content array");
    assert_eq!(unread_content.len(), 0);

    let read_response = ctx
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
                            "type": "ReadStatus",
                            "operator": "is",
                            "value": "READ"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list read request should build"),
        )
        .await
        .expect("strict books/list read request should complete");
    assert_eq!(read_response.status(), StatusCode::OK);
    let read_payload = response_json(read_response).await;
    let read_content = read_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict read payload should expose content array");
    assert_eq!(read_content.len(), 1);
    assert_eq!(
        read_content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    let excluded_read_response = ctx
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
                            "type": "ReadStatus",
                            "operator": "isNot",
                            "value": "READ"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list read isNot excluded request should build"),
        )
        .await
        .expect("strict books/list read isNot excluded request should complete");
    assert_eq!(excluded_read_response.status(), StatusCode::OK);
    let excluded_read_payload = response_json(excluded_read_response).await;
    let excluded_read_content = excluded_read_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict read isNot excluded payload should expose content array");
    assert_eq!(excluded_read_content.len(), 0);

    let kept_not_unread_response = ctx
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
                            "type": "ReadStatus",
                            "operator": "isNot",
                            "value": "UNREAD"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list read isNot kept request should build"),
        )
        .await
        .expect("strict books/list read isNot kept request should complete");
    assert_eq!(kept_not_unread_response.status(), StatusCode::OK);
    let kept_not_unread_payload = response_json(kept_not_unread_response).await;
    let kept_not_unread_content = kept_not_unread_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict read isNot kept payload should expose content array");
    assert_eq!(kept_not_unread_content.len(), 1);
    assert_eq!(
        kept_not_unread_content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );
}

#[tokio::test]
async fn router_discovery_books_list_supports_library_id_in_runtime_owned_mode() {
    let ctx = TestFixture::new("router-discovery-books-list-strict-library-id").await;

    let auth_token = ctx.login_admin().await;

    let matched_response = ctx
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
                            "type": "LibraryId",
                            "operator": "is",
                            "value": "library-1"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list library-id match request should build"),
        )
        .await
        .expect("strict books/list library-id match request should complete");
    assert_eq!(matched_response.status(), StatusCode::OK);
    let matched_payload = response_json(matched_response).await;
    let matched_content = matched_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books library-id match payload should expose content array");
    assert_eq!(matched_content.len(), 1);

    let missing_response = ctx
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
                            "type": "LibraryId",
                            "operator": "is",
                            "value": "library-missing"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list library-id miss request should build"),
        )
        .await
        .expect("strict books/list library-id miss request should complete");
    assert_eq!(missing_response.status(), StatusCode::OK);
    let missing_payload = response_json(missing_response).await;
    let missing_content = missing_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books library-id miss payload should expose content array");
    assert_eq!(missing_content.len(), 0);
}

#[tokio::test]
async fn router_discovery_books_list_supports_oneshot_filter_in_runtime_owned_mode() {
    let ctx = TestFixture::new("router-discovery-books-list-strict-oneshot").await;

    let auth_token = ctx.login_admin().await;

    let oneshot_true_response = ctx
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
                            "type": "OneShot",
                            "operator": "isTrue"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list oneshot=true request should build"),
        )
        .await
        .expect("strict books/list oneshot=true request should complete");
    assert_eq!(oneshot_true_response.status(), StatusCode::OK);
    let oneshot_true_payload = response_json(oneshot_true_response).await;
    let oneshot_true_content = oneshot_true_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict oneshot=true payload should expose content array");
    assert_eq!(oneshot_true_content.len(), 0);

    let oneshot_false_response = ctx
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
                            "type": "OneShot",
                            "operator": "isFalse"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list oneshot=false request should build"),
        )
        .await
        .expect("strict books/list oneshot=false request should complete");
    assert_eq!(oneshot_false_response.status(), StatusCode::OK);
    let oneshot_false_payload = response_json(oneshot_false_response).await;
    let oneshot_false_content = oneshot_false_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict oneshot=false payload should expose content array");
    assert_eq!(oneshot_false_content.len(), 1);
    assert_eq!(
        oneshot_false_content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );
}

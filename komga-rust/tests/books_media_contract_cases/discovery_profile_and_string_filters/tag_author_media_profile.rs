use super::*;

#[tokio::test]
async fn router_discovery_books_list_supports_tag_author_media_profile_in_runtime_owned_mode() {
    let ctx = TestFixture::new("router-discovery-books-list-strict-tag-author-media-profile").await;

    let auth_token = ctx.login_admin().await;

    let tag_match = ctx.app().clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Tag", "operator": "is", "value": "favorite-tag"}})
                        .to_string(),
                ))
                .expect("strict books/list tag match request should build"),
        )
        .await
        .expect("strict books/list tag match request should complete");
    assert_eq!(tag_match.status(), StatusCode::OK);
    let tag_match_payload = response_json(tag_match).await;
    let tag_match_content = tag_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books tag match payload should expose content array");
    assert_eq!(tag_match_content.len(), 1);

    let tag_miss = ctx
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
                    json!({"condition": {"type": "Tag", "operator": "is", "value": "missing-tag"}})
                        .to_string(),
                ))
                .expect("strict books/list tag miss request should build"),
        )
        .await
        .expect("strict books/list tag miss request should complete");
    assert_eq!(tag_miss.status(), StatusCode::OK);
    let tag_miss_payload = response_json(tag_miss).await;
    let tag_miss_content = tag_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books tag miss payload should expose content array");
    assert_eq!(tag_miss_content.len(), 0);

    let tag_is_not = ctx.app().clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Tag", "operator": "isNot", "value": "favorite-tag"}})
                        .to_string(),
                ))
                .expect("strict books/list tag isNot request should build"),
        )
        .await
        .expect("strict books/list tag isNot request should complete");
    assert_eq!(tag_is_not.status(), StatusCode::OK);
    let tag_is_not_payload = response_json(tag_is_not).await;
    let tag_is_not_content = tag_is_not_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books tag isNot payload should expose content array");
    assert_eq!(tag_is_not_content.len(), 0);

    let tag_is_null = ctx
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
                    json!({"condition": {"type": "Tag", "operator": "isNull"}}).to_string(),
                ))
                .expect("strict books/list tag isNull request should build"),
        )
        .await
        .expect("strict books/list tag isNull request should complete");
    assert_eq!(tag_is_null.status(), StatusCode::OK);
    let tag_is_null_payload = response_json(tag_is_null).await;
    let tag_is_null_content = tag_is_null_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books tag isNull payload should expose content array");
    assert_eq!(tag_is_null_content.len(), 0);

    let tag_is_not_null = ctx
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
                    json!({"condition": {"type": "Tag", "operator": "isNotNull"}}).to_string(),
                ))
                .expect("strict books/list tag isNotNull request should build"),
        )
        .await
        .expect("strict books/list tag isNotNull request should complete");
    assert_eq!(tag_is_not_null.status(), StatusCode::OK);
    let tag_is_not_null_payload = response_json(tag_is_not_null).await;
    let tag_is_not_null_content = tag_is_not_null_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books tag isNotNull payload should expose content array");
    assert_eq!(tag_is_not_null_content.len(), 1);

    let author_match = ctx.app().clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Author", "operator": "contains", "value": "jane"}})
                        .to_string(),
                ))
                .expect("strict books/list author match request should build"),
        )
        .await
        .expect("strict books/list author match request should complete");
    assert_eq!(author_match.status(), StatusCode::OK);
    let author_match_payload = response_json(author_match).await;
    let author_match_content = author_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books author match payload should expose content array");
    assert_eq!(author_match_content.len(), 1);

    let author_miss = ctx.app().clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Author", "operator": "contains", "value": "missing"}})
                        .to_string(),
                ))
                .expect("strict books/list author miss request should build"),
        )
        .await
        .expect("strict books/list author miss request should complete");
    assert_eq!(author_miss.status(), StatusCode::OK);
    let author_miss_payload = response_json(author_miss).await;
    let author_miss_content = author_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books author miss payload should expose content array");
    assert_eq!(author_miss_content.len(), 0);

    let author_role_match = ctx
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
                            "type": "Author",
                            "operator": "is",
                            "value": {
                                "name": "Jane Writer",
                                "role": "writer"
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list author role match request should build"),
        )
        .await
        .expect("strict books/list author role match request should complete");
    assert_eq!(author_role_match.status(), StatusCode::OK);
    let author_role_match_payload = response_json(author_role_match).await;
    let author_role_match_content = author_role_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books author role match payload should expose content array");
    assert_eq!(author_role_match_content.len(), 1);

    let author_role_miss = ctx
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
                            "type": "Author",
                            "operator": "is",
                            "value": {
                                "name": "Jane Writer",
                                "role": "editor"
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list author role miss request should build"),
        )
        .await
        .expect("strict books/list author role miss request should complete");
    assert_eq!(author_role_miss.status(), StatusCode::OK);
    let author_role_miss_payload = response_json(author_role_miss).await;
    let author_role_miss_content = author_role_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books author role miss payload should expose content array");
    assert_eq!(author_role_miss_content.len(), 0);

    let poster_match = ctx
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
                            "type": "Poster",
                            "operator": "is",
                            "value": {
                                "type": "USER_UPLOADED",
                                "selected": true
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list poster match request should build"),
        )
        .await
        .expect("strict books/list poster match request should complete");
    assert_eq!(poster_match.status(), StatusCode::OK);
    let poster_match_payload = response_json(poster_match).await;
    let poster_match_content = poster_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books poster match payload should expose content array");
    assert_eq!(poster_match_content.len(), 1);

    let poster_excluded = ctx
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
                            "type": "Poster",
                            "operator": "isNot",
                            "value": {
                                "type": "USER_UPLOADED",
                                "selected": true
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list poster excluded request should build"),
        )
        .await
        .expect("strict books/list poster excluded request should complete");
    assert_eq!(poster_excluded.status(), StatusCode::OK);
    let poster_excluded_payload = response_json(poster_excluded).await;
    let poster_excluded_content = poster_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books poster excluded payload should expose content array");
    assert_eq!(poster_excluded_content.len(), 0);

    let media_profile_match = ctx.app().clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "MediaProfile", "operator": "is", "value": "epub"}})
                        .to_string(),
                ))
                .expect("strict books/list media profile match request should build"),
        )
        .await
        .expect("strict books/list media profile match request should complete");
    assert_eq!(media_profile_match.status(), StatusCode::OK);
    let media_profile_match_payload = response_json(media_profile_match).await;
    let media_profile_match_content = media_profile_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books media profile match payload should expose content array");
    assert_eq!(media_profile_match_content.len(), 1);

    let media_profile_miss = ctx.app().clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "MediaProfile", "operator": "is", "value": "pdf"}})
                        .to_string(),
                ))
                .expect("strict books/list media profile miss request should build"),
        )
        .await
        .expect("strict books/list media profile miss request should complete");
    assert_eq!(media_profile_miss.status(), StatusCode::OK);
    let media_profile_miss_payload = response_json(media_profile_miss).await;
    let media_profile_miss_content = media_profile_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books media profile miss payload should expose content array");
    assert_eq!(media_profile_miss_content.len(), 0);

    let media_profile_is_not_excluded = ctx.app().clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "MediaProfile", "operator": "isNot", "value": "epub"}})
                        .to_string(),
                ))
                .expect("strict books/list media profile isNot excluded request should build"),
        )
        .await
        .expect("strict books/list media profile isNot excluded request should complete");
    assert_eq!(media_profile_is_not_excluded.status(), StatusCode::OK);
    let media_profile_is_not_excluded_payload = response_json(media_profile_is_not_excluded).await;
    let media_profile_is_not_excluded_content = media_profile_is_not_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books media profile isNot excluded payload should expose content array");
    assert_eq!(media_profile_is_not_excluded_content.len(), 0);

    let media_profile_is_not_kept = ctx.app().clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "MediaProfile", "operator": "isNot", "value": "pdf"}})
                        .to_string(),
                ))
                .expect("strict books/list media profile isNot kept request should build"),
        )
        .await
        .expect("strict books/list media profile isNot kept request should complete");
    assert_eq!(media_profile_is_not_kept.status(), StatusCode::OK);
    let media_profile_is_not_kept_payload = response_json(media_profile_is_not_kept).await;
    let media_profile_is_not_kept_content = media_profile_is_not_kept_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books media profile isNot kept payload should expose content array");
    assert_eq!(media_profile_is_not_kept_content.len(), 1);
}

use super::*;

#[tokio::test]
async fn router_discovery_books_list_supports_tag_author_media_profile_in_runtime_owned_mode() {
    let paths =
        new_router_fixture("router-discovery-books-list-strict-tag-author-media-profile").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let tag_match = app
        .clone()
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

    let tag_miss = app
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

    let tag_is_not = app
        .clone()
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

    let tag_is_null = app
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

    let tag_is_not_null = app
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

    let author_match = app
        .clone()
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

    let author_miss = app
        .clone()
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

    let author_role_match = app
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

    let author_role_miss = app
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

    let poster_match = app
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

    let poster_excluded = app
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

    let media_profile_match = app
        .clone()
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

    let media_profile_miss = app
        .clone()
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

    let media_profile_is_not_excluded = app
        .clone()
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

    let media_profile_is_not_kept = app
        .clone()
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_tag_nullable_operators_with_null_rows_in_runtime_owned_mode()
 {
    let paths =
        new_router_fixture("router-discovery-books-list-strict-tag-nullable-positive").await;
    seed_router_contract_data(&paths).await;
    seed_router_contract_nullable_samples(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

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

        let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_latest_supports_sort_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-latest-strict-sort").await;
    seed_router_contract_data(&paths).await;
    seed_router_cbz_book(&paths, "book-2", "book-2.cbz", "Another Book").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/latest?page=0&size=20&sort=metadata.title,asc")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .body(Body::empty())
                .expect("strict books/latest sort request should build"),
        )
        .await
        .expect("strict books/latest sort request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books/latest sort payload should expose content array");
    assert_eq!(content.len(), 2);
    assert_eq!(content[0].get("id"), Some(&json!("book-2")));
    assert_eq!(content[1].get("id"), Some(&json!("book-1")));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_release_date_string_ops_in_runtime_owned_mode() {
    let paths =
        new_router_fixture("router-discovery-books-list-strict-release-date-string-ops").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let begins_with_match = app
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
                            "type": "ReleaseDate",
                            "operator": "beginsWith",
                            "value": "2024-01"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date beginsWith match request should build"),
        )
        .await
        .expect("strict books/list release-date beginsWith match request should complete");
    assert_eq!(begins_with_match.status(), StatusCode::OK);
    let begins_with_match_payload = response_json(begins_with_match).await;
    let begins_with_match_content = begins_with_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date beginsWith match payload should expose content array");
    assert_eq!(begins_with_match_content.len(), 1);

    let begins_with_miss = app
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
                            "type": "ReleaseDate",
                            "operator": "beginsWith",
                            "value": "2025"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date beginsWith miss request should build"),
        )
        .await
        .expect("strict books/list release-date beginsWith miss request should complete");
    assert_eq!(begins_with_miss.status(), StatusCode::OK);
    let begins_with_miss_payload = response_json(begins_with_miss).await;
    let begins_with_miss_content = begins_with_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date beginsWith miss payload should expose content array");
    assert_eq!(begins_with_miss_content.len(), 0);

    let ends_with_match = app
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
                            "type": "ReleaseDate",
                            "operator": "endsWith",
                            "value": "-15"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date endsWith match request should build"),
        )
        .await
        .expect("strict books/list release-date endsWith match request should complete");
    assert_eq!(ends_with_match.status(), StatusCode::OK);
    let ends_with_match_payload = response_json(ends_with_match).await;
    let ends_with_match_content = ends_with_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date endsWith match payload should expose content array");
    assert_eq!(ends_with_match_content.len(), 1);

    let ends_with_miss = app
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
                            "type": "ReleaseDate",
                            "operator": "endsWith",
                            "value": "-99"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date endsWith miss request should build"),
        )
        .await
        .expect("strict books/list release-date endsWith miss request should complete");
    assert_eq!(ends_with_miss.status(), StatusCode::OK);
    let ends_with_miss_payload = response_json(ends_with_miss).await;
    let ends_with_miss_content = ends_with_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date endsWith miss payload should expose content array");
    assert_eq!(ends_with_miss_content.len(), 0);

    let does_not_contain_match = app
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
                            "type": "ReleaseDate",
                            "operator": "doesNotContain",
                            "value": "2025"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date doesNotContain keep request should build"),
        )
        .await
        .expect("strict books/list release-date doesNotContain keep request should complete");
    assert_eq!(does_not_contain_match.status(), StatusCode::OK);
    let does_not_contain_match_payload = response_json(does_not_contain_match).await;
    let does_not_contain_match_content = does_not_contain_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict books release-date doesNotContain keep payload should expose content array",
        );
    assert_eq!(does_not_contain_match_content.len(), 1);

    let does_not_contain_excluded = app
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
                            "type": "ReleaseDate",
                            "operator": "doesNotContain",
                            "value": "2024"
                        }
                    })
                    .to_string(),
                ))
                .expect(
                    "strict books/list release-date doesNotContain excluded request should build",
                ),
        )
        .await
        .expect("strict books/list release-date doesNotContain excluded request should complete");
    assert_eq!(does_not_contain_excluded.status(), StatusCode::OK);
    let does_not_contain_excluded_payload = response_json(does_not_contain_excluded).await;
    let does_not_contain_excluded_content = does_not_contain_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict books release-date doesNotContain excluded payload should expose content array",
        );
    assert_eq!(does_not_contain_excluded_content.len(), 0);

    let does_not_begin_with_keep = app
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
                            "type": "ReleaseDate",
                            "operator": "doesNotBeginWith",
                            "value": "2025"
                        }
                    })
                    .to_string(),
                ))
                .expect(
                    "strict books/list release-date doesNotBeginWith keep request should build",
                ),
        )
        .await
        .expect("strict books/list release-date doesNotBeginWith keep request should complete");
    assert_eq!(does_not_begin_with_keep.status(), StatusCode::OK);
    let does_not_begin_with_keep_payload = response_json(does_not_begin_with_keep).await;
    let does_not_begin_with_keep_content = does_not_begin_with_keep_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict books release-date doesNotBeginWith keep payload should expose content array",
        );
    assert_eq!(does_not_begin_with_keep_content.len(), 1);

    let does_not_begin_with_excluded = app
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
                            "type": "ReleaseDate",
                            "operator": "doesNotBeginWith",
                            "value": "2024"
                        }
                    })
                    .to_string(),
                ))
                .expect(
                    "strict books/list release-date doesNotBeginWith excluded request should build",
                ),
        )
        .await
        .expect("strict books/list release-date doesNotBeginWith excluded request should complete");
    assert_eq!(does_not_begin_with_excluded.status(), StatusCode::OK);
    let does_not_begin_with_excluded_payload = response_json(does_not_begin_with_excluded).await;
    let does_not_begin_with_excluded_content = does_not_begin_with_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict books release-date doesNotBeginWith excluded payload should expose content array",
        );
    assert_eq!(does_not_begin_with_excluded_content.len(), 0);

    let does_not_end_with_keep = app
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
                            "type": "ReleaseDate",
                            "operator": "doesNotEndWith",
                            "value": "-99"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date doesNotEndWith keep request should build"),
        )
        .await
        .expect("strict books/list release-date doesNotEndWith keep request should complete");
    assert_eq!(does_not_end_with_keep.status(), StatusCode::OK);
    let does_not_end_with_keep_payload = response_json(does_not_end_with_keep).await;
    let does_not_end_with_keep_content = does_not_end_with_keep_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict books release-date doesNotEndWith keep payload should expose content array",
        );
    assert_eq!(does_not_end_with_keep_content.len(), 1);

    let does_not_end_with_excluded = app
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
                            "type": "ReleaseDate",
                            "operator": "doesNotEndWith",
                            "value": "-15"
                        }
                    })
                    .to_string(),
                ))
                .expect(
                    "strict books/list release-date doesNotEndWith excluded request should build",
                ),
        )
        .await
        .expect("strict books/list release-date doesNotEndWith excluded request should complete");
    assert_eq!(does_not_end_with_excluded.status(), StatusCode::OK);
    let does_not_end_with_excluded_payload = response_json(does_not_end_with_excluded).await;
    let does_not_end_with_excluded_content = does_not_end_with_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict books release-date doesNotEndWith excluded payload should expose content array",
        );
    assert_eq!(does_not_end_with_excluded_content.len(), 0);

    cleanup_router_fixture(paths);
}

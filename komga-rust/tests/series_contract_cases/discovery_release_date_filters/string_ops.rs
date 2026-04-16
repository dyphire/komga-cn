use super::*;

#[tokio::test]
async fn router_discovery_series_list_supports_release_date_string_ops_in_runtime_owned_mode() {
    let paths =
        new_router_fixture("router-discovery-series-list-strict-release-date-string-ops").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let begins_with_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
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
                .expect("strict series/list release-date beginsWith match request should build"),
        )
        .await
        .expect("strict series/list release-date beginsWith match request should complete");
    assert_eq!(begins_with_match.status(), StatusCode::OK);
    let begins_with_match_payload = response_json(begins_with_match).await;
    let begins_with_match_content = begins_with_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date beginsWith match payload should expose content array");
    assert_eq!(begins_with_match_content.len(), 1);

    let begins_with_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
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
                .expect("strict series/list release-date beginsWith miss request should build"),
        )
        .await
        .expect("strict series/list release-date beginsWith miss request should complete");
    assert_eq!(begins_with_miss.status(), StatusCode::OK);
    let begins_with_miss_payload = response_json(begins_with_miss).await;
    let begins_with_miss_content = begins_with_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date beginsWith miss payload should expose content array");
    assert_eq!(begins_with_miss_content.len(), 0);

    let ends_with_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
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
                .expect("strict series/list release-date endsWith match request should build"),
        )
        .await
        .expect("strict series/list release-date endsWith match request should complete");
    assert_eq!(ends_with_match.status(), StatusCode::OK);
    let ends_with_match_payload = response_json(ends_with_match).await;
    let ends_with_match_content = ends_with_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date endsWith match payload should expose content array");
    assert_eq!(ends_with_match_content.len(), 1);

    let ends_with_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
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
                .expect("strict series/list release-date endsWith miss request should build"),
        )
        .await
        .expect("strict series/list release-date endsWith miss request should complete");
    assert_eq!(ends_with_miss.status(), StatusCode::OK);
    let ends_with_miss_payload = response_json(ends_with_miss).await;
    let ends_with_miss_content = ends_with_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date endsWith miss payload should expose content array");
    assert_eq!(ends_with_miss_content.len(), 0);

    let does_not_contain_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
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
                .expect("strict series/list release-date doesNotContain keep request should build"),
        )
        .await
        .expect("strict series/list release-date doesNotContain keep request should complete");
    assert_eq!(does_not_contain_match.status(), StatusCode::OK);
    let does_not_contain_match_payload = response_json(does_not_contain_match).await;
    let does_not_contain_match_content = does_not_contain_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict series release-date doesNotContain keep payload should expose content array",
        );
    assert_eq!(does_not_contain_match_content.len(), 1);

    let does_not_contain_excluded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
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
                    "strict series/list release-date doesNotContain excluded request should build",
                ),
        )
        .await
        .expect("strict series/list release-date doesNotContain excluded request should complete");
    assert_eq!(does_not_contain_excluded.status(), StatusCode::OK);
    let does_not_contain_excluded_payload = response_json(does_not_contain_excluded).await;
    let does_not_contain_excluded_content = does_not_contain_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict series release-date doesNotContain excluded payload should expose content array",
        );
    assert_eq!(does_not_contain_excluded_content.len(), 0);

    let does_not_begin_with_keep = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
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
                    "strict series/list release-date doesNotBeginWith keep request should build",
                ),
        )
        .await
        .expect("strict series/list release-date doesNotBeginWith keep request should complete");
    assert_eq!(does_not_begin_with_keep.status(), StatusCode::OK);
    let does_not_begin_with_keep_payload = response_json(does_not_begin_with_keep).await;
    let does_not_begin_with_keep_content = does_not_begin_with_keep_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict series release-date doesNotBeginWith keep payload should expose content array",
        );
    assert_eq!(does_not_begin_with_keep_content.len(), 1);

    let does_not_begin_with_excluded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
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
                    "strict series/list release-date doesNotBeginWith excluded request should build",
                ),
        )
        .await
        .expect(
            "strict series/list release-date doesNotBeginWith excluded request should complete",
        );
    assert_eq!(does_not_begin_with_excluded.status(), StatusCode::OK);
    let does_not_begin_with_excluded_payload = response_json(does_not_begin_with_excluded).await;
    let does_not_begin_with_excluded_content = does_not_begin_with_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict series release-date doesNotBeginWith excluded payload should expose content array",
        );
    assert_eq!(does_not_begin_with_excluded_content.len(), 0);

    let does_not_end_with_keep = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
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
                .expect("strict series/list release-date doesNotEndWith keep request should build"),
        )
        .await
        .expect("strict series/list release-date doesNotEndWith keep request should complete");
    assert_eq!(does_not_end_with_keep.status(), StatusCode::OK);
    let does_not_end_with_keep_payload = response_json(does_not_end_with_keep).await;
    let does_not_end_with_keep_content = does_not_end_with_keep_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict series release-date doesNotEndWith keep payload should expose content array",
        );
    assert_eq!(does_not_end_with_keep_content.len(), 1);

    let does_not_end_with_excluded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
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
                    "strict series/list release-date doesNotEndWith excluded request should build",
                ),
        )
        .await
        .expect("strict series/list release-date doesNotEndWith excluded request should complete");
    assert_eq!(does_not_end_with_excluded.status(), StatusCode::OK);
    let does_not_end_with_excluded_payload = response_json(does_not_end_with_excluded).await;
    let does_not_end_with_excluded_content = does_not_end_with_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict series release-date doesNotEndWith excluded payload should expose content array",
        );
    assert_eq!(does_not_end_with_excluded_content.len(), 0);

    cleanup_router_fixture(paths);
}

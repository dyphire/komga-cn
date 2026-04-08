use super::*;

#[tokio::test]
async fn router_discovery_series_list_supports_release_date_date_style_ops_in_runtime_owned_mode() {
    let paths =
        new_router_fixture("router-discovery-series-list-strict-release-date-date-style").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let after_match = app
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
                            "operator": "after",
                            "dateTime": "2024-01-01T00:00:00Z"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date after match request should build"),
        )
        .await
        .expect("strict series/list release-date after match request should complete");
    assert_eq!(after_match.status(), StatusCode::OK);
    let after_match_payload = response_json(after_match).await;
    let after_match_content = after_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date after match payload should expose content array");
    assert_eq!(after_match_content.len(), 1);

    let after_miss = app
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
                            "operator": "after",
                            "dateTime": "2024-02-01T00:00:00Z"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date after miss request should build"),
        )
        .await
        .expect("strict series/list release-date after miss request should complete");
    assert_eq!(after_miss.status(), StatusCode::OK);
    let after_miss_payload = response_json(after_miss).await;
    let after_miss_content = after_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date after miss payload should expose content array");
    assert_eq!(after_miss_content.len(), 0);

    let before_match = app
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
                            "operator": "before",
                            "dateTime": "2024-02-01T00:00:00Z"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date before match request should build"),
        )
        .await
        .expect("strict series/list release-date before match request should complete");
    assert_eq!(before_match.status(), StatusCode::OK);
    let before_match_payload = response_json(before_match).await;
    let before_match_content = before_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date before match payload should expose content array");
    assert_eq!(before_match_content.len(), 1);

    let before_miss = app
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
                            "operator": "before",
                            "dateTime": "2024-01-01T00:00:00Z"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date before miss request should build"),
        )
        .await
        .expect("strict series/list release-date before miss request should complete");
    assert_eq!(before_miss.status(), StatusCode::OK);
    let before_miss_payload = response_json(before_miss).await;
    let before_miss_content = before_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date before miss payload should expose content array");
    assert_eq!(before_miss_content.len(), 0);

    let is_in_the_last_match = app
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
                            "operator": "isInTheLast",
                            "duration": "P10000D"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date isInTheLast match request should build"),
        )
        .await
        .expect("strict series/list release-date isInTheLast match request should complete");
    assert_eq!(is_in_the_last_match.status(), StatusCode::OK);
    let is_in_the_last_match_payload = response_json(is_in_the_last_match).await;
    let is_in_the_last_match_content = is_in_the_last_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date isInTheLast match payload should expose content array");
    assert_eq!(is_in_the_last_match_content.len(), 1);

    let is_in_the_last_miss = app
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
                            "operator": "isInTheLast",
                            "duration": "P1D"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date isInTheLast miss request should build"),
        )
        .await
        .expect("strict series/list release-date isInTheLast miss request should complete");
    assert_eq!(is_in_the_last_miss.status(), StatusCode::OK);
    let is_in_the_last_miss_payload = response_json(is_in_the_last_miss).await;
    let is_in_the_last_miss_content = is_in_the_last_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date isInTheLast miss payload should expose content array");
    assert_eq!(is_in_the_last_miss_content.len(), 0);

    let is_not_in_the_last_match = app
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
                            "operator": "isNotInTheLast",
                            "duration": "P1D"
                        }
                    })
                    .to_string(),
                ))
                .expect(
                    "strict series/list release-date isNotInTheLast match request should build",
                ),
        )
        .await
        .expect("strict series/list release-date isNotInTheLast match request should complete");
    assert_eq!(is_not_in_the_last_match.status(), StatusCode::OK);
    let is_not_in_the_last_match_payload = response_json(is_not_in_the_last_match).await;
    let is_not_in_the_last_match_content = is_not_in_the_last_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict series release-date isNotInTheLast match payload should expose content array",
        );
    assert_eq!(is_not_in_the_last_match_content.len(), 1);

    let is_not_in_the_last_miss = app
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
                            "operator": "isNotInTheLast",
                            "duration": "P10000D"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date isNotInTheLast miss request should build"),
        )
        .await
        .expect("strict series/list release-date isNotInTheLast miss request should complete");
    assert_eq!(is_not_in_the_last_miss.status(), StatusCode::OK);
    let is_not_in_the_last_miss_payload = response_json(is_not_in_the_last_miss).await;
    let is_not_in_the_last_miss_content = is_not_in_the_last_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict series release-date isNotInTheLast miss payload should expose content array",
        );
    assert_eq!(is_not_in_the_last_miss_content.len(), 0);

    cleanup_router_fixture(paths);
}

use super::*;

#[tokio::test]
async fn router_discovery_series_list_supports_release_date_is_and_is_not_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-series-list-strict-release-date").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let matched_response = app
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
                            "operator": "is",
                            "value": "2024-01-15"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date is request should build"),
        )
        .await
        .expect("strict series/list release-date is request should complete");
    assert_eq!(matched_response.status(), StatusCode::OK);
    let matched_payload = response_json(matched_response).await;
    let matched_content = matched_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date is payload should expose content array");
    assert_eq!(matched_content.len(), 1);

    let excluded_response = app
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
                            "operator": "isNot",
                            "value": "2024-01-15"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date isNot excluded request should build"),
        )
        .await
        .expect("strict series/list release-date isNot excluded request should complete");
    assert_eq!(excluded_response.status(), StatusCode::OK);
    let excluded_payload = response_json(excluded_response).await;
    let excluded_content = excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date isNot excluded payload should expose content array");
    assert_eq!(excluded_content.len(), 0);

    let kept_response = app
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
                            "operator": "isNot",
                            "value": "2025-01-15"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date isNot kept request should build"),
        )
        .await
        .expect("strict series/list release-date isNot kept request should complete");
    assert_eq!(kept_response.status(), StatusCode::OK);
    let kept_payload = response_json(kept_response).await;
    let kept_content = kept_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date isNot kept payload should expose content array");
    assert_eq!(kept_content.len(), 1);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_release_date_null_operators_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-series-list-strict-release-date-null").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let is_null_response = app
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
                            "operator": "isNull"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date isNull request should build"),
        )
        .await
        .expect("strict series/list release-date isNull request should complete");
    assert_eq!(is_null_response.status(), StatusCode::OK);
    let is_null_payload = response_json(is_null_response).await;
    let is_null_content = is_null_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date isNull payload should expose content array");
    assert_eq!(is_null_content.len(), 0);

    let is_not_null_response = app
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
                            "operator": "isNotNull"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date isNotNull request should build"),
        )
        .await
        .expect("strict series/list release-date isNotNull request should complete");
    assert_eq!(is_not_null_response.status(), StatusCode::OK);
    let is_not_null_payload = response_json(is_not_null_response).await;
    let is_not_null_content = is_not_null_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date isNotNull payload should expose content array");
    assert_eq!(is_not_null_content.len(), 1);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_release_date_greater_than_and_less_than_in_runtime_owned_mode()
 {
    let paths = new_router_fixture("router-discovery-series-list-strict-release-date-range").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let gt_matched_response = app
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
                            "operator": "greaterThan",
                            "value": "2024-01-01"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date greaterThan match request should build"),
        )
        .await
        .expect("strict series/list release-date greaterThan match request should complete");
    assert_eq!(gt_matched_response.status(), StatusCode::OK);
    let gt_matched_payload = response_json(gt_matched_response).await;
    let gt_matched_content = gt_matched_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date greaterThan match payload should expose content array");
    assert_eq!(gt_matched_content.len(), 1);

    let gt_missing_response = app
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
                            "operator": "greaterThan",
                            "value": "2024-12-01"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date greaterThan missing request should build"),
        )
        .await
        .expect("strict series/list release-date greaterThan missing request should complete");
    assert_eq!(gt_missing_response.status(), StatusCode::OK);
    let gt_missing_payload = response_json(gt_missing_response).await;
    let gt_missing_content = gt_missing_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict series release-date greaterThan missing payload should expose content array",
        );
    assert_eq!(gt_missing_content.len(), 0);

    let lt_matched_response = app
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
                            "operator": "lessThan",
                            "value": "2024-12-01"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date lessThan match request should build"),
        )
        .await
        .expect("strict series/list release-date lessThan match request should complete");
    assert_eq!(lt_matched_response.status(), StatusCode::OK);
    let lt_matched_payload = response_json(lt_matched_response).await;
    let lt_matched_content = lt_matched_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date lessThan match payload should expose content array");
    assert_eq!(lt_matched_content.len(), 1);

    let lt_missing_response = app
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
                            "operator": "lessThan",
                            "value": "2024-01-01"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date lessThan missing request should build"),
        )
        .await
        .expect("strict series/list release-date lessThan missing request should complete");
    assert_eq!(lt_missing_response.status(), StatusCode::OK);
    let lt_missing_payload = response_json(lt_missing_response).await;
    let lt_missing_content = lt_missing_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date lessThan missing payload should expose content array");
    assert_eq!(lt_missing_content.len(), 0);

    cleanup_router_fixture(paths);
}

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

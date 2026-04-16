use super::*;

mod date_style_ops;

mod string_ops;

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

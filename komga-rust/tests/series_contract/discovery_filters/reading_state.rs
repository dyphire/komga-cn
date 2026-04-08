use super::*;

#[tokio::test]
async fn router_discovery_series_list_supports_read_status_is_and_is_not_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-series-list-strict-read-status").await;
    seed_router_contract_data(&paths).await;
    seed_router_series_counts(&paths, 1, Some(1)).await;
    seed_router_series_read_progress(&paths, 1, 0).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let matched_read_response = app
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
                            "type": "ReadStatus",
                            "operator": "is",
                            "value": "READ"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list read-status is=READ request should build"),
        )
        .await
        .expect("strict series/list read-status is=READ request should complete");
    assert_eq!(matched_read_response.status(), StatusCode::OK);
    let matched_read_payload = response_json(matched_read_response).await;
    let matched_read_content = matched_read_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series read-status is=READ payload should expose content array");
    assert_eq!(matched_read_content.len(), 1);

    let unmatched_unread_response = app
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
                            "type": "ReadStatus",
                            "operator": "is",
                            "value": "UNREAD"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list read-status is=UNREAD request should build"),
        )
        .await
        .expect("strict series/list read-status is=UNREAD request should complete");
    assert_eq!(unmatched_unread_response.status(), StatusCode::OK);
    let unmatched_unread_payload = response_json(unmatched_unread_response).await;
    let unmatched_unread_content = unmatched_unread_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series read-status is=UNREAD payload should expose content array");
    assert_eq!(unmatched_unread_content.len(), 0);

    let excluded_read_response = app
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
                            "type": "ReadStatus",
                            "operator": "isNot",
                            "value": "READ"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list read-status isNot=READ request should build"),
        )
        .await
        .expect("strict series/list read-status isNot=READ request should complete");
    assert_eq!(excluded_read_response.status(), StatusCode::OK);
    let excluded_read_payload = response_json(excluded_read_response).await;
    let excluded_read_content = excluded_read_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series read-status isNot=READ payload should expose content array");
    assert_eq!(excluded_read_content.len(), 0);

    let kept_not_unread_response = app
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
                            "type": "ReadStatus",
                            "operator": "isNot",
                            "value": "UNREAD"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list read-status isNot=UNREAD request should build"),
        )
        .await
        .expect("strict series/list read-status isNot=UNREAD request should complete");
    assert_eq!(kept_not_unread_response.status(), StatusCode::OK);
    let kept_not_unread_payload = response_json(kept_not_unread_response).await;
    let kept_not_unread_content = kept_not_unread_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series read-status isNot=UNREAD payload should expose content array");
    assert_eq!(kept_not_unread_content.len(), 1);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_complete_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-series-list-strict-complete").await;
    seed_router_contract_data(&paths).await;
    seed_router_series_counts(&paths, 1, Some(1)).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let complete_true_response = app
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
                            "type": "Complete",
                            "operator": "isTrue"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list complete isTrue request should build"),
        )
        .await
        .expect("strict series/list complete isTrue request should complete");
    assert_eq!(complete_true_response.status(), StatusCode::OK);
    let complete_true_payload = response_json(complete_true_response).await;
    let complete_true_content = complete_true_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series complete isTrue payload should expose content array");
    assert_eq!(complete_true_content.len(), 1);

    let complete_false_response = app
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
                            "type": "Complete",
                            "operator": "isFalse"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list complete isFalse request should build"),
        )
        .await
        .expect("strict series/list complete isFalse request should complete");
    assert_eq!(complete_false_response.status(), StatusCode::OK);
    let complete_false_payload = response_json(complete_false_response).await;
    let complete_false_content = complete_false_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series complete isFalse payload should expose content array");
    assert_eq!(complete_false_content.len(), 0);

    seed_router_series_counts(&paths, 1, Some(2)).await;

    let incomplete_false_response = app
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
                            "type": "Complete",
                            "operator": "isFalse"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list complete isFalse (incomplete) request should build"),
        )
        .await
        .expect("strict series/list complete isFalse (incomplete) request should complete");
    assert_eq!(incomplete_false_response.status(), StatusCode::OK);
    let incomplete_false_payload = response_json(incomplete_false_response).await;
    let incomplete_false_content = incomplete_false_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series complete isFalse (incomplete) payload should expose content array");
    assert_eq!(incomplete_false_content.len(), 1);

    seed_router_series_counts(&paths, 1, None).await;

    let null_total_false_response = app
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
                            "type": "Complete",
                            "operator": "isFalse"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list complete isFalse (null total) request should build"),
        )
        .await
        .expect("strict series/list complete isFalse (null total) request should complete");
    assert_eq!(null_total_false_response.status(), StatusCode::OK);
    let null_total_false_payload = response_json(null_total_false_response).await;
    let null_total_false_content = null_total_false_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series complete isFalse (null total) payload should expose content array");
    assert_eq!(null_total_false_content.len(), 0);

    cleanup_router_fixture(paths);
}

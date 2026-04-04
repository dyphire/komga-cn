use super::*;

#[tokio::test]
async fn router_discovery_series_list_supports_series_status_is_and_is_not_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-series-list-strict-series-status").await;
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
                            "type": "SeriesStatus",
                            "operator": "is",
                            "value": "ONGOING"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list status=is request should build"),
        )
        .await
        .expect("strict series/list status=is request should complete");
    assert_eq!(matched_response.status(), StatusCode::OK);
    let matched_payload = response_json(matched_response).await;
    let matched_content = matched_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series status=is payload should expose content array");
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
                            "type": "SeriesStatus",
                            "operator": "isNot",
                            "value": "ONGOING"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list status=isNot excluded request should build"),
        )
        .await
        .expect("strict series/list status=isNot excluded request should complete");
    assert_eq!(excluded_response.status(), StatusCode::OK);
    let excluded_payload = response_json(excluded_response).await;
    let excluded_content = excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series status=isNot excluded payload should expose content array");
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
                            "type": "SeriesStatus",
                            "operator": "isNot",
                            "value": "ENDED"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list status=isNot kept request should build"),
        )
        .await
        .expect("strict series/list status=isNot kept request should complete");
    assert_eq!(kept_response.status(), StatusCode::OK);
    let kept_payload = response_json(kept_response).await;
    let kept_content = kept_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series status=isNot kept payload should expose content array");
    assert_eq!(kept_content.len(), 1);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_library_id_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-series-list-strict-library-id").await;
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
                            "type": "LibraryId",
                            "operator": "is",
                            "value": "library-1"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list library-id match request should build"),
        )
        .await
        .expect("strict series/list library-id match request should complete");
    assert_eq!(matched_response.status(), StatusCode::OK);
    let matched_payload = response_json(matched_response).await;
    let matched_content = matched_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series library-id match payload should expose content array");
    assert_eq!(matched_content.len(), 1);

    let missing_response = app
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
                            "type": "LibraryId",
                            "operator": "is",
                            "value": "library-missing"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list library-id miss request should build"),
        )
        .await
        .expect("strict series/list library-id miss request should complete");
    assert_eq!(missing_response.status(), StatusCode::OK);
    let missing_payload = response_json(missing_response).await;
    let missing_content = missing_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series library-id miss payload should expose content array");
    assert_eq!(missing_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_tag_filter_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-series-list-strict-tag").await;
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
                            "type": "Tag",
                            "operator": "is",
                            "value": "favorite"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list tag match request should build"),
        )
        .await
        .expect("strict series/list tag match request should complete");
    assert_eq!(matched_response.status(), StatusCode::OK);
    let matched_payload = response_json(matched_response).await;
    let matched_content = matched_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series tag match payload should expose content array");
    assert_eq!(matched_content.len(), 1);

    let missing_response = app
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
                            "type": "Tag",
                            "operator": "is",
                            "value": "missing-tag"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list tag miss request should build"),
        )
        .await
        .expect("strict series/list tag miss request should complete");
    assert_eq!(missing_response.status(), StatusCode::OK);
    let missing_payload = response_json(missing_response).await;
    let missing_content = missing_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series tag miss payload should expose content array");
    assert_eq!(missing_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_anyof_and_allof_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-series-list-strict-anyof-allof").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let all_of_match_response = app
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
                            "type": "AllOfSeries",
                            "conditions": [
                                {"type": "LibraryId", "operator": "is", "value": "library-1"},
                                {"type": "SeriesStatus", "operator": "is", "value": "ONGOING"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list allOf match request should build"),
        )
        .await
        .expect("strict series/list allOf match request should complete");
    assert_eq!(all_of_match_response.status(), StatusCode::OK);
    let all_of_match_payload = response_json(all_of_match_response).await;
    let all_of_match_content = all_of_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series allOf match payload should expose content array");
    assert_eq!(all_of_match_content.len(), 1);

    let all_of_miss_response = app
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
                            "type": "AllOfSeries",
                            "conditions": [
                                {"type": "LibraryId", "operator": "is", "value": "library-1"},
                                {"type": "SeriesStatus", "operator": "is", "value": "ENDED"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list allOf miss request should build"),
        )
        .await
        .expect("strict series/list allOf miss request should complete");
    assert_eq!(all_of_miss_response.status(), StatusCode::OK);
    let all_of_miss_payload = response_json(all_of_miss_response).await;
    let all_of_miss_content = all_of_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series allOf miss payload should expose content array");
    assert_eq!(all_of_miss_content.len(), 0);

    let any_of_match_response = app
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
                            "type": "AnyOfSeries",
                            "conditions": [
                                {"type": "SeriesStatus", "operator": "is", "value": "ENDED"},
                                {"type": "SeriesStatus", "operator": "is", "value": "ONGOING"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list anyOf match request should build"),
        )
        .await
        .expect("strict series/list anyOf match request should complete");
    assert_eq!(any_of_match_response.status(), StatusCode::OK);
    let any_of_match_payload = response_json(any_of_match_response).await;
    let any_of_match_content = any_of_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series anyOf match payload should expose content array");
    assert_eq!(any_of_match_content.len(), 1);

    let any_of_miss_response = app
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
                            "type": "AnyOfSeries",
                            "conditions": [
                                {"type": "SeriesStatus", "operator": "is", "value": "ENDED"},
                                {"type": "SeriesStatus", "operator": "is", "value": "ENDED"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list anyOf miss request should build"),
        )
        .await
        .expect("strict series/list anyOf miss request should complete");
    assert_eq!(any_of_miss_response.status(), StatusCode::OK);
    let any_of_miss_payload = response_json(any_of_miss_response).await;
    let any_of_miss_content = any_of_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series anyOf miss payload should expose content array");
    assert_eq!(any_of_miss_content.len(), 0);

    cleanup_router_fixture(paths);
}

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

#[tokio::test]
async fn router_discovery_series_list_supports_nullable_metadata_operators_with_null_rows_in_runtime_owned_mode()
 {
    let paths =
        new_router_fixture("router-discovery-series-list-strict-nullable-metadata-positive").await;
    seed_router_contract_data(&paths).await;
    seed_router_contract_nullable_samples(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for (condition_type, operator, expected_id) in [
        ("Tag", "is", "series-1"),
        ("Tag", "isNot", "series-2"),
        ("Tag", "isNull", "series-2"),
        ("Tag", "isNotNull", "series-1"),
        ("Genre", "is", "series-1"),
        ("Genre", "isNot", "series-2"),
        ("Genre", "isNull", "series-2"),
        ("Genre", "isNotNull", "series-1"),
        ("SharingLabel", "is", "series-1"),
        ("SharingLabel", "isNot", "series-2"),
        ("SharingLabel", "isNull", "series-2"),
        ("SharingLabel", "isNotNull", "series-1"),
    ] {
        let value = match condition_type {
            "Tag" => "Favorite",
            "Genre" => "SciFi",
            _ => "Family",
        };
        let body = if operator == "is" || operator == "isNot" {
            json!({
                "condition": {
                    "type": condition_type,
                    "operator": operator,
                    "value": value,
                }
            })
            .to_string()
        } else {
            json!({
                "condition": {
                    "type": condition_type,
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
                    .uri("/api/v1/series/list?page=0&size=20")
                    .header("x-auth-token", &auth_token)
                    .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .expect("strict series/list nullable metadata request should build"),
            )
            .await
            .expect("strict series/list nullable metadata request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        let content = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("strict series nullable metadata payload should expose content array");
        assert_eq!(
            content.len(),
            1,
            "unexpected series nullable metadata count for type={condition_type}, operator={operator}",
        );
        assert_eq!(
            content[0].get("id"),
            Some(&Value::String(expected_id.to_string())),
            "unexpected series nullable metadata id for type={condition_type}, operator={operator}",
        );
    }

    cleanup_router_fixture(paths);
}

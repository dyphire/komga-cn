use super::*;

#[tokio::test]
async fn router_discovery_books_list_supports_number_sort_ops_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-number-sort").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let number_sort_is_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "NumberSort", "operator": "is", "value": 1.0}})
                        .to_string(),
                ))
                .expect("strict books/list number-sort is match request should build"),
        )
        .await
        .expect("strict books/list number-sort is match request should complete");
    assert_eq!(number_sort_is_match.status(), StatusCode::OK);
    let number_sort_is_match_payload = response_json(number_sort_is_match).await;
    let number_sort_is_match_content = number_sort_is_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books number-sort is match payload should expose content array");
    assert_eq!(number_sort_is_match_content.len(), 1);

    let number_sort_is_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "NumberSort", "operator": "is", "value": 2.0}})
                        .to_string(),
                ))
                .expect("strict books/list number-sort is miss request should build"),
        )
        .await
        .expect("strict books/list number-sort is miss request should complete");
    assert_eq!(number_sort_is_miss.status(), StatusCode::OK);
    let number_sort_is_miss_payload = response_json(number_sort_is_miss).await;
    let number_sort_is_miss_content = number_sort_is_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books number-sort is miss payload should expose content array");
    assert_eq!(number_sort_is_miss_content.len(), 0);

    let number_sort_is_not_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "NumberSort", "operator": "isNot", "value": 2.0}})
                        .to_string(),
                ))
                .expect("strict books/list number-sort isNot match request should build"),
        )
        .await
        .expect("strict books/list number-sort isNot match request should complete");
    assert_eq!(number_sort_is_not_match.status(), StatusCode::OK);
    let number_sort_is_not_match_payload = response_json(number_sort_is_not_match).await;
    let number_sort_is_not_match_content = number_sort_is_not_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books number-sort isNot match payload should expose content array");
    assert_eq!(number_sort_is_not_match_content.len(), 1);

    let number_sort_is_not_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "NumberSort", "operator": "isNot", "value": 1.0}})
                        .to_string(),
                ))
                .expect("strict books/list number-sort isNot miss request should build"),
        )
        .await
        .expect("strict books/list number-sort isNot miss request should complete");
    assert_eq!(number_sort_is_not_miss.status(), StatusCode::OK);
    let number_sort_is_not_miss_payload = response_json(number_sort_is_not_miss).await;
    let number_sort_is_not_miss_content = number_sort_is_not_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books number-sort isNot miss payload should expose content array");
    assert_eq!(number_sort_is_not_miss_content.len(), 0);

    let number_sort_gt_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "NumberSort", "operator": "greaterThan", "value": 0.0}})
                        .to_string(),
                ))
                .expect("strict books/list number-sort greaterThan match request should build"),
        )
        .await
        .expect("strict books/list number-sort greaterThan match request should complete");
    assert_eq!(number_sort_gt_match.status(), StatusCode::OK);
    let number_sort_gt_match_payload = response_json(number_sort_gt_match).await;
    let number_sort_gt_match_content = number_sort_gt_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books number-sort greaterThan match payload should expose content array");
    assert_eq!(number_sort_gt_match_content.len(), 1);

    let number_sort_gt_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "NumberSort", "operator": "greaterThan", "value": 1.0}})
                        .to_string(),
                ))
                .expect("strict books/list number-sort greaterThan miss request should build"),
        )
        .await
        .expect("strict books/list number-sort greaterThan miss request should complete");
    assert_eq!(number_sort_gt_miss.status(), StatusCode::OK);
    let number_sort_gt_miss_payload = response_json(number_sort_gt_miss).await;
    let number_sort_gt_miss_content = number_sort_gt_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books number-sort greaterThan miss payload should expose content array");
    assert_eq!(number_sort_gt_miss_content.len(), 0);

    let number_sort_lt_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "NumberSort", "operator": "lessThan", "value": 2.0}})
                        .to_string(),
                ))
                .expect("strict books/list number-sort lessThan match request should build"),
        )
        .await
        .expect("strict books/list number-sort lessThan match request should complete");
    assert_eq!(number_sort_lt_match.status(), StatusCode::OK);
    let number_sort_lt_match_payload = response_json(number_sort_lt_match).await;
    let number_sort_lt_match_content = number_sort_lt_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books number-sort lessThan match payload should expose content array");
    assert_eq!(number_sort_lt_match_content.len(), 1);

    let number_sort_lt_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "NumberSort", "operator": "lessThan", "value": 1.0}})
                        .to_string(),
                ))
                .expect("strict books/list number-sort lessThan miss request should build"),
        )
        .await
        .expect("strict books/list number-sort lessThan miss request should complete");
    assert_eq!(number_sort_lt_miss.status(), StatusCode::OK);
    let number_sort_lt_miss_payload = response_json(number_sort_lt_miss).await;
    let number_sort_lt_miss_content = number_sort_lt_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books number-sort lessThan miss payload should expose content array");
    assert_eq!(number_sort_lt_miss_content.len(), 0);

    cleanup_router_fixture(paths);
}

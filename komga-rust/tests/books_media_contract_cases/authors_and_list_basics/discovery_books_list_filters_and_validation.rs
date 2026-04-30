use super::*;

#[tokio::test]
async fn router_discovery_books_list_supports_genre_condition_in_runtime_owned_mode() {
    let ctx = TestFixture::new("router-discovery-books-list-strict-filter-combo").await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
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
                            "type": "Genre",
                            "operator": "is",
                            "value": "SciFi"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list unsupported condition request should build"),
        )
        .await
        .expect("strict books/list genre request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict genre payload should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );
}

#[tokio::test]
async fn router_discovery_books_list_rejects_unknown_condition_type_in_runtime_owned_mode() {
    let ctx = TestFixture::new("router-discovery-books-list-strict-unknown-condition").await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
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
                            "type": "UnknownBookCondition",
                            "operator": "is",
                            "value": "whatever"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list unknown-condition request should build"),
        )
        .await
        .expect("strict books/list unknown-condition request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn router_discovery_books_list_rejects_unknown_operator_in_runtime_owned_mode() {
    let ctx = TestFixture::new("router-discovery-books-list-strict-unknown-operator").await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
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
                            "type": "Deleted",
                            "operator": "maybe"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list unknown-operator request should build"),
        )
        .await
        .expect("strict books/list unknown-operator request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn router_discovery_books_list_supports_series_metadata_conditions_in_runtime_owned_mode() {
    let ctx = TestFixture::new("router-discovery-books-list-strict-series-metadata").await;

    let auth_token = ctx.login_admin().await;

    for condition in [
        json!({ "type": "Language", "operator": "is", "value": "EN" }),
        json!({ "type": "Publisher", "operator": "is", "value": "PubHouse" }),
        json!({ "type": "AgeRating", "operator": "is", "value": 16 }),
    ] {
        let response = ctx
            .app()
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/books/list?page=0&size=20")
                    .header("x-auth-token", &auth_token)
                    .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({ "condition": condition }).to_string()))
                    .expect("strict books/list series metadata request should build"),
            )
            .await
            .expect("strict books/list series metadata request should complete");

        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        let content = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("strict series metadata payload should expose content array");
        assert_eq!(content.len(), 1);
        assert_eq!(
            content[0].get("id"),
            Some(&Value::String("book-1".to_string()))
        );
    }
}

#[tokio::test]
async fn router_discovery_books_list_series_id_with_query_is_not_silent_empty_in_runtime_owned_mode()
 {
    let ctx = TestFixture::new("router-discovery-books-list-strict-seriesid-query").await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
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
                            "type": "SeriesId",
                            "operator": "is",
                            "value": "series-1"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list seriesId request should build"),
        )
        .await
        .expect("strict books/list seriesId request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict seriesId request should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    let excluded_response = ctx
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
                            "type": "SeriesId",
                            "operator": "isNot",
                            "value": "series-1"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list seriesId isNot request should build"),
        )
        .await
        .expect("strict books/list seriesId isNot request should complete");

    assert_eq!(excluded_response.status(), StatusCode::OK);
    let excluded_payload = response_json(excluded_response).await;
    let excluded_content = excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict seriesId isNot request should expose content array");
    assert_eq!(excluded_content.len(), 0);
}

#[tokio::test]
async fn router_discovery_books_list_supports_anyof_and_allof_in_runtime_owned_mode() {
    let ctx = TestFixture::new("router-discovery-books-list-strict-anyof-allof").await;

    let auth_token = ctx.login_admin().await;

    let all_of_match_response = ctx
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
                            "type": "AllOfBook",
                            "conditions": [
                                {"type": "LibraryId", "operator": "is", "value": "library-1"},
                                {"type": "MediaStatus", "operator": "is", "value": "READY"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list allOf match request should build"),
        )
        .await
        .expect("strict books/list allOf match request should complete");
    assert_eq!(all_of_match_response.status(), StatusCode::OK);
    let all_of_match_payload = response_json(all_of_match_response).await;
    let all_of_match_content = all_of_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books allOf match payload should expose content array");
    assert_eq!(all_of_match_content.len(), 1);

    let all_of_miss_response = ctx
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
                            "type": "AllOfBook",
                            "conditions": [
                                {"type": "LibraryId", "operator": "is", "value": "library-1"},
                                {"type": "MediaStatus", "operator": "is", "value": "UNKNOWN"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list allOf miss request should build"),
        )
        .await
        .expect("strict books/list allOf miss request should complete");
    assert_eq!(all_of_miss_response.status(), StatusCode::OK);
    let all_of_miss_payload = response_json(all_of_miss_response).await;
    let all_of_miss_content = all_of_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books allOf miss payload should expose content array");
    assert_eq!(all_of_miss_content.len(), 0);

    let any_of_match_response = ctx
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
                            "type": "AnyOfBook",
                            "conditions": [
                                {"type": "MediaStatus", "operator": "is", "value": "UNKNOWN"},
                                {"type": "MediaStatus", "operator": "is", "value": "READY"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list anyOf match request should build"),
        )
        .await
        .expect("strict books/list anyOf match request should complete");
    assert_eq!(any_of_match_response.status(), StatusCode::OK);
    let any_of_match_payload = response_json(any_of_match_response).await;
    let any_of_match_content = any_of_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books anyOf match payload should expose content array");
    assert_eq!(any_of_match_content.len(), 1);

    let any_of_miss_response = ctx
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
                            "type": "AnyOfBook",
                            "conditions": [
                                {"type": "MediaStatus", "operator": "is", "value": "UNKNOWN"},
                                {"type": "MediaStatus", "operator": "is", "value": "UNKNOWN"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list anyOf miss request should build"),
        )
        .await
        .expect("strict books/list anyOf miss request should complete");
    assert_eq!(any_of_miss_response.status(), StatusCode::OK);
    let any_of_miss_payload = response_json(any_of_miss_response).await;
    let any_of_miss_content = any_of_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books anyOf miss payload should expose content array");
    assert_eq!(any_of_miss_content.len(), 0);
}

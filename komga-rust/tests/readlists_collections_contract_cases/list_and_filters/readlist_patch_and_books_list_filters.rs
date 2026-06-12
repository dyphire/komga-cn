use super::*;

#[tokio::test]
async fn router_readlist_patch_preserves_unspecified_fields() {
    let ctx = TestFixture::new("router-readlist-patch-preserves-unspecified-fields").await;
    let auth_token = ctx.login_admin().await;

    let patch = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/readlists/readlist-1")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"name":"Renamed ReadList"}"#))
                .expect("readlist patch request should build"),
        )
        .await
        .expect("readlist patch request should complete");
    assert_eq!(patch.status(), StatusCode::NO_CONTENT);

    let detail = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist detail request should build"),
        )
        .await
        .expect("readlist detail request should complete");
    assert_eq!(detail.status(), StatusCode::OK);
    let payload = response_json(detail).await;
    assert_eq!(payload.get("name"), Some(&json!("Renamed ReadList")));
    assert_eq!(payload.get("summary"), Some(&json!("")));
    assert_eq!(payload.get("ordered"), Some(&json!(true)));
    assert_eq!(payload.get("bookIds"), Some(&json!(["book-1"])));
}

#[tokio::test]
async fn router_readlist_patch_rejects_malformed_json_before_merge() {
    let ctx = TestFixture::new("router-readlist-patch-malformed-json").await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/readlists/readlist-1")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"name":"Broken ReadList","#))
                .expect("readlist patch malformed-json request should build"),
        )
        .await
        .expect("readlist patch malformed-json request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_bad_request_message(
        &payload,
        "Request body must be a JSON object",
        "/api/v1/readlists/readlist-1",
    );
}

#[tokio::test]
async fn router_readlist_admin_routes_accept_basic_auth_like_kotlin_clients() {
    let ctx = TestFixture::new("router-readlist-admin-routes-basic-auth-compat").await;
    let authorization =
        basic_authorization_header_value("admin@example.org", "router-contract-admin-123");

    let create = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/readlists")
                .header(header::AUTHORIZATION, authorization.as_str())
                .header("x-auth-token", "")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": "Basic Auth ReadList",
                        "bookIds": ["book-1"]
                    })
                    .to_string(),
                ))
                .expect("readlist create basic-auth request should build"),
        )
        .await
        .expect("readlist create basic-auth request should complete");
    assert_eq!(create.status(), StatusCode::OK);
    let created = response_json(create).await;
    let readlist_id = created
        .get("id")
        .and_then(Value::as_str)
        .expect("readlist create basic-auth response should expose id")
        .to_string();

    let patch = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/v1/readlists/{readlist_id}"))
                .header(header::AUTHORIZATION, authorization.as_str())
                .header("x-auth-token", "")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": "Renamed Basic Auth ReadList",
                        "summary": "Updated through direct basic auth"
                    })
                    .to_string(),
                ))
                .expect("readlist patch basic-auth request should build"),
        )
        .await
        .expect("readlist patch basic-auth request should complete");
    assert_eq!(patch.status(), StatusCode::NO_CONTENT);

    let delete = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/readlists/{readlist_id}"))
                .header(header::AUTHORIZATION, authorization.as_str())
                .header("x-auth-token", "")
                .body(Body::empty())
                .expect("readlist delete basic-auth request should build"),
        )
        .await
        .expect("readlist delete basic-auth request should complete");
    assert_eq!(delete.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn router_readlist_create_rejects_invalid_requests_like_kotlin() {
    for (fixture_suffix, payload) in [
        ("missing-name", json!({ "bookIds": ["book-1"] })),
        ("missing-book-ids", json!({ "name": "New ReadList" })),
        (
            "blank-name",
            json!({
                "name": "   ",
                "bookIds": ["book-1"]
            }),
        ),
        (
            "empty-book-ids",
            json!({
                "name": "Empty BookIds",
                "bookIds": []
            }),
        ),
        (
            "duplicate-book-ids",
            json!({
                "name": "Duplicate BookIds",
                "bookIds": ["book-1", "book-1"]
            }),
        ),
        (
            "duplicate-name",
            json!({
                "name": "ReadList 1",
                "bookIds": ["book-1"]
            }),
        ),
    ] {
        let fixture_name = format!("router-readlist-create-{fixture_suffix}");
        let ctx = TestFixture::new(&fixture_name).await;
        let auth_token = ctx.login_admin().await;

        let response = ctx
            .app()
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/readlists")
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(payload.to_string()))
                    .expect("readlist create invalid request should build"),
            )
            .await
            .expect("readlist create invalid request should complete");

        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "fixture: {fixture_suffix}"
        );
    }
}

#[tokio::test]
async fn router_readlist_create_rejects_malformed_json_before_field_validation() {
    let ctx = TestFixture::new("router-readlist-create-malformed-json").await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/readlists")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"name":"Broken ReadList","#))
                .expect("readlist create malformed-json request should build"),
        )
        .await
        .expect("readlist create malformed-json request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_bad_request_message(
        &payload,
        "Request body must be a JSON object",
        "/api/v1/readlists",
    );
}

#[tokio::test]
async fn router_readlist_create_defaults_optional_fields_like_kotlin() {
    let ctx = TestFixture::new("router-readlist-create-defaults-optional-fields").await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/readlists")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": "Defaulted ReadList",
                        "bookIds": ["book-1"]
                    })
                    .to_string(),
                ))
                .expect("readlist create defaults request should build"),
        )
        .await
        .expect("readlist create defaults request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload.get("name"), Some(&json!("Defaulted ReadList")));
    assert_eq!(payload.get("summary"), Some(&json!("")));
    assert_eq!(payload.get("ordered"), Some(&json!(true)));
    assert_eq!(payload.get("bookIds"), Some(&json!(["book-1"])));
}

#[tokio::test]
async fn router_discovery_books_list_supports_read_list_id_ops_in_runtime_owned_mode() {
    let ctx = TestFixture::new("router-discovery-books-list-strict-read-list-id").await;
    let auth_token = ctx.login_admin().await;

    let read_list_is_match = ctx
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
                    json!({"condition": {"type": "ReadListId", "operator": "is", "value": "readlist-1"}})
                        .to_string(),
                ))
                .expect("strict books/list read-list is match request should build"),
        )
        .await
        .expect("strict books/list read-list is match request should complete");
    assert_eq!(read_list_is_match.status(), StatusCode::OK);
    let read_list_is_match_payload = response_json(read_list_is_match).await;
    let read_list_is_match_content = read_list_is_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books read-list is match payload should expose content array");
    assert_eq!(read_list_is_match_content.len(), 1);

    let read_list_is_miss = ctx
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
                    json!({"condition": {"type": "ReadListId", "operator": "is", "value": "missing-readlist"}})
                        .to_string(),
                ))
                .expect("strict books/list read-list is miss request should build"),
        )
        .await
        .expect("strict books/list read-list is miss request should complete");
    assert_eq!(read_list_is_miss.status(), StatusCode::OK);
    let read_list_is_miss_payload = response_json(read_list_is_miss).await;
    let read_list_is_miss_content = read_list_is_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books read-list is miss payload should expose content array");
    assert_eq!(read_list_is_miss_content.len(), 0);

    let read_list_is_not_match = ctx
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
                    json!({"condition": {"type": "ReadListId", "operator": "isNot", "value": "missing-readlist"}})
                        .to_string(),
                ))
                .expect("strict books/list read-list isNot match request should build"),
        )
        .await
        .expect("strict books/list read-list isNot match request should complete");
    assert_eq!(read_list_is_not_match.status(), StatusCode::OK);
    let read_list_is_not_match_payload = response_json(read_list_is_not_match).await;
    let read_list_is_not_match_content = read_list_is_not_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books read-list isNot match payload should expose content array");
    assert_eq!(read_list_is_not_match_content.len(), 1);

    let read_list_is_not_miss = ctx
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
                    json!({"condition": {"type": "ReadListId", "operator": "isNot", "value": "readlist-1"}})
                        .to_string(),
                ))
                .expect("strict books/list read-list isNot miss request should build"),
        )
        .await
        .expect("strict books/list read-list isNot miss request should complete");
    assert_eq!(read_list_is_not_miss.status(), StatusCode::OK);
    let read_list_is_not_miss_payload = response_json(read_list_is_not_miss).await;
    let read_list_is_not_miss_content = read_list_is_not_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books read-list isNot miss payload should expose content array");
    assert_eq!(read_list_is_not_miss_content.len(), 0);
}

#[tokio::test]
async fn router_discovery_books_list_supports_combined_read_list_id_filters_in_runtime_owned_mode()
{
    let ctx = TestFixture::new("router-discovery-books-list-strict-read-list-id-combined").await;

    let auth_token = ctx.login_admin().await;

    let included_response = ctx
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
                                {"type": "ReadListId", "operator": "is", "value": "readlist-1"},
                                {"type": "ReadListId", "operator": "isNot", "value": "missing-readlist"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list combined read-list include request should build"),
        )
        .await
        .expect("strict books/list combined read-list include request should complete");
    assert_eq!(included_response.status(), StatusCode::OK);
    let included_payload = response_json(included_response).await;
    let included_content = included_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books combined read-list include payload should expose content array");
    assert_eq!(included_content.len(), 1);

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
                            "type": "AllOfBook",
                            "conditions": [
                                {"type": "ReadListId", "operator": "is", "value": "readlist-1"},
                                {"type": "ReadListId", "operator": "isNot", "value": "readlist-1"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list combined read-list exclude request should build"),
        )
        .await
        .expect("strict books/list combined read-list exclude request should complete");
    assert_eq!(excluded_response.status(), StatusCode::OK);
    let excluded_payload = response_json(excluded_response).await;
    let excluded_content = excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books combined read-list exclude payload should expose content array");
    assert_eq!(excluded_content.len(), 0);
}

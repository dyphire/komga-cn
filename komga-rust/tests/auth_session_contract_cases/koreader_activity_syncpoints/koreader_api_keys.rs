use super::*;

fn assert_spring_forbidden(payload: &Value, message: &str, path: &str) {
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Forbidden".to_string()))
    );
    assert_eq!(
        payload.get("message"),
        Some(&Value::String(message.to_string()))
    );
    assert_eq!(payload.get("status"), Some(&Value::from(403)));
    assert_eq!(payload.get("path"), Some(&Value::String(path.to_string())));
    assert!(
        payload.get("timestamp").and_then(Value::as_u64).is_some(),
        "expected numeric timestamp in spring-style error payload: {payload:?}"
    );
}

#[tokio::test]
async fn router_koreader_user_auth_rejects_valid_x_auth_user_api_key_without_koreader_sync_role() {
    let paths = new_router_fixture("router-koreader-user-auth-missing-sync-role").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "member-no-koreader-sync",
        "member-no-koreader-sync@example.org",
        "member-no-koreader-sync-123",
        99,
        &["USER", "PAGE_STREAMING"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "member-no-koreader-sync@example.org",
        "member-no-koreader-sync-123",
    )
    .await;

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v2/users/me/api-keys")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "comment": "KOReader auth without sync role" }).to_string(),
                ))
                .expect("api key create request should build"),
        )
        .await
        .expect("api key create request should complete");
    assert_eq!(create_response.status(), StatusCode::OK);
    let api_key = response_json(create_response)
        .await
        .get("key")
        .and_then(Value::as_str)
        .expect("api key create response should expose key")
        .to_string();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/koreader/users/auth")
                .header("x-auth-user", &api_key)
                .body(Body::empty())
                .expect("koreader users auth valid-api-key request should build"),
        )
        .await
        .expect("koreader users auth valid-api-key request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_koreader_user_create_returns_unauthorized_for_invalid_x_auth_user() {
    let paths = new_router_fixture("router-koreader-user-create-invalid-auth-header").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/koreader/users/create")
                .header("x-auth-user", "invalid-api-key")
                .body(Body::empty())
                .expect("koreader users create request should build"),
        )
        .await
        .expect("koreader users create request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_koreader_user_create_ignores_invalid_x_api_key_for_koreader_auth() {
    let paths = new_router_fixture("router-koreader-user-create-invalid-x-api-key").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/koreader/users/create")
                .header("x-api-key", "invalid-api-key")
                .body(Body::empty())
                .expect("koreader users create x-api-key request should build"),
        )
        .await
        .expect("koreader users create x-api-key request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_koreader_user_create_returns_forbidden_without_x_auth_user_or_session() {
    let paths = new_router_fixture("router-koreader-user-create-missing-header").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/koreader/users/create")
                .body(Body::empty())
                .expect("koreader users create missing-header request should build"),
        )
        .await
        .expect("koreader users create missing-header request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("koreader users create missing-header body should be readable");
    assert!(body.is_empty(), "security gate should reject before disabled payload");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_koreader_user_create_rejects_valid_x_auth_user_api_key_without_koreader_sync_role() {
    let paths = new_router_fixture("router-koreader-user-create-missing-sync-role").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "member-no-koreader-sync",
        "member-no-koreader-sync@example.org",
        "member-no-koreader-sync-123",
        99,
        &["USER", "PAGE_STREAMING"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "member-no-koreader-sync@example.org",
        "member-no-koreader-sync-123",
    )
    .await;

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v2/users/me/api-keys")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "comment": "KOReader create without sync role" }).to_string(),
                ))
                .expect("api key create request should build"),
        )
        .await
        .expect("api key create request should complete");
    assert_eq!(create_response.status(), StatusCode::OK);
    let api_key = response_json(create_response)
        .await
        .get("key")
        .and_then(Value::as_str)
        .expect("api key create response should expose key")
        .to_string();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/koreader/users/create")
                .header("x-auth-user", &api_key)
                .body(Body::empty())
                .expect("koreader users create valid-api-key request should build"),
        )
        .await
        .expect("koreader users create valid-api-key request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("koreader users create valid-api-key body should be readable");
    assert!(body.is_empty(), "role gate should reject before disabled payload");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_koreader_user_create_returns_spring_forbidden_payload_when_creation_is_disabled() {
    let paths = new_router_fixture("router-koreader-user-create-disabled-spring-error").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v2/users/me/api-keys")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "comment": "KOReader disabled user creation" }).to_string(),
                ))
                .expect("api key create request should build"),
        )
        .await
        .expect("api key create request should complete");
    assert_eq!(create_response.status(), StatusCode::OK);
    let api_key = response_json(create_response)
        .await
        .get("key")
        .and_then(Value::as_str)
        .expect("api key create response should expose key")
        .to_string();

    let session_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/koreader/users/create")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("koreader users create session request should build"),
        )
        .await
        .expect("koreader users create session request should complete");

    assert_eq!(session_response.status(), StatusCode::FORBIDDEN);
    let session_payload = response_json(session_response).await;
    assert_spring_forbidden(
        &session_payload,
        "User creation is disabled",
        "/koreader/users/create",
    );

    let api_key_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/koreader/users/create")
                .header("x-auth-user", &api_key)
                .body(Body::empty())
                .expect("koreader users create disabled request should build"),
        )
        .await
        .expect("koreader users create disabled request should complete");

    assert_eq!(api_key_response.status(), StatusCode::FORBIDDEN);
    let payload = response_json(api_key_response).await;
    assert_spring_forbidden(
        &payload,
        "User creation is disabled",
        "/koreader/users/create",
    );

    cleanup_router_fixture(paths);
}
#[tokio::test]
async fn router_koreader_user_auth_returns_forbidden_without_x_auth_user() {
    let paths = new_router_fixture("router-koreader-user-auth-missing-header").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/koreader/users/auth")
                .body(Body::empty())
                .expect("koreader users auth missing-header request should build"),
        )
        .await
        .expect("koreader users auth missing-header request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_koreader_user_auth_accepts_koreader_sync_session_without_x_auth_user() {
    let paths = new_router_fixture("router-koreader-user-auth-session-success").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/koreader/users/auth")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("koreader users auth session request should build"),
        )
        .await
        .expect("koreader users auth session request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_json(response).await, json!({ "authorized": "OK" }));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_koreader_user_auth_rejects_empty_x_auth_user_even_with_koreader_sync_session() {
    let paths = new_router_fixture("router-koreader-user-auth-empty-header").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/koreader/users/auth")
                .header("x-auth-token", &auth_token)
                .header("x-auth-user", "")
                .body(Body::empty())
                .expect("koreader users auth empty-header request should build"),
        )
        .await
        .expect("koreader users auth empty-header request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_koreader_user_auth_accepts_valid_x_auth_user_api_key() {
    let paths = new_router_fixture("router-koreader-user-auth-valid-api-key").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v2/users/me/api-keys")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "comment": "KOReader auth" }).to_string(),
                ))
                .expect("api key create request should build"),
        )
        .await
        .expect("api key create request should complete");
    assert_eq!(create_response.status(), StatusCode::OK);
    let api_key = response_json(create_response)
        .await
        .get("key")
        .and_then(Value::as_str)
        .expect("api key create response should expose key")
        .to_string();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/koreader/users/auth")
                .header("x-auth-user", &api_key)
                .body(Body::empty())
                .expect("koreader users auth valid-api-key request should build"),
        )
        .await
        .expect("koreader users auth valid-api-key request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/vnd.koreader.v1+json")
    );
    assert_eq!(response_json(response).await, json!({ "authorized": "OK" }));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_users_me_api_keys_create_and_list_expose_expected_fields() {
    let paths = new_router_fixture("router-users-me-api-keys-roundtrip-fields").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v2/users/me/api-keys")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "comment": "Roundtrip dto" }).to_string(),
                ))
                .expect("api key roundtrip create request should build"),
        )
        .await
        .expect("api key roundtrip create request should complete");

    assert_eq!(create_response.status(), StatusCode::OK);
    let created = response_json(create_response).await;
    let created_id = created
        .get("id")
        .and_then(Value::as_str)
        .expect("api key create payload should expose id");
    let created_created_date = created
        .get("createdDate")
        .and_then(Value::as_str)
        .expect("api key create payload should expose createdDate")
        .to_string();
    assert_eq!(
        created.get("userId"),
        Some(&Value::String("admin-user".to_string()))
    );
    assert_eq!(
        created.get("comment"),
        Some(&Value::String("Roundtrip dto".to_string()))
    );
    assert!(
        created
            .get("key")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        "api key create payload should expose raw key: {created:?}"
    );
    assert!(
        created.get("createdDate").and_then(Value::as_str).is_some(),
        "api key create payload should expose createdDate: {created:?}"
    );
    assert!(
        created
            .get("lastModifiedDate")
            .and_then(Value::as_str)
            .is_some(),
        "api key create payload should expose lastModifiedDate: {created:?}"
    );

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for api key timestamp override");
    sqlx::query("UPDATE USER_API_KEY SET LAST_MODIFIED_DATE = ? WHERE ID = ?")
        .bind("2030-02-03 04:05:06")
        .bind(created_id)
        .execute(&pool)
        .await
        .expect("api key last modified date should be overridden");
    pool.close().await;

    let list_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me/api-keys")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("api key roundtrip list request should build"),
        )
        .await
        .expect("api key roundtrip list request should complete");
    assert_eq!(list_response.status(), StatusCode::OK);

    let payload = response_json(list_response).await;
    let items = payload
        .as_array()
        .expect("api key list payload should be an array");
    assert_eq!(items.len(), 1);
    let entry = &items[0];
    assert_eq!(entry.get("id").and_then(Value::as_str), Some(created_id));
    assert_eq!(entry.get("key"), Some(&Value::String("******".to_string())));
    assert_eq!(
        entry.get("comment"),
        Some(&Value::String("Roundtrip dto".to_string()))
    );
    assert!(
        entry.get("createdDate").and_then(Value::as_str).is_some(),
        "api key list entry should expose createdDate: {entry:?}"
    );
    assert_eq!(
        entry.get("createdDate").and_then(Value::as_str),
        Some(created_created_date.as_str())
    );
    assert_eq!(
        entry.get("lastModifiedDate").and_then(Value::as_str),
        Some(created_created_date.as_str()),
        "api key list entry should mirror Kotlin's createdDate-backed lastModifiedDate: {entry:?}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_users_me_api_keys_delete_is_scoped_to_current_user() {
    let paths = new_router_fixture("router-users-me-api-keys-delete-scope").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v2/users/me/api-keys")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "comment": "Delete me" }).to_string()))
                .expect("api key create for delete-scope request should build"),
        )
        .await
        .expect("api key create for delete-scope request should complete");
    assert_eq!(create_response.status(), StatusCode::OK);
    let owned_key_id = response_json(create_response)
        .await
        .get("id")
        .and_then(Value::as_str)
        .expect("created api key should expose id")
        .to_string();

    let stranger_delete = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v2/users/me/api-keys/not-my-key")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("api key delete unknown request should build"),
        )
        .await
        .expect("api key delete unknown request should complete");
    assert_eq!(stranger_delete.status(), StatusCode::NOT_FOUND);

    let owned_delete = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v2/users/me/api-keys/{owned_key_id}"))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("api key delete owned request should build"),
        )
        .await
        .expect("api key delete owned request should complete");
    assert_eq!(owned_delete.status(), StatusCode::NO_CONTENT);

    cleanup_router_fixture(paths);
}

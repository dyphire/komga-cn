use super::*;

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
async fn router_users_by_id_authentication_activity_latest_treats_blank_apikey_id_as_filter() {
    let paths = new_router_fixture("router-user-latest-auth-activity-blank-apikey-id").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for latest auth activity seed");
    sqlx::query(
        "INSERT INTO AUTHENTICATION_ACTIVITY (USER_ID, EMAIL, IP, USER_AGENT, SUCCESS, ERROR, DATE_TIME, SOURCE, API_KEY_ID, API_KEY_COMMENT) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("admin-user")
    .bind("admin@example.org")
    .bind("127.0.0.1")
    .bind("router-contract")
    .bind(true)
    .bind(Option::<String>::None)
    .bind("2024-01-02 03:04:05")
    .bind("BASIC")
    .bind(Option::<String>::None)
    .bind(Option::<String>::None)
    .execute(&pool)
    .await
    .expect("authentication activity row should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/admin-user/authentication-activity/latest?apikey_id=")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("latest auth activity blank-apikey request should build"),
        )
        .await
        .expect("latest auth activity blank-apikey request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_users_by_id_authentication_activity_latest_matches_email_only_activity_rows() {
    let paths = new_router_fixture("router-user-latest-auth-activity-email-fallback").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for email-only latest auth activity seed");
    sqlx::query(
        "INSERT INTO AUTHENTICATION_ACTIVITY (USER_ID, EMAIL, IP, USER_AGENT, SUCCESS, ERROR, DATE_TIME, SOURCE, API_KEY_ID, API_KEY_COMMENT) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Option::<String>::None)
    .bind("admin@example.org")
    .bind("127.0.0.1")
    .bind("router-contract")
    .bind(true)
    .bind(Option::<String>::None)
    .bind("2030-01-03 04:05:06")
    .bind("BASIC")
    .bind(Option::<String>::None)
    .bind(Option::<String>::None)
    .execute(&pool)
    .await
    .expect("email-only authentication activity row should be inserted");
    pool.close().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/admin-user/authentication-activity/latest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("latest auth activity email-only request should build"),
        )
        .await
        .expect("latest auth activity email-only request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("email"),
        Some(&Value::String("admin@example.org".to_string()))
    );
    assert_eq!(
        payload.get("dateTime"),
        Some(&Value::String("2030-01-03T04:05:06Z".to_string()))
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

#[tokio::test]
async fn router_users_delete_does_not_invalidate_target_users_existing_session() {
    let paths = new_router_fixture("router-users-delete-keeps-target-session").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "member-user",
        "member@example.org",
        "router-contract-member-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let admin_token = login_with_basic_and_get_token(app.clone()).await;
    let member_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "member@example.org",
        "router-contract-member-123",
    )
    .await;

    let delete_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v2/users/member-user")
                .header("x-auth-token", &admin_token)
                .body(Body::empty())
                .expect("user delete request should build"),
        )
        .await
        .expect("user delete request should complete");
    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

    let me_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me")
                .header("x-auth-token", &member_token)
                .body(Body::empty())
                .expect("deleted user existing session request should build"),
        )
        .await
        .expect("deleted user existing session request should complete");

    assert_eq!(me_response.status(), StatusCode::OK);
    let payload = response_json(me_response).await;
    assert_eq!(
        payload.get("id"),
        Some(&Value::String("member-user".to_string()))
    );
    assert_eq!(
        payload.get("email"),
        Some(&Value::String("member@example.org".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_non_admin_demo_mode_blocks_sensitive_me_endpoints() {
    let paths = new_router_fixture("router-users-me-demo-mode-forbidden-endpoints").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "member-user",
        "member@example.org",
        "router-contract-member-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_demo_config_for_paths(&paths));
    let member_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "member@example.org",
        "router-contract-member-123",
    )
    .await;

    for (method, uri, content_type, body) in [
        (
            "POST",
            "/api/v2/users/me/api-keys",
            Some("application/json"),
            Some(json!({ "comment": "Create dto" }).to_string()),
        ),
        ("GET", "/api/v2/users/me/api-keys", None, None),
        (
            "GET",
            "/api/v2/users/me/authentication-activity",
            None,
            None,
        ),
    ] {
        let mut request = Request::builder()
            .method(method)
            .uri(uri)
            .header("x-auth-token", &member_token);
        if let Some(content_type) = content_type {
            request = request.header(header::CONTENT_TYPE, content_type);
        }

        let response = app
            .clone()
            .oneshot(
                request
                    .body(match body {
                        Some(body) => Body::from(body),
                        None => Body::empty(),
                    })
                    .expect("demo me endpoint request should build"),
            )
            .await
            .expect("demo me endpoint request should complete");

        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "demo mode should forbid {method} {uri} for non-admin users"
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_users_me_authentication_activity_honors_page_and_date_time_sort() {
    let paths = new_router_fixture("router-users-me-auth-activity-page-sort").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for current-user auth activity seed");
    sqlx::query("DELETE FROM AUTHENTICATION_ACTIVITY WHERE USER_ID = ?")
        .bind("admin-user")
        .execute(&pool)
        .await
        .expect("current-user auth activity rows should delete");
    for date_time in [
        "2030-01-01 00:00:00",
        "2030-01-02 00:00:00",
        "2030-01-03 00:00:00",
    ] {
        sqlx::query(
            "INSERT INTO AUTHENTICATION_ACTIVITY (USER_ID, EMAIL, IP, USER_AGENT, SUCCESS, ERROR, DATE_TIME, SOURCE, API_KEY_ID, API_KEY_COMMENT) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("admin-user")
        .bind("admin@example.org")
        .bind("127.0.0.1")
        .bind("router-contract")
        .bind(true)
        .bind(Option::<String>::None)
        .bind(date_time)
        .bind("BASIC")
        .bind(Option::<String>::None)
        .bind(Option::<String>::None)
        .execute(&pool)
        .await
        .expect("current-user auth activity row should insert");
    }
    pool.close().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me/authentication-activity?page=1&size=1&sort=dateTime,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("current-user auth activity page-sort request should build"),
        )
        .await
        .expect("current-user auth activity page-sort request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload["totalElements"], json!(3));
    assert_eq!(payload["content"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        payload["content"][0]["dateTime"],
        json!("2030-01-02T00:00:00Z")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_users_me_authentication_activity_includes_email_only_rows() {
    let paths = new_router_fixture("router-users-me-auth-activity-email-fallback").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for current-user email-fallback seed");
    sqlx::query("DELETE FROM AUTHENTICATION_ACTIVITY")
        .execute(&pool)
        .await
        .expect("authentication activity rows should delete");
    sqlx::query(
        "INSERT INTO AUTHENTICATION_ACTIVITY (USER_ID, EMAIL, IP, USER_AGENT, SUCCESS, ERROR, DATE_TIME, SOURCE, API_KEY_ID, API_KEY_COMMENT) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Option::<String>::None)
    .bind("admin@example.org")
    .bind("127.0.0.1")
    .bind("router-contract")
    .bind(true)
    .bind(Option::<String>::None)
    .bind("2030-01-04 00:00:00")
    .bind("BASIC")
    .bind(Option::<String>::None)
    .bind(Option::<String>::None)
    .execute(&pool)
    .await
    .expect("email-only auth activity row should insert");
    sqlx::query(
        "INSERT INTO AUTHENTICATION_ACTIVITY (USER_ID, EMAIL, IP, USER_AGENT, SUCCESS, ERROR, DATE_TIME, SOURCE, API_KEY_ID, API_KEY_COMMENT) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Option::<String>::None)
    .bind("other@example.org")
    .bind("127.0.0.1")
    .bind("router-contract")
    .bind(true)
    .bind(Option::<String>::None)
    .bind("2030-01-05 00:00:00")
    .bind("BASIC")
    .bind(Option::<String>::None)
    .bind(Option::<String>::None)
    .execute(&pool)
    .await
    .expect("other-user auth activity row should insert");
    pool.close().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me/authentication-activity")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("current-user email-fallback request should build"),
        )
        .await
        .expect("current-user email-fallback request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload["totalElements"], json!(1));
    assert_eq!(payload["content"][0]["email"], json!("admin@example.org"));
    assert_eq!(
        payload["content"][0]["dateTime"],
        json!("2030-01-04T00:00:00Z")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_users_authentication_activity_honors_unpaged_date_time_sort() {
    let paths = new_router_fixture("router-users-auth-activity-unpaged-sort").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "member-user",
        "member@example.org",
        "router-contract-member-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for admin auth activity seed");
    sqlx::query("DELETE FROM AUTHENTICATION_ACTIVITY")
        .execute(&pool)
        .await
        .expect("admin auth activity rows should delete");
    for (user_id, email, date_time) in [
        ("member-user", "member@example.org", "2030-01-02 00:00:00"),
        ("admin-user", "admin@example.org", "2030-01-01 00:00:00"),
    ] {
        sqlx::query(
            "INSERT INTO AUTHENTICATION_ACTIVITY (USER_ID, EMAIL, IP, USER_AGENT, SUCCESS, ERROR, DATE_TIME, SOURCE, API_KEY_ID, API_KEY_COMMENT) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(user_id)
        .bind(email)
        .bind("127.0.0.1")
        .bind("router-contract")
        .bind(true)
        .bind(Option::<String>::None)
        .bind(date_time)
        .bind("BASIC")
        .bind(Option::<String>::None)
        .bind(Option::<String>::None)
        .execute(&pool)
        .await
        .expect("admin auth activity row should insert");
    }
    pool.close().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/authentication-activity?unpaged=true&sort=dateTime,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("admin auth activity unpaged-sort request should build"),
        )
        .await
        .expect("admin auth activity unpaged-sort request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload["totalElements"], json!(2));
    assert_eq!(
        payload["content"][0]["dateTime"],
        json!("2030-01-01T00:00:00Z")
    );
    assert_eq!(
        payload["content"][1]["dateTime"],
        json!("2030-01-02T00:00:00Z")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_users_update_expires_target_users_existing_session_when_restrictions_change() {
    let paths = new_router_fixture("router-users-update-expires-target-session").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "member-user",
        "member@example.org",
        "router-contract-member-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let admin_token = login_with_basic_and_get_token(app.clone()).await;
    let member_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "member@example.org",
        "router-contract-member-123",
    )
    .await;

    let patch_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v2/users/member-user")
                .header("x-auth-token", &admin_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "labelsAllow": ["patched"] }).to_string(),
                ))
                .expect("user patch request should build"),
        )
        .await
        .expect("user patch request should complete");
    assert_eq!(patch_response.status(), StatusCode::NO_CONTENT);

    let me_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me")
                .header("x-auth-token", &member_token)
                .body(Body::empty())
                .expect("updated user existing session request should build"),
        )
        .await
        .expect("updated user existing session request should complete");

    assert_eq!(me_response.status(), StatusCode::UNAUTHORIZED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_users_update_keeps_target_users_existing_session_when_effective_access_is_unchanged()
 {
    let paths = new_router_fixture("router-users-update-keeps-target-session-when-unchanged").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "member-user",
        "member@example.org",
        "router-contract-member-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let admin_token = login_with_basic_and_get_token(app.clone()).await;
    let member_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "member@example.org",
        "router-contract-member-123",
    )
    .await;

    let patch_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v2/users/member-user")
                .header("x-auth-token", &admin_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "sharedLibraries": {
                            "all": false,
                            "libraryIds": ["library-1"]
                        }
                    })
                    .to_string(),
                ))
                .expect("unchanged user patch request should build"),
        )
        .await
        .expect("unchanged user patch request should complete");
    assert_eq!(patch_response.status(), StatusCode::NO_CONTENT);

    let me_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me")
                .header("x-auth-token", &member_token)
                .body(Body::empty())
                .expect("unchanged updated user existing session request should build"),
        )
        .await
        .expect("unchanged updated user existing session request should complete");

    assert_eq!(me_response.status(), StatusCode::OK);
    let payload = response_json(me_response).await;
    assert_eq!(
        payload.get("id"),
        Some(&Value::String("member-user".to_string()))
    );
    assert_eq!(
        payload.get("email"),
        Some(&Value::String("member@example.org".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_users_update_keeps_session_when_labels_only_change_case_or_overlap() {
    let paths = new_router_fixture("router-users-update-keeps-session-label-normalization").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "member-user",
        "member@example.org",
        "router-contract-member-123",
        &["library-1"],
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for user label normalization seed");
    sqlx::query("INSERT INTO USER_SHARING (LABEL, ALLOW, USER_ID) VALUES (?, ?, ?)")
        .bind("patched")
        .bind(true)
        .bind("member-user")
        .execute(&pool)
        .await
        .expect("user allow label should be inserted");
    sqlx::query("INSERT INTO USER_SHARING (LABEL, ALLOW, USER_ID) VALUES (?, ?, ?)")
        .bind("both")
        .bind(false)
        .bind("member-user")
        .execute(&pool)
        .await
        .expect("user exclude label should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let admin_token = login_with_basic_and_get_token(app.clone()).await;
    let member_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "member@example.org",
        "router-contract-member-123",
    )
    .await;

    let patch_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v2/users/member-user")
                .header("x-auth-token", &admin_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "labelsAllow": ["PATCHED", "BOTH", "  "],
                        "labelsExclude": ["both", "BOTH"]
                    })
                    .to_string(),
                ))
                .expect("label normalization patch request should build"),
        )
        .await
        .expect("label normalization patch request should complete");
    assert_eq!(patch_response.status(), StatusCode::NO_CONTENT);

    let me_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me")
                .header("x-auth-token", &member_token)
                .body(Body::empty())
                .expect("label normalization existing session request should build"),
        )
        .await
        .expect("label normalization existing session request should complete");
    assert_eq!(me_response.status(), StatusCode::OK);

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for label normalization verification");
    let rows = sqlx::query(
        "SELECT LABEL, ALLOW FROM USER_SHARING WHERE USER_ID = ? ORDER BY ALLOW DESC, LABEL ASC",
    )
    .bind("member-user")
    .fetch_all(&verify_pool)
    .await
    .expect("normalized user sharing rows should be queryable");
    verify_pool.close().await;

    let normalized = rows
        .into_iter()
        .map(|row| (row.get::<String, _>("LABEL"), row.get::<bool, _>("ALLOW")))
        .collect::<Vec<_>>();
    assert_eq!(
        normalized,
        vec![("patched".to_string(), true), ("both".to_string(), false)]
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_users_create_normalizes_content_restriction_labels() {
    let paths = new_router_fixture("router-users-create-normalizes-labels").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let admin_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v2/users")
                .header("x-auth-token", &admin_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "email": "normalized@example.org",
                        "password": "normalized-secret",
                        "labelsAllow": ["PATCHED", "BOTH", "  "],
                        "labelsExclude": ["both", "BOTH"]
                    })
                    .to_string(),
                ))
                .expect("user create normalization request should build"),
        )
        .await
        .expect("user create normalization request should complete");

    assert_eq!(response.status(), StatusCode::CREATED);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("labelsAllow").and_then(Value::as_array),
        Some(&vec![Value::String("patched".to_string())])
    );
    assert_eq!(
        payload.get("labelsExclude").and_then(Value::as_array),
        Some(&vec![Value::String("both".to_string())])
    );

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for created user normalization verification");
    let user_id = payload
        .get("id")
        .and_then(Value::as_str)
        .expect("created user payload should contain id");
    let rows = sqlx::query(
        "SELECT LABEL, ALLOW FROM USER_SHARING WHERE USER_ID = ? ORDER BY ALLOW DESC, LABEL ASC",
    )
    .bind(user_id)
    .fetch_all(&verify_pool)
    .await
    .expect("normalized created user sharing rows should be queryable");
    verify_pool.close().await;

    let normalized = rows
        .into_iter()
        .map(|row| (row.get::<String, _>("LABEL"), row.get::<bool, _>("ALLOW")))
        .collect::<Vec<_>>();
    assert_eq!(
        normalized,
        vec![("patched".to_string(), true), ("both".to_string(), false)]
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_users_create_returns_kotlin_like_validation_violations() {
    for (fixture_name, request_body, field_name, message) in [
        (
            "router-users-create-invalid-email-violations",
            json!({
                "email": "invalid-email",
                "password": "normalized-secret"
            }),
            "email",
            "must be a well-formed email address",
        ),
        (
            "router-users-create-blank-password-violations",
            json!({
                "email": "valid@example.org",
                "password": "   "
            }),
            "password",
            "must not be blank",
        ),
    ] {
        let paths = new_router_fixture(fixture_name).await;
        seed_router_contract_data(&paths).await;

        let app = build_router_with_config(&runtime_config_for_paths(&paths));
        let admin_token = login_with_basic_and_get_token(app.clone()).await;

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v2/users")
                    .header("x-auth-token", &admin_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(request_body.to_string()))
                    .expect("user create validation request should build"),
            )
            .await
            .expect("user create validation request should complete");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let payload = response_json(response).await;
        assert_eq!(
            payload.get("violations"),
            Some(&json!([
                {
                    "fieldName": field_name,
                    "message": message
                }
            ]))
        );

        cleanup_router_fixture(paths);
    }
}

#[tokio::test]
async fn router_users_create_returns_spring_error_envelope_for_duplicate_email() {
    let paths = new_router_fixture("router-users-create-duplicate-email-envelope").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let admin_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v2/users")
                .header("x-auth-token", &admin_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "email": "admin@example.org",
                        "password": "normalized-secret"
                    })
                    .to_string(),
                ))
                .expect("user create duplicate email request should build"),
        )
        .await
        .expect("user create duplicate email request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Bad Request".to_string()))
    );
    assert_eq!(
        payload.get("message"),
        Some(&Value::String(
            "A user with this email already exists".to_string()
        ))
    );
    assert_eq!(payload.get("status"), Some(&Value::from(400)));
    assert_eq!(
        payload.get("path"),
        Some(&Value::String("/api/v2/users".to_string()))
    );
    assert!(
        payload.get("timestamp").and_then(Value::as_u64).is_some(),
        "duplicate-email response should include numeric timestamp: {payload:?}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_users_by_id_password_allows_self_update_without_expiring_current_session() {
    let paths = new_router_fixture("router-users-by-id-password-self-update").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "member-user",
        "member@example.org",
        "router-contract-member-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let member_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "member@example.org",
        "router-contract-member-123",
    )
    .await;

    let patch_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v2/users/member-user/password")
                .header("x-auth-token", &member_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "password": "router-contract-member-456" }).to_string(),
                ))
                .expect("self password patch request should build"),
        )
        .await
        .expect("self password patch request should complete");
    assert_eq!(patch_response.status(), StatusCode::NO_CONTENT);

    let me_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me")
                .header("x-auth-token", &member_token)
                .body(Body::empty())
                .expect("self password existing session request should build"),
        )
        .await
        .expect("self password existing session request should complete");
    assert_eq!(me_response.status(), StatusCode::OK);

    let new_login_token = login_with_basic_credentials_and_get_token(
        app,
        "member@example.org",
        "router-contract-member-456",
    )
    .await;
    assert!(!new_login_token.is_empty());

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_password_endpoints_return_forbidden_in_demo_mode() {
    let paths = new_router_fixture("router-users-password-demo-forbidden").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_demo_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for uri in [
        "/api/v2/users/me/password",
        "/api/v2/users/admin-user/password",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(uri)
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({ "password": "new-secret" }).to_string()))
                    .expect("demo password request should build"),
            )
            .await
            .expect("demo password request should complete");

        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "demo mode should forbid password updates at {uri}"
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_root_exposed_by_default_without_beans_link() {
    let paths = new_router_fixture("router-actuator-root-omits-beans-link").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("actuator root request should build"),
        )
        .await
        .expect("actuator root request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let links = payload
        .get("_links")
        .and_then(Value::as_object)
        .expect("actuator root should include links object");
    assert!(links.get("beans").is_none());

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_shutdown_requires_admin_authentication() {
    let paths = new_router_fixture("router-actuator-shutdown-auth").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/actuator/shutdown")
                .body(Body::empty())
                .expect("actuator shutdown request should build"),
        )
        .await
        .expect("actuator shutdown request should complete");

    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("actuator shutdown response body should be readable");
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "unexpected actuator shutdown status={status}, body={}",
        String::from_utf8_lossy(&body),
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_delete_syncpoints_me_without_key_id_deletes_all_syncpoints_for_current_user() {
    let paths = new_router_fixture("router-delete-syncpoints-me-all").await;
    seed_router_contract_data(&paths).await;
    seed_syncpoint_user(&paths, "other-user", "other@example.org").await;
    seed_syncpoints(
        &paths,
        &[
            ("sp-1", "admin-user", Some("key-1")),
            ("sp-2", "admin-user", Some("key-2")),
            ("sp-3", "admin-user", None),
            ("sp-4", "other-user", Some("key-1")),
        ],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/syncpoints/me")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("syncpoints delete-all request should build"),
        )
        .await
        .expect("syncpoints delete-all request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(load_syncpoint_ids(&paths).await, vec!["sp-4".to_string()]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_delete_syncpoints_me_with_repeated_key_id_deletes_only_matching_keys() {
    let paths = new_router_fixture("router-delete-syncpoints-me-many-keys").await;
    seed_router_contract_data(&paths).await;
    seed_syncpoint_user(&paths, "other-user", "other@example.org").await;
    seed_syncpoints(
        &paths,
        &[
            ("sp-1", "admin-user", Some("key-1")),
            ("sp-2", "admin-user", Some("key-2")),
            ("sp-3", "admin-user", Some("key-3")),
            ("sp-4", "other-user", Some("key-1")),
        ],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/syncpoints/me?key_id=key-1&key_id=key-3")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("syncpoints delete-many request should build"),
        )
        .await
        .expect("syncpoints delete-many request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        load_syncpoint_ids(&paths).await,
        vec!["sp-2".to_string(), "sp-4".to_string()],
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_delete_syncpoints_me_with_comma_delimited_single_key_id_deletes_matching_keys() {
    let paths = new_router_fixture("router-delete-syncpoints-me-comma-key-id").await;
    seed_router_contract_data(&paths).await;
    seed_syncpoint_user(&paths, "other-user", "other@example.org").await;
    seed_syncpoints(
        &paths,
        &[
            ("sp-1", "admin-user", Some("key-1")),
            ("sp-2", "admin-user", Some("key-2")),
            ("sp-3", "admin-user", Some("key-3")),
            ("sp-4", "other-user", Some("key-1")),
        ],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/syncpoints/me?key_id=key-1,key-3")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("syncpoints delete comma-delimited request should build"),
        )
        .await
        .expect("syncpoints delete comma-delimited request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        load_syncpoint_ids(&paths).await,
        vec!["sp-2".to_string(), "sp-4".to_string()],
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_delete_syncpoints_me_with_whitespace_only_single_key_id_does_not_delete_anything() {
    let paths = new_router_fixture("router-delete-syncpoints-me-whitespace-key-id").await;
    seed_router_contract_data(&paths).await;
    seed_syncpoint_user(&paths, "other-user", "other@example.org").await;
    seed_syncpoints(
        &paths,
        &[
            ("sp-1", "admin-user", Some("key-1")),
            ("sp-2", "admin-user", Some("key-2")),
            ("sp-3", "admin-user", None),
            ("sp-4", "other-user", Some("key-1")),
        ],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/syncpoints/me?key_id=++")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("syncpoints delete whitespace key request should build"),
        )
        .await
        .expect("syncpoints delete whitespace key request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        load_syncpoint_ids(&paths).await,
        vec![
            "sp-1".to_string(),
            "sp-2".to_string(),
            "sp-3".to_string(),
            "sp-4".to_string()
        ],
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_delete_syncpoints_me_without_key_id_deletes_syncpoint_child_rows_for_current_user()
{
    let paths = new_router_fixture("router-delete-syncpoints-me-all-subentities").await;
    seed_router_contract_data(&paths).await;
    seed_syncpoint_user(&paths, "other-user", "other@example.org").await;
    seed_syncpoints(
        &paths,
        &[
            ("sp-1", "admin-user", Some("key-1")),
            ("sp-2", "other-user", Some("key-2")),
        ],
    )
    .await;
    seed_syncpoint_children(&paths, "sp-1").await;
    seed_syncpoint_children(&paths, "sp-2").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/syncpoints/me")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("syncpoints delete-all with subentities request should build"),
        )
        .await
        .expect("syncpoints delete-all with subentities request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(load_syncpoint_ids(&paths).await, vec!["sp-2".to_string()]);
    assert_eq!(
        load_syncpoint_child_counts(&paths, "sp-1").await,
        [0, 0, 0, 0, 0]
    );
    assert_eq!(
        load_syncpoint_child_counts(&paths, "sp-2").await,
        [1, 1, 1, 1, 1]
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_delete_syncpoints_me_with_key_id_deletes_syncpoint_child_rows_only_for_matching_keys()
 {
    let paths = new_router_fixture("router-delete-syncpoints-me-key-subentities").await;
    seed_router_contract_data(&paths).await;
    seed_syncpoint_user(&paths, "other-user", "other@example.org").await;
    seed_syncpoints(
        &paths,
        &[
            ("sp-1", "admin-user", Some("key-1")),
            ("sp-2", "admin-user", Some("key-2")),
            ("sp-3", "other-user", Some("key-1")),
        ],
    )
    .await;
    seed_syncpoint_children(&paths, "sp-1").await;
    seed_syncpoint_children(&paths, "sp-2").await;
    seed_syncpoint_children(&paths, "sp-3").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/syncpoints/me?key_id=key-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("syncpoints delete key-scoped with subentities request should build"),
        )
        .await
        .expect("syncpoints delete key-scoped with subentities request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        load_syncpoint_ids(&paths).await,
        vec!["sp-2".to_string(), "sp-3".to_string()],
    );
    assert_eq!(
        load_syncpoint_child_counts(&paths, "sp-1").await,
        [0, 0, 0, 0, 0]
    );
    assert_eq!(
        load_syncpoint_child_counts(&paths, "sp-2").await,
        [1, 1, 1, 1, 1]
    );
    assert_eq!(
        load_syncpoint_child_counts(&paths, "sp-3").await,
        [1, 1, 1, 1, 1]
    );

    cleanup_router_fixture(paths);
}

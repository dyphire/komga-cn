use super::*;

#[tokio::test]
async fn router_users_delete_invalidates_target_users_existing_session() {
    let paths = new_router_fixture("router-users-delete-invalidates-target-session").await;
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

    assert_eq!(me_response.status(), StatusCode::UNAUTHORIZED);

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

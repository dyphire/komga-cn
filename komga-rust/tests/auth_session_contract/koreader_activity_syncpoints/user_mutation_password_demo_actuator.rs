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
async fn router_actuator_root_returns_unauthorized_for_anonymous() {
    let paths = new_router_fixture("router-actuator-root-anonymous-unauthorized").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator")
                .body(Body::empty())
                .expect("anonymous actuator root request should build"),
        )
        .await
        .expect("anonymous actuator root request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_root_returns_forbidden_for_authenticated_non_admin() {
    let paths = new_router_fixture("router-actuator-root-non-admin-forbidden").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "user-actuator-root-1",
        "actuator-root-user@example.org",
        "router-contract-user-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "actuator-root-user@example.org",
        "router-contract-user-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("non-admin actuator root request should build"),
        )
        .await
        .expect("non-admin actuator root request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_info_returns_build_and_os_metadata_for_admin() {
    let paths = new_router_fixture("router-actuator-info-build-and-os").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/info")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("actuator info request should build"),
        )
        .await
        .expect("actuator info request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;

    let build = payload
        .get("build")
        .and_then(Value::as_object)
        .expect("actuator info should include build object");
    assert!(
        build
            .get("artifact")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        "actuator info build.artifact should be non-empty: {payload:?}"
    );
    assert!(
        build
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        "actuator info build.name should be non-empty: {payload:?}"
    );
    assert!(
        build
            .get("group")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        "actuator info build.group should be non-empty: {payload:?}"
    );
    assert!(
        build
            .get("version")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        "actuator info build.version should be non-empty: {payload:?}"
    );

    let os = payload
        .get("os")
        .and_then(Value::as_object)
        .expect("actuator info should include os object");
    assert!(
        os.get("name")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        "actuator info os.name should be non-empty: {payload:?}"
    );
    assert!(
        os.get("arch")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        "actuator info os.arch should be non-empty: {payload:?}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_logfile_returns_unauthorized_for_anonymous() {
    let paths = new_router_fixture("router-actuator-logfile-anonymous-unauthorized").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/logfile")
                .body(Body::empty())
                .expect("anonymous actuator logfile request should build"),
        )
        .await
        .expect("anonymous actuator logfile request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_logfile_returns_forbidden_for_authenticated_non_admin() {
    let paths = new_router_fixture("router-actuator-logfile-non-admin-forbidden").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "user-actuator-logfile-1",
        "actuator-logfile-user@example.org",
        "router-contract-user-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "actuator-logfile-user@example.org",
        "router-contract-user-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/logfile")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("non-admin actuator logfile request should build"),
        )
        .await
        .expect("non-admin actuator logfile request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_logfile_returns_plaintext_body_for_admin() {
    let paths = new_router_fixture("router-actuator-logfile-admin-plaintext").await;
    seed_router_contract_data(&paths).await;

    let config = runtime_config_for_paths(&paths);
    std::fs::create_dir_all(
        config
            .log_file
            .parent()
            .expect("actuator logfile fixture should have parent directory"),
    )
    .expect("actuator logfile parent directory should be created");
    std::fs::write(&config.log_file, b"first line\nsecond line\n")
        .expect("actuator logfile fixture should be writable");

    let app = build_router_with_config(&config);
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/logfile")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("admin actuator logfile request should build"),
        )
        .await
        .expect("admin actuator logfile request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/plain; charset=utf-8")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("actuator logfile response body should be readable");
    assert_eq!(String::from_utf8_lossy(&body), "first line\nsecond line\n");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_metrics_returns_unauthorized_for_anonymous() {
    let paths = new_router_fixture("router-actuator-metrics-anonymous-unauthorized").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/metrics")
                .body(Body::empty())
                .expect("anonymous actuator metrics request should build"),
        )
        .await
        .expect("anonymous actuator metrics request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_metrics_returns_forbidden_for_authenticated_non_admin() {
    let paths = new_router_fixture("router-actuator-metrics-non-admin-forbidden").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "user-actuator-metrics-1",
        "actuator-metrics-user@example.org",
        "router-contract-user-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "actuator-metrics-user@example.org",
        "router-contract-user-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/metrics")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("non-admin actuator metrics request should build"),
        )
        .await
        .expect("non-admin actuator metrics request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_metrics_returns_metric_names_for_admin() {
    let paths = new_router_fixture("router-actuator-metrics-admin-names").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/metrics")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("admin actuator metrics request should build"),
        )
        .await
        .expect("admin actuator metrics request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let names = payload
        .get("names")
        .and_then(Value::as_array)
        .expect("actuator metrics should return names array");
    assert!(
        names
            .iter()
            .any(|value| value.as_str() == Some("komga.tasks.execution")),
        "actuator metrics names should include komga.tasks.execution: {payload:?}"
    );
    assert!(
        names
            .iter()
            .any(|value| value.as_str() == Some("komga.books")),
        "actuator metrics names should include komga.books: {payload:?}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_metric_detail_includes_base_unit_for_books_filesize() {
    let paths = new_router_fixture("router-actuator-metric-detail-base-unit").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/metrics/komga.books.filesize")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("admin actuator metric detail request should build"),
        )
        .await
        .expect("admin actuator metric detail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("name").and_then(Value::as_str),
        Some("komga.books.filesize")
    );
    assert_eq!(
        payload.get("baseUnit").and_then(Value::as_str),
        Some("bytes")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_metric_detail_returns_unauthorized_for_anonymous() {
    let paths = new_router_fixture("router-actuator-metric-detail-anonymous-unauthorized").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/metrics/komga.books.filesize")
                .body(Body::empty())
                .expect("anonymous actuator metric detail request should build"),
        )
        .await
        .expect("anonymous actuator metric detail request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_metric_detail_returns_forbidden_for_authenticated_non_admin() {
    let paths = new_router_fixture("router-actuator-metric-detail-non-admin-forbidden").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "user-actuator-metric-detail-1",
        "actuator-metric-detail-user@example.org",
        "router-contract-user-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "actuator-metric-detail-user@example.org",
        "router-contract-user-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/metrics/komga.books.filesize")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("non-admin actuator metric detail request should build"),
        )
        .await
        .expect("non-admin actuator metric detail request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_health_is_public_and_hides_details_for_anonymous() {
    let paths = new_router_fixture("router-actuator-health-public-status-only").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/health")
                .body(Body::empty())
                .expect("actuator health request should build"),
        )
        .await
        .expect("actuator health request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload.get("status").and_then(Value::as_str), Some("UP"));
    assert!(
        payload.get("components").is_none(),
        "anonymous actuator health should not expose component details: {payload:?}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_health_hides_details_for_authenticated_non_admin() {
    let paths = new_router_fixture("router-actuator-health-non-admin-status-only").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "user-health-1",
        "health-user@example.org",
        "router-contract-user-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "health-user@example.org",
        "router-contract-user-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/health")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("actuator health non-admin request should build"),
        )
        .await
        .expect("actuator health non-admin request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload.get("status").and_then(Value::as_str), Some("UP"));
    assert!(
        payload.get("components").is_none(),
        "non-admin actuator health should not expose component details: {payload:?}"
    );

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
async fn router_actuator_shutdown_returns_ok_message_for_admin() {
    let paths = new_router_fixture("router-actuator-shutdown-admin-success").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/actuator/shutdown")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("admin actuator shutdown request should build"),
        )
        .await
        .expect("admin actuator shutdown request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("message").and_then(Value::as_str),
        Some("Shutting down, bye...")
    );

    cleanup_router_fixture(paths);
}

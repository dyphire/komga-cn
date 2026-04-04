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

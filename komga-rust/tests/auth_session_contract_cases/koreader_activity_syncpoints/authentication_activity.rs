use super::*;
use axum::extract::ConnectInfo;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::net::SocketAddr;

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
    .bind("Password")
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
    .bind("Password")
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
async fn router_users_by_id_authentication_activity_latest_uses_connect_info_for_koreader_api_key_auth()
 {
    let paths =
        new_router_fixture("router-user-latest-auth-activity-koreader-api-key-connect-info").await;
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
                    json!({ "comment": "KOReader connect info" }).to_string(),
                ))
                .expect("api key create request should build"),
        )
        .await
        .expect("api key create request should complete");

    assert_eq!(create_response.status(), StatusCode::OK);
    let created_api_key = response_json(create_response).await;
    let created_api_key_id = created_api_key
        .get("id")
        .and_then(Value::as_str)
        .expect("api key create response should expose id")
        .to_string();
    let created_api_key_secret = created_api_key
        .get("key")
        .and_then(Value::as_str)
        .expect("api key create response should expose raw key")
        .to_string();

    let connect_info_addr = "198.51.100.77:43123"
        .parse::<SocketAddr>()
        .expect("socket address should parse");
    let koreader_auth_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/koreader/users/auth")
                .header("x-auth-user", &created_api_key_secret)
                .header(header::USER_AGENT, "router-contract-koreader-device")
                .extension(ConnectInfo(connect_info_addr))
                .body(Body::empty())
                .expect("koreader users auth request should build"),
        )
        .await
        .expect("koreader users auth request should complete");

    assert_eq!(koreader_auth_response.status(), StatusCode::OK);

    let latest_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/v2/users/admin-user/authentication-activity/latest?apikey_id={created_api_key_id}"
                ))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("latest auth activity api key request should build"),
        )
        .await
        .expect("latest auth activity api key request should complete");

    assert_eq!(latest_response.status(), StatusCode::OK);
    let payload = response_json(latest_response).await;
    assert_eq!(payload["email"], json!("admin@example.org"));
    assert_eq!(payload["apiKeyId"], json!(created_api_key_id));
    assert_eq!(payload["apiKeyComment"], json!("KOReader connect info"));
    assert_eq!(payload["ip"], json!("198.51.100.77"));
    assert_eq!(
        payload["userAgent"],
        json!("router-contract-koreader-device")
    );
    assert_eq!(payload["source"], json!("ApiKey"));

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
        .bind("Password")
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
async fn router_users_by_id_authentication_activity_latest_returns_successful_kobo_path_api_key_auth()
 {
    let paths =
        new_router_fixture("router-user-latest-auth-activity-kobo-path-api-key-success").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-user",
        "kobo@example.org",
        "router-contract-kobo-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "kobo@example.org",
        "router-contract-kobo-123",
    )
    .await;

    let ping_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/validkobotoken/ping")
                .header("x-forwarded-for", "203.0.113.44")
                .header(header::USER_AGENT, "router-contract-kobo-device")
                .body(Body::empty())
                .expect("kobo ping request should build"),
        )
        .await
        .expect("kobo ping request should complete");

    assert_eq!(ping_response.status(), StatusCode::OK);

    let latest_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(
                    "/api/v2/users/kobo-user/authentication-activity/latest?apikey_id=api-key-validkobotoken",
                )
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("latest auth activity kobo request should build"),
        )
        .await
        .expect("latest auth activity kobo request should complete");

    assert_eq!(latest_response.status(), StatusCode::OK);
    let payload = response_json(latest_response).await;
    assert_eq!(payload["email"], json!("kobo@example.org"));
    assert_eq!(payload["apiKeyId"], json!("api-key-validkobotoken"));
    assert_eq!(payload["apiKeyComment"], json!("kobo sync"));
    assert_eq!(payload["ip"], json!("203.0.113.44"));
    assert_eq!(payload["userAgent"], json!("router-contract-kobo-device"));
    assert_eq!(payload["source"], json!("ApiKey"));

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
    .bind("Password")
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
    .bind("Password")
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
async fn router_users_me_basic_auth_records_forwarded_ip_and_user_agent() {
    let paths = new_router_fixture("router-users-me-basic-records-request-metadata").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for auth activity cleanup");
    sqlx::query("DELETE FROM AUTHENTICATION_ACTIVITY WHERE EMAIL = ?")
        .bind("admin@example.org")
        .execute(&pool)
        .await
        .expect("existing auth activity rows should delete");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let basic_token = STANDARD.encode("admin@example.org:router-contract-admin-123");

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me")
                .header(header::AUTHORIZATION, format!("Basic {basic_token}"))
                .header("x-auth-token", "")
                .header("x-forwarded-for", "203.0.113.10, 10.0.0.1")
                .header(header::USER_AGENT, "router-contract-agent")
                .body(Body::empty())
                .expect("users/me metadata request should build"),
        )
        .await
        .expect("users/me metadata request should complete");

    assert_eq!(response.status(), StatusCode::OK);

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for auth activity assertion");
    let (ip, user_agent, source): (Option<String>, Option<String>, Option<String>) =
        sqlx::query_as(
            "SELECT IP, USER_AGENT, SOURCE FROM AUTHENTICATION_ACTIVITY WHERE EMAIL = ? ORDER BY DATE_TIME DESC LIMIT 1",
        )
        .bind("admin@example.org")
        .fetch_one(&pool)
        .await
        .expect("auth activity row should exist after successful login");
    pool.close().await;

    assert_eq!(ip.as_deref(), Some("203.0.113.10"));
    assert_eq!(user_agent.as_deref(), Some("router-contract-agent"));
    assert_eq!(source.as_deref(), Some("Password"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_users_me_api_keys_list_api_key_auth_uses_connect_info_fallback() {
    let paths = new_router_fixture("router-users-me-api-keys-list-connect-info").await;
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
                    json!({ "comment": "Helper connect info" }).to_string(),
                ))
                .expect("api key create request should build"),
        )
        .await
        .expect("api key create request should complete");
    assert_eq!(create_response.status(), StatusCode::OK);
    let created_api_key = response_json(create_response).await;
    let api_key = created_api_key
        .get("key")
        .and_then(Value::as_str)
        .expect("api key create payload should expose key")
        .to_string();

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for auth activity cleanup");
    sqlx::query("DELETE FROM AUTHENTICATION_ACTIVITY WHERE EMAIL = ?")
        .bind("admin@example.org")
        .execute(&pool)
        .await
        .expect("existing auth activity rows should delete");
    pool.close().await;

    let connect_info_addr = "203.0.113.92:51234"
        .parse::<SocketAddr>()
        .expect("socket address should parse");
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me/api-keys")
                .header("x-api-key", &api_key)
                .header(header::USER_AGENT, "router-contract-helper-api-key-agent")
                .extension(ConnectInfo(connect_info_addr))
                .body(Body::empty())
                .expect("users me api keys list request should build"),
        )
        .await
        .expect("users me api keys list request should complete");

    assert_eq!(response.status(), StatusCode::OK);

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for auth activity assertion");
    let (ip, user_agent, source, api_key_comment): (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT IP, USER_AGENT, SOURCE, API_KEY_COMMENT FROM AUTHENTICATION_ACTIVITY WHERE EMAIL = ? ORDER BY DATE_TIME DESC LIMIT 1",
    )
    .bind("admin@example.org")
    .fetch_one(&pool)
    .await
    .expect("auth activity row should exist after helper api key auth");
    pool.close().await;

    assert_eq!(ip.as_deref(), Some("203.0.113.92"));
    assert_eq!(
        user_agent.as_deref(),
        Some("router-contract-helper-api-key-agent")
    );
    assert_eq!(source.as_deref(), Some("ApiKey"));
    assert_eq!(api_key_comment.as_deref(), Some("Helper connect info"));

    cleanup_router_fixture(paths);
}

pub(crate) async fn verify_api_key_login_records_apikey_source_after_auth_refactor() {
    let paths = new_router_fixture("router-api-key-login-records-kotlin-source").await;
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
                    json!({ "comment": "Contract api key source" }).to_string(),
                ))
                .expect("api key create request should build"),
        )
        .await
        .expect("api key create request should complete");
    assert_eq!(create_response.status(), StatusCode::OK);
    let created_api_key = response_json(create_response).await;
    let api_key = created_api_key
        .get("key")
        .and_then(Value::as_str)
        .expect("api key create payload should expose key")
        .to_string();

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for auth activity cleanup");
    sqlx::query("DELETE FROM AUTHENTICATION_ACTIVITY WHERE EMAIL = ?")
        .bind("admin@example.org")
        .execute(&pool)
        .await
        .expect("existing auth activity rows should delete");
    pool.close().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me/api-keys")
                .header("x-api-key", &api_key)
                .body(Body::empty())
                .expect("users me api keys list request should build"),
        )
        .await
        .expect("users me api keys list request should complete");

    assert_eq!(response.status(), StatusCode::OK);

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for auth activity assertion");
    let source = sqlx::query_scalar::<_, Option<String>>(
        "SELECT SOURCE FROM AUTHENTICATION_ACTIVITY WHERE EMAIL = ? ORDER BY DATE_TIME DESC LIMIT 1",
    )
    .bind("admin@example.org")
    .fetch_one(&pool)
    .await
    .expect("api key login should record authentication activity");
    pool.close().await;

    assert_eq!(source.as_deref(), Some("ApiKey"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn api_key_login_records_apikey_source_after_auth_refactor() {
    verify_api_key_login_records_apikey_source_after_auth_refactor().await;
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
        .bind("Password")
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
async fn router_users_me_invalid_basic_auth_does_not_record_failure_activity_yet() {
    let paths = new_router_fixture("router-users-me-invalid-basic-auth-failure-gap").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for failure-gap auth activity cleanup");
    sqlx::query("DELETE FROM AUTHENTICATION_ACTIVITY WHERE EMAIL = ?")
        .bind("admin@example.org")
        .execute(&pool)
        .await
        .expect("existing auth activity rows should delete");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let basic_token = STANDARD.encode("admin@example.org:wrong-password");

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me")
                .header(header::AUTHORIZATION, format!("Basic {basic_token}"))
                .body(Body::empty())
                .expect("invalid users/me request should build"),
        )
        .await
        .expect("invalid users/me request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for failure-gap auth activity assertion");
    let row_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM AUTHENTICATION_ACTIVITY WHERE EMAIL = ?",
    )
    .bind("admin@example.org")
    .fetch_one(&pool)
    .await
    .expect("failure-gap auth activity count should be queryable");
    pool.close().await;

    assert_eq!(
        row_count, 0,
        "Rust still leaves authentication failure activity persistence as an explicit parity gap"
    );

    cleanup_router_fixture(paths);
}

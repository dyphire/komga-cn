use super::*;

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

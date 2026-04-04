use super::*;

#[tokio::test]
async fn router_discovery_books_list_supports_read_status_is_and_is_not_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-read-status").await;
    seed_router_contract_data(&paths).await;
    seed_router_read_progress(&paths, true).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let unread_response = app
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
                            "type": "ReadStatus",
                            "operator": "is",
                            "value": "UNREAD"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list unread request should build"),
        )
        .await
        .expect("strict books/list unread request should complete");
    assert_eq!(unread_response.status(), StatusCode::OK);
    let unread_payload = response_json(unread_response).await;
    let unread_content = unread_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict unread payload should expose content array");
    assert_eq!(unread_content.len(), 0);

    let read_response = app
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
                            "type": "ReadStatus",
                            "operator": "is",
                            "value": "READ"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list read request should build"),
        )
        .await
        .expect("strict books/list read request should complete");
    assert_eq!(read_response.status(), StatusCode::OK);
    let read_payload = response_json(read_response).await;
    let read_content = read_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict read payload should expose content array");
    assert_eq!(read_content.len(), 1);
    assert_eq!(
        read_content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    let excluded_read_response = app
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
                            "type": "ReadStatus",
                            "operator": "isNot",
                            "value": "READ"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list read isNot excluded request should build"),
        )
        .await
        .expect("strict books/list read isNot excluded request should complete");
    assert_eq!(excluded_read_response.status(), StatusCode::OK);
    let excluded_read_payload = response_json(excluded_read_response).await;
    let excluded_read_content = excluded_read_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict read isNot excluded payload should expose content array");
    assert_eq!(excluded_read_content.len(), 0);

    let kept_not_unread_response = app
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
                            "type": "ReadStatus",
                            "operator": "isNot",
                            "value": "UNREAD"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list read isNot kept request should build"),
        )
        .await
        .expect("strict books/list read isNot kept request should complete");
    assert_eq!(kept_not_unread_response.status(), StatusCode::OK);
    let kept_not_unread_payload = response_json(kept_not_unread_response).await;
    let kept_not_unread_content = kept_not_unread_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict read isNot kept payload should expose content array");
    assert_eq!(kept_not_unread_content.len(), 1);
    assert_eq!(
        kept_not_unread_content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_library_id_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-library-id").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let matched_response = app
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
                            "type": "LibraryId",
                            "operator": "is",
                            "value": "library-1"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list library-id match request should build"),
        )
        .await
        .expect("strict books/list library-id match request should complete");
    assert_eq!(matched_response.status(), StatusCode::OK);
    let matched_payload = response_json(matched_response).await;
    let matched_content = matched_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books library-id match payload should expose content array");
    assert_eq!(matched_content.len(), 1);

    let missing_response = app
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
                            "type": "LibraryId",
                            "operator": "is",
                            "value": "library-missing"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list library-id miss request should build"),
        )
        .await
        .expect("strict books/list library-id miss request should complete");
    assert_eq!(missing_response.status(), StatusCode::OK);
    let missing_payload = response_json(missing_response).await;
    let missing_content = missing_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books library-id miss payload should expose content array");
    assert_eq!(missing_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_deleted_filter_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-deleted").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let not_deleted_response = app
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
                            "operator": "isFalse"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list deleted isFalse request should build"),
        )
        .await
        .expect("strict books/list deleted isFalse request should complete");
    assert_eq!(not_deleted_response.status(), StatusCode::OK);
    let not_deleted_payload = response_json(not_deleted_response).await;
    let not_deleted_content = not_deleted_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books deleted isFalse payload should expose content array");
    assert_eq!(not_deleted_content.len(), 1);

    let deleted_response = app
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
                            "operator": "isTrue"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list deleted isTrue request should build"),
        )
        .await
        .expect("strict books/list deleted isTrue request should complete");
    assert_eq!(deleted_response.status(), StatusCode::OK);
    let deleted_payload = response_json(deleted_response).await;
    let deleted_content = deleted_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books deleted isTrue payload should expose content array");
    assert_eq!(deleted_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_oneshot_filter_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-oneshot").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let oneshot_true_response = app
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
                            "type": "OneShot",
                            "operator": "isTrue"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list oneshot=true request should build"),
        )
        .await
        .expect("strict books/list oneshot=true request should complete");
    assert_eq!(oneshot_true_response.status(), StatusCode::OK);
    let oneshot_true_payload = response_json(oneshot_true_response).await;
    let oneshot_true_content = oneshot_true_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict oneshot=true payload should expose content array");
    assert_eq!(oneshot_true_content.len(), 0);

    let oneshot_false_response = app
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
                            "type": "OneShot",
                            "operator": "isFalse"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list oneshot=false request should build"),
        )
        .await
        .expect("strict books/list oneshot=false request should complete");
    assert_eq!(oneshot_false_response.status(), StatusCode::OK);
    let oneshot_false_payload = response_json(oneshot_false_response).await;
    let oneshot_false_content = oneshot_false_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict oneshot=false payload should expose content array");
    assert_eq!(oneshot_false_content.len(), 1);
    assert_eq!(
        oneshot_false_content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_latest_supports_deleted_filter() {
    let paths = new_router_fixture("router-series-latest-deleted-filter").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-deleted", "Deleted Series", "library-1").await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("series latest deleted db should open");
    sqlx::query("UPDATE SERIES SET DELETED_DATE = ? WHERE ID = ?")
        .bind("2025-01-01 00:00:00")
        .bind("series-deleted")
        .execute(&pool)
        .await
        .expect("series deleted date should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let deleted_true_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/latest?deleted=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series latest deleted=true request should build"),
        )
        .await
        .expect("series latest deleted=true request should complete");
    assert_eq!(deleted_true_response.status(), StatusCode::OK);
    let deleted_true_payload = response_json(deleted_true_response).await;
    let deleted_true_content = deleted_true_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("series latest deleted=true payload should expose content array");
    assert_eq!(deleted_true_content.len(), 1);
    assert_eq!(
        deleted_true_content[0].get("id"),
        Some(&json!("series-deleted"))
    );

    let deleted_false_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/latest?deleted=false")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series latest deleted=false request should build"),
        )
        .await
        .expect("series latest deleted=false request should complete");
    assert_eq!(deleted_false_response.status(), StatusCode::OK);
    let deleted_false_payload = response_json(deleted_false_response).await;
    let deleted_false_content = deleted_false_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("series latest deleted=false payload should expose content array");
    assert_eq!(deleted_false_content.len(), 1);
    assert_eq!(deleted_false_content[0].get("id"), Some(&json!("series-1")));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_latest_supports_oneshot_filter() {
    let paths = new_router_fixture("router-series-latest-oneshot-filter").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("series latest oneshot db should open");
    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, ONESHOT) VALUES (?, ?, ?, ?, ?, ?)",
    )
        .bind("series-oneshot")
        .bind(0_i64)
        .bind("OneShot Series")
        .bind("series/series-oneshot")
        .bind("library-1")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("oneshot series row should insert");
    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
        .bind("ONGOING")
        .bind("OneShot Series")
        .bind("OneShot Series")
        .bind("PubHouse")
        .bind("EN")
        .bind(16_i64)
        .bind("series-oneshot")
        .execute(&pool)
        .await
        .expect("oneshot series metadata should insert");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let oneshot_true_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/latest?oneshot=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series latest oneshot=true request should build"),
        )
        .await
        .expect("series latest oneshot=true request should complete");
    assert_eq!(oneshot_true_response.status(), StatusCode::OK);
    let oneshot_true_payload = response_json(oneshot_true_response).await;
    let oneshot_true_content = oneshot_true_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("series latest oneshot=true payload should expose content array");
    assert_eq!(oneshot_true_content.len(), 1);
    assert_eq!(
        oneshot_true_content[0].get("id"),
        Some(&json!("series-oneshot"))
    );

    let oneshot_false_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/latest?oneshot=false")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series latest oneshot=false request should build"),
        )
        .await
        .expect("series latest oneshot=false request should complete");
    assert_eq!(oneshot_false_response.status(), StatusCode::OK);
    let oneshot_false_payload = response_json(oneshot_false_response).await;
    let oneshot_false_content = oneshot_false_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("series latest oneshot=false payload should expose content array");
    assert_eq!(oneshot_false_content.len(), 1);
    assert_eq!(oneshot_false_content[0].get("id"), Some(&json!("series-1")));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_latest_rejects_malformed_boolean_filters() {
    let paths = new_router_fixture("router-series-latest-invalid-boolean-filter").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let invalid_deleted_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/latest?deleted=foo")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series latest invalid deleted filter request should build"),
        )
        .await
        .expect("series latest invalid deleted filter request should complete");
    assert_eq!(invalid_deleted_response.status(), StatusCode::BAD_REQUEST);

    let invalid_oneshot_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/latest?oneshot=foo")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series latest invalid oneshot filter request should build"),
        )
        .await
        .expect("series latest invalid oneshot filter request should complete");
    assert_eq!(invalid_oneshot_response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
}

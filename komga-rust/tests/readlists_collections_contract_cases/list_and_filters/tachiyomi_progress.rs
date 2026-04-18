use super::*;

#[tokio::test]
async fn router_readlist_tachiyomi_progress_get_returns_kotlin_counter_fields() {
    let paths = new_router_fixture("router-readlist-tachiyomi-progress-get-fields").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/read-progress/tachiyomi")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist tachiyomi get request should build"),
        )
        .await
        .expect("readlist tachiyomi get request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "booksCount": 3,
            "booksReadCount": 0,
            "booksUnreadCount": 3,
            "booksInProgressCount": 0,
            "lastReadContinuousIndex": 0,
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_tachiyomi_progress_routes_accept_basic_auth_like_kotlin_clients() {
    let paths = new_router_fixture("router-readlist-tachiyomi-basic-auth-compat").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let authorization =
        basic_authorization_header_value("admin@example.org", "router-contract-admin-123");

    let get_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/read-progress/tachiyomi")
                .header(header::AUTHORIZATION, authorization.as_str())
                .header("x-auth-token", "")
                .body(Body::empty())
                .expect("readlist tachiyomi basic-auth get should build"),
        )
        .await
        .expect("readlist tachiyomi basic-auth get should complete");
    assert_eq!(get_response.status(), StatusCode::OK);

    let put_response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/readlists/readlist-1/read-progress/tachiyomi")
                .header(header::AUTHORIZATION, authorization.as_str())
                .header("x-auth-token", "")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "lastBookRead": 1 }).to_string()))
                .expect("readlist tachiyomi basic-auth put should build"),
        )
        .await
        .expect("readlist tachiyomi basic-auth put should complete");
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_tachiyomi_progress_get_counts_in_progress_and_continuous_prefix() {
    let paths =
        new_router_fixture("router-readlist-tachiyomi-progress-get-continuous-prefix").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for readlist tachiyomi counter seed");
    for (book_id, page, completed) in [
        ("book-1", 10_i64, true),
        ("book-2", 4_i64, false),
        ("book-3", 12_i64, true),
    ] {
        sqlx::query(
            "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) VALUES (?, ?, ?, ?)",
        )
        .bind(book_id)
        .bind("admin-user")
        .bind(page)
        .bind(completed)
        .execute(&pool)
        .await
        .expect("readlist tachiyomi read progress row should insert");
    }
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/read-progress/tachiyomi")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist tachiyomi continuous-prefix request should build"),
        )
        .await
        .expect("readlist tachiyomi continuous-prefix request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "booksCount": 3,
            "booksReadCount": 2,
            "booksUnreadCount": 0,
            "booksInProgressCount": 1,
            "lastReadContinuousIndex": 1,
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_tachiyomi_progress_get_counts_page_zero_incomplete_as_in_progress() {
    let paths =
        new_router_fixture("router-readlist-tachiyomi-progress-page-zero-in-progress").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for readlist tachiyomi page-zero seed");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) VALUES (?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(0_i64)
    .bind(false)
    .execute(&pool)
    .await
    .expect("page-zero incomplete read progress row should insert");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/read-progress/tachiyomi")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist tachiyomi page-zero request should build"),
        )
        .await
        .expect("readlist tachiyomi page-zero request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "booksCount": 3,
            "booksReadCount": 0,
            "booksUnreadCount": 2,
            "booksInProgressCount": 1,
            "lastReadContinuousIndex": 0,
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_tachiyomi_progress_marks_books_completed_at_real_page_count() {
    let paths = new_router_fixture("router-readlist-tachiyomi-progress-real-page-count").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/readlists/readlist-1/read-progress/tachiyomi")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "lastBookRead": 2 }).to_string()))
                .expect("readlist tachiyomi write request should build"),
        )
        .await
        .expect("readlist tachiyomi write request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for tachiyomi verification");
    let rows = sqlx::query(
        "SELECT BOOK_ID, PAGE, COMPLETED FROM READ_PROGRESS WHERE USER_ID = ? ORDER BY BOOK_ID ASC",
    )
    .bind("admin-user")
    .fetch_all(&pool)
    .await
    .expect("read progress rows should be queryable");
    pool.close().await;

    let persisted = rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("BOOK_ID"),
                row.get::<i64, _>("PAGE"),
                row.get::<i64, _>("COMPLETED"),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        persisted,
        vec![("book-1".to_string(), 10, 1), ("book-2".to_string(), 11, 1),]
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_tachiyomi_progress_skips_books_already_completed() {
    let paths = new_router_fixture("router-readlist-tachiyomi-progress-skip-completed").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for tachiyomi skip-completed seed");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) VALUES (?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(3_i64)
    .bind(true)
    .execute(&pool)
    .await
    .expect("existing completed read-progress row should insert");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/readlists/readlist-1/read-progress/tachiyomi")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "lastBookRead": 2 }).to_string()))
                .expect("readlist tachiyomi skip-completed request should build"),
        )
        .await
        .expect("readlist tachiyomi skip-completed request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should reopen for tachiyomi skip-completed verification");
    let rows = sqlx::query(
        "SELECT BOOK_ID, PAGE, COMPLETED FROM READ_PROGRESS WHERE USER_ID = ? ORDER BY BOOK_ID ASC",
    )
    .bind("admin-user")
    .fetch_all(&pool)
    .await
    .expect("read progress rows should be queryable after skip-completed write");
    pool.close().await;

    let persisted = rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("BOOK_ID"),
                row.get::<i64, _>("PAGE"),
                row.get::<i64, _>("COMPLETED"),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        persisted,
        vec![("book-1".to_string(), 3, 1), ("book-2".to_string(), 11, 1),]
    );

    cleanup_router_fixture(paths);
}

use super::*;

#[tokio::test]
async fn router_post_page_hash_delete_all_enqueues_remove_hashed_pages_tasks_without_touching_media_rows()
 {
    let paths = new_router_fixture("router-page-hash-delete-all-enqueue-only").await;
    seed_router_contract_data(&paths).await;
    seed_known_page_hash_samples(&paths).await;

    let setup_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for page hash delete-all setup");
    sqlx::query(
        "INSERT INTO MEDIA_PAGE (BOOK_ID, NUMBER, FILE_HASH, FILE_NAME, MEDIA_TYPE, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("book-known-1")
    .bind(1_i64)
    .bind("alpha-hash")
    .bind("book-known-1-page-2.png")
    .bind("image/png")
    .bind(222_i64)
    .execute(&setup_pool)
    .await
    .expect("second duplicate page row should be inserted");
    setup_pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/page-hashes/alpha-hash/delete-all")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("page hash delete-all request should build"),
        )
        .await
        .expect("page hash delete-all request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for page hash delete-all verification");
    let remaining_media_rows =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM MEDIA_PAGE WHERE FILE_HASH = ?")
            .bind("alpha-hash")
            .fetch_one(&verify_pool)
            .await
            .expect("remaining page hash match rows should be queryable")
            .get::<i64, _>("COUNT");
    let delete_count =
        sqlx::query("SELECT DELETE_COUNT AS DELETE_COUNT FROM PAGE_HASH WHERE HASH = ?")
            .bind("alpha-hash")
            .fetch_one(&verify_pool)
            .await
            .expect("page hash delete count should be queryable")
            .get::<i64, _>("DELETE_COUNT");
    verify_pool.close().await;

    assert_eq!(remaining_media_rows, 3);
    assert_eq!(delete_count, 1);

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for page hash delete-all verification");
    let rows = sqlx::query("SELECT ID, SIMPLE_TYPE, GROUP_ID, PAYLOAD FROM TASK ORDER BY ID ASC")
        .fetch_all(&tasks_pool)
        .await
        .expect("page hash delete-all task rows should be queryable");
    tasks_pool.close().await;

    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0].get::<String, _>("ID"),
        "REMOVE_HASHED_PAGES_book-known-1"
    );
    assert_eq!(rows[0].get::<String, _>("SIMPLE_TYPE"), "RemoveHashedPages");
    assert_eq!(rows[0].get::<Option<String>, _>("GROUP_ID"), None);
    let first_payload: Value = serde_json::from_str(&rows[0].get::<String, _>("PAYLOAD"))
        .expect("first delete-all payload should be valid json");
    assert_eq!(
        first_payload,
        json!({
            "bookId": "book-known-1",
            "pages": [
                {
                    "fileHash": "alpha-hash",
                    "fileSize": 111,
                    "fileName": "book-known-1.png",
                    "mediaType": "image/png",
                    "pageNumber": 1
                },
                {
                    "fileHash": "alpha-hash",
                    "fileSize": 222,
                    "fileName": "book-known-1-page-2.png",
                    "mediaType": "image/png",
                    "pageNumber": 2
                }
            ],
            "priority": 4,
            "groupId": Value::Null,
            "uniqueId": "REMOVE_HASHED_PAGES_book-known-1"
        })
    );

    assert_eq!(
        rows[1].get::<String, _>("ID"),
        "REMOVE_HASHED_PAGES_book-known-2"
    );
    assert_eq!(rows[1].get::<String, _>("SIMPLE_TYPE"), "RemoveHashedPages");
    assert_eq!(rows[1].get::<Option<String>, _>("GROUP_ID"), None);
    let second_payload: Value = serde_json::from_str(&rows[1].get::<String, _>("PAYLOAD"))
        .expect("second delete-all payload should be valid json");
    assert_eq!(
        second_payload,
        json!({
            "bookId": "book-known-2",
            "pages": [
                {
                    "fileHash": "alpha-hash",
                    "fileSize": 111,
                    "fileName": "book-known-2.png",
                    "mediaType": "image/png",
                    "pageNumber": 1
                }
            ],
            "priority": 4,
            "groupId": Value::Null,
            "uniqueId": "REMOVE_HASHED_PAGES_book-known-2"
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_post_page_hash_delete_all_accepts_missing_hash_without_enqueuing_tasks_like_kotlin()
{
    let paths = new_router_fixture("router-page-hash-delete-all-missing-hash").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/page-hashes/missing-hash/delete-all")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing-hash page hash delete-all request should build"),
        )
        .await
        .expect("missing-hash page hash delete-all request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for missing-hash delete-all verification");
    let queued_count = sqlx::query("SELECT COUNT(*) AS COUNT FROM TASK")
        .fetch_one(&tasks_pool)
        .await
        .expect("missing-hash delete-all task count should be queryable")
        .get::<i64, _>("COUNT");
    tasks_pool.close().await;

    assert_eq!(queued_count, 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_post_page_hash_delete_match_enqueues_remove_hashed_pages_task_without_touching_media_rows_like_kotlin()
 {
    let paths = new_router_fixture("router-page-hash-delete-match-enqueue-only").await;
    seed_router_contract_data(&paths).await;
    seed_known_page_hash_samples(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/page-hashes/alpha-hash/delete-match")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(delete_match_payload(
                    "book-known-1",
                    "books/book-known-1.epub",
                    1,
                    "book-known-1.png",
                    111,
                    "image/png",
                )))
                .expect("page hash delete-match request should build"),
        )
        .await
        .expect("page hash delete-match request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for page hash delete-match verification");
    let remaining_media_rows =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM MEDIA_PAGE WHERE FILE_HASH = ?")
            .bind("alpha-hash")
            .fetch_one(&verify_pool)
            .await
            .expect("remaining delete-match media rows should be queryable")
            .get::<i64, _>("COUNT");
    let delete_count =
        sqlx::query("SELECT DELETE_COUNT AS DELETE_COUNT FROM PAGE_HASH WHERE HASH = ?")
            .bind("alpha-hash")
            .fetch_one(&verify_pool)
            .await
            .expect("delete-match page hash delete count should be queryable")
            .get::<i64, _>("DELETE_COUNT");
    verify_pool.close().await;

    assert_eq!(remaining_media_rows, 2);
    assert_eq!(delete_count, 1);

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for page hash delete-match verification");
    let rows = sqlx::query("SELECT ID, SIMPLE_TYPE, GROUP_ID, PAYLOAD FROM TASK ORDER BY ID ASC")
        .fetch_all(&tasks_pool)
        .await
        .expect("page hash delete-match task rows should be queryable");
    tasks_pool.close().await;

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get::<String, _>("ID"),
        "REMOVE_HASHED_PAGES_book-known-1"
    );
    assert_eq!(rows[0].get::<String, _>("SIMPLE_TYPE"), "RemoveHashedPages");
    assert_eq!(rows[0].get::<Option<String>, _>("GROUP_ID"), None);
    let payload: Value = serde_json::from_str(&rows[0].get::<String, _>("PAYLOAD"))
        .expect("delete-match payload should be valid json");
    assert_eq!(
        payload,
        json!({
            "bookId": "book-known-1",
            "pages": [
                {
                    "fileHash": "alpha-hash",
                    "fileSize": 111,
                    "fileName": "book-known-1.png",
                    "mediaType": "image/png",
                    "pageNumber": 1
                }
            ],
            "priority": 4,
            "groupId": Value::Null,
            "uniqueId": "REMOVE_HASHED_PAGES_book-known-1"
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_post_page_hash_delete_match_accepts_missing_hash_and_still_enqueues_task_like_kotlin()
 {
    let paths = new_router_fixture("router-page-hash-delete-match-missing-hash").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/page-hashes/missing-hash/delete-match")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(delete_match_payload(
                    "book-missing-hash",
                    "books/book-missing-hash.epub",
                    1,
                    "book-missing-hash.png",
                    111,
                    "image/png",
                )))
                .expect("missing-hash page hash delete-match request should build"),
        )
        .await
        .expect("missing-hash page hash delete-match request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for missing-hash delete-match verification");
    let rows = sqlx::query("SELECT ID, SIMPLE_TYPE, GROUP_ID, PAYLOAD FROM TASK ORDER BY ID ASC")
        .fetch_all(&tasks_pool)
        .await
        .expect("missing-hash delete-match task rows should be queryable");
    tasks_pool.close().await;

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get::<String, _>("ID"),
        "REMOVE_HASHED_PAGES_book-missing-hash"
    );
    assert_eq!(rows[0].get::<String, _>("SIMPLE_TYPE"), "RemoveHashedPages");
    assert_eq!(rows[0].get::<Option<String>, _>("GROUP_ID"), None);
    let payload: Value = serde_json::from_str(&rows[0].get::<String, _>("PAYLOAD"))
        .expect("missing-hash delete-match payload should be valid json");
    assert_eq!(payload["bookId"], json!("book-missing-hash"));
    assert_eq!(payload["pages"][0]["fileHash"], json!("missing-hash"));
    assert_eq!(payload["pages"][0]["pageNumber"], json!(1));

    cleanup_router_fixture(paths);
}

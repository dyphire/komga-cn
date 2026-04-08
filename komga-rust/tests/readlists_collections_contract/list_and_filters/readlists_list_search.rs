use super::*;
use komga_rust::{SearchEntityType, SearchIndexLifecycle};

#[tokio::test]
async fn router_readlists_default_name_order_and_filtered_flags_match_kotlin() {
    let paths = new_router_fixture("router-readlists-default-order-filtered-flags").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-1-user",
        "library1@example.org",
        "router-contract-library1-123",
        &["library-1"],
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for readlists default-order seed");
    sqlx::query("UPDATE READLIST SET NAME = ?, BOOK_COUNT = ? WHERE ID = ?")
        .bind("Gamma ReadList")
        .bind(3_i64)
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist-1 should update for readlists default-order seed");
    sqlx::query("INSERT INTO READLIST (ID, NAME, BOOK_COUNT) VALUES (?, ?, ?)")
        .bind("readlist-2")
        .bind("Alpha ReadList")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("readlist-2 row should insert for readlists default-order seed");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-2")
        .bind("book-2")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("readlist-2 membership should insert for readlists default-order seed");
    sqlx::query("INSERT INTO READLIST (ID, NAME, BOOK_COUNT) VALUES (?, ?, ?)")
        .bind("readlist-3")
        .bind("Zulu ReadList")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("readlist-3 row should insert for readlists default-order seed");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-3")
        .bind("book-3")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("readlist-3 membership should insert for readlists default-order seed");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library1@example.org",
        "router-contract-library1-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists?unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlists default-order request should build"),
        )
        .await
        .expect("readlists default-order request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlists default-order payload should expose content array");
    assert_eq!(content.len(), 2);
    assert_eq!(
        content[0].get("id").and_then(Value::as_str),
        Some("readlist-2")
    );
    assert_eq!(
        content[0].get("filtered").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        content[1].get("id").and_then(Value::as_str),
        Some("readlist-1")
    );
    assert_eq!(
        content[1].get("filtered").and_then(Value::as_bool),
        Some(true)
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlists_apply_content_restrictions_and_filtered_flags_like_kotlin() {
    let paths = new_router_fixture("router-readlists-content-restrictions").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        15,
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for readlists content-restriction seed");
    sqlx::query("INSERT INTO READLIST (ID, NAME, BOOK_COUNT) VALUES (?, ?, ?)")
        .bind("readlist-2")
        .bind("Denied ReadList")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("denied readlist row should insert for content restriction seed");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-2")
        .bind("book-1")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("denied readlist membership should insert for content restriction seed");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists?unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlists content-restriction request should build"),
        )
        .await
        .expect("readlists content-restriction request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlists content-restriction payload should expose content array");

    let filtered = content
        .iter()
        .find(|entry| entry.get("id") == Some(&json!("readlist-1")))
        .expect("partially visible readlist should remain visible");
    assert_eq!(filtered.get("filtered"), Some(&Value::Bool(true)));
    assert!(
        content
            .iter()
            .all(|entry| entry.get("id") != Some(&json!("readlist-2"))),
        "fully hidden readlist should be omitted",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlists_library_filter_and_content_restriction_exclude_nonmatching_mixed_readlists_like_kotlin()
 {
    let paths = new_router_fixture("router-readlists-library-filter-content-restriction").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        15,
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for readlists library-filter restriction seed");
    sqlx::query("INSERT INTO READLIST (ID, NAME, BOOK_COUNT) VALUES (?, ?, ?)")
        .bind("readlist-2")
        .bind("Requested Library Hidden")
        .bind(2_i64)
        .execute(&pool)
        .await
        .expect("mixed-library restricted readlist row should insert");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-2")
        .bind("book-2")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("requested-library restricted book should insert");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-2")
        .bind("book-3")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("other-library visible book should insert");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists?library_id=library-1&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlists library-filter restriction request should build"),
        )
        .await
        .expect("readlists library-filter restriction request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlists library-filter restriction payload should expose content array");
    assert!(
        content
            .iter()
            .all(|entry| entry.get("id") != Some(&json!("readlist-2"))),
        "readlist should be excluded when requested-library books are all restricted",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlists_unpaged_returns_full_sorted_result_set_like_kotlin() {
    let paths = new_router_fixture("router-readlists-unpaged-full-result-set").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for readlists unpaged seed");
    sqlx::query("UPDATE READLIST SET NAME = ? WHERE ID = ?")
        .bind("ReadList 01")
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist-1 should update for readlists unpaged seed");
    for index in 2..=25 {
        let readlist_id = format!("readlist-{index:02}");
        let readlist_name = format!("ReadList {index:02}");
        sqlx::query("INSERT INTO READLIST (ID, NAME, BOOK_COUNT) VALUES (?, ?, ?)")
            .bind(&readlist_id)
            .bind(&readlist_name)
            .bind(1_i64)
            .execute(&pool)
            .await
            .expect("unpaged readlist row should insert");
        sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
            .bind(&readlist_id)
            .bind("book-1")
            .bind(0_i64)
            .execute(&pool)
            .await
            .expect("unpaged readlist membership should insert");
    }
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists?unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlists unpaged request should build"),
        )
        .await
        .expect("readlists unpaged request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlists unpaged payload should expose content array");
    assert_eq!(content.len(), 25);
    assert_eq!(
        payload.get("totalElements").and_then(Value::as_u64),
        Some(25)
    );
    assert_eq!(payload.get("size").and_then(Value::as_u64), Some(25));
    assert_eq!(
        payload
            .get("pageable")
            .and_then(|pageable| pageable.get("pageSize"))
            .and_then(Value::as_u64),
        Some(25)
    );
    assert_eq!(
        content.first().and_then(|entry| entry.get("name")),
        Some(&json!("ReadList 01"))
    );
    assert_eq!(
        content.last().and_then(|entry| entry.get("name")),
        Some(&json!("ReadList 25"))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlists_library_id_does_not_filter_book_ids_for_all_library_user_like_kotlin() {
    let paths = new_router_fixture("router-readlists-library-id-all-library-user").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists?library_id=library-1&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlists library-id all-library-user request should build"),
        )
        .await
        .expect("readlists library-id all-library-user request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlists library-id all-library-user payload should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(content[0].get("id"), Some(&json!("readlist-1")));
    assert_eq!(
        content[0].get("bookIds"),
        Some(&json!(["book-1", "book-2", "book-3"]))
    );
    assert_eq!(content[0].get("filtered"), Some(&Value::Bool(false)));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlists_returns_empty_page_when_no_readlists_exist_like_kotlin() {
    let paths = new_router_fixture("router-readlists-empty-page-when-no-readlists").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for empty readlists seed");
    sqlx::query("DELETE FROM READLIST_BOOK")
        .execute(&pool)
        .await
        .expect("readlist books should delete for empty readlists seed");
    sqlx::query("DELETE FROM READLIST")
        .execute(&pool)
        .await
        .expect("readlists should delete for empty readlists seed");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlists empty-page request should build"),
        )
        .await
        .expect("readlists empty-page request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlists empty payload should expose content array");
    assert!(content.is_empty());
    assert_eq!(
        payload.get("totalElements").and_then(Value::as_u64),
        Some(0)
    );
    assert_eq!(payload.get("size").and_then(Value::as_u64), Some(20));
    assert_eq!(
        payload
            .get("pageable")
            .and_then(|pageable| pageable.get("pageSize"))
            .and_then(Value::as_u64),
        Some(20)
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlists_keeps_genuinely_empty_readlists_like_kotlin() {
    let paths = new_router_fixture("router-readlists-keep-genuinely-empty").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for empty-readlist listing seed");
    sqlx::query("UPDATE READLIST SET NAME = ?, BOOK_COUNT = ? WHERE ID = ?")
        .bind("Gamma ReadList")
        .bind(1_i64)
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist-1 should update for empty-readlist listing seed");
    sqlx::query("INSERT INTO READLIST (ID, NAME, BOOK_COUNT) VALUES (?, ?, ?)")
        .bind("readlist-2")
        .bind("Alpha Empty ReadList")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("empty readlist row should insert for listing seed");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists?unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlists empty-readlist request should build"),
        )
        .await
        .expect("readlists empty-readlist request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlists empty-readlist payload should expose content array");
    assert_eq!(content.len(), 2);
    assert_eq!(content[0].get("id"), Some(&json!("readlist-2")));
    assert_eq!(content[0].get("bookIds"), Some(&json!([])));
    assert_eq!(content[0].get("filtered"), Some(&Value::Bool(false)));
    assert_eq!(content[1].get("id"), Some(&json!("readlist-1")));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlists_explicit_created_date_sort_matches_kotlin() {
    let paths = new_router_fixture("router-readlists-created-date-sort").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for readlists created-date sort seed");
    sqlx::query(
        "UPDATE READLIST SET NAME = ?, BOOK_COUNT = ?, CREATED_DATE = ?, LAST_MODIFIED_DATE = ? WHERE ID = ?",
    )
    .bind("ReadList 1")
    .bind(1_i64)
    .bind("2024-01-01T00:00:00")
    .bind("2024-01-11T00:00:00")
    .bind("readlist-1")
    .execute(&pool)
    .await
    .expect("readlist-1 should update for created-date sort seed");
    sqlx::query(
        "INSERT INTO READLIST (ID, NAME, BOOK_COUNT, CREATED_DATE, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("readlist-2")
    .bind("ReadList 2")
    .bind(1_i64)
    .bind("2024-01-02T00:00:00")
    .bind("2024-01-10T00:00:00")
    .execute(&pool)
    .await
    .expect("readlist-2 row should insert for created-date sort seed");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-2")
        .bind("book-1")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("readlist-2 membership should insert for created-date sort seed");
    sqlx::query(
        "INSERT INTO READLIST (ID, NAME, BOOK_COUNT, CREATED_DATE, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("readlist-3")
    .bind("ReadList 3")
    .bind(1_i64)
    .bind("2024-01-03T00:00:00")
    .bind("2024-01-12T00:00:00")
    .execute(&pool)
    .await
    .expect("readlist-3 row should insert for created-date sort seed");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-3")
        .bind("book-1")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("readlist-3 membership should insert for created-date sort seed");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists?sort=createdDate,desc&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlists created-date sort request should build"),
        )
        .await
        .expect("readlists created-date sort request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let ids = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlists created-date sort payload should expose content array")
        .iter()
        .map(|entry| entry.get("id").and_then(Value::as_str).unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["readlist-3", "readlist-2", "readlist-1"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlists_explicit_last_modified_date_sort_matches_kotlin() {
    let paths = new_router_fixture("router-readlists-last-modified-date-sort").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for readlists last-modified sort seed");
    sqlx::query(
        "UPDATE READLIST SET NAME = ?, BOOK_COUNT = ?, CREATED_DATE = ?, LAST_MODIFIED_DATE = ? WHERE ID = ?",
    )
    .bind("ReadList 1")
    .bind(1_i64)
    .bind("2024-01-01T00:00:00")
    .bind("2024-01-11T00:00:00")
    .bind("readlist-1")
    .execute(&pool)
    .await
    .expect("readlist-1 should update for last-modified sort seed");
    sqlx::query(
        "INSERT INTO READLIST (ID, NAME, BOOK_COUNT, CREATED_DATE, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("readlist-2")
    .bind("ReadList 2")
    .bind(1_i64)
    .bind("2024-01-03T00:00:00")
    .bind("2024-01-10T00:00:00")
    .execute(&pool)
    .await
    .expect("readlist-2 row should insert for last-modified sort seed");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-2")
        .bind("book-1")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("readlist-2 membership should insert for last-modified sort seed");
    sqlx::query(
        "INSERT INTO READLIST (ID, NAME, BOOK_COUNT, CREATED_DATE, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("readlist-3")
    .bind("ReadList 3")
    .bind(1_i64)
    .bind("2024-01-02T00:00:00")
    .bind("2024-01-12T00:00:00")
    .execute(&pool)
    .await
    .expect("readlist-3 row should insert for last-modified sort seed");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-3")
        .bind("book-1")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("readlist-3 membership should insert for last-modified sort seed");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists?sort=lastModifiedDate,asc&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlists last-modified sort request should build"),
        )
        .await
        .expect("readlists last-modified sort request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let ids = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlists last-modified sort payload should expose content array")
        .iter()
        .map(|entry| entry.get("id").and_then(Value::as_str).unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["readlist-2", "readlist-1", "readlist-3"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlists_default_name_order_uses_unicode_collation_like_kotlin() {
    let paths = new_router_fixture("router-readlists-default-unicode-order").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for readlists unicode-order seed");
    sqlx::query("UPDATE READLIST SET NAME = ? WHERE ID = ?")
        .bind("Éclair ReadList")
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist-1 should update for readlists unicode-order seed");
    sqlx::query("INSERT INTO READLIST (ID, NAME, BOOK_COUNT) VALUES (?, ?, ?)")
        .bind("readlist-3")
        .bind("Zulu ReadList")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("readlist-3 row should insert for readlists unicode-order seed");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-3")
        .bind("book-1")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("readlist-3 membership should insert for readlists unicode-order seed");
    sqlx::query("INSERT INTO READLIST (ID, NAME, BOOK_COUNT) VALUES (?, ?, ?)")
        .bind("readlist-4")
        .bind("Alpha ReadList")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("readlist-4 row should insert for readlists unicode-order seed");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-4")
        .bind("book-1")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("readlist-4 membership should insert for readlists unicode-order seed");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists?unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlists unicode-order request should build"),
        )
        .await
        .expect("readlists unicode-order request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let ids = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlists unicode-order payload should expose content array")
        .iter()
        .map(|entry| entry.get("id").and_then(Value::as_str).unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["readlist-4", "readlist-1", "readlist-3"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlists_explicit_name_sort_uses_unicode_collation_like_kotlin() {
    let paths = new_router_fixture("router-readlists-explicit-name-unicode-order").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for readlists explicit unicode-order seed");
    sqlx::query("UPDATE READLIST SET NAME = ? WHERE ID = ?")
        .bind("Éclair ReadList")
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist-1 should update for readlists explicit unicode-order seed");
    sqlx::query("INSERT INTO READLIST (ID, NAME, BOOK_COUNT) VALUES (?, ?, ?)")
        .bind("readlist-3")
        .bind("Zulu ReadList")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("readlist-3 row should insert for readlists explicit unicode-order seed");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-3")
        .bind("book-1")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("readlist-3 membership should insert for readlists explicit unicode-order seed");
    sqlx::query("INSERT INTO READLIST (ID, NAME, BOOK_COUNT) VALUES (?, ?, ?)")
        .bind("readlist-4")
        .bind("Alpha ReadList")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("readlist-4 row should insert for readlists explicit unicode-order seed");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-4")
        .bind("book-1")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("readlist-4 membership should insert for readlists explicit unicode-order seed");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists?sort=name,desc&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlists explicit unicode-order request should build"),
        )
        .await
        .expect("readlists explicit unicode-order request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let ids = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlists explicit unicode-order payload should expose content array")
        .iter()
        .map(|entry| entry.get("id").and_then(Value::as_str).unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["readlist-3", "readlist-1", "readlist-4"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlists_search_uses_relevance_order_like_kotlin() {
    let paths = new_router_fixture("router-readlists-search-relevance-order").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for readlists search relevance seed");
    sqlx::query("UPDATE READLIST SET NAME = ?, SUMMARY = ? WHERE ID = ?")
        .bind("Alpha ReadList")
        .bind("")
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist-1 should update for readlists search relevance seed");
    sqlx::query("INSERT INTO READLIST (ID, NAME, SUMMARY, BOOK_COUNT) VALUES (?, ?, ?, ?)")
        .bind("readlist-2")
        .bind("Alpha Alpha ReadList")
        .bind("")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("readlist-2 row should insert for readlists search relevance seed");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-2")
        .bind("book-1")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("readlist-2 membership should insert for readlists search relevance seed");
    sqlx::query("INSERT INTO READLIST (ID, NAME, SUMMARY, BOOK_COUNT) VALUES (?, ?, ?, ?)")
        .bind("readlist-3")
        .bind("Zulu Alpha ReadList")
        .bind("")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("readlist-3 row should insert for readlists search relevance seed");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-3")
        .bind("book-1")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("readlist-3 membership should insert for readlists search relevance seed");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists?search=alpha&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlists search relevance request should build"),
        )
        .await
        .expect("readlists search relevance request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let ids = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlists search relevance payload should expose content array")
        .iter()
        .map(|entry| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .expect("readlists search relevance entry should expose id")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["readlist-2", "readlist-1", "readlist-3"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlists_search_does_not_match_summary_only_hits_like_kotlin() {
    let paths = new_router_fixture("router-readlists-search-name-only-matches").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for readlists name-only search seed");
    sqlx::query("UPDATE READLIST SET NAME = ?, SUMMARY = ? WHERE ID = ?")
        .bind("Alpha ReadList")
        .bind("")
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist-1 should update for readlists name-only search seed");
    sqlx::query("INSERT INTO READLIST (ID, NAME, SUMMARY, BOOK_COUNT) VALUES (?, ?, ?, ?)")
        .bind("readlist-2")
        .bind("Beta ReadList")
        .bind("alpha")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("readlist-2 row should insert for readlists name-only search seed");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-2")
        .bind("book-1")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("readlist-2 membership should insert for readlists name-only search seed");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists?search=alpha&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlists name-only search request should build"),
        )
        .await
        .expect("readlists name-only search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let ids = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlists name-only search payload should expose content array")
        .iter()
        .map(|entry| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .expect("readlists name-only search entry should expose id")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["readlist-1"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlists_search_matches_accent_folded_names_like_kotlin() {
    let paths = new_router_fixture("router-readlists-search-accent-folding").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for readlists accent-folding seed");
    sqlx::query("UPDATE READLIST SET NAME = ?, SUMMARY = ? WHERE ID = ?")
        .bind("Éclair ReadList")
        .bind("")
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist-1 should update for readlists accent-folding seed");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists?search=eclair&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlists accent-folding search request should build"),
        )
        .await
        .expect("readlists accent-folding search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let ids = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlists accent-folding payload should expose content array")
        .iter()
        .map(|entry| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .expect("readlists accent-folding entry should expose id")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["readlist-1"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlists_search_matches_non_contiguous_multi_token_names_like_kotlin() {
    let paths = new_router_fixture("router-readlists-search-multi-token").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for readlists multi-token seed");
    sqlx::query("UPDATE READLIST SET NAME = ?, SUMMARY = ? WHERE ID = ?")
        .bind("Zulu Alpha ReadList")
        .bind("")
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist-1 should update for readlists multi-token seed");
    sqlx::query("INSERT INTO READLIST (ID, NAME, SUMMARY, BOOK_COUNT) VALUES (?, ?, ?, ?)")
        .bind("readlist-2")
        .bind("Alpha Only ReadList")
        .bind("")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("readlist-2 row should insert for readlists multi-token seed");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-2")
        .bind("book-1")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("readlist-2 membership should insert for readlists multi-token seed");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists?search=alpha%20zulu&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlists multi-token search request should build"),
        )
        .await
        .expect("readlists multi-token search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let ids = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlists multi-token payload should expose content array")
        .iter()
        .map(|entry| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .expect("readlists multi-token entry should expose id")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["readlist-1"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlists_invalid_search_syntax_returns_empty_result_like_kotlin() {
    let paths = new_router_fixture("router-readlists-search-invalid-syntax").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists?search=%28&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlists invalid search syntax request should build"),
        )
        .await
        .expect("readlists invalid search syntax request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlists invalid search syntax payload should expose content array");
    assert!(content.is_empty());
    assert_eq!(
        payload.get("totalElements").and_then(Value::as_u64),
        Some(0)
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlists_search_does_not_drop_visible_hits_after_hidden_ranked_hits_like_kotlin() {
    let paths = new_router_fixture("router-readlists-search-hidden-hits-window").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-1-user",
        "library1@example.org",
        "router-contract-library1-123",
        &["library-1"],
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for readlists hidden-hit window seed");
    sqlx::query("UPDATE READLIST SET NAME = ?, SUMMARY = ? WHERE ID = ?")
        .bind("Alpha Visible ReadList")
        .bind("")
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist-1 should update for readlists hidden-hit window seed");

    for index in 0..5_i64 {
        let readlist_id = format!("hidden-readlist-{index}");
        let readlist_name = format!("Alpha Alpha Alpha Hidden ReadList {index}");

        sqlx::query("INSERT INTO READLIST (ID, NAME, SUMMARY, BOOK_COUNT) VALUES (?, ?, ?, ?)")
            .bind(&readlist_id)
            .bind(&readlist_name)
            .bind("")
            .bind(1_i64)
            .execute(&pool)
            .await
            .expect("hidden readlist row should insert for hidden-hit window seed");
        sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
            .bind(&readlist_id)
            .bind("book-3")
            .bind(0_i64)
            .execute(&pool)
            .await
            .expect("hidden readlist membership should insert for hidden-hit window seed");
    }
    pool.close().await;

    let config = runtime_config_for_paths(&paths);
    let app = build_router_with_config(&config);
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library1@example.org",
        "router-contract-library1-123",
    )
    .await;

    let ranked_ids = SearchIndexLifecycle::bootstrap(config.lucene_data_directory.as_path())
        .expect("readlists hidden-hit window search index should bootstrap")
        .search_ids("alpha", SearchEntityType::ReadList, 1000)
        .expect("readlists hidden-hit window search query should succeed");
    let expected_visible_ids = ranked_ids
        .into_iter()
        .filter(|id| id == "readlist-1")
        .collect::<Vec<_>>();
    assert_eq!(expected_visible_ids, vec!["readlist-1".to_string()]);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists?search=alpha&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlists hidden-hit window request should build"),
        )
        .await
        .expect("readlists hidden-hit window request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let ids = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlists hidden-hit window payload should expose content array")
        .iter()
        .map(|entry| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .expect("readlists hidden-hit window entry should expose id")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(ids, expected_visible_ids);

    cleanup_router_fixture(paths);
}

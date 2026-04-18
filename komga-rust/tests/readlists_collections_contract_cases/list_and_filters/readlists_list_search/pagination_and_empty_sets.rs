use super::*;

#[tokio::test]
async fn router_readlists_unpaged_returns_full_sorted_result_set_like_kotlin() {
    let paths = new_router_fixture("router-readlists-unpaged-full-result-set").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
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
async fn router_readlists_returns_empty_page_when_no_readlists_exist_like_kotlin() {
    let paths = new_router_fixture("router-readlists-empty-page-when-no-readlists").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
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

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
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

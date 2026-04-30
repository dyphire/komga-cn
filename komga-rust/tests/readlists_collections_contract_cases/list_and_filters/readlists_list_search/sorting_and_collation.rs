use super::*;

#[tokio::test]
async fn router_readlists_explicit_created_date_sort_matches_kotlin() {
    let ctx = TestFixture::builder("router-readlists-created-date-sort")
        .with_seed(|paths| async move {
            let pool = connect_test_pool(paths.main_db.as_path(), 1)
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
            sqlx::query(
                "INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)",
            )
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
            sqlx::query(
                "INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)",
            )
            .bind("readlist-3")
            .bind("book-1")
            .bind(0_i64)
            .execute(&pool)
            .await
            .expect("readlist-3 membership should insert for created-date sort seed");
            pool.close().await;
        })
        .build()
        .await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_readlists_explicit_last_modified_date_sort_matches_kotlin() {
    let ctx = TestFixture::builder("router-readlists-last-modified-date-sort")
        .with_seed(|paths| async move {
            let pool = connect_test_pool(paths.main_db.as_path(), 1)
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
            sqlx::query(
                "INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)",
            )
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
            sqlx::query(
                "INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)",
            )
            .bind("readlist-3")
            .bind("book-1")
            .bind(0_i64)
            .execute(&pool)
            .await
            .expect("readlist-3 membership should insert for last-modified sort seed");
            pool.close().await;
        })
        .build()
        .await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_readlists_default_name_order_uses_unicode_collation_like_kotlin() {
    let ctx = TestFixture::builder("router-readlists-default-unicode-order")
        .with_seed(|paths| async move {
            let pool = connect_test_pool(paths.main_db.as_path(), 1)
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
            sqlx::query(
                "INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)",
            )
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
            sqlx::query(
                "INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)",
            )
            .bind("readlist-4")
            .bind("book-1")
            .bind(0_i64)
            .execute(&pool)
            .await
            .expect("readlist-4 membership should insert for readlists unicode-order seed");
            pool.close().await;
        })
        .build()
        .await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_readlists_explicit_name_sort_uses_unicode_collation_like_kotlin() {
    let ctx = TestFixture::builder("router-readlists-explicit-name-unicode-order")
        .with_seed(|paths| async move {
            let pool = connect_test_pool(paths.main_db.as_path(), 1)
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
            sqlx::query(
                "INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)",
            )
            .bind("readlist-3")
            .bind("book-1")
            .bind(0_i64)
            .execute(&pool)
            .await
            .expect(
                "readlist-3 membership should insert for readlists explicit unicode-order seed",
            );
            sqlx::query("INSERT INTO READLIST (ID, NAME, BOOK_COUNT) VALUES (?, ?, ?)")
                .bind("readlist-4")
                .bind("Alpha ReadList")
                .bind(1_i64)
                .execute(&pool)
                .await
                .expect("readlist-4 row should insert for readlists explicit unicode-order seed");
            sqlx::query(
                "INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)",
            )
            .bind("readlist-4")
            .bind("book-1")
            .bind(0_i64)
            .execute(&pool)
            .await
            .expect(
                "readlist-4 membership should insert for readlists explicit unicode-order seed",
            );
            pool.close().await;
        })
        .build()
        .await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
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
}

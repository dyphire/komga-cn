use super::*;

#[tokio::test]
async fn router_readlists_default_name_order_and_filtered_flags_match_kotlin() {
    let ctx = TestFixture::builder("router-readlists-default-order-filtered-flags")
        .with_seed(|paths| async move {
            seed_readlist_endpoint_variants(&paths).await;
            seed_router_library_restricted_user(
                &paths,
                "library-1-user",
                "library1@example.org",
                "router-contract-library1-123",
                &["library-1"],
            )
            .await;

            let pool = connect_test_pool(paths.main_db.as_path(), 1)
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
            sqlx::query(
                "INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)",
            )
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
            sqlx::query(
                "INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)",
            )
            .bind("readlist-3")
            .bind("book-3")
            .bind(0_i64)
            .execute(&pool)
            .await
            .expect("readlist-3 membership should insert for readlists default-order seed");
            pool.close().await;
        })
        .build()
        .await;

    let auth_token = ctx
        .login_with_credentials("library1@example.org", "router-contract-library1-123")
        .await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_readlists_apply_content_restrictions_and_filtered_flags_like_kotlin() {
    let ctx = TestFixture::builder("router-readlists-content-restrictions")
        .with_seed(|paths| async move {
            seed_readlist_endpoint_variants(&paths).await;
            seed_router_age_exclude_user(
                &paths,
                "restricted-user",
                "restricted@example.org",
                "router-contract-restricted-123",
                15,
            )
            .await;

            let pool = connect_test_pool(paths.main_db.as_path(), 1)
                .await
                .expect("main db should open for readlists content-restriction seed");
            sqlx::query("INSERT INTO READLIST (ID, NAME, BOOK_COUNT) VALUES (?, ?, ?)")
                .bind("readlist-2")
                .bind("Denied ReadList")
                .bind(1_i64)
                .execute(&pool)
                .await
                .expect("denied readlist row should insert for content restriction seed");
            sqlx::query(
                "INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)",
            )
            .bind("readlist-2")
            .bind("book-1")
            .bind(0_i64)
            .execute(&pool)
            .await
            .expect("denied readlist membership should insert for content restriction seed");
            pool.close().await;
        })
        .build()
        .await;

    let auth_token = ctx
        .login_with_credentials("restricted@example.org", "router-contract-restricted-123")
        .await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_readlists_library_filter_and_content_restriction_exclude_nonmatching_mixed_readlists_like_kotlin()
 {
    let ctx = TestFixture::builder("router-readlists-library-filter-content-restriction")
        .with_seed(|paths| async move {
            seed_readlist_endpoint_variants(&paths).await;
            seed_router_age_exclude_user(
                &paths,
                "restricted-user",
                "restricted@example.org",
                "router-contract-restricted-123",
                15,
            )
            .await;

            let pool = connect_test_pool(paths.main_db.as_path(), 1)
                .await
                .expect("main db should open for readlists library-filter restriction seed");
            sqlx::query("INSERT INTO READLIST (ID, NAME, BOOK_COUNT) VALUES (?, ?, ?)")
                .bind("readlist-2")
                .bind("Requested Library Hidden")
                .bind(2_i64)
                .execute(&pool)
                .await
                .expect("mixed-library restricted readlist row should insert");
            sqlx::query(
                "INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)",
            )
            .bind("readlist-2")
            .bind("book-2")
            .bind(0_i64)
            .execute(&pool)
            .await
            .expect("requested-library restricted book should insert");
            sqlx::query(
                "INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)",
            )
            .bind("readlist-2")
            .bind("book-3")
            .bind(1_i64)
            .execute(&pool)
            .await
            .expect("other-library visible book should insert");
            pool.close().await;
        })
        .build()
        .await;

    let auth_token = ctx
        .login_with_credentials("restricted@example.org", "router-contract-restricted-123")
        .await;

    let response = ctx
        .app()
        .clone()
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
}

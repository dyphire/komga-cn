use super::*;

#[tokio::test]
async fn router_series_latest_excludes_deleted_series_by_default() {
    let ctx = TestFixture::builder("router-series-latest-default-deleted-hidden")
        .with_seed(|paths| async move {
            seed_router_custom_series(&paths, "series-deleted", "Deleted Series", "library-1")
                .await;
        })
        .build()
        .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("series latest default deleted db should open");
    sqlx::query("UPDATE SERIES SET DELETED_DATE = ? WHERE ID = ?")
        .bind("2025-01-01 00:00:00")
        .bind("series-deleted")
        .execute(&pool)
        .await
        .expect("series latest default deleted date should update");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/latest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series latest default request should build"),
        )
        .await
        .expect("series latest default request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("series latest default payload should expose content array");
    let ids = content
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["series-1"]);
}

#[tokio::test]
async fn router_series_latest_supports_deleted_filter() {
    let ctx = TestFixture::builder("router-series-latest-deleted-filter")
        .with_seed(|paths| async move {
            seed_router_custom_series(&paths, "series-deleted", "Deleted Series", "library-1")
                .await;
        })
        .build()
        .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("series latest deleted db should open");
    sqlx::query("UPDATE SERIES SET DELETED_DATE = ? WHERE ID = ?")
        .bind("2025-01-01 00:00:00")
        .bind("series-deleted")
        .execute(&pool)
        .await
        .expect("series deleted date should update");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let deleted_true_response = ctx
        .app()
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

    let deleted_false_response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_series_latest_supports_oneshot_filter() {
    let ctx = TestFixture::new("router-series-latest-oneshot-filter").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
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

    let auth_token = ctx.login_admin().await;

    let oneshot_true_response = ctx
        .app()
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

    let oneshot_false_response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_series_new_sorts_by_created_desc() {
    let ctx = TestFixture::builder("router-series-new-created-desc")
        .with_seed(|paths| async move {
            seed_router_custom_series(&paths, "series-new", "New Series", "library-1").await;
        })
        .build()
        .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("series new created-desc db should open");
    sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ?, CREATED_DATE = ? WHERE ID = ?")
        .bind("2025-03-01 00:00:00")
        .bind("2025-01-01 00:00:00")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("baseline series timestamps should update");
    sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ?, CREATED_DATE = ? WHERE ID = ?")
        .bind("2025-02-01 00:00:00")
        .bind("2025-04-01 00:00:00")
        .bind("series-new")
        .execute(&pool)
        .await
        .expect("new series timestamps should update");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/new")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series new request should build"),
        )
        .await
        .expect("series new request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("series new payload should expose content array");
    assert_eq!(content.len(), 2);
    assert_eq!(content[0].get("id"), Some(&json!("series-new")));
    assert_eq!(content[1].get("id"), Some(&json!("series-1")));
}

#[tokio::test]
async fn router_series_updated_excludes_newly_added_series() {
    let ctx = TestFixture::builder("router-series-updated-excludes-newly-added")
        .with_seed(|paths| async move {
            seed_router_custom_series(&paths, "series-new", "New Series", "library-1").await;
        })
        .build()
        .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("series updated db should open");
    sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ?, CREATED_DATE = ? WHERE ID = ?")
        .bind("2025-04-01 00:00:00")
        .bind("2025-01-01 00:00:00")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("updated series timestamps should update");
    sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ?, CREATED_DATE = ? WHERE ID = ?")
        .bind("2025-04-02 00:00:00")
        .bind("2025-04-02 00:00:00")
        .bind("series-new")
        .execute(&pool)
        .await
        .expect("newly added series timestamps should update");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/updated")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series updated request should build"),
        )
        .await
        .expect("series updated request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("series updated payload should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(content[0].get("id"), Some(&json!("series-1")));
}

#[tokio::test]
async fn router_series_updated_unpaged_keeps_kotlin_page_shape() {
    let ctx = TestFixture::builder("router-series-updated-unpaged-shape")
        .with_seed(|paths| async move {
            seed_router_custom_series(&paths, "series-new", "New Series", "library-1").await;
        })
        .build()
        .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("series updated unpaged db should open");
    sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ?, CREATED_DATE = ? WHERE ID = ?")
        .bind("2025-04-01 00:00:00")
        .bind("2025-01-01 00:00:00")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("updated series timestamps should update for unpaged shape test");
    sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ?, CREATED_DATE = ? WHERE ID = ?")
        .bind("2025-04-02 00:00:00")
        .bind("2025-04-02 00:00:00")
        .bind("series-new")
        .execute(&pool)
        .await
        .expect("newly added series timestamps should update for unpaged shape test");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/updated?unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series updated unpaged request should build"),
        )
        .await
        .expect("series updated unpaged request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("series updated unpaged payload should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(content[0].get("id"), Some(&json!("series-1")));
    assert_eq!(payload.get("size"), Some(&json!(20)));
    assert_eq!(payload.get("number"), Some(&json!(0)));
    assert_eq!(payload.get("totalElements"), Some(&json!(1)));
    assert_eq!(payload.get("totalPages"), Some(&json!(1)));
    let pageable = payload
        .get("pageable")
        .expect("series updated unpaged payload should expose pageable object");
    assert_eq!(pageable.get("pageSize"), Some(&json!(20)));
    assert_eq!(pageable.get("pageNumber"), Some(&json!(0)));
    assert_eq!(pageable.get("paged"), Some(&json!(true)));
    assert_eq!(pageable.get("unpaged"), Some(&json!(false)));
}

#[tokio::test]
async fn router_series_latest_rejects_malformed_boolean_filters() {
    let ctx = TestFixture::new("router-series-latest-invalid-boolean-filter").await;

    let auth_token = ctx.login_admin().await;

    let invalid_deleted_response = ctx
        .app()
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

    let invalid_oneshot_response = ctx
        .app()
        .clone()
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
}

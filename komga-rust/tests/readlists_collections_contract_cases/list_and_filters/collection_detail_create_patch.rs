use super::*;

fn kotlin_collection_datetime(raw: &str) -> String {
    raw.replace(' ', "T") + "Z"
}

#[tokio::test]
async fn router_collection_detail_returns_kotlin_collection_dto_fields() {
    let ctx = TestFixture::builder("router-collection-detail-kotlin-dto-fields")
        .with_seed(|paths| async move {
            seed_collection_series_variants(&paths).await;

            let pool = connect_test_pool(paths.main_db.as_path(), 1)
                .await
                .expect("main db should open for collection detail fixture alignment");
            sqlx::query("UPDATE COLLECTION SET SERIES_COUNT = ? WHERE ID = ?")
                .bind(2_i64)
                .bind("collection-1")
                .execute(&pool)
                .await
                .expect("collection-1 series count should align with attached series");
            pool.close().await;
        })
        .build()
        .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for collection detail fixture alignment");
    let timestamps =
        sqlx::query("SELECT CREATED_DATE, LAST_MODIFIED_DATE FROM COLLECTION WHERE ID = ?")
            .bind("collection-1")
            .fetch_one(&pool)
            .await
            .expect("collection-1 timestamps should be queryable");
    pool.close().await;

    let created_date = kotlin_collection_datetime(&timestamps.get::<String, _>("CREATED_DATE"));
    let last_modified_date =
        kotlin_collection_datetime(&timestamps.get::<String, _>("LAST_MODIFIED_DATE"));

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/collections/collection-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collection detail request should build"),
        )
        .await
        .expect("collection detail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "id": "collection-1",
            "name": "Collection 1",
            "ordered": false,
            "seriesIds": ["series-1", "series-2"],
            "createdDate": created_date,
            "lastModifiedDate": last_modified_date,
            "filtered": false,
        })
    );
}

#[tokio::test]
async fn router_collection_detail_marks_partially_visible_collection_as_filtered() {
    let ctx = TestFixture::builder("router-collection-detail-partially-visible")
        .with_seed(|paths| async move {
            seed_collection_series_variants(&paths).await;
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
                .expect("main db should open for filtered collection detail alignment");
            sqlx::query("UPDATE COLLECTION SET SERIES_COUNT = ? WHERE ID = ?")
                .bind(2_i64)
                .bind("collection-1")
                .execute(&pool)
                .await
                .expect("collection-1 series count should align for filtered detail");
            pool.close().await;
        })
        .build()
        .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for filtered collection detail alignment");
    let timestamps =
        sqlx::query("SELECT CREATED_DATE, LAST_MODIFIED_DATE FROM COLLECTION WHERE ID = ?")
            .bind("collection-1")
            .fetch_one(&pool)
            .await
            .expect("filtered collection detail timestamps should be queryable");
    pool.close().await;

    let created_date = kotlin_collection_datetime(&timestamps.get::<String, _>("CREATED_DATE"));
    let last_modified_date =
        kotlin_collection_datetime(&timestamps.get::<String, _>("LAST_MODIFIED_DATE"));

    let auth_token = ctx
        .login_with_credentials("library1@example.org", "router-contract-library1-123")
        .await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/collections/collection-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("filtered collection detail request should build"),
        )
        .await
        .expect("filtered collection detail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "id": "collection-1",
            "name": "Collection 1",
            "ordered": false,
            "seriesIds": ["series-1"],
            "createdDate": created_date,
            "lastModifiedDate": last_modified_date,
            "filtered": true,
        })
    );
}

#[tokio::test]
async fn router_collection_create_rejects_missing_name_like_kotlin() {
    let ctx = TestFixture::new("router-collection-create-missing-name").await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "ordered": false,
                        "seriesIds": ["series-1"]
                    })
                    .to_string(),
                ))
                .expect("collection create missing-name request should build"),
        )
        .await
        .expect("collection create missing-name request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn router_collection_create_rejects_malformed_json_before_field_validation() {
    let ctx = TestFixture::new("router-collection-create-malformed-json").await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"name":"Broken Collection","#))
                .expect("collection create malformed-json request should build"),
        )
        .await
        .expect("collection create malformed-json request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_bad_request_message(
        &payload,
        "Request body must be a JSON object",
        "/api/v1/collections",
    );
}

#[tokio::test]
async fn router_collection_create_rejects_missing_ordered_like_kotlin() {
    let ctx = TestFixture::new("router-collection-create-missing-ordered").await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": "New Collection",
                        "seriesIds": ["series-1"]
                    })
                    .to_string(),
                ))
                .expect("collection create missing-ordered request should build"),
        )
        .await
        .expect("collection create missing-ordered request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn router_collection_create_rejects_blank_name_like_kotlin() {
    let ctx = TestFixture::new("router-collection-create-blank-name").await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": "   ",
                        "ordered": false,
                        "seriesIds": ["series-1"]
                    })
                    .to_string(),
                ))
                .expect("collection create blank-name request should build"),
        )
        .await
        .expect("collection create blank-name request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn router_collection_create_rejects_empty_series_ids_like_kotlin() {
    let ctx = TestFixture::new("router-collection-create-empty-series-ids").await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": "Empty SeriesIds",
                        "ordered": false,
                        "seriesIds": []
                    })
                    .to_string(),
                ))
                .expect("collection create empty-series-ids request should build"),
        )
        .await
        .expect("collection create empty-series-ids request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn router_collection_create_rejects_duplicate_series_ids_like_kotlin() {
    let ctx = TestFixture::new("router-collection-create-duplicate-series-ids").await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": "Duplicate SeriesIds",
                        "ordered": false,
                        "seriesIds": ["series-1", "series-1"]
                    })
                    .to_string(),
                ))
                .expect("collection create duplicate-series-ids request should build"),
        )
        .await
        .expect("collection create duplicate-series-ids request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn router_collection_create_rejects_duplicate_name_like_kotlin() {
    let ctx = TestFixture::new("router-collection-create-duplicate-name").await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": "Collection 1",
                        "ordered": false,
                        "seriesIds": ["series-1"]
                    })
                    .to_string(),
                ))
                .expect("collection create duplicate-name request should build"),
        )
        .await
        .expect("collection create duplicate-name request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn router_collection_patch_preserves_unspecified_fields_like_kotlin() {
    let ctx = TestFixture::new("router-collection-patch-preserves-unspecified").await;
    let auth_token = ctx.login_admin().await;

    let patch_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/collections/collection-1")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "ordered": true }).to_string()))
                .expect("collection patch partial request should build"),
        )
        .await
        .expect("collection patch partial request should complete");

    assert_eq!(patch_response.status(), StatusCode::NO_CONTENT);

    let detail_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/collections/collection-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collection detail after patch request should build"),
        )
        .await
        .expect("collection detail after patch request should complete");

    assert_eq!(detail_response.status(), StatusCode::OK);
    let payload = response_json(detail_response).await;
    assert_eq!(
        payload.get("name"),
        Some(&Value::String("Collection 1".to_string()))
    );
    assert_eq!(payload.get("ordered"), Some(&Value::Bool(true)));
    assert_eq!(payload.get("seriesIds"), Some(&json!(["series-1"])));
    assert_eq!(payload.get("filtered"), Some(&Value::Bool(false)));
}

#[tokio::test]
async fn router_collection_patch_rejects_duplicate_name_like_kotlin() {
    let ctx = TestFixture::builder("router-collection-patch-duplicate-name")
        .with_seed(|paths| async move {
            seed_collection_listing_variants(&paths).await;
        })
        .build()
        .await;

    let auth_token = ctx.login_admin().await;

    let patch_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/collections/collection-1")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": "Beta Collection",
                        "ordered": false,
                        "seriesIds": ["series-1"]
                    })
                    .to_string(),
                ))
                .expect("collection patch duplicate-name request should build"),
        )
        .await
        .expect("collection patch duplicate-name request should complete");

    assert_eq!(patch_response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn router_collection_patch_ignores_historical_duplicate_when_name_is_unchanged_like_kotlin() {
    let ctx = TestFixture::builder(
        "router-collection-patch-unchanged-name-historical-duplicate",
    )
    .with_seed(|paths| async move {
        let pool = connect_test_pool(paths.main_db.as_path(), 1)
            .await
            .expect("main db should open for collection patch historical duplicate seed");
        sqlx::query(
            "INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT, CREATED_DATE, LAST_MODIFIED_DATE) \
             VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .bind("collection-duplicate")
        .bind("Collection 1")
        .bind(true)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("historical duplicate collection row should insert");
        sqlx::query(
            "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
        )
        .bind("collection-duplicate")
        .bind("series-1")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("historical duplicate collection series row should insert");
        pool.close().await;
    })
    .build()
    .await;

    let auth_token = ctx.login_admin().await;

    let patch_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/collections/collection-1")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "ordered": true }).to_string()))
                .expect(
                    "collection patch unchanged-name historical-duplicate request should build",
                ),
        )
        .await
        .expect("collection patch unchanged-name historical-duplicate request should complete");

    assert_eq!(patch_response.status(), StatusCode::NO_CONTENT);

    let detail_response = ctx
        .app().clone()
.oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/collections/collection-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collection detail after unchanged-name historical-duplicate patch should build"),
        )
        .await
        .expect("collection detail after unchanged-name historical-duplicate patch should complete");

    assert_eq!(detail_response.status(), StatusCode::OK);
    let payload = response_json(detail_response).await;
    assert_eq!(
        payload.get("name"),
        Some(&Value::String("Collection 1".to_string()))
    );
    assert_eq!(payload.get("ordered"), Some(&Value::Bool(true)));
}

#[tokio::test]
async fn router_collection_patch_rejects_duplicate_series_ids_like_kotlin() {
    let ctx = TestFixture::new("router-collection-patch-duplicate-series-ids").await;
    let auth_token = ctx.login_admin().await;

    let patch_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/collections/collection-1")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "seriesIds": ["series-1", "series-1"]
                    })
                    .to_string(),
                ))
                .expect("collection patch duplicate-series-ids request should build"),
        )
        .await
        .expect("collection patch duplicate-series-ids request should complete");

    assert_eq!(patch_response.status(), StatusCode::BAD_REQUEST);
}

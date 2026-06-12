use super::*;

mod metadata_update;

#[tokio::test]
async fn router_discovery_series_detail_uses_persisted_title_sort_value() {
    let ctx = TestFixture::builder("router-discovery-series-detail-title-sort")
        .with_seed(|paths| async move {
            seed_router_series_title_sort(&paths, "series-1", "Series Sort 1").await;
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
                .uri("/api/v1/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series detail request should build"),
        )
        .await
        .expect("series detail request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("titleSort")),
        Some(&Value::String("Series Sort 1".to_string())),
    );
}

#[tokio::test]
async fn router_discovery_series_detail_includes_persisted_metadata_and_aggregates() {
    let ctx = TestFixture::builder("router-discovery-series-detail-persisted-metadata-aggregates")
        .with_seed(|paths| async move {
            seed_router_series_counts(&paths, 1, Some(5)).await;
            seed_router_series_read_progress(&paths, 1, 0).await;
            seed_router_series_aggregated_tag(&paths, "series-1", "aggregated-tag").await;
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
                .uri("/api/v1/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series detail persisted metadata request should build"),
        )
        .await
        .expect("series detail persisted metadata request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("genres")),
        Some(&json!(["SciFi"])),
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("tags")),
        Some(&json!(["Favorite"])),
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("totalBookCount")),
        Some(&Value::Number(5.into())),
    );
    assert_eq!(
        payload
            .get("booksMetadata")
            .and_then(|metadata| metadata.get("releaseDate")),
        Some(&Value::String("2024-01-15".to_string())),
    );
    assert_eq!(
        payload
            .get("booksMetadata")
            .and_then(|metadata| metadata.get("tags")),
        Some(&json!(["aggregated-tag"])),
    );
    assert_eq!(
        payload.get("booksReadCount"),
        Some(&Value::Number(1.into()))
    );
    assert_eq!(
        payload.get("booksInProgressCount"),
        Some(&Value::Number(0.into()))
    );
    assert_eq!(
        payload.get("booksUnreadCount"),
        Some(&Value::Number(0.into()))
    );
}

#[tokio::test]
async fn router_discovery_series_detail_uses_persisted_series_name_for_top_level_name() {
    let ctx = TestFixture::new("router-discovery-series-detail-uses-series-name").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("series detail name parity db should open");
    sqlx::query("UPDATE SERIES SET NAME = ? WHERE ID = ?")
        .bind("Series Shelf Name")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series name should update");
    sqlx::query("UPDATE SERIES_METADATA SET TITLE = ? WHERE SERIES_ID = ?")
        .bind("Series Metadata Title")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series metadata title should update");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series detail name request should build"),
        )
        .await
        .expect("series detail name request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(
        payload.get("name"),
        Some(&Value::String("Series Shelf Name".to_string()))
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("title")),
        Some(&Value::String("Series Metadata Title".to_string()))
    );
}

#[tokio::test]
async fn router_discovery_series_detail_id_bridge_preserves_real_library_id() {
    let ctx = TestFixture::builder("router-discovery-series-detail-id-bridge-library-id")
        .with_seed(|paths| async move {
            seed_router_custom_series(&paths, "custom-series-2", "Series 2", "library-1").await;
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
                .uri("/api/v1/series/series-2")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series detail id-bridge request should build"),
        )
        .await
        .expect("series detail id-bridge request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(
        payload.get("id"),
        Some(&Value::String("custom-series-2".to_string())),
    );
    assert_eq!(
        payload.get("libraryId"),
        Some(&Value::String("library-1".to_string())),
    );
}

#[tokio::test]
async fn router_discovery_series_detail_accepts_oneshot_true_with_extra_query_parameters() {
    let ctx = TestFixture::new("router-discovery-series-detail-oneshot-query-shape").await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-1?oneshot=true&extra=ignored")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series detail oneshot query request should build"),
        )
        .await
        .expect("series detail oneshot query request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response
            .headers()
            .get("x-komga-runtime-search-ownership")
            .is_none(),
        "accepted oneshot=true detail requests should not be marked persisted-owned",
    );

    let payload = response_json(response).await;
    assert!(
        payload.get("_diagnostics").is_none(),
        "accepted oneshot=true detail requests should not emit unsupported diagnostics",
    );
}

use super::*;

mod metadata_update;

#[tokio::test]
async fn router_discovery_series_detail_uses_persisted_title_sort_value() {
    let paths = new_router_fixture("router-discovery-series-detail-title-sort").await;
    seed_router_contract_data(&paths).await;
    seed_router_series_title_sort(&paths, "series-1", "Series Sort 1").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_detail_includes_persisted_metadata_and_aggregates() {
    let paths =
        new_router_fixture("router-discovery-series-detail-persisted-metadata-aggregates").await;
    seed_router_contract_data(&paths).await;
    seed_router_series_counts(&paths, 1, Some(5)).await;
    seed_router_series_read_progress(&paths, 1, 0).await;
    seed_router_series_aggregated_tag(&paths, "series-1", "aggregated-tag").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_detail_uses_persisted_series_name_for_top_level_name() {
    let paths = new_router_fixture("router-discovery-series-detail-uses-series-name").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_detail_id_bridge_preserves_real_library_id() {
    let paths = new_router_fixture("router-discovery-series-detail-id-bridge-library-id").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "custom-series-2", "Series 2", "library-1").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_detail_accepts_oneshot_true_with_extra_query_parameters() {
    let paths = new_router_fixture("router-discovery-series-detail-oneshot-query-shape").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

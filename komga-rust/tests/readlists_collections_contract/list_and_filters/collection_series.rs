use super::*;

#[tokio::test]
async fn router_collection_series_supports_kotlin_style_query_filters() {
    let paths = new_router_fixture("router-collection-series-query-filters").await;
    seed_router_contract_data(&paths).await;
    seed_collection_series_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/collections/collection-1/series?library_id=library-1&publisher=PubHouse&language=EN&genre=SciFi&age_rating=16&author=John+Doe,writer&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collection series filter request should build"),
        )
        .await
        .expect("collection series filter request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("collection series filter payload should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id").and_then(Value::as_str),
        Some("series-1")
    );
    assert_eq!(
        payload
            .get("pageable")
            .and_then(|pageable| pageable.get("unpaged"))
            .and_then(Value::as_bool),
        Some(true)
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_series_ignores_search_query_like_kotlin() {
    let paths = new_router_fixture("router-collection-series-ignore-search-query").await;
    seed_router_contract_data(&paths).await;
    seed_collection_series_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/collections/collection-1/series?search=Series%202&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collection series search-ignore request should build"),
        )
        .await
        .expect("collection series search-ignore request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("collection series search-ignore payload should expose content array");
    let ids = content
        .iter()
        .map(|entry| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .expect("collection series search-ignore entry should expose id")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["series-1".to_string(), "series-2".to_string()]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_series_uses_collection_order_for_ordered_collection_like_kotlin() {
    let paths = new_router_fixture("router-collection-series-ordered-collection-order").await;
    seed_router_contract_data(&paths).await;
    seed_collection_series_variants(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for ordered collection series alignment");
    sqlx::query("UPDATE COLLECTION SET ORDERED = ?, SERIES_COUNT = ? WHERE ID = ?")
        .bind(true)
        .bind(2_i64)
        .bind("collection-1")
        .execute(&pool)
        .await
        .expect("collection-1 should become ordered for collection series alignment");
    sqlx::query("UPDATE SERIES_METADATA SET TITLE_SORT = ? WHERE SERIES_ID = ?")
        .bind("Zeta Series")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series-1 titleSort should update for ordered collection series alignment");
    sqlx::query("UPDATE SERIES_METADATA SET TITLE_SORT = ? WHERE SERIES_ID = ?")
        .bind("Alpha Series")
        .bind("series-2")
        .execute(&pool)
        .await
        .expect("series-2 titleSort should update for ordered collection series alignment");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/collections/collection-1/series?sort=metadata.titleSort,asc&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("ordered collection series request should build"),
        )
        .await
        .expect("ordered collection series request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("ordered collection series payload should expose content array");
    let ids = content
        .iter()
        .map(|entry| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .expect("ordered collection series entry should expose id")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["series-1".to_string(), "series-2".to_string()]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_series_paginates_after_ordering_for_ordered_collection_like_kotlin() {
    let paths = new_router_fixture("router-collection-series-ordered-pagination").await;
    seed_router_contract_data(&paths).await;
    seed_collection_series_variants(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for ordered collection pagination alignment");
    sqlx::query("UPDATE COLLECTION SET ORDERED = ?, SERIES_COUNT = ? WHERE ID = ?")
        .bind(true)
        .bind(2_i64)
        .bind("collection-1")
        .execute(&pool)
        .await
        .expect("collection-1 should become ordered for ordered collection pagination");
    sqlx::query("UPDATE SERIES_METADATA SET TITLE_SORT = ? WHERE SERIES_ID = ?")
        .bind("Zeta Series")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series-1 titleSort should update for ordered collection pagination");
    sqlx::query("UPDATE SERIES_METADATA SET TITLE_SORT = ? WHERE SERIES_ID = ?")
        .bind("Alpha Series")
        .bind("series-2")
        .execute(&pool)
        .await
        .expect("series-2 titleSort should update for ordered collection pagination");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/collections/collection-1/series?page=1&size=1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("ordered collection series pagination request should build"),
        )
        .await
        .expect("ordered collection series pagination request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("ordered collection pagination payload should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id").and_then(Value::as_str),
        Some("series-2")
    );
    assert_eq!(payload.get("totalElements"), Some(&json!(2)));
    assert_eq!(payload.get("totalPages"), Some(&json!(2)));
    assert_eq!(payload.get("number"), Some(&json!(1)));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_series_filters_invisible_series_for_partially_visible_user() {
    let paths = new_router_fixture("router-collection-series-partially-visible").await;
    seed_router_contract_data(&paths).await;
    seed_collection_series_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-1-user",
        "library1@example.org",
        "router-contract-library1-123",
        &["library-1"],
    )
    .await;

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
                .uri("/api/v1/collections/collection-1/series?unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("partially visible collection series request should build"),
        )
        .await
        .expect("partially visible collection series request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("partially visible collection series payload should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id").and_then(Value::as_str),
        Some("series-1")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_series_returns_not_found_for_fully_hidden_collection_like_kotlin() {
    let paths = new_router_fixture("router-collection-series-fully-hidden").await;
    seed_router_contract_data(&paths).await;
    seed_collection_listing_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-1-user",
        "library1@example.org",
        "router-contract-library1-123",
        &["library-1"],
    )
    .await;

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
                .uri("/api/v1/collections/collection-2/series")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("fully hidden collection series request should build"),
        )
        .await
        .expect("fully hidden collection series request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_collections_filter_series_ids_for_partially_visible_user() {
    let paths = new_router_fixture("router-series-collections-partially-visible").await;
    seed_router_contract_data(&paths).await;
    seed_collection_series_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-1-user",
        "library1@example.org",
        "router-contract-library1-123",
        &["library-1"],
    )
    .await;

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
                .uri("/api/v1/series/series-1/collections")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series collections partially visible request should build"),
        )
        .await
        .expect("series collections partially visible request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let collections = payload
        .as_array()
        .expect("series collections payload should be an array");
    assert_eq!(collections.len(), 1);
    assert_eq!(
        collections[0].get("id").and_then(Value::as_str),
        Some("collection-1")
    );
    assert_eq!(collections[0].get("filtered"), Some(&Value::Bool(true)));
    assert_eq!(collections[0].get("seriesIds"), Some(&json!(["series-1"])));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_collections_does_not_accept_sorted_position_series_alias() {
    let paths = new_router_fixture("router-series-collections-no-id-bridge").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract db should open for series alias test");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-real")
    .bind(0_i64)
    .bind("Series Real")
    .bind("series/series-real")
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("alias target series row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind("Series Real")
    .bind("ZZZ Series Real")
    .bind("PubHouse")
    .bind("EN")
    .bind(0_i64)
    .bind("series-real")
    .execute(&pool)
    .await
    .expect("alias target series metadata row should be inserted");

    sqlx::query(
        "INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) \
         VALUES (?, ?, ?, ?)",
    )
    .bind("collection-alias")
    .bind("Collection Alias")
    .bind(false)
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("alias collection row should be inserted");

    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) \
         VALUES (?, ?, ?)",
    )
    .bind("collection-alias")
    .bind("series-real")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("alias collection membership row should be inserted");

    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-2/collections")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series collections alias request should build"),
        )
        .await
        .expect("series collections alias request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload, json!([]));

    cleanup_router_fixture(paths);
}

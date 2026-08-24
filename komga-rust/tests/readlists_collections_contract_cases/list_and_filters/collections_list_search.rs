use super::*;
use komga_infrastructure_search::{SearchEntityType, SearchIndexLifecycle};

#[tokio::test]
async fn router_collections_supports_search_library_id_and_unpaged() {
    let ctx = TestFixture::builder("router-collections-search-library-unpaged")
        .with_search_index()
        .with_seed(|paths| async move {
            seed_collection_listing_variants(&paths).await;
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
                .uri("/api/v1/collections?search=beta&library_id=library-2&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collections search request should build"),
        )
        .await
        .expect("collections search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("collections payload should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id").and_then(Value::as_str),
        Some("collection-2")
    );
    assert_eq!(
        payload
            .get("pageable")
            .and_then(|pageable| pageable.get("unpaged"))
            .and_then(Value::as_bool),
        Some(true)
    );
}

#[tokio::test]
async fn router_collections_search_uses_index_relevance_order_like_kotlin() {
    let ctx = TestFixture::builder("router-collections-search-relevance-order")
        .with_search_index()
        .with_seed(|paths| async move {
            seed_collection_listing_variants(&paths).await;

            let pool = connect_test_pool(paths.main_db.as_path(), 1)
                .await
                .expect("main db should open for collections search relevance seed");
            sqlx::query("UPDATE COLLECTION SET NAME = ? WHERE ID = ?")
                .bind("Collection Collection 2")
                .bind("collection-2")
                .execute(&pool)
                .await
                .expect("collection-2 name should update for collections search relevance seed");
            sqlx::query(
                "INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)",
            )
            .bind("collection-3")
            .bind("Collection 3")
            .bind(false)
            .bind(1_i64)
            .execute(&pool)
            .await
            .expect("collection-3 row should insert for collections search relevance seed");
            sqlx::query(
                "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
            )
            .bind("collection-3")
            .bind("series-1")
            .bind(0_i64)
            .execute(&pool)
            .await
            .expect(
                "collection-3 series membership should insert for collections search relevance seed",
            );
            pool.close().await;
        })
        .build()
        .await;

    let auth_token = ctx.login_admin().await;

    let expected_ids =
        SearchIndexLifecycle::bootstrap(ctx.config().lucene_data_directory.as_path())
            .expect("collections search relevance index should bootstrap")
            .search_ids("collection", SearchEntityType::Collection, 10)
            .expect("collections search relevance query should succeed");
    assert_eq!(expected_ids.len(), 3);

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/collections?search=collection&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collections search relevance request should build"),
        )
        .await
        .expect("collections search relevance request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("collections search relevance payload should expose content array");
    let ids = content
        .iter()
        .map(|entry| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .expect("collections search relevance entry should expose id")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(ids, expected_ids);
}

#[tokio::test]
async fn router_collections_missing_search_index_returns_internal_error() {
    let ctx = TestFixture::builder("router-collections-search-missing-index-error")
        .with_search_index()
        .with_seed(|paths| async move {
            seed_collection_listing_variants(&paths).await;
        })
        .build()
        .await;

    let config = ctx.config();
    if config.lucene_data_directory.exists() {
        std::fs::remove_dir_all(&config.lucene_data_directory)
            .expect("collections search index fixture should be removable");
    }

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/collections?search=collection&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collections missing-index search request should build"),
        )
        .await
        .expect("collections missing-index search request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        !config.lucene_data_directory.exists(),
        "query-only collection search must not recreate the missing index directory",
    );
}

#[tokio::test]
async fn router_collections_default_name_order_and_filtered_flags_match_kotlin() {
    let ctx = TestFixture::builder("router-collections-default-order-filtered-flags")
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
                .expect("main db should open for collections default-order filtered seed");
            sqlx::query("UPDATE COLLECTION SET NAME = ?, SERIES_COUNT = ? WHERE ID = ?")
                .bind("Gamma Collection")
                .bind(2_i64)
                .bind("collection-1")
                .execute(&pool)
                .await
                .expect("collection-1 should update for collections default-order filtered seed");
            sqlx::query(
                "INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)",
            )
            .bind("collection-3")
            .bind("Alpha Collection")
            .bind(false)
            .bind(1_i64)
            .execute(&pool)
            .await
            .expect("collection-3 row should insert for collections default-order filtered seed");
            sqlx::query(
                "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
            )
            .bind("collection-3")
            .bind("series-1")
            .bind(0_i64)
            .execute(&pool)
            .await
            .expect(
                "collection-3 series membership should insert for collections default-order filtered seed",
            );
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
                .uri("/api/v1/collections?unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collections default-order filtered request should build"),
        )
        .await
        .expect("collections default-order filtered request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("collections default-order filtered payload should expose content array");
    assert_eq!(content.len(), 2);
    assert_eq!(
        content[0].get("id").and_then(Value::as_str),
        Some("collection-3")
    );
    assert_eq!(
        content[0].get("filtered").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        content[1].get("id").and_then(Value::as_str),
        Some("collection-1")
    );
    assert_eq!(
        content[1].get("filtered").and_then(Value::as_bool),
        Some(true)
    );
}

#[tokio::test]
async fn router_collections_default_name_order_uses_unicode_collation_like_kotlin() {
    let ctx = TestFixture::builder("router-collections-default-unicode-order")
        .with_seed(|paths| async move {
            let pool = connect_test_pool(paths.main_db.as_path(), 1)
                .await
                .expect("main db should open for collections unicode-order seed");
            sqlx::query("UPDATE COLLECTION SET NAME = ? WHERE ID = ?")
                .bind("Éclair Collection")
                .bind("collection-1")
                .execute(&pool)
                .await
                .expect("collection-1 name should update for collections unicode-order seed");
            sqlx::query(
                "INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)",
            )
            .bind("collection-3")
            .bind("Zulu Collection")
            .bind(false)
            .bind(1_i64)
            .execute(&pool)
            .await
            .expect("collection-3 row should insert for collections unicode-order seed");
            sqlx::query(
                "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
            )
            .bind("collection-3")
            .bind("series-1")
            .bind(0_i64)
            .execute(&pool)
            .await
            .expect("collection-3 membership should insert for collections unicode-order seed");
            sqlx::query(
                "INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)",
            )
            .bind("collection-4")
            .bind("Alpha Collection")
            .bind(false)
            .bind(1_i64)
            .execute(&pool)
            .await
            .expect("collection-4 row should insert for collections unicode-order seed");
            sqlx::query(
                "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
            )
            .bind("collection-4")
            .bind("series-1")
            .bind(0_i64)
            .execute(&pool)
            .await
            .expect("collection-4 membership should insert for collections unicode-order seed");
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
                .uri("/api/v1/collections?unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collections unicode-order request should build"),
        )
        .await
        .expect("collections unicode-order request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("collections unicode-order payload should expose content array");
    let ids = content
        .iter()
        .map(|entry| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .expect("collections unicode-order entry should expose id")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec![
            "collection-4".to_string(),
            "collection-1".to_string(),
            "collection-3".to_string(),
        ]
    );
}

#[tokio::test]
async fn router_collections_library_id_does_not_filter_series_ids_for_all_library_user_like_kotlin()
{
    let ctx = TestFixture::builder("router-collections-library-id-all-library-user")
        .with_seed(|paths| async move {
            seed_collection_series_variants(&paths).await;
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
                .uri("/api/v1/collections?library_id=library-1&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collections library-id all-library-user request should build"),
        )
        .await
        .expect("collections library-id all-library-user request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("collections library-id all-library-user payload should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id").and_then(Value::as_str),
        Some("collection-1")
    );
    assert_eq!(
        content[0].get("seriesIds"),
        Some(&json!(["series-1", "series-2"]))
    );
    assert_eq!(
        content[0].get("filtered").and_then(Value::as_bool),
        Some(false)
    );
}

#[tokio::test]
async fn router_collections_search_does_not_drop_visible_hits_after_hidden_ranked_hits_like_kotlin()
{
    let ctx = TestFixture::builder(
        "router-collections-search-visible-hits-after-hidden-ranked",
    )
    .with_search_index()
    .with_seed(|paths| async move {
        seed_collection_listing_variants(&paths).await;
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
            .expect("main db should open for collections hidden-ranked search seed");
        sqlx::query("UPDATE COLLECTION SET NAME = ? WHERE ID = ?")
            .bind("Collection Collection 2")
            .bind("collection-2")
            .execute(&pool)
            .await
            .expect("collection-2 should update for collections hidden-ranked search seed");
        sqlx::query(
            "INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)",
        )
        .bind("collection-3")
        .bind("Collection 3")
        .bind(false)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("collection-3 row should insert for collections hidden-ranked search seed");
        sqlx::query(
            "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
        )
        .bind("collection-3")
        .bind("series-1")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect(
            "collection-3 series membership should insert for collections hidden-ranked search seed",
        );
        pool.close().await;
    })
    .build()
    .await;

    let auth_token = ctx
        .login_with_credentials("library1@example.org", "router-contract-library1-123")
        .await;

    let ranked_ids = SearchIndexLifecycle::bootstrap(ctx.config().lucene_data_directory.as_path())
        .expect("collections hidden-ranked search index should bootstrap")
        .search_ids("collection", SearchEntityType::Collection, 10)
        .expect("collections hidden-ranked search query should succeed");
    assert_eq!(ranked_ids.first().map(String::as_str), Some("collection-2"));
    let expected_visible_ids = ranked_ids
        .into_iter()
        .filter(|id| id != "collection-2")
        .collect::<Vec<_>>();
    assert_eq!(
        expected_visible_ids,
        vec!["collection-1".to_string(), "collection-3".to_string()]
    );

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/collections?search=collection&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collections hidden-ranked search request should build"),
        )
        .await
        .expect("collections hidden-ranked search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("collections hidden-ranked search payload should expose content array");
    let ids = content
        .iter()
        .map(|entry| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .expect("collections hidden-ranked search entry should expose id")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(ids, expected_visible_ids);
}

use super::*;
use komga_rust::infrastructure::{SearchEntityType, SearchIndexLifecycle};

fn kotlin_collection_datetime(raw: &str) -> String {
    raw.replace(' ', "T") + "Z"
}

#[tokio::test]
async fn router_readlist_tachiyomi_progress_get_returns_kotlin_counter_fields() {
    let paths = new_router_fixture("router-readlist-tachiyomi-progress-get-fields").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/read-progress/tachiyomi")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist tachiyomi get request should build"),
        )
        .await
        .expect("readlist tachiyomi get request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "booksCount": 3,
            "booksReadCount": 0,
            "booksUnreadCount": 3,
            "booksInProgressCount": 0,
            "lastReadContinuousIndex": 0,
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_tachiyomi_progress_get_counts_in_progress_and_continuous_prefix() {
    let paths =
        new_router_fixture("router-readlist-tachiyomi-progress-get-continuous-prefix").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for readlist tachiyomi counter seed");
    for (book_id, page, completed) in [
        ("book-1", 10_i64, true),
        ("book-2", 4_i64, false),
        ("book-3", 12_i64, true),
    ] {
        sqlx::query(
            "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) VALUES (?, ?, ?, ?)",
        )
        .bind(book_id)
        .bind("admin-user")
        .bind(page)
        .bind(completed)
        .execute(&pool)
        .await
        .expect("readlist tachiyomi read progress row should insert");
    }
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/read-progress/tachiyomi")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist tachiyomi continuous-prefix request should build"),
        )
        .await
        .expect("readlist tachiyomi continuous-prefix request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "booksCount": 3,
            "booksReadCount": 2,
            "booksUnreadCount": 0,
            "booksInProgressCount": 1,
            "lastReadContinuousIndex": 1,
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_tachiyomi_progress_get_counts_page_zero_incomplete_as_in_progress() {
    let paths =
        new_router_fixture("router-readlist-tachiyomi-progress-page-zero-in-progress").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for readlist tachiyomi page-zero seed");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) VALUES (?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(0_i64)
    .bind(false)
    .execute(&pool)
    .await
    .expect("page-zero incomplete read progress row should insert");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/read-progress/tachiyomi")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist tachiyomi page-zero request should build"),
        )
        .await
        .expect("readlist tachiyomi page-zero request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "booksCount": 3,
            "booksReadCount": 0,
            "booksUnreadCount": 2,
            "booksInProgressCount": 1,
            "lastReadContinuousIndex": 0,
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_tachiyomi_progress_marks_books_completed_at_real_page_count() {
    let paths = new_router_fixture("router-readlist-tachiyomi-progress-real-page-count").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/readlists/readlist-1/read-progress/tachiyomi")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "lastBookRead": 2 }).to_string()))
                .expect("readlist tachiyomi write request should build"),
        )
        .await
        .expect("readlist tachiyomi write request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for tachiyomi verification");
    let rows = sqlx::query(
        "SELECT BOOK_ID, PAGE, COMPLETED FROM READ_PROGRESS WHERE USER_ID = ? ORDER BY BOOK_ID ASC",
    )
    .bind("admin-user")
    .fetch_all(&pool)
    .await
    .expect("read progress rows should be queryable");
    pool.close().await;

    let persisted = rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("BOOK_ID"),
                row.get::<i64, _>("PAGE"),
                row.get::<i64, _>("COMPLETED"),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        persisted,
        vec![("book-1".to_string(), 10, 1), ("book-2".to_string(), 11, 1),]
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_tachiyomi_progress_skips_books_already_completed() {
    let paths = new_router_fixture("router-readlist-tachiyomi-progress-skip-completed").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for tachiyomi skip-completed seed");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) VALUES (?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(3_i64)
    .bind(true)
    .execute(&pool)
    .await
    .expect("existing completed read-progress row should insert");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/readlists/readlist-1/read-progress/tachiyomi")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "lastBookRead": 2 }).to_string()))
                .expect("readlist tachiyomi skip-completed request should build"),
        )
        .await
        .expect("readlist tachiyomi skip-completed request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should reopen for tachiyomi skip-completed verification");
    let rows = sqlx::query(
        "SELECT BOOK_ID, PAGE, COMPLETED FROM READ_PROGRESS WHERE USER_ID = ? ORDER BY BOOK_ID ASC",
    )
    .bind("admin-user")
    .fetch_all(&pool)
    .await
    .expect("read progress rows should be queryable after skip-completed write");
    pool.close().await;

    let persisted = rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("BOOK_ID"),
                row.get::<i64, _>("PAGE"),
                row.get::<i64, _>("COMPLETED"),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        persisted,
        vec![("book-1".to_string(), 3, 1), ("book-2".to_string(), 11, 1),]
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collections_supports_search_library_id_and_unpaged() {
    let paths = new_router_fixture("router-collections-search-library-unpaged").await;
    seed_router_contract_data(&paths).await;
    seed_collection_listing_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collections_search_uses_index_relevance_order_like_kotlin() {
    let paths = new_router_fixture("router-collections-search-relevance-order").await;
    seed_router_contract_data(&paths).await;
    seed_collection_listing_variants(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for collections search relevance seed");
    sqlx::query("UPDATE COLLECTION SET NAME = ? WHERE ID = ?")
        .bind("Collection Collection 2")
        .bind("collection-2")
        .execute(&pool)
        .await
        .expect("collection-2 name should update for collections search relevance seed");
    sqlx::query("INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)")
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
    .expect("collection-3 series membership should insert for collections search relevance seed");
    pool.close().await;

    let config = runtime_config_for_paths(&paths);
    let app = build_router_with_config(&config);
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let expected_ids = SearchIndexLifecycle::bootstrap(config.lucene_data_directory.as_path())
        .expect("collections search relevance index should bootstrap")
        .search_ids("collection", SearchEntityType::Collection, 10)
        .expect("collections search relevance query should succeed");
    assert_eq!(expected_ids.len(), 3);

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collections_default_name_order_and_filtered_flags_match_kotlin() {
    let paths = new_router_fixture("router-collections-default-order-filtered-flags").await;
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

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for collections default-order filtered seed");
    sqlx::query("UPDATE COLLECTION SET NAME = ?, SERIES_COUNT = ? WHERE ID = ?")
        .bind("Gamma Collection")
        .bind(2_i64)
        .bind("collection-1")
        .execute(&pool)
        .await
        .expect("collection-1 should update for collections default-order filtered seed");
    sqlx::query("INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)")
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collections_default_name_order_uses_unicode_collation_like_kotlin() {
    let paths = new_router_fixture("router-collections-default-unicode-order").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for collections unicode-order seed");
    sqlx::query("UPDATE COLLECTION SET NAME = ? WHERE ID = ?")
        .bind("Éclair Collection")
        .bind("collection-1")
        .execute(&pool)
        .await
        .expect("collection-1 name should update for collections unicode-order seed");
    sqlx::query("INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)")
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
    sqlx::query("INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)")
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collections_library_id_does_not_filter_series_ids_for_all_library_user_like_kotlin()
{
    let paths = new_router_fixture("router-collections-library-id-all-library-user").await;
    seed_router_contract_data(&paths).await;
    seed_collection_series_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collections_search_does_not_drop_visible_hits_after_hidden_ranked_hits_like_kotlin()
{
    let paths =
        new_router_fixture("router-collections-search-visible-hits-after-hidden-ranked").await;
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

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for collections hidden-ranked search seed");
    sqlx::query("UPDATE COLLECTION SET NAME = ? WHERE ID = ?")
        .bind("Collection Collection 2")
        .bind("collection-2")
        .execute(&pool)
        .await
        .expect("collection-2 should update for collections hidden-ranked search seed");
    sqlx::query("INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)")
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

    let config = runtime_config_for_paths(&paths);
    let app = build_router_with_config(&config);
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library1@example.org",
        "router-contract-library1-123",
    )
    .await;

    let ranked_ids = SearchIndexLifecycle::bootstrap(config.lucene_data_directory.as_path())
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

    let response = app
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

    cleanup_router_fixture(paths);
}

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
async fn router_collection_detail_returns_kotlin_collection_dto_fields() {
    let paths = new_router_fixture("router-collection-detail-kotlin-dto-fields").await;
    seed_router_contract_data(&paths).await;
    seed_collection_series_variants(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for collection detail fixture alignment");
    sqlx::query("UPDATE COLLECTION SET SERIES_COUNT = ? WHERE ID = ?")
        .bind(2_i64)
        .bind("collection-1")
        .execute(&pool)
        .await
        .expect("collection-1 series count should align with attached series");
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_detail_marks_partially_visible_collection_as_filtered() {
    let paths = new_router_fixture("router-collection-detail-partially-visible").await;
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

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for filtered collection detail alignment");
    sqlx::query("UPDATE COLLECTION SET SERIES_COUNT = ? WHERE ID = ?")
        .bind(2_i64)
        .bind("collection-1")
        .execute(&pool)
        .await
        .expect("collection-1 series count should align for filtered detail");
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_create_rejects_missing_name_like_kotlin() {
    let paths = new_router_fixture("router-collection-create-missing-name").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_create_rejects_missing_ordered_like_kotlin() {
    let paths = new_router_fixture("router-collection-create-missing-ordered").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_create_rejects_blank_name_like_kotlin() {
    let paths = new_router_fixture("router-collection-create-blank-name").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_create_rejects_empty_series_ids_like_kotlin() {
    let paths = new_router_fixture("router-collection-create-empty-series-ids").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_create_rejects_duplicate_series_ids_like_kotlin() {
    let paths = new_router_fixture("router-collection-create-duplicate-series-ids").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_create_rejects_duplicate_name_like_kotlin() {
    let paths = new_router_fixture("router-collection-create-duplicate-name").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_patch_preserves_unspecified_fields_like_kotlin() {
    let paths = new_router_fixture("router-collection-patch-preserves-unspecified").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let patch_response = app
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

    let detail_response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_patch_rejects_duplicate_name_like_kotlin() {
    let paths = new_router_fixture("router-collection-patch-duplicate-name").await;
    seed_router_contract_data(&paths).await;
    seed_collection_listing_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let patch_response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_patch_ignores_historical_duplicate_when_name_is_unchanged_like_kotlin() {
    let paths =
        new_router_fixture("router-collection-patch-unchanged-name-historical-duplicate").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let patch_response = app
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

    let detail_response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_patch_rejects_duplicate_series_ids_like_kotlin() {
    let paths = new_router_fixture("router-collection-patch-duplicate-series-ids").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let patch_response = app
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

#[tokio::test]
async fn router_readlist_patch_preserves_unspecified_fields() {
    let paths = new_router_fixture("router-readlist-patch-preserves-unspecified-fields").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let patch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/readlists/readlist-1")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"name":"Renamed ReadList"}"#))
                .expect("readlist patch request should build"),
        )
        .await
        .expect("readlist patch request should complete");
    assert_eq!(patch.status(), StatusCode::NO_CONTENT);

    let detail = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist detail request should build"),
        )
        .await
        .expect("readlist detail request should complete");
    assert_eq!(detail.status(), StatusCode::OK);
    let payload = response_json(detail).await;
    assert_eq!(payload.get("name"), Some(&json!("Renamed ReadList")));
    assert_eq!(payload.get("summary"), Some(&json!("")));
    assert_eq!(payload.get("ordered"), Some(&json!(true)));
    assert_eq!(payload.get("bookIds"), Some(&json!(["book-1"])));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_read_list_id_ops_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-read-list-id").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let read_list_is_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "ReadListId", "operator": "is", "value": "readlist-1"}})
                        .to_string(),
                ))
                .expect("strict books/list read-list is match request should build"),
        )
        .await
        .expect("strict books/list read-list is match request should complete");
    assert_eq!(read_list_is_match.status(), StatusCode::OK);
    let read_list_is_match_payload = response_json(read_list_is_match).await;
    let read_list_is_match_content = read_list_is_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books read-list is match payload should expose content array");
    assert_eq!(read_list_is_match_content.len(), 1);

    let read_list_is_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "ReadListId", "operator": "is", "value": "missing-readlist"}})
                        .to_string(),
                ))
                .expect("strict books/list read-list is miss request should build"),
        )
        .await
        .expect("strict books/list read-list is miss request should complete");
    assert_eq!(read_list_is_miss.status(), StatusCode::OK);
    let read_list_is_miss_payload = response_json(read_list_is_miss).await;
    let read_list_is_miss_content = read_list_is_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books read-list is miss payload should expose content array");
    assert_eq!(read_list_is_miss_content.len(), 0);

    let read_list_is_not_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "ReadListId", "operator": "isNot", "value": "missing-readlist"}})
                        .to_string(),
                ))
                .expect("strict books/list read-list isNot match request should build"),
        )
        .await
        .expect("strict books/list read-list isNot match request should complete");
    assert_eq!(read_list_is_not_match.status(), StatusCode::OK);
    let read_list_is_not_match_payload = response_json(read_list_is_not_match).await;
    let read_list_is_not_match_content = read_list_is_not_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books read-list isNot match payload should expose content array");
    assert_eq!(read_list_is_not_match_content.len(), 1);

    let read_list_is_not_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "ReadListId", "operator": "isNot", "value": "readlist-1"}})
                        .to_string(),
                ))
                .expect("strict books/list read-list isNot miss request should build"),
        )
        .await
        .expect("strict books/list read-list isNot miss request should complete");
    assert_eq!(read_list_is_not_miss.status(), StatusCode::OK);
    let read_list_is_not_miss_payload = response_json(read_list_is_not_miss).await;
    let read_list_is_not_miss_content = read_list_is_not_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books read-list isNot miss payload should expose content array");
    assert_eq!(read_list_is_not_miss_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_combined_read_list_id_filters_in_runtime_owned_mode()
{
    let paths =
        new_router_fixture("router-discovery-books-list-strict-read-list-id-combined").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let included_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "AllOfBook",
                            "conditions": [
                                {"type": "ReadListId", "operator": "is", "value": "readlist-1"},
                                {"type": "ReadListId", "operator": "isNot", "value": "missing-readlist"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list combined read-list include request should build"),
        )
        .await
        .expect("strict books/list combined read-list include request should complete");
    assert_eq!(included_response.status(), StatusCode::OK);
    let included_payload = response_json(included_response).await;
    let included_content = included_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books combined read-list include payload should expose content array");
    assert_eq!(included_content.len(), 1);

    let excluded_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "AllOfBook",
                            "conditions": [
                                {"type": "ReadListId", "operator": "is", "value": "readlist-1"},
                                {"type": "ReadListId", "operator": "isNot", "value": "readlist-1"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list combined read-list exclude request should build"),
        )
        .await
        .expect("strict books/list combined read-list exclude request should complete");
    assert_eq!(excluded_response.status(), StatusCode::OK);
    let excluded_payload = response_json(excluded_response).await;
    let excluded_content = excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books combined read-list exclude payload should expose content array");
    assert_eq!(excluded_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_referential_facets_support_repeated_library_id() {
    let paths = new_router_fixture("router-referential-facets-repeated-library-id").await;
    seed_router_contract_data(&paths).await;
    seed_facet_scope_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let cases = [
        (
            "/api/v1/genres?library_id=library-1&library_id=library-2",
            json!(["Drama", "SciFi"]),
        ),
        (
            "/api/v1/tags?library_id=library-1&library_id=library-2",
            json!([
                "Favorite",
                "favorite-tag",
                "other-book-tag",
                "other-series-tag"
            ]),
        ),
        (
            "/api/v1/tags/series?library_id=library-1&library_id=library-2",
            json!(["Favorite", "other-series-tag"]),
        ),
        (
            "/api/v1/languages?library_id=library-1&library_id=library-2",
            json!(["EN", "FR"]),
        ),
        (
            "/api/v1/publishers?library_id=library-1&library_id=library-2",
            json!(["OtherPub", "PubHouse"]),
        ),
        (
            "/api/v1/age-ratings?library_id=library-1&library_id=library-2",
            json!(["12", "16"]),
        ),
        (
            "/api/v1/sharing-labels?library_id=library-1&library_id=library-2",
            json!(["Family", "Friends"]),
        ),
        (
            "/api/v1/series/release-dates?library_id=library-1&library_id=library-2",
            json!(["2025", "2024"]),
        ),
    ];

    for (route, expected) in cases {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("facet repeated-library request should build"),
            )
            .await
            .expect("facet repeated-library request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
        let payload = response_json(response).await;
        assert_eq!(payload, expected, "route: {route}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_referential_facets_filter_repeated_library_scope_to_authorized_libraries() {
    let paths = new_router_fixture("router-referential-facets-authorized-library-scope").await;
    seed_router_contract_data(&paths).await;
    seed_facet_scope_variants(&paths).await;
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

    let cases = [
        (
            "/api/v1/genres?library_id=library-1&library_id=library-2",
            json!(["SciFi"]),
        ),
        (
            "/api/v1/tags?library_id=library-1&library_id=library-2",
            json!(["Favorite", "favorite-tag"]),
        ),
        (
            "/api/v1/tags/series?library_id=library-1&library_id=library-2",
            json!(["Favorite"]),
        ),
        (
            "/api/v1/languages?library_id=library-1&library_id=library-2",
            json!(["EN"]),
        ),
        (
            "/api/v1/publishers?library_id=library-1&library_id=library-2",
            json!(["PubHouse"]),
        ),
        (
            "/api/v1/age-ratings?library_id=library-1&library_id=library-2",
            json!(["16"]),
        ),
        (
            "/api/v1/sharing-labels?library_id=library-1&library_id=library-2",
            json!(["Family"]),
        ),
        (
            "/api/v1/series/release-dates?library_id=library-1&library_id=library-2",
            json!(["2024"]),
        ),
    ];

    for (route, expected) in cases {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("authorized-library facet request should build"),
            )
            .await
            .expect("authorized-library facet request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
        let payload = response_json(response).await;
        assert_eq!(payload, expected, "route: {route}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_genres_deduplicates_shared_values_across_series() {
    let paths = new_router_fixture("router-genres-deduplicates-shared-values").await;
    seed_router_contract_data(&paths).await;
    seed_facet_scope_variants(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("genres dedup db should open");
    sqlx::query("INSERT INTO SERIES_METADATA_GENRE (SERIES_ID, GENRE) VALUES (?, ?)")
        .bind("series-2")
        .bind("SciFi")
        .execute(&pool)
        .await
        .expect("duplicate genre row should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/genres?library_id=library-1&library_id=library-2")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("genres dedup request should build"),
        )
        .await
        .expect("genres dedup request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload, json!(["Drama", "SciFi"]));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_referential_facets_handle_empty_and_null_series_metadata_values() {
    let paths =
        new_router_fixture("router-referential-facets-empty-and-null-series-metadata-values").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("referential facets metadata db should open");
    sqlx::query("UPDATE SERIES_METADATA SET LANGUAGE = ? WHERE SERIES_ID = ?")
        .bind("")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series language should update to empty string for facet test");
    sqlx::query("UPDATE SERIES_METADATA SET PUBLISHER = ? WHERE SERIES_ID = ?")
        .bind("")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series publisher should update to empty string for facet test");
    sqlx::query("UPDATE SERIES_METADATA SET AGE_RATING = NULL WHERE SERIES_ID = ?")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series age rating should update to null for facet test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let cases = [
        ("/api/v1/languages", json!([])),
        ("/api/v1/publishers", json!([])),
        ("/api/v1/age-ratings", json!(["None"])),
    ];

    for (route, expected) in cases {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("metadata facet request should build"),
            )
            .await
            .expect("metadata facet request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
        let payload = response_json(response).await;
        assert_eq!(payload, expected, "route: {route}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_sharing_labels_prefers_library_scope_over_collection_scope() {
    let paths = new_router_fixture("router-sharing-labels-library-wins-over-collection").await;
    seed_router_contract_data(&paths).await;
    seed_facet_scope_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/sharing-labels?library_id=library-2&collection_id=collection-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("sharing labels precedence request should build"),
        )
        .await
        .expect("sharing labels precedence request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload, json!(["Friends"]));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_release_dates_prefers_library_scope_over_collection_scope() {
    let paths =
        new_router_fixture("router-series-release-dates-library-wins-over-collection").await;
    seed_router_contract_data(&paths).await;
    seed_facet_scope_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/release-dates?library_id=library-2&collection_id=collection-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series release dates precedence request should build"),
        )
        .await
        .expect("series release dates precedence request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload, json!(["2025"]));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_release_dates_uses_series_aggregated_release_date() {
    let paths = new_router_fixture("router-series-release-dates-uses-aggregation").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("series release dates aggregation db should open");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-2")
    .bind(0_i64)
    .bind("book-2.epub")
    .bind("books/book-2.epub")
    .bind("series-1")
    .bind(2_048_i64)
    .bind(2_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("secondary same-series book should be inserted");
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("2")
    .bind(2.0_f64)
    .bind("Book 2")
    .bind("2025-02-20")
    .bind("book-2")
    .execute(&pool)
    .await
    .expect("secondary same-series book metadata should be inserted");
    sqlx::query("UPDATE BOOK_METADATA_AGGREGATION SET RELEASE_DATE = ? WHERE SERIES_ID = ?")
        .bind("2024-01-15")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series aggregation release date should be updated");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/release-dates")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series release dates aggregation request should build"),
        )
        .await
        .expect("series release dates aggregation request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload, json!(["2024"]));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_tags_scope_rules_match_visible_libraries() {
    let paths = new_router_fixture("router-book-tags-scope-rules").await;
    seed_router_contract_data(&paths).await;
    seed_facet_scope_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-1-user",
        "library1@example.org",
        "router-contract-library1-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let admin_token = login_with_basic_and_get_token(app.clone()).await;
    let restricted_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library1@example.org",
        "router-contract-library1-123",
    )
    .await;

    let cases = [
        (
            restricted_token.as_str(),
            "/api/v1/tags/book?library_id=library-1&library_id=library-2",
            json!(["favorite-tag"]),
        ),
        (
            restricted_token.as_str(),
            "/api/v1/tags/book?series_id=series-2",
            json!([]),
        ),
        (
            restricted_token.as_str(),
            "/api/v1/tags/book",
            json!(["favorite-tag"]),
        ),
        (
            restricted_token.as_str(),
            "/api/v1/tags/book?series_id=series-1&library_id=library-2",
            json!(["favorite-tag"]),
        ),
        (
            restricted_token.as_str(),
            "/api/v1/tags/book?readlist_id=readlist-1&library_id=library-2",
            json!(["favorite-tag"]),
        ),
        (
            admin_token.as_str(),
            "/api/v1/tags/book",
            json!(["favorite-tag", "other-book-tag"]),
        ),
    ];

    for (token, route, expected) in cases {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", token)
                    .body(Body::empty())
                    .expect("book tags scope request should build"),
            )
            .await
            .expect("book tags scope request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
        let payload = response_json(response).await;
        assert_eq!(payload, expected, "route: {route}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_referential_facets_support_collection_id_scope() {
    let paths = new_router_fixture("router-referential-facets-collection-scope").await;
    seed_router_contract_data(&paths).await;
    seed_facet_scope_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let cases = [
        (
            "/api/v1/genres?collection_id=collection-1",
            json!(["SciFi"]),
        ),
        (
            "/api/v1/tags?collection_id=collection-1",
            json!(["Favorite", "favorite-tag"]),
        ),
        (
            "/api/v1/tags/series?collection_id=collection-1",
            json!(["Favorite"]),
        ),
        (
            "/api/v1/languages?collection_id=collection-1",
            json!(["EN"]),
        ),
        (
            "/api/v1/publishers?collection_id=collection-1",
            json!(["PubHouse"]),
        ),
        (
            "/api/v1/age-ratings?collection_id=collection-1",
            json!(["16"]),
        ),
        (
            "/api/v1/sharing-labels?collection_id=collection-1",
            json!(["Family"]),
        ),
        (
            "/api/v1/series/release-dates?collection_id=collection-1",
            json!(["2024"]),
        ),
    ];

    for (route, expected) in cases {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("facet collection request should build"),
            )
            .await
            .expect("facet collection request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
        let payload = response_json(response).await;
        assert_eq!(payload, expected, "route: {route}");
    }

    cleanup_router_fixture(paths);
}

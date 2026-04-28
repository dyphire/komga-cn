use super::*;
use komga_infrastructure::search::index_lifecycle::{SearchEntityType, SearchIndexLifecycle};

#[tokio::test]
async fn router_readlists_search_uses_relevance_order_like_kotlin() {
    let paths = new_router_fixture("router-readlists-search-relevance-order").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for readlists search relevance seed");
    sqlx::query("UPDATE READLIST SET NAME = ?, SUMMARY = ? WHERE ID = ?")
        .bind("Alpha ReadList")
        .bind("")
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist-1 should update for readlists search relevance seed");
    sqlx::query("INSERT INTO READLIST (ID, NAME, SUMMARY, BOOK_COUNT) VALUES (?, ?, ?, ?)")
        .bind("readlist-2")
        .bind("Alpha Alpha ReadList")
        .bind("")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("readlist-2 row should insert for readlists search relevance seed");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-2")
        .bind("book-1")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("readlist-2 membership should insert for readlists search relevance seed");
    sqlx::query("INSERT INTO READLIST (ID, NAME, SUMMARY, BOOK_COUNT) VALUES (?, ?, ?, ?)")
        .bind("readlist-3")
        .bind("Zulu Alpha ReadList")
        .bind("")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("readlist-3 row should insert for readlists search relevance seed");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-3")
        .bind("book-1")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("readlist-3 membership should insert for readlists search relevance seed");
    pool.close().await;

    let app = build_router_with_config(&search_ready_runtime_config_for_paths(&paths).await).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists?search=alpha&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlists search relevance request should build"),
        )
        .await
        .expect("readlists search relevance request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let ids = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlists search relevance payload should expose content array")
        .iter()
        .map(|entry| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .expect("readlists search relevance entry should expose id")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["readlist-2", "readlist-1", "readlist-3"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlists_search_does_not_match_summary_only_hits_like_kotlin() {
    let paths = new_router_fixture("router-readlists-search-name-only-matches").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for readlists name-only search seed");
    sqlx::query("UPDATE READLIST SET NAME = ?, SUMMARY = ? WHERE ID = ?")
        .bind("Alpha ReadList")
        .bind("")
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist-1 should update for readlists name-only search seed");
    sqlx::query("INSERT INTO READLIST (ID, NAME, SUMMARY, BOOK_COUNT) VALUES (?, ?, ?, ?)")
        .bind("readlist-2")
        .bind("Beta ReadList")
        .bind("alpha")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("readlist-2 row should insert for readlists name-only search seed");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-2")
        .bind("book-1")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("readlist-2 membership should insert for readlists name-only search seed");
    pool.close().await;

    let app = build_router_with_config(&search_ready_runtime_config_for_paths(&paths).await).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists?search=alpha&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlists name-only search request should build"),
        )
        .await
        .expect("readlists name-only search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let ids = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlists name-only search payload should expose content array")
        .iter()
        .map(|entry| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .expect("readlists name-only search entry should expose id")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["readlist-1"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlists_search_matches_accent_folded_names_like_kotlin() {
    let paths = new_router_fixture("router-readlists-search-accent-folding").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for readlists accent-folding seed");
    sqlx::query("UPDATE READLIST SET NAME = ?, SUMMARY = ? WHERE ID = ?")
        .bind("Éclair ReadList")
        .bind("")
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist-1 should update for readlists accent-folding seed");
    pool.close().await;

    let app = build_router_with_config(&search_ready_runtime_config_for_paths(&paths).await).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists?search=eclair&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlists accent-folding search request should build"),
        )
        .await
        .expect("readlists accent-folding search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let ids = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlists accent-folding payload should expose content array")
        .iter()
        .map(|entry| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .expect("readlists accent-folding entry should expose id")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["readlist-1"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlists_search_matches_non_contiguous_multi_token_names_like_kotlin() {
    let paths = new_router_fixture("router-readlists-search-multi-token").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for readlists multi-token seed");
    sqlx::query("UPDATE READLIST SET NAME = ?, SUMMARY = ? WHERE ID = ?")
        .bind("Zulu Alpha ReadList")
        .bind("")
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist-1 should update for readlists multi-token seed");
    sqlx::query("INSERT INTO READLIST (ID, NAME, SUMMARY, BOOK_COUNT) VALUES (?, ?, ?, ?)")
        .bind("readlist-2")
        .bind("Alpha Only ReadList")
        .bind("")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("readlist-2 row should insert for readlists multi-token seed");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-2")
        .bind("book-1")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("readlist-2 membership should insert for readlists multi-token seed");
    pool.close().await;

    let app = build_router_with_config(&search_ready_runtime_config_for_paths(&paths).await).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists?search=alpha%20zulu&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlists multi-token search request should build"),
        )
        .await
        .expect("readlists multi-token search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let ids = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlists multi-token payload should expose content array")
        .iter()
        .map(|entry| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .expect("readlists multi-token entry should expose id")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["readlist-1"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlists_invalid_search_syntax_returns_empty_result_like_kotlin() {
    let paths = new_router_fixture("router-readlists-search-invalid-syntax").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&search_ready_runtime_config_for_paths(&paths).await).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists?search=%28&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlists invalid search syntax request should build"),
        )
        .await
        .expect("readlists invalid search syntax request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlists invalid search syntax payload should expose content array");
    assert!(content.is_empty());
    assert_eq!(
        payload.get("totalElements").and_then(Value::as_u64),
        Some(0)
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlists_search_does_not_drop_visible_hits_after_hidden_ranked_hits_like_kotlin() {
    let paths = new_router_fixture("router-readlists-search-hidden-hits-window").await;
    seed_router_contract_data(&paths).await;
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
        .expect("main db should open for readlists hidden-hit window seed");
    sqlx::query("UPDATE READLIST SET NAME = ?, SUMMARY = ? WHERE ID = ?")
        .bind("Alpha Visible ReadList")
        .bind("")
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist-1 should update for readlists hidden-hit window seed");

    for index in 0..5_i64 {
        let readlist_id = format!("hidden-readlist-{index}");
        let readlist_name = format!("Alpha Alpha Alpha Hidden ReadList {index}");

        sqlx::query("INSERT INTO READLIST (ID, NAME, SUMMARY, BOOK_COUNT) VALUES (?, ?, ?, ?)")
            .bind(&readlist_id)
            .bind(&readlist_name)
            .bind("")
            .bind(1_i64)
            .execute(&pool)
            .await
            .expect("hidden readlist row should insert for hidden-hit window seed");
        sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
            .bind(&readlist_id)
            .bind("book-3")
            .bind(0_i64)
            .execute(&pool)
            .await
            .expect("hidden readlist membership should insert for hidden-hit window seed");
    }
    pool.close().await;

    let config = search_ready_runtime_config_for_paths(&paths).await;
    let app = build_router_with_config(&config).await;
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library1@example.org",
        "router-contract-library1-123",
    )
    .await;

    let ranked_ids = SearchIndexLifecycle::bootstrap(config.lucene_data_directory.as_path())
        .expect("readlists hidden-hit window search index should bootstrap")
        .search_ids("alpha", SearchEntityType::ReadList, 1000)
        .expect("readlists hidden-hit window search query should succeed");
    let expected_visible_ids = ranked_ids
        .into_iter()
        .filter(|id| id == "readlist-1")
        .collect::<Vec<_>>();
    assert_eq!(expected_visible_ids, vec!["readlist-1".to_string()]);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists?search=alpha&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlists hidden-hit window request should build"),
        )
        .await
        .expect("readlists hidden-hit window request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let ids = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlists hidden-hit window payload should expose content array")
        .iter()
        .map(|entry| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .expect("readlists hidden-hit window entry should expose id")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(ids, expected_visible_ids);

    cleanup_router_fixture(paths);
}

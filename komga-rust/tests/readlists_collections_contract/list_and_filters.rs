use super::*;

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

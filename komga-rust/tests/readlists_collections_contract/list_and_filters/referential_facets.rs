use super::*;

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

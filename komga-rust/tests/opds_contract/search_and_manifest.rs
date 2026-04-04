use super::*;

#[tokio::test]
async fn router_opds_v2_manifest_sets_private_cache_and_supports_if_none_match() {
    let paths = new_router_fixture("router-opds-v2-manifest-cache-headers").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let first_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/books/book-1/manifest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 manifest conditional request should build"),
        )
        .await
        .expect("opds v2 manifest conditional request should complete");

    assert_eq!(first_response.status(), StatusCode::OK);
    assert_eq!(
        first_response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("max-age=0, must-revalidate, private")
    );

    let etag = first_response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("opds v2 manifest response should include etag");

    let second_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/books/book-1/manifest")
                .header("x-auth-token", &auth_token)
                .header(header::IF_NONE_MATCH, etag)
                .body(Body::empty())
                .expect("opds v2 manifest conditional follow-up request should build"),
        )
        .await
        .expect("opds v2 manifest conditional follow-up request should complete");

    assert_eq!(second_response.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(
        second_response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("max-age=0, must-revalidate, private")
    );
    assert!(second_response.headers().contains_key(header::ETAG));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_latest_books_feed_hides_books_for_age_exclude_restricted_user() {
    let paths = new_router_fixture("router-opds-v2-latest-books-age-restricted").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/library-1/books/latest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 latest-books request should build"),
        )
        .await
        .expect("opds v2 latest-books request should complete");

    let status = response.status();
    let payload = response_json(response).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "unexpected OPDS latest-books response payload: {payload}",
    );
    assert_eq!(
        payload
            .get("publications")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(0),
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_search_feed_uses_search_title_for_non_blank_search() {
    let paths = new_router_fixture("router-opds-v1-series-search-title").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series?search=Series")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 series search request should build"),
        )
        .await
        .expect("opds v1 series search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains("Series search for: Series"),
        "OPDS v1 non-blank search must expose Kotlin-compatible feed title, body={body}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_search_uses_acquisition_type_and_utf8_encodings() {
    let paths = new_router_fixture("router-opds-v1-search-opensearch-shape").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/search")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 search request should build"),
        )
        .await
        .expect("opds v1 search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains("application/atom+xml;profile=opds-catalog;kind=acquisition"),
        "OPDS v1 search must use acquisition kind in OpenSearch Url, body={body}"
    );
    assert!(
        body.contains("<InputEncoding>UTF-8</InputEncoding>"),
        "OPDS v1 search must include InputEncoding UTF-8, body={body}"
    );
    assert!(
        body.contains("<OutputEncoding>UTF-8</OutputEncoding>"),
        "OPDS v1 search must include OutputEncoding UTF-8, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_blank_search_behaves_as_unfiltered_series_feed() {
    let paths = new_router_fixture("router-opds-v1-series-blank-search").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-alpha", "Alpha Series", "library-1").await;
    seed_router_custom_series(&paths, "series-zeta", "Zeta Series", "library-1").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series?search=%20%20%20")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 blank-search request should build"),
        )
        .await
        .expect("opds v1 blank-search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains("<title>All series</title>"),
        "OPDS v1 blank search must fall back to unfiltered All series feed, body={body}",
    );
    assert!(
        body.contains("/opds/v1.2/series/series-alpha")
            && body.contains("/opds/v1.2/series/series-zeta"),
        "OPDS v1 blank search must not filter out matching libraries, body={body}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_search_hides_unauthorized_library_series() {
    let paths = new_router_fixture("router-opds-v1-series-library-visibility").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-restricted-user",
        "library.restricted@example.org",
        "router-contract-library-restricted-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library.restricted@example.org",
        "router-contract-library-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series?search=Series")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 restricted search request should build"),
        )
        .await
        .expect("opds v1 restricted search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        !body.contains("/opds/v1.2/series/series-3"),
        "OPDS v1 search must hide series from unauthorized libraries, body={body}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_latest_series_feed_hides_series_for_age_exclude_restricted_user() {
    let paths = new_router_fixture("router-opds-v1-latest-series-age-restricted").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series/latest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 latest-series request should build"),
        )
        .await
        .expect("opds v1 latest-series request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        !body.contains("/opds/v1.2/series/series-1"),
        "OPDS v1 latest-series feed must hide restricted series, body={body}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_latest_series_feed_paginates_after_restriction_filtering() {
    let paths = new_router_fixture("router-opds-v1-latest-series-restricted-pagination").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-2", "Series 2", "library-1").await;
    seed_router_custom_series(&paths, "series-0", "Series 0", "library-1").await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds latest-series pagination db should open");
    sqlx::query("UPDATE SERIES_METADATA SET AGE_RATING = ? WHERE SERIES_ID = ?")
        .bind(0_i64)
        .bind("series-0")
        .execute(&pool)
        .await
        .expect("visible latest series age rating should update");
    for (series_id, last_modified) in [
        ("series-2", "2024-03-03T00:00:00"),
        ("series-1", "2024-03-02T00:00:00"),
        ("series-0", "2024-03-01T00:00:00"),
    ] {
        sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ?, CREATED_DATE = ? WHERE ID = ?")
            .bind(last_modified)
            .bind(last_modified)
            .bind(series_id)
            .execute(&pool)
            .await
            .expect("series latest ordering should update");
    }
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series/latest?page=0&size=1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 latest-series paged request should build"),
        )
        .await
        .expect("opds v1 latest-series paged request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("/opds/v1.2/series/series-0"));
    assert!(!body.contains("/opds/v1.2/series/series-2"));
    assert!(!body.contains("/opds/v1.2/series/series-1"));
    assert!(
        !body.contains("rel=\"next\""),
        "OPDS v1 latest-series must compute pagination after restrictions filtering, body={body}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_latest_series_feed_normalizes_entry_updated_to_utc_z() {
    let paths = new_router_fixture("router-opds-v1-latest-series-updated-format").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds latest-series updated db should open");
    sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ?, CREATED_DATE = ? WHERE ID = ?")
        .bind("2024-03-03 00:00:00")
        .bind("2024-03-03 00:00:00")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("latest series updated timestamp should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series/latest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 latest-series updated request should build"),
        )
        .await
        .expect("opds v1 latest-series updated request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains("<updated>2024-03-03T00:00:00Z</updated>"),
        "OPDS v1 latest-series entry updated must be normalized to UTC/Z, body={body}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_search_supports_fielded_query_candidate_lookup() {
    let paths = new_router_fixture("router-opds-v1-series-fielded-query").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series?search=publisher:AltPub")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 fielded search request should build"),
        )
        .await
        .expect("opds v1 fielded search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains("Series search for: publisher:AltPub"),
        "OPDS v1 fielded search should preserve feed title semantics, body={body}",
    );
    assert!(
        body.contains("/opds/v1.2/series/series-3"),
        "OPDS v1 fielded search should surface unified-search candidate matches, body={body}",
    );
    assert!(
        !body.contains("/opds/v1.2/series/series-1")
            && !body.contains("/opds/v1.2/series/series-2"),
        "OPDS v1 fielded search should keep result set narrowed to matching candidates, body={body}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_search_query_contract_covers_group_presence_and_order() {
    let paths = new_router_fixture("router-opds-v2-search-group-contract").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let expectations = [
        (
            "/opds/v2/search?query=Series%201",
            vec!["Series"],
            "single-group search should only retain non-empty groups",
        ),
        (
            "/opds/v2/search?query=1",
            vec!["Series", "Books", "Read Lists"],
            "multi-group search should preserve Kotlin group ordering",
        ),
    ];

    for (uri, expected_group_titles, context) in expectations {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(uri)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("opds v2 search request should build"),
            )
            .await
            .expect("opds v2 search request should complete");

        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        let groups = payload
            .get("groups")
            .and_then(Value::as_array)
            .expect("opds v2 search payload should expose groups array");
        let group_titles = groups
            .iter()
            .filter_map(|group| {
                group
                    .get("metadata")
                    .and_then(|value| value.get("title"))
                    .and_then(Value::as_str)
            })
            .collect::<Vec<_>>();

        assert_eq!(group_titles, expected_group_titles, "{context}: {payload}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_search_supports_fielded_query_candidate_lookup() {
    let paths = new_router_fixture("router-opds-v2-search-fielded-query").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/search?query=title:1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 fielded search request should build"),
        )
        .await
        .expect("opds v2 fielded search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let groups = payload
        .get("groups")
        .and_then(Value::as_array)
        .expect("opds v2 fielded search payload should expose groups array");
    let group_titles = groups
        .iter()
        .filter_map(|group| {
            group
                .get("metadata")
                .and_then(|value| value.get("title"))
                .and_then(Value::as_str)
        })
        .collect::<Vec<_>>();

    assert_eq!(
        group_titles,
        vec!["Series", "Books", "Read Lists"],
        "{payload}"
    );

    let rendered = payload.to_string();
    assert!(
        rendered.contains("/opds/v2/series/series-1")
            && rendered.contains("book-1/manifest")
            && rendered.contains("/opds/v2/readlists/readlist-1"),
        "OPDS v2 fielded search should include unified-search candidate matches: {payload}",
    );
    assert!(
        !rendered.contains("/opds/v2/series/series-2")
            && !rendered.contains("/opds/v2/series/series-3")
            && !rendered.contains("book-2/manifest")
            && !rendered.contains("book-3/manifest"),
        "OPDS v2 fielded search should keep non-matching entities out of groups: {payload}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_search_hides_unauthorized_library_results() {
    let paths = new_router_fixture("router-opds-v2-search-library-visibility").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-restricted-user-v2",
        "library.restricted.v2@example.org",
        "router-contract-library-restricted-v2-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library.restricted.v2@example.org",
        "router-contract-library-restricted-v2-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/search?query=Series%203")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 restricted search request should build"),
        )
        .await
        .expect("opds v2 restricted search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let groups = payload
        .get("groups")
        .and_then(Value::as_array)
        .expect("opds v2 restricted search payload should expose groups array");
    assert!(
        groups.is_empty(),
        "OPDS v2 search must omit unauthorized-only results instead of returning empty groups: {payload}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_search_supports_accent_folded_and_cjk_series_queries() {
    let paths = new_router_fixture("router-opds-search-accent-cjk-recall").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-cafe", "Café 東京 Series", "library-1").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let v1_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series?search=Cafe%20東京")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 accent+cjk search request should build"),
        )
        .await
        .expect("opds v1 accent+cjk search request should complete");
    assert_eq!(v1_response.status(), StatusCode::OK);
    let v1_body = response_text(v1_response).await;
    assert!(
        v1_body.contains("/opds/v1.2/series/series-cafe"),
        "OPDS v1 search should retain accent-folded mixed CJK recall: {v1_body}",
    );

    let v2_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/search?query=Cafe%20東京")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 accent+cjk search request should build"),
        )
        .await
        .expect("opds v2 accent+cjk search request should complete");
    assert_eq!(v2_response.status(), StatusCode::OK);
    let v2_payload = response_json(v2_response).await;
    let rendered = v2_payload.to_string();
    assert!(
        rendered.contains("/opds/v2/series/series-cafe"),
        "OPDS v2 search should retain accent-folded mixed CJK recall: {v2_payload}",
    );

    cleanup_router_fixture(paths);
}

use super::*;

mod v2;

#[tokio::test]
async fn router_opds_v2_latest_books_feed_hides_books_for_age_exclude_restricted_user() {
    let ctx = TestFixture::builder("router-opds-v2-latest-books-age-restricted")
        .with_search_index()
        .build()
        .await;
    seed_router_age_exclude_user(
        ctx.paths(),
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;

    let auth_token = ctx
        .login_with_credentials("restricted@example.org", "router-contract-restricted-123")
        .await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_opds_v1_series_search_feed_uses_search_title_for_non_blank_search() {
    let ctx = TestFixture::builder("router-opds-v1-series-search-title")
        .with_search_index()
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_opds_v1_series_preserves_active_query_params_in_self_prev_next_links() {
    let ctx = TestFixture::builder("router-opds-v1-series-query-links")
        .with_search_index()
        .with_seed(|paths| async move {
            seed_router_authors_scope_variants(&paths).await;
        })
        .build()
        .await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app().clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series?page=1&size=1&search=Series&publisher=PubHouse&publisher=AltPub")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 series query-links request should build"),
        )
        .await
        .expect("opds v1 series query-links request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains("rel=\"self\" href=\"http://localhost/opds/v1.2/series?search=Series&amp;publisher=PubHouse&amp;publisher=AltPub\""),
        "body={body}"
    );
    assert!(
        body.contains("rel=\"previous\" href=\"http://localhost/opds/v1.2/series?search=Series&amp;publisher=PubHouse&amp;publisher=AltPub&amp;page=0\""),
        "body={body}"
    );
    assert!(
        body.contains("rel=\"next\" href=\"http://localhost/opds/v1.2/series?search=Series&amp;publisher=PubHouse&amp;publisher=AltPub&amp;page=2\""),
        "body={body}"
    );
}

#[tokio::test]
async fn router_opds_v1_series_feeds_use_series_last_modified_for_entry_updated() {
    for (fixture_name, route, timestamp) in [
        (
            "router-opds-v1-series-entry-updated",
            "/opds/v1.2/series",
            "2024-03-03 00:00:00",
        ),
        (
            "router-opds-v1-series-search-entry-updated",
            "/opds/v1.2/series?search=Series%201",
            "2024-03-04 00:00:00",
        ),
    ] {
        let ctx = TestFixture::builder(fixture_name)
            .with_search_index()
            .build()
            .await;

        let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
            .await
            .expect("opds v1 series entry-updated db should open");
        sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ?, CREATED_DATE = ? WHERE ID = ?")
            .bind(timestamp)
            .bind(timestamp)
            .bind("series-1")
            .execute(&pool)
            .await
            .expect("series last modified should update for entry-updated test");
        pool.close().await;

        let auth_token = ctx.login_admin().await;

        let response = ctx
            .app()
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("opds v1 series entry-updated request should build"),
            )
            .await
            .expect("opds v1 series entry-updated request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
        let body = response_text(response).await;
        let expected_updated = format!(
            "<entry><title>Series 1</title><updated>{}T00:00:00Z</updated><id>series-1</id><content></content>",
            &timestamp[..10]
        );
        assert!(
            body.contains(&expected_updated),
            "route: {route}, body={body}"
        );
    }
}

#[tokio::test]
async fn router_opds_v1_search_uses_acquisition_type_and_utf8_encodings() {
    let ctx = TestFixture::builder("router-opds-v1-search-opensearch-shape")
        .with_search_index()
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_opds_v1_series_blank_search_behaves_as_unfiltered_series_feed() {
    let ctx = TestFixture::builder("router-opds-v1-series-blank-search")
        .with_search_index()
        .with_seed(|paths| async move {
            seed_router_custom_series(&paths, "series-alpha", "Alpha Series", "library-1").await;
            seed_router_custom_series(&paths, "series-zeta", "Zeta Series", "library-1").await;
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
}

#[tokio::test]
async fn router_opds_v1_series_search_hides_unauthorized_library_series() {
    let ctx = TestFixture::builder("router-opds-v1-series-library-visibility")
        .with_search_index()
        .with_seed(|paths| async move {
            seed_router_authors_scope_variants(&paths).await;
            seed_router_library_restricted_user(
                &paths,
                "library-restricted-user",
                "library.restricted@example.org",
                "router-contract-library-restricted-123",
                &["library-1"],
            )
            .await;
        })
        .build()
        .await;

    let auth_token = ctx
        .login_with_credentials(
            "library.restricted@example.org",
            "router-contract-library-restricted-123",
        )
        .await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_opds_v1_latest_series_feed_hides_series_for_age_exclude_restricted_user() {
    let ctx = TestFixture::builder("router-opds-v1-latest-series-age-restricted")
        .with_search_index()
        .build()
        .await;
    seed_router_age_exclude_user(
        ctx.paths(),
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;

    let auth_token = ctx
        .login_with_credentials("restricted@example.org", "router-contract-restricted-123")
        .await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_opds_v1_latest_series_feed_paginates_after_restriction_filtering() {
    let ctx = TestFixture::builder("router-opds-v1-latest-series-restricted-pagination")
        .with_search_index()
        .with_seed(|paths| async move {
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
        })
        .build()
        .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
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

    let auth_token = ctx
        .login_with_credentials("restricted@example.org", "router-contract-restricted-123")
        .await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_opds_v1_latest_series_feed_normalizes_entry_updated_to_utc_z() {
    let ctx = TestFixture::builder("router-opds-v1-latest-series-updated-format")
        .with_search_index()
        .build()
        .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
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

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_opds_v1_series_search_supports_fielded_query_candidate_lookup() {
    let ctx = TestFixture::builder("router-opds-v1-series-fielded-query")
        .with_search_index()
        .with_seed(|paths| async move {
            seed_router_authors_scope_variants(&paths).await;
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
}

#[tokio::test]
async fn router_opds_search_supports_accent_folded_and_cjk_series_queries() {
    let ctx = TestFixture::builder("router-opds-search-accent-cjk-recall")
        .with_search_index()
        .with_seed(|paths| async move {
            seed_router_custom_series(&paths, "series-cafe", "Café 東京 Series", "library-1").await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let v1_response = ctx
        .app()
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

    let v2_response = ctx
        .app()
        .clone()
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
}

use super::*;

#[tokio::test]
async fn router_opds_v2_search_query_contract_covers_group_presence_and_order() {
    let ctx = TestFixture::builder("router-opds-v2-search-group-contract")
        .with_search_index()
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

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
        let response = ctx
            .app().clone()
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
}

#[tokio::test]
async fn router_opds_v2_search_supports_fielded_query_candidate_lookup() {
    let ctx = TestFixture::builder("router-opds-v2-search-fielded-query")
        .with_search_index()
        .build()
        .await;
    seed_router_authors_scope_variants(ctx.paths()).await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app().clone()
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

    if response.status() != StatusCode::OK {
        let status = response.status();
        let body = response_text(response).await;
        panic!("unexpected search status {status}: {body}");
    }
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
}

#[tokio::test]
async fn router_opds_v2_search_excludes_one_shot_series_for_blank_and_ranked_queries() {
    let ctx = TestFixture::builder("router-opds-v2-search-excludes-one-shots")
        .with_search_index()
        .with_seed(|paths| async move {
            seed_router_custom_series(
                &paths,
                "series-oneshot-search",
                "One Shot Search",
                "library-1",
            )
            .await;
        })
        .build()
        .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("opds v2 search one-shot db should open");
    sqlx::query("UPDATE SERIES SET ONESHOT = ? WHERE ID = ?")
        .bind(true)
        .bind("series-oneshot-search")
        .execute(&pool)
        .await
        .expect("opds v2 search one-shot series should update");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let expectations = [
        (
            "/opds/v2/search",
            "blank search should omit one-shot series from default results",
        ),
        (
            "/opds/v2/search?query=One%20Shot%20Search",
            "fielded search should omit one-shot series from ranked results",
        ),
    ];

    for (uri, context) in expectations {
        let response = ctx
            .app().clone()
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(uri)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("opds v2 one-shot search request should build"),
            )
            .await
            .expect("opds v2 one-shot search request should complete");

        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        let rendered = payload.to_string();
        assert!(
            !rendered.contains("/opds/v2/series/series-oneshot-search"),
            "{context}: {payload}",
        );
    }
}

#[tokio::test]
async fn router_opds_v2_search_books_group_uses_shared_publication_shape() {
    let ctx = TestFixture::builder("router-opds-v2-search-book-publication-shape")
        .with_search_index()
        .build()
        .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("opds v2 search publication db should open");
    sqlx::query(
        "UPDATE BOOK_METADATA SET SUMMARY = ?, ISBN = ?, RELEASE_DATE = ? WHERE BOOK_ID = ?",
    )
    .bind("Search fixture summary")
    .bind("9781234567890")
    .bind("2024-02-03T04:05:06Z")
    .bind("book-1")
    .execute(&pool)
    .await
    .expect("opds v2 search book metadata should seed");
    sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
        .bind("book-1")
        .bind("Search Author")
        .bind("author")
        .execute(&pool)
        .await
        .expect("opds v2 search author should seed");
    sqlx::query("INSERT INTO BOOK_METADATA_TAG (BOOK_ID, TAG) VALUES (?, ?)")
        .bind("book-1")
        .bind("SearchTag")
        .execute(&pool)
        .await
        .expect("opds v2 search tag should seed");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app().clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/search?query=title:1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 search book publication request should build"),
        )
        .await
        .expect("opds v2 search book publication request should complete");

    if response.status() != StatusCode::OK {
        let status = response.status();
        let body = response_text(response).await;
        panic!("unexpected search status {status}: {body}");
    }
    let payload = response_json(response).await;
    let books_group = payload
        .get("groups")
        .and_then(Value::as_array)
        .and_then(|groups| {
            groups.iter().find(|group| {
                group
                    .get("metadata")
                    .and_then(|value| value.get("title"))
                    .and_then(Value::as_str)
                    == Some("Books")
            })
        })
        .expect("opds v2 search should expose Books group");
    let publication = books_group
        .get("publications")
        .and_then(Value::as_array)
        .and_then(|publications| publications.first())
        .expect("opds v2 search Books group should expose publications");

    assert_eq!(
        publication
            .get("metadata")
            .and_then(|value| value.get("identifier"))
            .and_then(Value::as_str),
        Some("urn:isbn:9781234567890")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|value| value.get("description"))
            .and_then(Value::as_str),
        Some("Search fixture summary")
    );
    assert!(
        publication
            .get("metadata")
            .and_then(|value| value.get("author"))
            .and_then(Value::as_array)
            .is_some_and(|authors| authors
                .iter()
                .any(|author| author.as_str() == Some("Search Author"))),
        "search publication should expose author metadata: {publication:?}"
    );
    assert!(
        publication
            .get("metadata")
            .and_then(|value| value.get("subject"))
            .and_then(Value::as_array)
            .is_some_and(|subjects| subjects
                .iter()
                .any(|subject| subject.as_str() == Some("SearchTag"))),
        "search publication should expose subject metadata: {publication:?}"
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|value| value.get("belongsTo"))
            .and_then(|value| value.get("series"))
            .and_then(Value::as_array)
            .and_then(|series| series.first())
            .and_then(|series| series.get("links"))
            .and_then(Value::as_array)
            .and_then(|links| links.first())
            .and_then(|link| link.get("href"))
            .and_then(Value::as_str),
        Some("http://localhost/opds/v2/series/series-1")
    );
    assert!(
        publication
            .get("links")
            .and_then(Value::as_array)
            .is_some_and(|links| {
                links.iter().any(|link| {
                    link.get("rel").and_then(Value::as_str)
                        == Some("http://opds-spec.org/acquisition")
                        && link.get("href").and_then(Value::as_str)
                            == Some("http://localhost/opds/v2/books/book-1/file")
                })
            }),
        "search publication should expose acquisition link: {publication:?}"
    );
    assert!(
        publication
            .get("links")
            .and_then(Value::as_array)
            .is_some_and(|links| {
                links.iter().any(|link| {
                    link.get("rel").and_then(Value::as_str)
                        == Some("http://www.cantook.com/api/progression")
                        && link.get("href").and_then(Value::as_str)
                            == Some("http://localhost/opds/v2/books/book-1/progression")
                })
            }),
        "search publication should expose progression link: {publication:?}"
    );
    assert_eq!(
        publication
            .get("images")
            .and_then(Value::as_array)
            .and_then(|images| images.first())
            .and_then(|image| image.get("href"))
            .and_then(Value::as_str),
        Some("http://localhost/opds/v2/books/book-1/thumbnail")
    );
}

#[tokio::test]
async fn router_opds_v2_search_hides_unauthorized_library_results() {
    let ctx = TestFixture::builder("router-opds-v2-search-library-visibility")
        .with_search_index()
        .build()
        .await;
    seed_router_authors_scope_variants(ctx.paths()).await;
    seed_router_library_restricted_user(
        ctx.paths(),
        "library-restricted-user-v2",
        "library.restricted.v2@example.org",
        "router-contract-library-restricted-v2-123",
        &["library-1"],
    )
    .await;

    let auth_token = ctx
        .login_with_credentials(
            "library.restricted.v2@example.org",
            "router-contract-library-restricted-v2-123",
        )
        .await;

    let response = ctx
        .app().clone()
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
}

#[tokio::test]
async fn router_opds_v2_search_hides_results_for_age_exclude_restricted_user() {
    let ctx = TestFixture::builder("router-opds-v2-search-age-restricted")
        .with_search_index()
        .build()
        .await;
    seed_router_age_exclude_user(
        ctx.paths(),
        "search-restricted-user",
        "search.restricted@example.org",
        "router-contract-search-restricted-123",
        12,
    )
    .await;

    let restricted_auth_token = ctx
        .login_with_credentials(
            "search.restricted@example.org",
            "router-contract-search-restricted-123",
        )
        .await;

    for uri in ["/opds/v2/search?query=1", "/opds/v2/search"] {
        let restricted_response = ctx
            .app().clone()
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(uri)
                    .header("x-auth-token", &restricted_auth_token)
                    .body(Body::empty())
                    .expect("opds v2 search restricted request should build"),
            )
            .await
            .expect("opds v2 search restricted request should complete");

        assert_eq!(restricted_response.status(), StatusCode::OK);
        let restricted_payload = response_json(restricted_response).await;
        let restricted_groups = restricted_payload
            .get("groups")
            .and_then(Value::as_array)
            .expect("opds v2 search restricted payload should expose groups array");
        assert!(
            restricted_groups.is_empty(),
            "age-exclude restricted OPDS search should hide restricted groups for {uri}: {restricted_payload}",
        );
    }
}

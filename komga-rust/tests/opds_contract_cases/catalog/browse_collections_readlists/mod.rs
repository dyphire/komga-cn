use super::*;

mod readlists;

#[tokio::test]
async fn router_opds_v2_library_browse_uses_kotlin_top_level_title_and_links() {
    let paths = new_router_fixture("router-opds-v2-library-browse-route").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for (route, expected_title, expected_self_href) in [
        (
            "/opds/v2/libraries/browse",
            "All libraries",
            "http://localhost/opds/v2/libraries/browse",
        ),
        (
            "/opds/v2/libraries/library-1/browse",
            "Library 1",
            "http://localhost/opds/v2/libraries/library-1/browse",
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("opds v2 browse request should build"),
            )
            .await
            .expect("opds v2 browse request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
        let payload = response_json(response).await;
        assert_eq!(
            payload
                .get("metadata")
                .and_then(|metadata| metadata.get("title"))
                .and_then(Value::as_str),
            Some(expected_title),
            "route: {route}"
        );

        let self_link = payload
            .get("links")
            .and_then(Value::as_array)
            .and_then(|links| {
                links
                    .iter()
                    .find(|link| link.get("rel").and_then(Value::as_str) == Some("self"))
            })
            .expect("browse self link should be present");
        assert_eq!(
            self_link.get("href").and_then(Value::as_str),
            Some(expected_self_href),
            "route: {route}"
        );
        assert!(
            self_link.get("type").is_none(),
            "route: {route}, browse self link should omit type like Kotlin"
        );

        let start_link = payload
            .get("links")
            .and_then(Value::as_array)
            .and_then(|links| {
                links
                    .iter()
                    .find(|link| link.get("rel").and_then(Value::as_str) == Some("start"))
            })
            .expect("browse start link should be present");
        assert_eq!(
            start_link.get("title").and_then(Value::as_str),
            Some("Home"),
            "route: {route}"
        );

        let search_link = payload
            .get("links")
            .and_then(Value::as_array)
            .and_then(|links| {
                links
                    .iter()
                    .find(|link| link.get("rel").and_then(Value::as_str) == Some("search"))
            })
            .expect("browse search link should be present");
        assert_eq!(
            search_link.get("title").and_then(Value::as_str),
            Some("Search"),
            "route: {route}"
        );

        assert!(
            payload
                .get("navigation")
                .and_then(Value::as_array)
                .is_some_and(|entries| !entries.is_empty()),
            "route: {route}"
        );
        assert!(
            payload
                .get("groups")
                .and_then(Value::as_array)
                .is_some_and(|groups| !groups.is_empty()),
            "route: {route}"
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_collections_use_kotlin_top_level_shape() {
    let paths = new_router_fixture("router-opds-v2-collections-shape").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for (route, expected_title, expected_self_href) in [
        (
            "/opds/v2/libraries/collections",
            "All libraries - Collections",
            "http://localhost/opds/v2/libraries/collections",
        ),
        (
            "/opds/v2/libraries/library-1/collections",
            "Library 1 - Collections",
            "http://localhost/opds/v2/libraries/library-1/collections",
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("opds v2 collections request should build"),
            )
            .await
            .expect("opds v2 collections request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
        let payload = response_json(response).await;
        assert_eq!(
            payload
                .get("metadata")
                .and_then(|metadata| metadata.get("title"))
                .and_then(Value::as_str),
            Some(expected_title),
            "route: {route}"
        );
        assert!(
            payload
                .get("metadata")
                .and_then(|metadata| metadata.get("modified"))
                .and_then(Value::as_str)
                .is_some(),
            "route: {route}, metadata.modified should be present"
        );

        let self_link = payload
            .get("links")
            .and_then(Value::as_array)
            .and_then(|links| {
                links
                    .iter()
                    .find(|link| link.get("rel").and_then(Value::as_str) == Some("self"))
            })
            .expect("collections self link should be present");
        assert_eq!(
            self_link.get("href").and_then(Value::as_str),
            Some(expected_self_href),
            "route: {route}"
        );
        assert!(
            self_link.get("type").is_none(),
            "route: {route}, collections self link should omit type like Kotlin"
        );

        assert!(
            payload
                .get("navigation")
                .and_then(Value::as_array)
                .is_some_and(|entries| entries.iter().any(|entry| {
                    entry.get("title").and_then(Value::as_str) == Some("Collections")
                })),
            "route: {route}, top-level navigation should keep subsection links"
        );

        let groups = payload
            .get("groups")
            .and_then(Value::as_array)
            .expect("collections groups should be present");
        let collections_group = groups
            .iter()
            .find(|group| {
                group
                    .get("metadata")
                    .and_then(|metadata| metadata.get("title"))
                    .and_then(Value::as_str)
                    == Some("Collections")
            })
            .expect("collections group should be present");
        assert!(
            collections_group.get("links").is_none()
                || collections_group
                    .get("links")
                    .and_then(Value::as_array)
                    .is_some_and(Vec::is_empty),
            "route: {route}, collections group should not need links"
        );
        let group_navigation = collections_group
            .get("navigation")
            .and_then(Value::as_array)
            .expect("collections group navigation should be present");
        assert!(!group_navigation.is_empty(), "route: {route}");
        assert!(
            payload
                .get("navigation")
                .and_then(Value::as_array)
                .is_some_and(|entries| entries.iter().all(|entry| {
                    entry
                        .get("href")
                        .and_then(Value::as_str)
                        .is_none_or(|href| !href.contains("/opds/v2/collections/"))
                })),
            "route: {route}, collection detail links should not live in top-level navigation"
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_collections_include_kotlin_paging_metadata_and_links() {
    let paths = new_router_fixture("router-opds-v2-collections-paging-metadata").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v2 collections paging db should open");
    sqlx::query("INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)")
        .bind("collection-2")
        .bind("Collection 2")
        .bind(false)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("second collection should be inserted");
    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind("collection-2")
    .bind("series-1")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("second collection series should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for (route, expected_self_href, expected_next_href) in [
        (
            "/opds/v2/libraries/collections?size=1",
            "http://localhost/opds/v2/libraries/collections",
            "http://localhost/opds/v2/libraries/collections?page=1",
        ),
        (
            "/opds/v2/libraries/library-1/collections?size=1",
            "http://localhost/opds/v2/libraries/library-1/collections",
            "http://localhost/opds/v2/libraries/library-1/collections?page=1",
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("opds v2 collections paging request should build"),
            )
            .await
            .expect("opds v2 collections paging request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
        let payload = response_json(response).await;
        let metadata = payload.get("metadata").expect("collections metadata should be present");
        assert_eq!(
            metadata.get("itemsPerPage").and_then(Value::as_u64),
            Some(1),
            "route: {route}"
        );
        assert_eq!(
            metadata.get("currentPage").and_then(Value::as_u64),
            Some(1),
            "route: {route}"
        );
        assert_eq!(
            metadata.get("numberOfItems").and_then(Value::as_u64),
            Some(2),
            "route: {route}"
        );

        let links = payload
            .get("links")
            .and_then(Value::as_array)
            .expect("collections links should be present");
        let self_link = links
            .iter()
            .find(|link| link.get("rel").and_then(Value::as_str) == Some("self"))
            .expect("collections self link should be present");
        assert_eq!(
            self_link.get("href").and_then(Value::as_str),
            Some(expected_self_href),
            "route: {route}"
        );
        let next_link = links
            .iter()
            .find(|link| link.get("rel").and_then(Value::as_str) == Some("next"))
            .expect("collections next link should be present");
        assert_eq!(
            next_link.get("href").and_then(Value::as_str),
            Some(expected_next_href),
            "route: {route}"
        );

        let groups = payload
            .get("groups")
            .and_then(Value::as_array)
            .expect("collections groups should be present");
        let collection_titles = groups[0]
            .get("navigation")
            .and_then(Value::as_array)
            .expect("collections group navigation should be present")
            .iter()
            .filter_map(|entry| entry.get("title").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(collection_titles, vec!["Collection 1"], "route: {route}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_collections_keep_restricted_collections_in_groups_like_kotlin() {
    let paths = new_router_fixture("router-opds-v2-collections-restricted-group-visibility").await;
    seed_router_contract_data(&paths).await;
    update_router_series_age_rating(&paths, "series-1", 21).await;
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

    for route in [
        "/opds/v2/libraries/collections",
        "/opds/v2/libraries/library-1/collections",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("opds v2 collections restricted request should build"),
            )
            .await
            .expect("opds v2 collections restricted request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
        let payload = response_json(response).await;
        assert_eq!(
            payload
                .get("metadata")
                .and_then(|metadata| metadata.get("numberOfItems"))
                .and_then(Value::as_u64),
            Some(1),
            "route: {route}"
        );

        let groups = payload
            .get("groups")
            .and_then(Value::as_array)
            .expect("collections groups should be present");
        let collection_titles = groups[0]
            .get("navigation")
            .and_then(Value::as_array)
            .expect("collections group navigation should be present")
            .iter()
            .filter_map(|entry| entry.get("title").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(collection_titles, vec!["Collection 1"], "route: {route}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_collections_top_level_navigation_hides_empty_subsections() {
    let paths = new_router_fixture("router-opds-v2-collections-empty-subsections").await;
    seed_router_contract_data(&paths).await;
    clear_router_collections_and_readlists(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for route in [
        "/opds/v2/libraries/collections",
        "/opds/v2/libraries/library-1/collections",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("opds v2 collections request should build"),
            )
            .await
            .expect("opds v2 collections request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
        let payload = response_json(response).await;
        let navigation_titles = payload
            .get("navigation")
            .and_then(Value::as_array)
            .expect("collections top-level navigation should be present")
            .iter()
            .filter_map(|entry| entry.get("title").and_then(Value::as_str))
            .collect::<Vec<_>>();

        assert_eq!(
            navigation_titles,
            vec!["Recommended", "Browse"],
            "route: {route}, empty collections/readlists should be omitted from top-level navigation"
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_collection_unauthorized_returns_opds_auth_document() {
    let paths = new_router_fixture("router-opds-v2-collection-unauthorized-auth-doc").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/collections/collection-1")
                .body(Body::empty())
                .expect("opds v2 collection unauthorized request should build"),
        )
        .await
        .expect("opds v2 collection unauthorized request should complete");

    assert_unauthorized_opds_auth_document(response).await;

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_collection_returns_not_found_for_missing_or_out_of_scope_collection() {
    let paths = new_router_fixture("router-opds-v2-collection-scope-not-found").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-user",
        "library-user@example.org",
        "router-contract-library-123",
        &["library-1"],
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v2 collection scope db should open");
    sqlx::query("INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)")
        .bind("collection-2")
        .bind("Collection 2")
        .bind(false)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("library-scoped collection should be inserted");
    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind("collection-2")
    .bind("series-3")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("library-scoped collection series should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library-user@example.org",
        "router-contract-library-123",
    )
    .await;

    let hidden_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/collections/collection-2")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("hidden collection request should build"),
        )
        .await
        .expect("hidden collection request should complete");
    assert_eq!(hidden_response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_collection_uses_kotlin_navigation_shape_and_ordering() {
    let paths = new_router_fixture("router-opds-v2-collection-shape").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-2", "Alpha Display", "library-1").await;
    update_router_series_metadata_titles(&paths, "series-1", "Zeta Display", "Zulu Sort").await;
    update_router_series_metadata_titles(&paths, "series-2", "Alpha Display", "Alpha Sort").await;
    update_router_collection_last_modified(&paths, "collection-1", "2024-01-20 01:02:03").await;
    seed_router_collection_series_entry(&paths, "collection-1", "series-2", 99).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/collections/collection-1?size=1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 collection request should build"),
        )
        .await
        .expect("opds v2 collection request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let metadata = payload
        .get("metadata")
        .expect("collection metadata should be present");
    assert_eq!(
        metadata.get("title").and_then(Value::as_str),
        Some("Collection 1")
    );
    assert_eq!(
        metadata.get("modified").and_then(Value::as_str),
        Some("2024-01-20T01:02:03Z")
    );
    assert_eq!(
        metadata.get("itemsPerPage").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(metadata.get("currentPage").and_then(Value::as_u64), Some(1));
    assert_eq!(
        metadata.get("numberOfItems").and_then(Value::as_u64),
        Some(2)
    );

    let links = payload
        .get("links")
        .and_then(Value::as_array)
        .expect("collection links should be present");
    let self_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("self"))
        .expect("collection self link should be present");
    assert_eq!(
        self_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/collections/collection-1")
    );
    let next_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("next"))
        .expect("collection next link should be present");
    assert_eq!(
        next_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/collections/collection-1?page=1")
    );

    let navigation = payload
        .get("navigation")
        .and_then(Value::as_array)
        .expect("collection navigation should be present");
    assert_eq!(navigation.len(), 1);
    assert_eq!(
        navigation[0].get("title").and_then(Value::as_str),
        Some("Alpha Display")
    );
    assert_eq!(
        navigation[0].get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/series/series-2")
    );
    assert_eq!(
        navigation[0].get("type").and_then(Value::as_str),
        Some("application/opds+json")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_collection_returns_empty_feed_when_series_are_filtered_by_restrictions() {
    let paths = new_router_fixture("router-opds-v2-collection-empty-visible-feed").await;
    seed_router_contract_data(&paths).await;
    update_router_series_age_rating(&paths, "series-1", 21).await;
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
                .uri("/opds/v2/collections/collection-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 collection empty-feed request should build"),
        )
        .await
        .expect("opds v2 collection empty-feed request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("numberOfItems"))
            .and_then(Value::as_u64),
        Some(0)
    );
    assert_eq!(
        payload
            .get("navigation")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(0)
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_collection_keeps_empty_navigation_for_out_of_range_pages() {
    let paths = new_router_fixture("router-opds-v2-collection-out-of-range-empty-page").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-2", "Alpha Display", "library-1").await;
    update_router_series_metadata_titles(&paths, "series-1", "Zeta Display", "Zulu Sort").await;
    update_router_series_metadata_titles(&paths, "series-2", "Alpha Display", "Alpha Sort").await;
    seed_router_collection_series_entry(&paths, "collection-1", "series-2", 99).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/collections/collection-1?page=2&size=1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 collection out-of-range request should build"),
        )
        .await
        .expect("opds v2 collection out-of-range request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("numberOfItems"))
            .and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        payload
            .get("navigation")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(0)
    );
    let links = payload
        .get("links")
        .and_then(Value::as_array)
        .expect("collection out-of-range links should be present");
    assert!(links.iter().any(|link| {
        link.get("rel").and_then(Value::as_str) == Some("previous")
            && link.get("href").and_then(Value::as_str)
                == Some("http://localhost/opds/v2/collections/collection-1?page=1")
    }));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_series_uses_kotlin_shape_tag_filter_and_facets() {
    let paths = new_router_fixture("router-opds-v2-series-shape-and-tag-filter").await;
    seed_router_contract_data(&paths).await;
    update_router_series_catalog_fields(&paths, "series-1", false, "2024-02-20 01:02:03").await;
    seed_catalog_book(
        &paths,
        "book-untagged",
        "series-1",
        "library-1",
        "Book Untagged",
        2,
        "2024-01-16 00:00:00",
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v2 series detail db should open");
    sqlx::query("UPDATE SERIES_METADATA SET SUMMARY = ? WHERE SERIES_ID = ?")
        .bind("Series detail summary")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series summary should be updated for opds series route");
    sqlx::query("UPDATE BOOK_METADATA SET SUMMARY = ? WHERE BOOK_ID = ?")
        .bind("Tagged book summary")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("tagged book summary should be updated for opds series route");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/series/series-1?tag=favorite-tag&size=1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 series request should build"),
        )
        .await
        .expect("opds v2 series request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;

    let metadata = payload
        .get("metadata")
        .expect("series metadata should be present");
    assert_eq!(
        metadata.get("title").and_then(Value::as_str),
        Some("Series 1")
    );
    assert_eq!(
        metadata.get("description").and_then(Value::as_str),
        Some("Series detail summary")
    );
    assert_eq!(
        metadata.get("modified").and_then(Value::as_str),
        Some("2024-02-20T01:02:03Z")
    );
    assert_eq!(
        metadata.get("itemsPerPage").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(metadata.get("currentPage").and_then(Value::as_u64), Some(1));
    assert_eq!(
        metadata.get("numberOfItems").and_then(Value::as_u64),
        Some(1)
    );

    let links = payload
        .get("links")
        .and_then(Value::as_array)
        .expect("series links should be present");
    let self_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("self"))
        .expect("series self link should be present");
    assert_eq!(
        self_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/series/series-1")
    );
    assert!(self_link.get("type").is_none());
    let search_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("search"))
        .expect("series search link should be present");
    assert_eq!(
        search_link.get("templated").and_then(Value::as_bool),
        Some(true)
    );

    let facets = payload
        .get("facets")
        .and_then(Value::as_array)
        .expect("series facets should be present");
    assert_eq!(facets.len(), 1);
    assert_eq!(
        facets[0]
            .get("metadata")
            .and_then(|metadata| metadata.get("title"))
            .and_then(Value::as_str),
        Some("Tag")
    );
    let facet_links = facets[0]
        .get("links")
        .and_then(Value::as_array)
        .expect("series tag facet links should be present");
    assert!(facet_links.iter().any(|link| {
        link.get("title").and_then(Value::as_str) == Some("favorite-tag")
            && link.get("href").and_then(Value::as_str)
                == Some("http://localhost/opds/v2/series/series-1?tag=favorite-tag")
            && link.get("rel").and_then(Value::as_str) == Some("self")
    }));

    let publications = payload
        .get("publications")
        .and_then(Value::as_array)
        .expect("series publications should be present");
    assert_eq!(publications.len(), 1);
    let publication = &publications[0];
    assert_eq!(
        publication.get("@context").and_then(Value::as_str),
        Some("https://readium.org/webpub-manifest/context.jsonld")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("title"))
            .and_then(Value::as_str),
        Some("Book 1")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("description"))
            .and_then(Value::as_str),
        Some("Tagged book summary")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("subject"))
            .and_then(Value::as_array)
            .and_then(|subjects| subjects.first())
            .and_then(Value::as_str),
        Some("favorite-tag")
    );
    let publication_links = publication
        .get("links")
        .and_then(Value::as_array)
        .expect("series publication links should be present");
    assert!(publication_links.iter().any(|link| {
        link.get("rel").and_then(Value::as_str) == Some("http://www.cantook.com/api/progression")
            && link.get("href").and_then(Value::as_str)
                == Some("http://localhost/opds/v2/books/book-1/progression")
    }));
    assert!(publication_links.iter().all(|link| {
        link.get("properties")
            .and_then(|properties| properties.get("authenticate"))
            .and_then(|authenticate| authenticate.get("href"))
            .and_then(Value::as_str)
            == Some("http://localhost/opds/v2/auth")
    }));
    assert!(
        publication
            .get("images")
            .and_then(Value::as_array)
            .is_some_and(|images| {
                images
                    .first()
                    .and_then(|image| image.get("href"))
                    .and_then(Value::as_str)
                    == Some("http://localhost/opds/v2/books/book-1/thumbnail")
            })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_series_facets_keep_tags_from_non_ready_books() {
    let paths = new_router_fixture("router-opds-v2-series-facets-include-non-ready-tags").await;
    seed_router_contract_data(&paths).await;
    seed_catalog_book(
        &paths,
        "book-hidden-tag",
        "series-1",
        "library-1",
        "Book Hidden Tag",
        3,
        "2024-01-17 00:00:00",
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v2 series hidden-tag db should open");
    sqlx::query("UPDATE MEDIA SET STATUS = ? WHERE BOOK_ID = ?")
        .bind("ERROR")
        .bind("book-hidden-tag")
        .execute(&pool)
        .await
        .expect("hidden-tag book media status should be updated");
    sqlx::query("INSERT INTO BOOK_METADATA_TAG (BOOK_ID, TAG) VALUES (?, ?)")
        .bind("book-hidden-tag")
        .bind("hidden-tag")
        .execute(&pool)
        .await
        .expect("hidden-tag book metadata tag should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 series hidden-tag request should build"),
        )
        .await
        .expect("opds v2 series hidden-tag request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;

    let facets = payload
        .get("facets")
        .and_then(Value::as_array)
        .expect("series facets should be present");
    let facet_links = facets[0]
        .get("links")
        .and_then(Value::as_array)
        .expect("series tag facet links should be present");
    assert!(facet_links.iter().any(|link| {
        link.get("title").and_then(Value::as_str) == Some("hidden-tag")
            && link.get("href").and_then(Value::as_str)
                == Some("http://localhost/opds/v2/series/series-1?tag=hidden-tag")
    }));

    let publications = payload
        .get("publications")
        .and_then(Value::as_array)
        .expect("series publications should be present");
    assert!(publications.iter().all(|publication| {
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("title"))
            .and_then(Value::as_str)
            != Some("Book Hidden Tag")
    }));

    cleanup_router_fixture(paths);
}

use super::*;

#[tokio::test]
async fn router_opds_v1_catalog_route_returns_atom_feed() {
    let paths = new_router_fixture("router-opds-v1-catalog-route").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/catalog")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 catalog request should build"),
        )
        .await
        .expect("opds v1 catalog request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(content_type.contains("application/atom+xml"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_catalog_includes_search_and_opds_v2_alternate_links() {
    let paths = new_router_fixture("router-opds-v1-catalog-links").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/catalog")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 catalog links request should build"),
        )
        .await
        .expect("opds v1 catalog links request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains("rel=\"search\"") && body.contains("/opds/v1.2/search"),
        "OPDS v1 catalog must include search link, body={body}"
    );
    assert!(
        body.contains("rel=\"alternate\"")
            && body.contains("type=\"application/opds+json\"")
            && body.contains("/opds/v2/catalog"),
        "OPDS v1 catalog must include OPDS v2 alternate link, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_libraries_unauthorized_includes_basic_challenge() {
    let paths = new_router_fixture("router-opds-v1-libraries-basic-challenge").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/libraries")
                .body(Body::empty())
                .expect("opds v1 libraries unauthorized request should build"),
        )
        .await
        .expect("opds v1 libraries unauthorized request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"Realm\"")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_libraries_preserves_kotlin_dao_iteration_order() {
    let paths = new_router_fixture("router-opds-v1-libraries-dao-order").await;
    seed_router_contract_data(&paths).await;
    update_router_library_name(&paths, "library-1", "Z Library").await;
    seed_router_library(&paths, "library-2", "A Library").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/libraries")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 libraries request should build"),
        )
        .await
        .expect("opds v1 libraries request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    let library_1_pos = body
        .find("/opds/v1.2/libraries/library-1")
        .expect("library-1 entry should be present");
    let library_2_pos = body
        .find("/opds/v1.2/libraries/library-2")
        .expect("library-2 entry should be present");
    assert!(
        library_1_pos < library_2_pos,
        "OPDS v1 libraries should keep Kotlin DAO iteration order instead of name-sorting, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_library_browse_unauthorized_returns_opds_auth_document() {
    let paths = new_router_fixture("router-opds-v2-library-browse-auth-doc").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    for route in [
        "/opds/v2/libraries/browse",
        "/opds/v2/libraries/library-1/browse",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .body(Body::empty())
                    .expect("opds v2 browse unauthorized request should build"),
            )
            .await
            .expect("opds v2 browse unauthorized request should complete");

        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "route: {route}"
        );
        assert_eq!(
            response
                .headers()
                .get(header::WWW_AUTHENTICATE)
                .and_then(|value| value.to_str().ok()),
            Some("Basic realm=\"Realm\""),
            "route: {route}"
        );
        assert!(
            response
                .headers()
                .get(header::LINK)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| {
                    value.contains("/opds/v2/auth")
                        && value.contains("http://opds-spec.org/auth/document")
                        && value.contains("application/opds-authentication+json")
                }),
            "route: {route}"
        );
        assert!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.contains("application/opds-authentication+json")),
            "route: {route}"
        );

        let payload = response_json(response).await;
        assert!(
            payload
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|value| value.contains("/opds/v2/auth")),
            "route: {route}"
        );
    }

    cleanup_router_fixture(paths);
}

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
async fn router_opds_v2_collections_unauthorized_returns_opds_auth_document() {
    let paths = new_router_fixture("router-opds-v2-collections-auth-doc").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

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
                    .body(Body::empty())
                    .expect("opds v2 collections unauthorized request should build"),
            )
            .await
            .expect("opds v2 collections unauthorized request should complete");

        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "route: {route}"
        );
        assert_eq!(
            response
                .headers()
                .get(header::WWW_AUTHENTICATE)
                .and_then(|value| value.to_str().ok()),
            Some("Basic realm=\"Realm\""),
            "route: {route}"
        );
        assert!(
            response
                .headers()
                .get(header::LINK)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| {
                    value.contains("/opds/v2/auth")
                        && value.contains("http://opds-spec.org/auth/document")
                        && value.contains("application/opds-authentication+json")
                }),
            "route: {route}"
        );

        let payload = response_json(response).await;
        assert!(
            payload
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|value| value.contains("/opds/v2/auth")),
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

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"Realm\"")
    );
    assert!(
        response
            .headers()
            .get(header::LINK)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value.contains("/opds/v2/auth")
                    && value.contains("http://opds-spec.org/auth/document")
                    && value.contains("application/opds-authentication+json")
            })
    );
    assert!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.contains("application/opds-authentication+json"))
    );

    let payload = response_json(response).await;
    assert!(
        payload
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|value| value.contains("/opds/v2/auth"))
    );

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

    let missing_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/collections/missing-collection")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing collection request should build"),
        )
        .await
        .expect("missing collection request should complete");
    assert_eq!(missing_response.status(), StatusCode::NOT_FOUND);

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
    assert!(payload.get("publications").is_none());
    assert!(payload.get("groups").is_none());
    assert!(payload.get("facets").is_none());

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
    assert!(self_link.get("type").is_none());
    let start_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("start"))
        .expect("collection start link should be present");
    assert_eq!(
        start_link.get("title").and_then(Value::as_str),
        Some("Home")
    );
    let search_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("search"))
        .expect("collection search link should be present");
    assert_eq!(
        search_link.get("title").and_then(Value::as_str),
        Some("Search")
    );
    assert_eq!(
        search_link.get("templated").and_then(Value::as_bool),
        Some(true)
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
            .and_then(|metadata| metadata.get("title"))
            .and_then(Value::as_str),
        Some("Collection 1")
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("numberOfItems"))
            .and_then(Value::as_u64),
        Some(0)
    );
    assert!(payload.get("navigation").is_none());
    assert!(payload.get("publications").is_none());
    assert!(payload.get("groups").is_none());

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

#[tokio::test]
async fn router_opds_v2_readlists_unauthorized_returns_opds_auth_document() {
    let paths = new_router_fixture("router-opds-v2-readlists-auth-doc").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    for route in [
        "/opds/v2/libraries/readlists",
        "/opds/v2/libraries/library-1/readlists",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .body(Body::empty())
                    .expect("opds v2 readlists unauthorized request should build"),
            )
            .await
            .expect("opds v2 readlists unauthorized request should complete");

        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "route: {route}"
        );
        assert_eq!(
            response
                .headers()
                .get(header::WWW_AUTHENTICATE)
                .and_then(|value| value.to_str().ok()),
            Some("Basic realm=\"Realm\""),
            "route: {route}"
        );
        assert!(
            response
                .headers()
                .get(header::LINK)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| {
                    value.contains("/opds/v2/auth")
                        && value.contains("http://opds-spec.org/auth/document")
                        && value.contains("application/opds-authentication+json")
                }),
            "route: {route}"
        );

        let payload = response_json(response).await;
        assert!(
            payload
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|value| value.contains("/opds/v2/auth")),
            "route: {route}"
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_library_readlists_respect_kotlin_library_scope_statuses() {
    let paths = new_router_fixture("router-opds-v2-library-readlists-scope").await;
    seed_router_contract_data(&paths).await;
    seed_router_library(&paths, "library-2", "Library 2").await;
    seed_router_library_restricted_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "restricted-pass-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "restricted-pass-123",
    )
    .await;

    let forbidden_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/library-2/readlists")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("forbidden readlists request should build"),
        )
        .await
        .expect("forbidden readlists request should complete");
    assert_eq!(forbidden_response.status(), StatusCode::FORBIDDEN);

    let missing_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/missing-library/readlists")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing-library readlists request should build"),
        )
        .await
        .expect("missing-library readlists request should complete");
    assert_eq!(missing_response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_readlists_use_kotlin_grouped_feed_shape() {
    let paths = new_router_fixture("router-opds-v2-readlists-shape").await;
    seed_router_contract_data(&paths).await;
    seed_router_readlist(&paths, "readlist-0", "Alpha ReadList", "book-1").await;
    update_router_library_last_modified(&paths, "library-1", "2024-02-03 04:05:06").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for (route, expected_title, expected_self_href, expected_next_href, expected_modified) in [
        (
            "/opds/v2/libraries/readlists?size=1",
            "All libraries - Read Lists",
            "http://localhost/opds/v2/libraries/readlists",
            "http://localhost/opds/v2/libraries/readlists?page=1",
            None,
        ),
        (
            "/opds/v2/libraries/library-1/readlists?size=1",
            "Library 1 - Read Lists",
            "http://localhost/opds/v2/libraries/library-1/readlists",
            "http://localhost/opds/v2/libraries/library-1/readlists?page=1",
            Some("2024-02-03T04:05:06Z"),
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
                    .expect("opds v2 readlists request should build"),
            )
            .await
            .expect("opds v2 readlists request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
        let payload = response_json(response).await;
        let metadata = payload
            .get("metadata")
            .expect("readlists metadata should be present");
        assert_eq!(
            metadata.get("title").and_then(Value::as_str),
            Some(expected_title),
            "route: {route}"
        );
        let modified = metadata.get("modified").and_then(Value::as_str);
        assert!(
            modified.is_some(),
            "route: {route}, metadata.modified should be present"
        );
        if let Some(expected_modified) = expected_modified {
            assert_eq!(modified, Some(expected_modified), "route: {route}");
        }
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
            .expect("readlists links should be present");
        let self_link = links
            .iter()
            .find(|link| link.get("rel").and_then(Value::as_str) == Some("self"))
            .expect("readlists self link should be present");
        assert_eq!(
            self_link.get("href").and_then(Value::as_str),
            Some(expected_self_href),
            "route: {route}"
        );
        assert!(
            self_link.get("type").is_none(),
            "route: {route}, Kotlin self link omits type"
        );
        let start_link = links
            .iter()
            .find(|link| link.get("rel").and_then(Value::as_str) == Some("start"))
            .expect("readlists start link should be present");
        assert_eq!(
            start_link.get("title").and_then(Value::as_str),
            Some("Home"),
            "route: {route}"
        );
        let search_link = links
            .iter()
            .find(|link| link.get("rel").and_then(Value::as_str) == Some("search"))
            .expect("readlists search link should be present");
        assert_eq!(
            search_link.get("title").and_then(Value::as_str),
            Some("Search"),
            "route: {route}"
        );
        assert_eq!(
            search_link.get("templated").and_then(Value::as_bool),
            Some(true),
            "route: {route}"
        );
        let next_link = links
            .iter()
            .find(|link| link.get("rel").and_then(Value::as_str) == Some("next"))
            .expect("readlists next link should be present");
        assert_eq!(
            next_link.get("href").and_then(Value::as_str),
            Some(expected_next_href),
            "route: {route}"
        );

        let top_level_navigation = payload
            .get("navigation")
            .and_then(Value::as_array)
            .expect("readlists top-level navigation should be present");
        let top_level_titles = top_level_navigation
            .iter()
            .filter_map(|entry| entry.get("title").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert!(
            top_level_titles.contains(&"Recommended")
                && top_level_titles.contains(&"Browse")
                && top_level_titles.contains(&"Read lists"),
            "route: {route}, top-level navigation should stay on subsection links"
        );
        assert!(
            top_level_navigation.iter().all(|entry| {
                entry
                    .get("href")
                    .and_then(Value::as_str)
                    .is_none_or(|href| !href.contains("/opds/v2/readlists/"))
            }),
            "route: {route}, readlist detail links should not live in top-level navigation"
        );

        let groups = payload
            .get("groups")
            .and_then(Value::as_array)
            .expect("readlists groups should be present");
        let readlists_group = groups
            .iter()
            .find(|group| {
                group
                    .get("metadata")
                    .and_then(|metadata| metadata.get("title"))
                    .and_then(Value::as_str)
                    == Some("Read Lists")
            })
            .expect("readlists group should be present");
        let group_navigation = readlists_group
            .get("navigation")
            .and_then(Value::as_array)
            .expect("readlists group navigation should be present");
        assert_eq!(group_navigation.len(), 1, "route: {route}");
        assert_eq!(
            group_navigation[0].get("title").and_then(Value::as_str),
            Some("Alpha ReadList"),
            "route: {route}, paged readlist entry should keep name sort"
        );
        assert!(
            group_navigation[0]
                .get("href")
                .and_then(Value::as_str)
                .is_some_and(|href| href.contains("/opds/v2/readlists/readlist-0")),
            "route: {route}"
        );
    }

    cleanup_router_fixture(paths);
}

async fn clear_router_collections_and_readlists(paths: &RuntimeDbPaths) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds collection/readlist cleanup db should open");

    for sql in [
        "DELETE FROM COLLECTION_SERIES",
        "DELETE FROM COLLECTION",
        "DELETE FROM READLIST_BOOK",
        "DELETE FROM READLIST",
    ] {
        sqlx::query(sql)
            .execute(&pool)
            .await
            .expect("collections/readlists should be deleted");
    }

    pool.close().await;
}

async fn seed_router_collection_series_entry(
    paths: &RuntimeDbPaths,
    collection_id: &str,
    series_id: &str,
    number: i64,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds collection-series seed db should open");

    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind(collection_id)
    .bind(series_id)
    .bind(number)
    .execute(&pool)
    .await
    .expect("collection series entry should be inserted");

    pool.close().await;
}

async fn seed_router_readlist(
    paths: &RuntimeDbPaths,
    readlist_id: &str,
    name: &str,
    book_id: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds readlist seed db should open");

    sqlx::query("INSERT INTO READLIST (ID, NAME, BOOK_COUNT) VALUES (?, ?, ?)")
        .bind(readlist_id)
        .bind(name)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("readlist row should be inserted");

    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind(readlist_id)
        .bind(book_id)
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("readlist book row should be inserted");

    pool.close().await;
}

async fn seed_router_readlist_book_entry(
    paths: &RuntimeDbPaths,
    readlist_id: &str,
    book_id: &str,
    number: i64,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds readlist-book seed db should open");

    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind(readlist_id)
        .bind(book_id)
        .bind(number)
        .execute(&pool)
        .await
        .expect("readlist book entry should be inserted");

    pool.close().await;
}

#[tokio::test]
async fn router_opds_v2_catalog_returns_feed_when_authenticated() {
    let paths = new_router_fixture("router-opds-v2-catalog-authenticated").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/catalog")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 catalog request should build"),
        )
        .await
        .expect("opds v2 catalog request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert!(payload.get("metadata").is_some());
    assert!(
        payload
            .get("metadata")
            .and_then(|value| value.get("modified"))
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty())
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_catalog_uses_kotlin_top_level_links_when_authenticated() {
    let paths = new_router_fixture("router-opds-v2-catalog-self-link").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/catalog")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 catalog self link request should build"),
        )
        .await
        .expect("opds v2 catalog self link request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let links = payload
        .get("links")
        .and_then(Value::as_array)
        .expect("catalog links should be present");

    let self_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("self"))
        .expect("catalog self link should be present");
    assert_eq!(
        self_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/libraries")
    );
    assert!(
        self_link.get("type").is_none(),
        "catalog self link should omit type like Kotlin, link={self_link}"
    );

    let start_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("start"))
        .expect("catalog start link should be present");
    assert_eq!(
        start_link.get("title").and_then(Value::as_str),
        Some("Home")
    );
    assert_eq!(
        start_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/catalog")
    );
    assert_eq!(
        start_link.get("type").and_then(Value::as_str),
        Some("application/opds+json")
    );

    let search_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("search"))
        .expect("catalog search link should be present");
    assert_eq!(
        search_link.get("title").and_then(Value::as_str),
        Some("Search")
    );
    assert_eq!(
        search_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/search{?query}")
    );
    assert_eq!(
        search_link.get("type").and_then(Value::as_str),
        Some("application/opds+json")
    );
    assert_eq!(
        search_link.get("templated").and_then(Value::as_bool),
        Some(true)
    );

    let recommended_href = payload
        .get("navigation")
        .and_then(Value::as_array)
        .and_then(|entries| {
            entries.iter().find_map(|entry| {
                (entry.get("title").and_then(Value::as_str) == Some("Recommended"))
                    .then(|| entry.get("href").and_then(Value::as_str))
                    .flatten()
            })
        });
    assert_eq!(recommended_href, Some("http://localhost/opds/v2/libraries"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_libraries_unauthorized_returns_opds_auth_document() {
    let paths = new_router_fixture("router-opds-v2-libraries-unauthorized-auth-doc").await;
    seed_router_contract_data(&paths).await;
    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries")
                .body(Body::empty())
                .expect("opds v2 libraries unauthorized request should build"),
        )
        .await
        .expect("opds v2 libraries unauthorized request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"Realm\"")
    );
    assert!(
        response
            .headers()
            .get(header::LINK)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value.contains("/opds/v2/auth")
                    && value.contains("http://opds-spec.org/auth/document")
                    && value.contains("application/opds-authentication+json")
            })
    );
    assert!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.contains("application/opds-authentication+json"))
    );

    let payload = response_json(response).await;
    assert!(
        payload
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|value| value.contains("/opds/v2/auth"))
    );
    assert_eq!(payload.get("title").and_then(Value::as_str), Some("Komga"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_libraries_uses_kotlin_top_level_links_when_authenticated() {
    let paths = new_router_fixture("router-opds-v2-libraries-top-level-links").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 libraries request should build"),
        )
        .await
        .expect("opds v2 libraries request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("title"))
            .and_then(Value::as_str),
        Some("All libraries - Recommended")
    );

    let links = payload
        .get("links")
        .and_then(Value::as_array)
        .expect("libraries links should be present");

    let self_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("self"))
        .expect("libraries self link should be present");
    assert_eq!(
        self_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/libraries")
    );
    assert!(
        self_link.get("type").is_none(),
        "libraries self link should omit type like Kotlin, link={self_link}"
    );

    let start_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("start"))
        .expect("libraries start link should be present");
    assert_eq!(
        start_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/catalog")
    );
    assert_eq!(
        start_link.get("title").and_then(Value::as_str),
        Some("Home")
    );

    let recommended_link = payload
        .get("navigation")
        .and_then(Value::as_array)
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.get("title").and_then(Value::as_str) == Some("Recommended"))
        })
        .expect("libraries recommended navigation should be present");
    assert_eq!(
        recommended_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/libraries")
    );

    let browse_link = payload
        .get("navigation")
        .and_then(Value::as_array)
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.get("title").and_then(Value::as_str) == Some("Browse"))
        })
        .expect("libraries browse navigation should be present");
    assert_eq!(
        browse_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/libraries/browse")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_catalog_groups_include_pagination_metadata() {
    let paths = new_router_fixture("router-opds-v2-catalog-group-metadata").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/catalog")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 catalog group metadata request should build"),
        )
        .await
        .expect("opds v2 catalog group metadata request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let groups = payload
        .get("groups")
        .and_then(Value::as_array)
        .expect("catalog groups should be present");
    assert!(!groups.is_empty(), "catalog groups should not be empty");

    for group in groups {
        let title = group
            .get("metadata")
            .and_then(|metadata| metadata.get("title"))
            .and_then(Value::as_str)
            .expect("group metadata title should be present");
        let metadata = group
            .get("metadata")
            .expect("catalog group metadata should be present");
        assert!(
            metadata.get("title").and_then(Value::as_str).is_some(),
            "group metadata should keep title, group={group}"
        );

        if title == "Libraries" {
            assert!(
                metadata.get("itemsPerPage").is_none()
                    && metadata.get("currentPage").is_none()
                    && metadata.get("numberOfItems").is_none(),
                "libraries group should omit pagination metadata like Kotlin, group={group}"
            );

            let link = group
                .get("links")
                .and_then(Value::as_array)
                .and_then(|links| links.first())
                .expect("libraries group self link should be present");
            assert_eq!(
                link.get("href").and_then(Value::as_str),
                Some("http://localhost/opds/v2/libraries")
            );
            assert!(
                link.get("title").is_none() && link.get("type").is_none(),
                "libraries group self link should omit title/type, link={link}"
            );
        } else {
            assert!(
                metadata
                    .get("itemsPerPage")
                    .and_then(Value::as_u64)
                    .is_some_and(|value| value > 0),
                "group metadata should include positive itemsPerPage, group={group}"
            );
            assert_eq!(
                metadata.get("currentPage").and_then(Value::as_u64),
                Some(1),
                "recommended groups should expose first-page metadata, group={group}"
            );
            assert!(
                metadata
                    .get("numberOfItems")
                    .and_then(Value::as_u64)
                    .is_some(),
                "group metadata should include numberOfItems, group={group}"
            );

            let link = group
                .get("links")
                .and_then(Value::as_array)
                .and_then(|links| links.first())
                .expect("recommended group self link should be present");
            assert_eq!(link.get("title").and_then(Value::as_str), Some(title));
            assert_eq!(
                link.get("type").and_then(Value::as_str),
                Some("application/opds+json")
            );
        }
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_catalog_latest_books_publication_uses_webpub_like_shape() {
    let paths = new_router_fixture("router-opds-v2-catalog-publication-shape").await;
    seed_router_contract_data(&paths).await;
    update_router_book_isbn(&paths, "book-1", "9781234567890").await;
    update_router_book_number_metadata(&paths, "book-1", "Special", 10.0).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/catalog")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 catalog publication shape request should build"),
        )
        .await
        .expect("opds v2 catalog publication shape request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let latest_books_group = payload
        .get("groups")
        .and_then(Value::as_array)
        .and_then(|groups| {
            groups.iter().find(|group| {
                group
                    .get("metadata")
                    .and_then(|metadata| metadata.get("title"))
                    .and_then(Value::as_str)
                    == Some("Latest Books")
            })
        })
        .expect("latest books group should be present");
    let publication = latest_books_group
        .get("publications")
        .and_then(Value::as_array)
        .and_then(|publications| publications.first())
        .expect("latest books group should include a publication");

    assert_eq!(
        publication.get("@context").and_then(Value::as_str),
        Some("https://readium.org/webpub-manifest/context.jsonld")
    );

    let links = publication
        .get("links")
        .and_then(Value::as_array)
        .expect("publication links should be present");
    let self_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("self"))
        .expect("publication self link should be present");
    assert_eq!(
        self_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/books/book-1/manifest")
    );
    assert_eq!(
        self_link.get("type").and_then(Value::as_str),
        Some("application/webpub+json")
    );
    assert_eq!(
        self_link
            .get("properties")
            .and_then(|properties| properties.get("authenticate"))
            .and_then(|authenticate| authenticate.get("href"))
            .and_then(Value::as_str),
        Some("http://localhost/opds/v2/auth")
    );

    let metadata = publication
        .get("metadata")
        .expect("publication metadata should be present");
    assert_eq!(
        metadata.get("identifier").and_then(Value::as_str),
        Some("urn:isbn:9781234567890")
    );
    assert_eq!(
        metadata.get("numberOfPages").and_then(Value::as_u64),
        Some(10)
    );
    assert_eq!(
        metadata.get("published").and_then(Value::as_str),
        Some("2024-01-15")
    );
    assert_eq!(
        metadata
            .get("subject")
            .and_then(Value::as_array)
            .and_then(|subjects| subjects.first())
            .and_then(Value::as_str),
        Some("favorite-tag")
    );
    assert_eq!(
        metadata
            .get("contributor")
            .and_then(Value::as_array)
            .and_then(|contributors| contributors.first())
            .and_then(Value::as_str),
        Some("Jane Writer")
    );
    assert_eq!(
        metadata
            .get("belongsTo")
            .and_then(|belongs_to| belongs_to.get("series"))
            .and_then(Value::as_array)
            .and_then(|series| series.first())
            .and_then(|series| series.get("name"))
            .and_then(Value::as_str),
        Some("Series 1")
    );
    assert_eq!(
        metadata
            .get("belongsTo")
            .and_then(|belongs_to| belongs_to.get("series"))
            .and_then(Value::as_array)
            .and_then(|series| series.first())
            .and_then(|series| series.get("position"))
            .and_then(Value::as_f64),
        Some(10.0)
    );
    assert_eq!(
        metadata
            .get("belongsTo")
            .and_then(|belongs_to| belongs_to.get("series"))
            .and_then(Value::as_array)
            .and_then(|series| series.first())
            .and_then(|series| series.get("links"))
            .and_then(Value::as_array)
            .and_then(|links| links.first())
            .and_then(|link| link.get("href"))
            .and_then(Value::as_str),
        Some("http://localhost/opds/v2/series/series-1")
    );

    let progression_link = links
        .iter()
        .find(|link| {
            link.get("rel").and_then(Value::as_str)
                == Some("http://www.cantook.com/api/progression")
        })
        .expect("publication progression link should be present");
    assert_eq!(
        progression_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/books/book-1/progression")
    );
    assert_eq!(
        progression_link.get("type").and_then(Value::as_str),
        Some("application/vnd.readium.progression+json")
    );

    let images = publication
        .get("images")
        .and_then(Value::as_array)
        .expect("publication images should be present");
    assert_eq!(images.len(), 1);
    assert_eq!(
        images[0].get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/books/book-1/thumbnail")
    );
    assert_eq!(
        images[0].get("type").and_then(Value::as_str),
        Some("image/jpeg")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_catalog_latest_books_filters_before_limit_for_library_restricted_user() {
    let paths = new_router_fixture("router-opds-v2-catalog-latest-books-prefiltered").await;
    seed_router_contract_data(&paths).await;
    seed_router_library(&paths, "library-2", "Library 2").await;
    seed_router_custom_series(&paths, "series-2", "Hidden Series", "library-2").await;
    update_router_book_created_date(&paths, "book-1", "2024-01-01 00:00:00").await;

    for (index, created_date) in [
        "2024-02-05 00:00:00",
        "2024-02-04 00:00:00",
        "2024-02-03 00:00:00",
        "2024-02-02 00:00:00",
        "2024-02-01 00:00:00",
    ]
    .into_iter()
    .enumerate()
    {
        let book_id = format!("hidden-book-{}", index + 1);
        seed_catalog_book(
            &paths,
            &book_id,
            "series-2",
            "library-2",
            &format!("Hidden Book {}", index + 1),
            (index + 2) as i64,
            created_date,
        )
        .await;
    }

    seed_router_library_restricted_user(
        &paths,
        "library-user",
        "library-user@example.org",
        "library-user-pass-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library-user@example.org",
        "library-user-pass-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/catalog")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 catalog library restricted request should build"),
        )
        .await
        .expect("opds v2 catalog library restricted request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let latest_books_group = payload
        .get("groups")
        .and_then(Value::as_array)
        .and_then(|groups| {
            groups.iter().find(|group| {
                group
                    .get("metadata")
                    .and_then(|metadata| metadata.get("title"))
                    .and_then(Value::as_str)
                    == Some("Latest Books")
            })
        })
        .expect("latest books group should be present for restricted user");

    assert_eq!(
        latest_books_group
            .get("metadata")
            .and_then(|metadata| metadata.get("numberOfItems"))
            .and_then(Value::as_u64),
        Some(1)
    );
    let publications = latest_books_group
        .get("publications")
        .and_then(Value::as_array)
        .expect("latest books publications should be present");
    assert_eq!(publications.len(), 1);
    assert_eq!(
        publications[0]
            .get("metadata")
            .and_then(|metadata| metadata.get("title"))
            .and_then(Value::as_str),
        Some("Book 1")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_catalog_latest_series_skips_one_shots_before_limit() {
    let paths = new_router_fixture("router-opds-v2-catalog-latest-series-skips-oneshots").await;
    seed_router_contract_data(&paths).await;
    update_router_series_catalog_fields(&paths, "series-1", false, "2024-01-01 00:00:00").await;

    for index in 0..5 {
        let series_id = format!("oneshot-series-{}", index + 1);
        let title = format!("OneShot {}", index + 1);
        seed_router_custom_series(&paths, &series_id, &title, "library-1").await;
        update_router_series_catalog_fields(
            &paths,
            &series_id,
            true,
            format!("2024-02-0{} 00:00:00", index + 1).as_str(),
        )
        .await;
    }

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/catalog")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 catalog latest series request should build"),
        )
        .await
        .expect("opds v2 catalog latest series request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let latest_series_group = payload
        .get("groups")
        .and_then(Value::as_array)
        .and_then(|groups| {
            groups.iter().find(|group| {
                group
                    .get("metadata")
                    .and_then(|metadata| metadata.get("title"))
                    .and_then(Value::as_str)
                    == Some("Latest Series")
            })
        })
        .expect("latest series group should be present");

    assert_eq!(
        latest_series_group
            .get("metadata")
            .and_then(|metadata| metadata.get("numberOfItems"))
            .and_then(Value::as_u64),
        Some(1)
    );
    let navigation = latest_series_group
        .get("navigation")
        .and_then(Value::as_array)
        .expect("latest series navigation should be present");
    assert_eq!(navigation.len(), 1);
    assert_eq!(
        navigation[0].get("title").and_then(Value::as_str),
        Some("Series 1")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_latest_series_unauthorized_returns_opds_auth_document() {
    let paths = new_router_fixture("router-opds-v2-latest-series-unauthorized-auth-doc").await;
    seed_router_contract_data(&paths).await;
    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    for route in [
        "/opds/v2/libraries/series/latest",
        "/opds/v2/libraries/library-1/series/latest",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .body(Body::empty())
                    .expect("opds v2 latest series unauthorized request should build"),
            )
            .await
            .expect("opds v2 latest series unauthorized request should complete");

        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "route: {route}"
        );
        assert_eq!(
            response
                .headers()
                .get(header::WWW_AUTHENTICATE)
                .and_then(|value| value.to_str().ok()),
            Some("Basic realm=\"Realm\""),
            "route: {route}"
        );
        assert!(
            response
                .headers()
                .get(header::LINK)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| {
                    value.contains("/opds/v2/auth")
                        && value.contains("http://opds-spec.org/auth/document")
                        && value.contains("application/opds-authentication+json")
                }),
            "route: {route}"
        );
        assert!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.contains("application/opds-authentication+json")),
            "route: {route}"
        );

        let payload = response_json(response).await;
        assert!(
            payload
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|value| value.contains("/opds/v2/auth")),
            "route: {route}"
        );
        assert_eq!(
            payload.get("title").and_then(Value::as_str),
            Some("Komga"),
            "route: {route}"
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_latest_series_uses_kotlin_self_links() {
    let paths = new_router_fixture("router-opds-v2-latest-series-self-link").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for (route, expected_title, expected_self_href) in [
        (
            "/opds/v2/libraries/series/latest",
            "All libraries - Latest Series",
            "http://localhost/opds/v2/libraries/books/latest",
        ),
        (
            "/opds/v2/libraries/library-1/series/latest",
            "Library 1 - Latest Series",
            "http://localhost/opds/v2/libraries/library-1/books/latest",
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
                    .expect("opds v2 latest series request should build"),
            )
            .await
            .expect("opds v2 latest series request should complete");

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
            .expect("latest series self link should be present");
        assert_eq!(
            self_link.get("href").and_then(Value::as_str),
            Some(expected_self_href),
            "route: {route}"
        );
        assert!(
            self_link.get("type").is_none(),
            "route: {route}, self link should omit type like Kotlin"
        );

        let start_link = payload
            .get("links")
            .and_then(Value::as_array)
            .and_then(|links| {
                links
                    .iter()
                    .find(|link| link.get("rel").and_then(Value::as_str) == Some("start"))
            })
            .expect("latest series start link should be present");
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
            .expect("latest series search link should be present");
        assert_eq!(
            search_link.get("title").and_then(Value::as_str),
            Some("Search"),
            "route: {route}"
        );

        let navigation = payload
            .get("navigation")
            .and_then(Value::as_array)
            .expect("latest series navigation should be present");
        assert!(!navigation.is_empty(), "route: {route}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_latest_series_includes_page_metadata_and_filters_one_shots_before_paging() {
    let paths = new_router_fixture("router-opds-v2-latest-series-page-metadata").await;
    seed_router_contract_data(&paths).await;
    update_router_series_catalog_fields(&paths, "series-1", false, "2024-02-01 00:00:00").await;

    for (index, last_modified) in [
        "2024-02-05 00:00:00",
        "2024-02-04 00:00:00",
        "2024-02-03 00:00:00",
        "2024-02-02 00:00:00",
    ]
    .into_iter()
    .enumerate()
    {
        let series_id = format!("latest-series-{}", index + 2);
        let title = format!("Series {}", index + 2);
        seed_router_custom_series(&paths, &series_id, &title, "library-1").await;
        update_router_series_catalog_fields(&paths, &series_id, false, last_modified).await;
    }

    for (index, last_modified) in ["2024-03-02 00:00:00", "2024-03-01 00:00:00"]
        .into_iter()
        .enumerate()
    {
        let series_id = format!("oneshot-latest-series-{}", index + 1);
        let title = format!("OneShot Latest {}", index + 1);
        seed_router_custom_series(&paths, &series_id, &title, "library-1").await;
        update_router_series_catalog_fields(&paths, &series_id, true, last_modified).await;
    }

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/library-1/series/latest?page=1&size=2")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 latest series paged request should build"),
        )
        .await
        .expect("opds v2 latest series paged request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let metadata = payload
        .get("metadata")
        .expect("latest series metadata should be present");
    assert_eq!(
        metadata.get("title").and_then(Value::as_str),
        Some("Library 1 - Latest Series")
    );
    assert!(
        metadata.get("modified").and_then(Value::as_str).is_some(),
        "latest series metadata should expose modified timestamp"
    );
    assert_eq!(
        metadata.get("itemsPerPage").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(metadata.get("currentPage").and_then(Value::as_u64), Some(2));
    assert_eq!(
        metadata.get("numberOfItems").and_then(Value::as_u64),
        Some(5)
    );

    let links = payload
        .get("links")
        .and_then(Value::as_array)
        .expect("latest series links should be present");
    let previous_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("previous"))
        .expect("latest series previous link should be present");
    assert_eq!(
        previous_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/libraries/library-1/books/latest?page=0")
    );
    let next_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("next"))
        .expect("latest series next link should be present");
    assert_eq!(
        next_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/libraries/library-1/books/latest?page=2")
    );

    let navigation = payload
        .get("navigation")
        .and_then(Value::as_array)
        .expect("latest series navigation should be present");
    assert_eq!(navigation.len(), 2);
    assert_eq!(
        navigation[0].get("title").and_then(Value::as_str),
        Some("Series 4")
    );
    assert_eq!(
        navigation[1].get("title").and_then(Value::as_str),
        Some("Series 5")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_latest_series_hides_age_restricted_series_for_exclude_user() {
    let paths = new_router_fixture("router-opds-v2-latest-series-age-restricted").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted-series@example.org",
        "router-contract-restricted-series-123",
        12,
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted-series@example.org",
        "router-contract-restricted-series-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/library-1/series/latest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 latest series restricted request should build"),
        )
        .await
        .expect("opds v2 latest series restricted request should complete");

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
async fn router_opds_v2_book_file_unauthorized_returns_opds_auth_document() {
    let paths = new_router_fixture("router-opds-v2-book-file-unauthorized-auth-doc").await;
    seed_router_contract_data(&paths).await;
    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    for route in [
        "/opds/v2/books/book-1/file",
        "/opds/v2/books/book-1/file/book-1.epub",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .body(Body::empty())
                    .expect("opds v2 book file unauthorized request should build"),
            )
            .await
            .expect("opds v2 book file unauthorized request should complete");

        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "route: {route}"
        );
        assert_eq!(
            response
                .headers()
                .get(header::WWW_AUTHENTICATE)
                .and_then(|value| value.to_str().ok()),
            Some("Basic realm=\"Realm\""),
            "route: {route}"
        );
        assert!(
            response
                .headers()
                .get(header::LINK)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| {
                    value.contains("/opds/v2/auth")
                        && value.contains("http://opds-spec.org/auth/document")
                        && value.contains("application/opds-authentication+json")
                }),
            "route: {route}"
        );
        assert!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.contains("application/opds-authentication+json")),
            "route: {route}"
        );

        let payload = response_json(response).await;
        assert!(
            payload
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|value| value.contains("/opds/v2/auth")),
            "route: {route}"
        );
        assert_eq!(
            payload.get("title").and_then(Value::as_str),
            Some("Komga"),
            "route: {route}"
        );
        assert_eq!(
            payload
                .get("authentication")
                .and_then(Value::as_array)
                .and_then(|entries| entries.first())
                .and_then(|entry| entry.get("labels"))
                .and_then(|labels| labels.get("login"))
                .and_then(Value::as_str),
            Some("Email"),
            "route: {route}"
        );
        assert_eq!(
            payload
                .get("authentication")
                .and_then(Value::as_array)
                .and_then(|entries| entries.first())
                .and_then(|entry| entry.get("labels"))
                .and_then(|labels| labels.get("password"))
                .and_then(Value::as_str),
            Some("Password"),
            "route: {route}"
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_book_page_unauthorized_returns_opds_auth_document() {
    let paths = new_router_fixture("router-opds-v2-book-page-unauthorized-auth-doc").await;
    seed_router_contract_data(&paths).await;
    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/books/book-1/pages/1")
                .body(Body::empty())
                .expect("opds v2 book page unauthorized request should build"),
        )
        .await
        .expect("opds v2 book page unauthorized request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"Realm\"")
    );
    assert!(
        response
            .headers()
            .get(header::LINK)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value.contains("/opds/v2/auth")
                    && value.contains("http://opds-spec.org/auth/document")
                    && value.contains("application/opds-authentication+json")
            })
    );
    assert!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.contains("application/opds-authentication+json"))
    );

    let payload = response_json(response).await;
    assert!(
        payload
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|value| value.contains("/opds/v2/auth"))
    );
    assert_eq!(payload.get("title").and_then(Value::as_str), Some("Komga"));
    assert_eq!(
        payload
            .get("authentication")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("labels"))
            .and_then(|labels| labels.get("login"))
            .and_then(Value::as_str),
        Some("Email")
    );
    assert_eq!(
        payload
            .get("authentication")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("labels"))
            .and_then(|labels| labels.get("password"))
            .and_then(Value::as_str),
        Some("Password")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_book_page_raw_unauthorized_returns_opds_auth_document() {
    let paths = new_router_fixture("router-opds-v2-book-page-raw-unauthorized-auth-doc").await;
    seed_router_contract_data(&paths).await;
    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/books/book-1/pages/1/raw")
                .body(Body::empty())
                .expect("opds v2 book raw page unauthorized request should build"),
        )
        .await
        .expect("opds v2 book raw page unauthorized request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"Realm\"")
    );
    assert!(
        response
            .headers()
            .get(header::LINK)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value.contains("/opds/v2/auth")
                    && value.contains("http://opds-spec.org/auth/document")
                    && value.contains("application/opds-authentication+json")
            })
    );
    assert!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.contains("application/opds-authentication+json"))
    );

    let payload = response_json(response).await;
    assert!(
        payload
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|value| value.contains("/opds/v2/auth"))
    );
    assert_eq!(payload.get("title").and_then(Value::as_str), Some("Komga"));
    assert_eq!(
        payload
            .get("authentication")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("labels"))
            .and_then(|labels| labels.get("login"))
            .and_then(Value::as_str),
        Some("Email")
    );
    assert_eq!(
        payload
            .get("authentication")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("labels"))
            .and_then(|labels| labels.get("password"))
            .and_then(Value::as_str),
        Some("Password")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_library_recommended_unauthorized_returns_opds_auth_document() {
    let paths =
        new_router_fixture("router-opds-v2-library-recommended-unauthorized-auth-doc").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/library-1")
                .body(Body::empty())
                .expect("opds v2 library recommended unauthorized request should build"),
        )
        .await
        .expect("opds v2 library recommended unauthorized request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"Realm\"")
    );
    assert!(
        response
            .headers()
            .get(header::LINK)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value.contains("/opds/v2/auth")
                    && value.contains("http://opds-spec.org/auth/document")
                    && value.contains("application/opds-authentication+json")
            })
    );

    let payload = response_json(response).await;
    assert!(
        payload
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|value| value.contains("/opds/v2/auth"))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_library_recommended_uses_kotlin_shape() {
    let paths = new_router_fixture("router-opds-v2-library-recommended-shape").await;
    seed_router_contract_data(&paths).await;
    update_router_library_last_modified(&paths, "library-1", "2024-02-03 04:05:06").await;
    update_router_book_isbn(&paths, "book-1", "9781234567890").await;
    update_router_book_number_metadata(&paths, "book-1", "Special", 10.0).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 library recommended request should build"),
        )
        .await
        .expect("opds v2 library recommended request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;

    let metadata = payload
        .get("metadata")
        .expect("library recommended metadata should be present");
    assert_eq!(
        metadata.get("title").and_then(Value::as_str),
        Some("Library 1 - Recommended")
    );
    assert_eq!(
        metadata.get("modified").and_then(Value::as_str),
        Some("2024-02-03T04:05:06Z")
    );

    let links = payload
        .get("links")
        .and_then(Value::as_array)
        .expect("library recommended links should be present");
    let self_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("self"))
        .expect("library recommended self link should be present");
    assert_eq!(
        self_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/libraries/library-1")
    );
    assert!(self_link.get("type").is_none());

    let start_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("start"))
        .expect("library recommended start link should be present");
    assert_eq!(
        start_link.get("title").and_then(Value::as_str),
        Some("Home")
    );

    let search_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("search"))
        .expect("library recommended search link should be present");
    assert_eq!(
        search_link.get("title").and_then(Value::as_str),
        Some("Search")
    );
    assert_eq!(
        search_link.get("templated").and_then(Value::as_bool),
        Some(true)
    );

    let navigation = payload
        .get("navigation")
        .and_then(Value::as_array)
        .expect("library recommended navigation should be present");
    let navigation_titles = navigation
        .iter()
        .filter_map(|entry| entry.get("title").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert_eq!(
        navigation_titles,
        vec!["Recommended", "Browse", "Collections", "Read lists"]
    );

    let groups = payload
        .get("groups")
        .and_then(Value::as_array)
        .expect("library recommended groups should be present");
    assert!(
        groups.iter().all(|group| {
            group
                .get("metadata")
                .and_then(|metadata| metadata.get("title"))
                .and_then(Value::as_str)
                != Some("Libraries")
        }),
        "library route should not include all-libraries group"
    );
    for group in groups {
        let group_metadata = group
            .get("metadata")
            .expect("library recommended group metadata should be present");
        assert!(
            group_metadata
                .get("itemsPerPage")
                .and_then(Value::as_u64)
                .is_some_and(|value| value > 0),
            "recommended group should expose pagination metadata, group={group}"
        );
        assert_eq!(
            group_metadata.get("currentPage").and_then(Value::as_u64),
            Some(1),
            "recommended group should expose first page, group={group}"
        );
        assert!(
            group_metadata
                .get("numberOfItems")
                .and_then(Value::as_u64)
                .is_some(),
            "recommended group should expose total items, group={group}"
        );
    }

    let latest_books_group = groups
        .iter()
        .find(|group| {
            group
                .get("metadata")
                .and_then(|metadata| metadata.get("title"))
                .and_then(Value::as_str)
                == Some("Latest Books")
        })
        .expect("latest books group should be present");
    let publication = latest_books_group
        .get("publications")
        .and_then(Value::as_array)
        .and_then(|publications| publications.first())
        .expect("latest books group should include a publication");
    assert_eq!(
        publication.get("@context").and_then(Value::as_str),
        Some("https://readium.org/webpub-manifest/context.jsonld")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("identifier"))
            .and_then(Value::as_str),
        Some("urn:isbn:9781234567890")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("belongsTo"))
            .and_then(|belongs_to| belongs_to.get("series"))
            .and_then(Value::as_array)
            .and_then(|series| series.first())
            .and_then(|series| series.get("position"))
            .and_then(Value::as_f64),
        Some(10.0)
    );
    let publication_links = publication
        .get("links")
        .and_then(Value::as_array)
        .expect("publication links should be present");
    assert!(
        publication_links.iter().any(|link| {
            link.get("rel").and_then(Value::as_str)
                == Some("http://www.cantook.com/api/progression")
        }),
        "library recommended publication should expose progression link"
    );
    assert!(
        publication
            .get("images")
            .and_then(Value::as_array)
            .is_some_and(|images| !images.is_empty()),
        "library recommended publication should expose images"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_on_deck_unauthorized_returns_opds_auth_document() {
    let paths = new_router_fixture("router-opds-v2-on-deck-unauthorized-auth-doc").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/on-deck")
                .body(Body::empty())
                .expect("opds v2 on-deck unauthorized request should build"),
        )
        .await
        .expect("opds v2 on-deck unauthorized request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"Realm\"")
    );
    assert!(
        response
            .headers()
            .get(header::LINK)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value.contains("/opds/v2/auth")
                    && value.contains("http://opds-spec.org/auth/document")
                    && value.contains("application/opds-authentication+json")
            })
    );

    let payload = response_json(response).await;
    assert!(
        payload
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|value| value.contains("/opds/v2/auth"))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_on_deck_uses_kotlin_shape_and_visible_results() {
    let paths = new_router_fixture("router-opds-v2-on-deck-shape").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(&paths, "book-pdf", "series-1", "book-pdf.pdf", "Book PDF").await;
    seed_router_library(&paths, "library-2", "Library 2").await;
    seed_router_custom_series(&paths, "series-2", "Series 2", "library-2").await;
    seed_catalog_book(
        &paths,
        "book-library-2-read",
        "series-2",
        "library-2",
        "Book Library 2 Read",
        1,
        "2024-03-01 00:00:00",
    )
    .await;
    seed_catalog_book(
        &paths,
        "book-library-2-on-deck",
        "series-2",
        "library-2",
        "Book Library 2 On Deck",
        2,
        "2024-03-01 00:00:00",
    )
    .await;
    update_router_book_isbn(&paths, "book-pdf", "9780000000002").await;
    update_router_book_isbn(&paths, "book-library-2-on-deck", "9780000000006").await;
    seed_router_read_progress_entry(
        &paths,
        "book-1",
        "admin-user",
        10,
        true,
        "2024-03-01 00:00:00",
    )
    .await;
    seed_router_read_progress_entry(
        &paths,
        "book-library-2-read",
        "admin-user",
        10,
        true,
        "2024-03-02 00:00:00",
    )
    .await;
    upsert_router_series_read_progress(
        &paths,
        "series-1",
        "admin-user",
        1,
        0,
        "2024-03-01 00:00:00",
    )
    .await;
    upsert_router_series_read_progress(
        &paths,
        "series-2",
        "admin-user",
        1,
        0,
        "2024-03-02 00:00:00",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/on-deck?size=1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 on-deck request should build"),
        )
        .await
        .expect("opds v2 on-deck request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert!(payload.get("navigation").is_none());
    assert!(payload.get("groups").is_none());

    let metadata = payload
        .get("metadata")
        .expect("on-deck metadata should be present");
    assert_eq!(
        metadata.get("title").and_then(Value::as_str),
        Some("All libraries - On Deck")
    );
    assert!(
        metadata.get("modified").and_then(Value::as_str).is_some(),
        "on-deck metadata.modified should be present"
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
        .expect("on-deck links should be present");
    let self_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("self"))
        .expect("on-deck self link should be present");
    assert_eq!(
        self_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/libraries/on-deck")
    );
    assert!(self_link.get("type").is_none());
    let start_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("start"))
        .expect("on-deck start link should be present");
    assert_eq!(
        start_link.get("title").and_then(Value::as_str),
        Some("Home")
    );
    let search_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("search"))
        .expect("on-deck search link should be present");
    assert_eq!(
        search_link.get("title").and_then(Value::as_str),
        Some("Search")
    );
    assert_eq!(
        search_link.get("templated").and_then(Value::as_bool),
        Some(true)
    );
    let next_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("next"))
        .expect("on-deck next link should be present");
    assert_eq!(
        next_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/libraries/on-deck?page=1")
    );

    let publications = payload
        .get("publications")
        .and_then(Value::as_array)
        .expect("on-deck publications should be present");
    assert_eq!(publications.len(), 1);
    let publication = &publications[0];
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("title"))
            .and_then(Value::as_str),
        Some("Book Library 2 On Deck"),
        "top-level on-deck should expose the most recent visible series pick across all visible libraries"
    );
    assert_eq!(
        publication.get("@context").and_then(Value::as_str),
        Some("https://readium.org/webpub-manifest/context.jsonld")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("identifier"))
            .and_then(Value::as_str),
        Some("urn:isbn:9780000000006")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("published"))
            .and_then(Value::as_str),
        Some("2024-01-15")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("belongsTo"))
            .and_then(|belongs_to| belongs_to.get("series"))
            .and_then(Value::as_array)
            .and_then(|series| series.first())
            .and_then(|series| series.get("name"))
            .and_then(Value::as_str),
        Some("Series 2")
    );
    let publication_links = publication
        .get("links")
        .and_then(Value::as_array)
        .expect("on-deck publication links should be present");
    assert!(
        publication_links.iter().any(|link| {
            link.get("rel").and_then(Value::as_str)
                == Some("http://www.cantook.com/api/progression")
        }),
        "on-deck publication should expose progression link"
    );
    assert!(
        publication_links.iter().all(|link| {
            link.get("properties")
                .and_then(|properties| properties.get("authenticate"))
                .and_then(|authenticate| authenticate.get("href"))
                .and_then(Value::as_str)
                == Some("http://localhost/opds/v2/auth")
        }),
        "all on-deck publication links should carry authenticate properties"
    );
    assert!(
        publication
            .get("images")
            .and_then(Value::as_array)
            .is_some_and(|images| !images.is_empty()),
        "on-deck publication should expose images"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_on_deck_filters_results_for_restricted_user() {
    let paths = new_router_fixture("router-opds-v2-on-deck-restricted-results").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(&paths, "book-pdf", "series-1", "book-pdf.pdf", "Book PDF").await;
    seed_router_library(&paths, "library-2", "Library 2").await;
    seed_router_custom_series(&paths, "series-2", "Series 2", "library-2").await;
    seed_catalog_book(
        &paths,
        "book-library-2-read",
        "series-2",
        "library-2",
        "Book Library 2 Read",
        1,
        "2024-03-01 00:00:00",
    )
    .await;
    seed_catalog_book(
        &paths,
        "book-library-2-on-deck",
        "series-2",
        "library-2",
        "Book Library 2 On Deck",
        2,
        "2024-03-01 00:00:00",
    )
    .await;
    seed_router_library_restricted_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "restricted-pass-123",
        &["library-1"],
    )
    .await;
    seed_router_read_progress_entry(
        &paths,
        "book-1",
        "restricted-user",
        10,
        true,
        "2024-03-01 00:00:00",
    )
    .await;
    seed_router_read_progress_entry(
        &paths,
        "book-library-2-read",
        "restricted-user",
        10,
        true,
        "2024-03-02 00:00:00",
    )
    .await;
    upsert_router_series_read_progress(
        &paths,
        "series-1",
        "restricted-user",
        1,
        0,
        "2024-03-01 00:00:00",
    )
    .await;
    upsert_router_series_read_progress(
        &paths,
        "series-2",
        "restricted-user",
        1,
        0,
        "2024-03-02 00:00:00",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "restricted-pass-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/on-deck")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("restricted on-deck request should build"),
        )
        .await
        .expect("restricted on-deck request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("numberOfItems"))
            .and_then(Value::as_u64),
        Some(1)
    );
    let publications = payload
        .get("publications")
        .and_then(Value::as_array)
        .expect("restricted on-deck publications should be present");
    assert_eq!(publications.len(), 1);
    assert_eq!(
        publications[0]
            .get("metadata")
            .and_then(|metadata| metadata.get("title"))
            .and_then(Value::as_str),
        Some("Book PDF")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_library_on_deck_unauthorized_returns_opds_auth_document() {
    let paths = new_router_fixture("router-opds-v2-library-on-deck-unauthorized-auth-doc").await;
    seed_router_contract_data(&paths).await;
    seed_router_read_progress(&paths, true).await;
    seed_router_pdf_book(&paths, "book-pdf", "series-1", "book-pdf.pdf", "Book PDF").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/library-1/on-deck")
                .body(Body::empty())
                .expect("opds v2 library on-deck unauthorized request should build"),
        )
        .await
        .expect("opds v2 library on-deck unauthorized request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"Realm\"")
    );
    assert!(
        response
            .headers()
            .get(header::LINK)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value.contains("/opds/v2/auth")
                    && value.contains("http://opds-spec.org/auth/document")
                    && value.contains("application/opds-authentication+json")
            })
    );

    let payload = response_json(response).await;
    assert!(
        payload
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|value| value.contains("/opds/v2/auth"))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_library_on_deck_respects_kotlin_library_scope_statuses() {
    let paths = new_router_fixture("router-opds-v2-library-on-deck-scope").await;
    seed_router_contract_data(&paths).await;
    seed_router_library(&paths, "library-2", "Library 2").await;
    seed_router_library_restricted_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "restricted-pass-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "restricted-pass-123",
    )
    .await;

    let forbidden_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/library-2/on-deck")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("forbidden on-deck request should build"),
        )
        .await
        .expect("forbidden on-deck request should complete");
    assert_eq!(forbidden_response.status(), StatusCode::FORBIDDEN);

    let missing_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/missing-library/on-deck")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing-library on-deck request should build"),
        )
        .await
        .expect("missing-library on-deck request should complete");
    assert_eq!(missing_response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_library_on_deck_uses_kotlin_shape() {
    let paths = new_router_fixture("router-opds-v2-library-on-deck-shape").await;
    seed_router_contract_data(&paths).await;
    seed_router_read_progress(&paths, true).await;
    seed_router_pdf_book(&paths, "book-pdf", "series-1", "book-pdf.pdf", "Book PDF").await;
    update_router_library_last_modified(&paths, "library-1", "2024-02-03 04:05:06").await;
    update_router_book_isbn(&paths, "book-pdf", "9780000000002").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/library-1/on-deck?size=1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 library on-deck request should build"),
        )
        .await
        .expect("opds v2 library on-deck request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;

    let metadata = payload
        .get("metadata")
        .expect("library on-deck metadata should be present");
    assert_eq!(
        metadata.get("title").and_then(Value::as_str),
        Some("Library 1 - On Deck")
    );
    assert_eq!(
        metadata.get("modified").and_then(Value::as_str),
        Some("2024-02-03T04:05:06Z")
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
        .expect("library on-deck links should be present");
    let self_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("self"))
        .expect("library on-deck self link should be present");
    assert_eq!(
        self_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/libraries/library-1/on-deck")
    );
    assert!(self_link.get("type").is_none());

    let start_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("start"))
        .expect("library on-deck start link should be present");
    assert_eq!(
        start_link.get("title").and_then(Value::as_str),
        Some("Home")
    );
    let search_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("search"))
        .expect("library on-deck search link should be present");
    assert_eq!(
        search_link.get("title").and_then(Value::as_str),
        Some("Search")
    );
    assert_eq!(
        search_link.get("templated").and_then(Value::as_bool),
        Some(true)
    );
    assert!(
        links
            .iter()
            .all(|link| link.get("rel").and_then(Value::as_str) != Some("next")),
        "single-item on-deck feed should not expose next link"
    );

    let publications = payload
        .get("publications")
        .and_then(Value::as_array)
        .expect("library on-deck publications should be present");
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
        Some("Book PDF")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("identifier"))
            .and_then(Value::as_str),
        Some("urn:isbn:9780000000002")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("numberOfPages"))
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("published"))
            .and_then(Value::as_str),
        Some("2024-02-01")
    );

    let publication_links = publication
        .get("links")
        .and_then(Value::as_array)
        .expect("on-deck publication links should be present");
    let progression_link = publication_links
        .iter()
        .find(|link| {
            link.get("rel").and_then(Value::as_str)
                == Some("http://www.cantook.com/api/progression")
        })
        .expect("on-deck publication progression link should be present");
    assert_eq!(
        progression_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/books/book-pdf/progression")
    );
    assert!(
        publication_links.iter().all(|link| {
            link.get("properties")
                .and_then(|properties| properties.get("authenticate"))
                .and_then(|authenticate| authenticate.get("href"))
                .and_then(Value::as_str)
                == Some("http://localhost/opds/v2/auth")
        }),
        "all on-deck publication links should carry authenticate properties"
    );

    let images = publication
        .get("images")
        .and_then(Value::as_array)
        .expect("on-deck publication images should be present");
    assert_eq!(
        images[0].get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/books/book-pdf/thumbnail")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_latest_books_unauthorized_returns_opds_auth_document() {
    let paths = new_router_fixture("router-opds-v2-latest-books-unauthorized-auth-doc").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/books/latest")
                .body(Body::empty())
                .expect("opds v2 latest-books unauthorized request should build"),
        )
        .await
        .expect("opds v2 latest-books unauthorized request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"Realm\"")
    );
    assert!(
        response
            .headers()
            .get(header::LINK)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value.contains("/opds/v2/auth")
                    && value.contains("http://opds-spec.org/auth/document")
                    && value.contains("application/opds-authentication+json")
            })
    );

    let payload = response_json(response).await;
    assert!(
        payload
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|value| value.contains("/opds/v2/auth"))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_latest_books_uses_kotlin_shape_and_visible_results() {
    let paths = new_router_fixture("router-opds-v2-latest-books-shape").await;
    seed_router_contract_data(&paths).await;
    seed_router_library(&paths, "library-2", "Library 2").await;
    seed_router_custom_series(&paths, "series-2", "Series 2", "library-2").await;
    seed_catalog_book(
        &paths,
        "book-library-2",
        "series-2",
        "library-2",
        "Book Library 2",
        1,
        "2024-03-01 00:00:00",
    )
    .await;
    update_router_book_created_date(&paths, "book-1", "2024-01-01 00:00:00").await;
    update_router_book_isbn(&paths, "book-library-2", "9780000000003").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/books/latest?size=1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 latest-books request should build"),
        )
        .await
        .expect("opds v2 latest-books request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert!(payload.get("navigation").is_none());
    assert!(payload.get("groups").is_none());

    let metadata = payload
        .get("metadata")
        .expect("latest-books metadata should be present");
    assert_eq!(
        metadata.get("title").and_then(Value::as_str),
        Some("All libraries - Latest Books")
    );
    assert!(
        metadata.get("modified").and_then(Value::as_str).is_some(),
        "latest-books metadata.modified should be present"
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
        .expect("latest-books links should be present");
    let self_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("self"))
        .expect("latest-books self link should be present");
    assert_eq!(
        self_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/libraries/books/latest")
    );
    assert!(self_link.get("type").is_none());
    let start_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("start"))
        .expect("latest-books start link should be present");
    assert_eq!(
        start_link.get("title").and_then(Value::as_str),
        Some("Home")
    );
    let search_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("search"))
        .expect("latest-books search link should be present");
    assert_eq!(
        search_link.get("title").and_then(Value::as_str),
        Some("Search")
    );
    assert_eq!(
        search_link.get("templated").and_then(Value::as_bool),
        Some(true)
    );
    let next_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("next"))
        .expect("latest-books next link should be present");
    assert_eq!(
        next_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/libraries/books/latest?page=1")
    );

    let publications = payload
        .get("publications")
        .and_then(Value::as_array)
        .expect("latest-books publications should be present");
    assert_eq!(publications.len(), 1);
    let publication = &publications[0];
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("title"))
            .and_then(Value::as_str),
        Some("Book Library 2"),
        "top-level latest-books should expose the most recent visible book across all visible libraries"
    );
    assert_eq!(
        publication.get("@context").and_then(Value::as_str),
        Some("https://readium.org/webpub-manifest/context.jsonld")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("identifier"))
            .and_then(Value::as_str),
        Some("urn:isbn:9780000000003")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("published"))
            .and_then(Value::as_str),
        Some("2024-01-15")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("belongsTo"))
            .and_then(|belongs_to| belongs_to.get("series"))
            .and_then(Value::as_array)
            .and_then(|series| series.first())
            .and_then(|series| series.get("name"))
            .and_then(Value::as_str),
        Some("Series 2")
    );
    let publication_links = publication
        .get("links")
        .and_then(Value::as_array)
        .expect("latest-books publication links should be present");
    assert!(
        publication_links.iter().any(|link| {
            link.get("rel").and_then(Value::as_str)
                == Some("http://www.cantook.com/api/progression")
        }),
        "latest-books publication should expose progression link"
    );
    assert!(
        publication_links.iter().all(|link| {
            link.get("properties")
                .and_then(|properties| properties.get("authenticate"))
                .and_then(|authenticate| authenticate.get("href"))
                .and_then(Value::as_str)
                == Some("http://localhost/opds/v2/auth")
        }),
        "all latest-books publication links should carry authenticate properties"
    );
    assert!(
        publication
            .get("images")
            .and_then(Value::as_array)
            .is_some_and(|images| !images.is_empty()),
        "latest-books publication should expose images"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_latest_books_filters_results_for_restricted_user() {
    let paths = new_router_fixture("router-opds-v2-latest-books-restricted-results").await;
    seed_router_contract_data(&paths).await;
    seed_router_library(&paths, "library-2", "Library 2").await;
    seed_router_custom_series(&paths, "series-2", "Series 2", "library-2").await;
    seed_catalog_book(
        &paths,
        "book-library-2",
        "series-2",
        "library-2",
        "Book Library 2",
        1,
        "2024-03-01 00:00:00",
    )
    .await;
    seed_router_library_restricted_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "restricted-pass-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "restricted-pass-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/books/latest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("restricted latest-books request should build"),
        )
        .await
        .expect("restricted latest-books request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("numberOfItems"))
            .and_then(Value::as_u64),
        Some(1)
    );
    let publications = payload
        .get("publications")
        .and_then(Value::as_array)
        .expect("restricted latest-books publications should be present");
    assert_eq!(publications.len(), 1);
    assert_eq!(
        publications[0]
            .get("metadata")
            .and_then(|metadata| metadata.get("title"))
            .and_then(Value::as_str),
        Some("Book 1")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_library_latest_books_unauthorized_returns_opds_auth_document() {
    let paths =
        new_router_fixture("router-opds-v2-library-latest-books-unauthorized-auth-doc").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/library-1/books/latest")
                .body(Body::empty())
                .expect("opds v2 library latest-books unauthorized request should build"),
        )
        .await
        .expect("opds v2 library latest-books unauthorized request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"Realm\"")
    );
    assert!(
        response
            .headers()
            .get(header::LINK)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value.contains("/opds/v2/auth")
                    && value.contains("http://opds-spec.org/auth/document")
                    && value.contains("application/opds-authentication+json")
            })
    );

    let payload = response_json(response).await;
    assert!(
        payload
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|value| value.contains("/opds/v2/auth"))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_library_latest_books_respects_kotlin_library_scope_statuses() {
    let paths = new_router_fixture("router-opds-v2-library-latest-books-scope").await;
    seed_router_contract_data(&paths).await;
    seed_router_library(&paths, "library-2", "Library 2").await;
    seed_router_library_restricted_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "restricted-pass-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "restricted-pass-123",
    )
    .await;

    let forbidden_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/library-2/books/latest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("forbidden latest-books request should build"),
        )
        .await
        .expect("forbidden latest-books request should complete");
    assert_eq!(forbidden_response.status(), StatusCode::FORBIDDEN);

    let missing_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/missing-library/books/latest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing-library latest-books request should build"),
        )
        .await
        .expect("missing-library latest-books request should complete");
    assert_eq!(missing_response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_library_latest_books_uses_kotlin_shape_and_unscoped_results() {
    let paths = new_router_fixture("router-opds-v2-library-latest-books-shape").await;
    seed_router_contract_data(&paths).await;
    seed_router_library(&paths, "library-2", "Library 2").await;
    seed_router_custom_series(&paths, "series-2", "Series 2", "library-2").await;
    seed_catalog_book(
        &paths,
        "book-library-2",
        "series-2",
        "library-2",
        "Book Library 2",
        1,
        "2024-03-01 00:00:00",
    )
    .await;
    update_router_library_last_modified(&paths, "library-1", "2024-02-03 04:05:06").await;
    update_router_book_created_date(&paths, "book-1", "2024-01-01 00:00:00").await;
    update_router_book_isbn(&paths, "book-library-2", "9780000000003").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/library-1/books/latest?size=1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 library latest-books request should build"),
        )
        .await
        .expect("opds v2 library latest-books request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert!(payload.get("navigation").is_none());
    assert!(payload.get("groups").is_none());

    let metadata = payload
        .get("metadata")
        .expect("library latest-books metadata should be present");
    assert_eq!(
        metadata.get("title").and_then(Value::as_str),
        Some("Library 1 - Latest Books")
    );
    assert_eq!(
        metadata.get("modified").and_then(Value::as_str),
        Some("2024-02-03T04:05:06Z")
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
        .expect("library latest-books links should be present");
    let self_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("self"))
        .expect("library latest-books self link should be present");
    assert_eq!(
        self_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/libraries/library-1/books/latest")
    );
    assert!(self_link.get("type").is_none());
    let start_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("start"))
        .expect("library latest-books start link should be present");
    assert_eq!(
        start_link.get("title").and_then(Value::as_str),
        Some("Home")
    );
    let search_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("search"))
        .expect("library latest-books search link should be present");
    assert_eq!(
        search_link.get("title").and_then(Value::as_str),
        Some("Search")
    );
    assert_eq!(
        search_link.get("templated").and_then(Value::as_bool),
        Some(true)
    );
    let next_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("next"))
        .expect("library latest-books next link should be present");
    assert_eq!(
        next_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/libraries/library-1/books/latest?page=1")
    );

    let publications = payload
        .get("publications")
        .and_then(Value::as_array)
        .expect("library latest-books publications should be present");
    assert_eq!(publications.len(), 1);
    let publication = &publications[0];
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("title"))
            .and_then(Value::as_str),
        Some("Book Library 2"),
        "Kotlin keeps the library-scoped route title/self metadata, but latest-books still comes from all visible libraries"
    );
    assert_eq!(
        publication.get("@context").and_then(Value::as_str),
        Some("https://readium.org/webpub-manifest/context.jsonld")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("identifier"))
            .and_then(Value::as_str),
        Some("urn:isbn:9780000000003")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("published"))
            .and_then(Value::as_str),
        Some("2024-01-15")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("belongsTo"))
            .and_then(|belongs_to| belongs_to.get("series"))
            .and_then(Value::as_array)
            .and_then(|series| series.first())
            .and_then(|series| series.get("name"))
            .and_then(Value::as_str),
        Some("Series 2")
    );
    let publication_links = publication
        .get("links")
        .and_then(Value::as_array)
        .expect("latest-books publication links should be present");
    assert!(
        publication_links.iter().any(|link| {
            link.get("rel").and_then(Value::as_str)
                == Some("http://www.cantook.com/api/progression")
        }),
        "latest-books publication should expose progression link"
    );
    assert!(
        publication_links.iter().all(|link| {
            link.get("properties")
                .and_then(|properties| properties.get("authenticate"))
                .and_then(|authenticate| authenticate.get("href"))
                .and_then(Value::as_str)
                == Some("http://localhost/opds/v2/auth")
        }),
        "all latest-books publication links should carry authenticate properties"
    );
    assert!(
        publication
            .get("images")
            .and_then(Value::as_array)
            .is_some_and(|images| !images.is_empty()),
        "latest-books publication should expose images"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_keep_reading_unauthorized_returns_opds_auth_document() {
    let paths = new_router_fixture("router-opds-v2-keep-reading-unauthorized-auth-doc").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/keep-reading")
                .body(Body::empty())
                .expect("opds v2 keep-reading unauthorized request should build"),
        )
        .await
        .expect("opds v2 keep-reading unauthorized request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"Realm\"")
    );
    assert!(
        response
            .headers()
            .get(header::LINK)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value.contains("/opds/v2/auth")
                    && value.contains("http://opds-spec.org/auth/document")
                    && value.contains("application/opds-authentication+json")
            })
    );

    let payload = response_json(response).await;
    assert!(
        payload
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|value| value.contains("/opds/v2/auth"))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_keep_reading_uses_kotlin_shape_and_visible_results() {
    let paths = new_router_fixture("router-opds-v2-keep-reading-shape").await;
    seed_router_contract_data(&paths).await;
    seed_router_library(&paths, "library-2", "Library 2").await;
    seed_router_custom_series(&paths, "series-2", "Series 2", "library-2").await;
    seed_catalog_book(
        &paths,
        "book-library-2-keep-reading",
        "series-2",
        "library-2",
        "Book Library 2 Keep Reading",
        1,
        "2024-03-01 00:00:00",
    )
    .await;
    update_router_book_isbn(&paths, "book-library-2-keep-reading", "9780000000004").await;
    seed_router_read_progress_entry(
        &paths,
        "book-1",
        "admin-user",
        4,
        false,
        "2024-01-01 00:00:00",
    )
    .await;
    seed_router_read_progress_entry(
        &paths,
        "book-library-2-keep-reading",
        "admin-user",
        7,
        false,
        "2024-03-02 00:00:00",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/keep-reading?size=1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 keep-reading request should build"),
        )
        .await
        .expect("opds v2 keep-reading request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert!(payload.get("navigation").is_none());
    assert!(payload.get("groups").is_none());

    let metadata = payload
        .get("metadata")
        .expect("keep-reading metadata should be present");
    assert_eq!(
        metadata.get("title").and_then(Value::as_str),
        Some("All libraries - Keep Reading")
    );
    assert!(
        metadata.get("modified").and_then(Value::as_str).is_some(),
        "keep-reading metadata.modified should be present"
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
        .expect("keep-reading links should be present");
    let self_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("self"))
        .expect("keep-reading self link should be present");
    assert_eq!(
        self_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/libraries/keep-reading")
    );
    assert!(self_link.get("type").is_none());
    let start_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("start"))
        .expect("keep-reading start link should be present");
    assert_eq!(
        start_link.get("title").and_then(Value::as_str),
        Some("Home")
    );
    let search_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("search"))
        .expect("keep-reading search link should be present");
    assert_eq!(
        search_link.get("title").and_then(Value::as_str),
        Some("Search")
    );
    assert_eq!(
        search_link.get("templated").and_then(Value::as_bool),
        Some(true)
    );
    let next_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("next"))
        .expect("keep-reading next link should be present");
    assert_eq!(
        next_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/libraries/keep-reading?page=1")
    );

    let publications = payload
        .get("publications")
        .and_then(Value::as_array)
        .expect("keep-reading publications should be present");
    assert_eq!(publications.len(), 1);
    let publication = &publications[0];
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("title"))
            .and_then(Value::as_str),
        Some("Book Library 2 Keep Reading"),
        "top-level keep-reading should expose the most recent visible in-progress book across all visible libraries"
    );
    assert_eq!(
        publication.get("@context").and_then(Value::as_str),
        Some("https://readium.org/webpub-manifest/context.jsonld")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("identifier"))
            .and_then(Value::as_str),
        Some("urn:isbn:9780000000004")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("published"))
            .and_then(Value::as_str),
        Some("2024-01-15")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("belongsTo"))
            .and_then(|belongs_to| belongs_to.get("series"))
            .and_then(Value::as_array)
            .and_then(|series| series.first())
            .and_then(|series| series.get("name"))
            .and_then(Value::as_str),
        Some("Series 2")
    );
    let publication_links = publication
        .get("links")
        .and_then(Value::as_array)
        .expect("keep-reading publication links should be present");
    assert!(
        publication_links.iter().any(|link| {
            link.get("rel").and_then(Value::as_str)
                == Some("http://www.cantook.com/api/progression")
        }),
        "keep-reading publication should expose progression link"
    );
    assert!(
        publication_links.iter().all(|link| {
            link.get("properties")
                .and_then(|properties| properties.get("authenticate"))
                .and_then(|authenticate| authenticate.get("href"))
                .and_then(Value::as_str)
                == Some("http://localhost/opds/v2/auth")
        }),
        "all keep-reading publication links should carry authenticate properties"
    );
    assert!(
        publication
            .get("images")
            .and_then(Value::as_array)
            .is_some_and(|images| !images.is_empty()),
        "keep-reading publication should expose images"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_keep_reading_filters_results_for_restricted_user() {
    let paths = new_router_fixture("router-opds-v2-keep-reading-restricted-results").await;
    seed_router_contract_data(&paths).await;
    seed_router_library(&paths, "library-2", "Library 2").await;
    seed_router_custom_series(&paths, "series-2", "Series 2", "library-2").await;
    seed_catalog_book(
        &paths,
        "book-library-2-keep-reading",
        "series-2",
        "library-2",
        "Book Library 2 Keep Reading",
        1,
        "2024-03-01 00:00:00",
    )
    .await;
    seed_router_library_restricted_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "restricted-pass-123",
        &["library-1"],
    )
    .await;
    seed_router_read_progress_entry(
        &paths,
        "book-1",
        "restricted-user",
        4,
        false,
        "2024-01-01 00:00:00",
    )
    .await;
    seed_router_read_progress_entry(
        &paths,
        "book-library-2-keep-reading",
        "restricted-user",
        7,
        false,
        "2024-03-02 00:00:00",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "restricted-pass-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/keep-reading")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("restricted keep-reading request should build"),
        )
        .await
        .expect("restricted keep-reading request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("numberOfItems"))
            .and_then(Value::as_u64),
        Some(1)
    );
    let publications = payload
        .get("publications")
        .and_then(Value::as_array)
        .expect("restricted keep-reading publications should be present");
    assert_eq!(publications.len(), 1);
    assert_eq!(
        publications[0]
            .get("metadata")
            .and_then(|metadata| metadata.get("title"))
            .and_then(Value::as_str),
        Some("Book 1")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_library_keep_reading_unauthorized_returns_opds_auth_document() {
    let paths =
        new_router_fixture("router-opds-v2-library-keep-reading-unauthorized-auth-doc").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/library-1/keep-reading")
                .body(Body::empty())
                .expect("opds v2 library keep-reading unauthorized request should build"),
        )
        .await
        .expect("opds v2 library keep-reading unauthorized request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"Realm\"")
    );
    assert!(
        response
            .headers()
            .get(header::LINK)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value.contains("/opds/v2/auth")
                    && value.contains("http://opds-spec.org/auth/document")
                    && value.contains("application/opds-authentication+json")
            })
    );

    let payload = response_json(response).await;
    assert!(
        payload
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|value| value.contains("/opds/v2/auth"))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_library_keep_reading_respects_kotlin_library_scope_statuses() {
    let paths = new_router_fixture("router-opds-v2-library-keep-reading-scope").await;
    seed_router_contract_data(&paths).await;
    seed_router_library(&paths, "library-2", "Library 2").await;
    seed_router_library_restricted_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "restricted-pass-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "restricted-pass-123",
    )
    .await;

    let forbidden_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/library-2/keep-reading")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("forbidden keep-reading request should build"),
        )
        .await
        .expect("forbidden keep-reading request should complete");
    assert_eq!(forbidden_response.status(), StatusCode::FORBIDDEN);

    let missing_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/missing-library/keep-reading")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing-library keep-reading request should build"),
        )
        .await
        .expect("missing-library keep-reading request should complete");
    assert_eq!(missing_response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_library_keep_reading_uses_kotlin_shape_and_unscoped_results() {
    let paths = new_router_fixture("router-opds-v2-library-keep-reading-shape").await;
    seed_router_contract_data(&paths).await;
    seed_router_library(&paths, "library-2", "Library 2").await;
    seed_router_custom_series(&paths, "series-2", "Series 2", "library-2").await;
    seed_catalog_book(
        &paths,
        "book-library-2-keep-reading",
        "series-2",
        "library-2",
        "Book Library 2 Keep Reading",
        1,
        "2024-03-01 00:00:00",
    )
    .await;
    update_router_library_last_modified(&paths, "library-1", "2024-02-03 04:05:06").await;
    update_router_book_isbn(&paths, "book-library-2-keep-reading", "9780000000004").await;
    seed_router_read_progress_entry(
        &paths,
        "book-1",
        "admin-user",
        4,
        false,
        "2024-01-01 00:00:00",
    )
    .await;
    seed_router_read_progress_entry(
        &paths,
        "book-library-2-keep-reading",
        "admin-user",
        7,
        false,
        "2024-03-02 00:00:00",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/library-1/keep-reading?size=1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 library keep-reading request should build"),
        )
        .await
        .expect("opds v2 library keep-reading request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert!(payload.get("navigation").is_none());
    assert!(payload.get("groups").is_none());

    let metadata = payload
        .get("metadata")
        .expect("library keep-reading metadata should be present");
    assert_eq!(
        metadata.get("title").and_then(Value::as_str),
        Some("Library 1 - Keep Reading")
    );
    assert_eq!(
        metadata.get("modified").and_then(Value::as_str),
        Some("2024-02-03T04:05:06Z")
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
        .expect("library keep-reading links should be present");
    let self_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("self"))
        .expect("library keep-reading self link should be present");
    assert_eq!(
        self_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/libraries/library-1/keep-reading")
    );
    assert!(self_link.get("type").is_none());
    let start_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("start"))
        .expect("library keep-reading start link should be present");
    assert_eq!(
        start_link.get("title").and_then(Value::as_str),
        Some("Home")
    );
    let search_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("search"))
        .expect("library keep-reading search link should be present");
    assert_eq!(
        search_link.get("title").and_then(Value::as_str),
        Some("Search")
    );
    assert_eq!(
        search_link.get("templated").and_then(Value::as_bool),
        Some(true)
    );
    let next_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("next"))
        .expect("library keep-reading next link should be present");
    assert_eq!(
        next_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/libraries/library-1/keep-reading?page=1")
    );

    let publications = payload
        .get("publications")
        .and_then(Value::as_array)
        .expect("library keep-reading publications should be present");
    assert_eq!(publications.len(), 1);
    let publication = &publications[0];
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("title"))
            .and_then(Value::as_str),
        Some("Book Library 2 Keep Reading"),
        "Kotlin keeps the library-scoped route title/self metadata, but keep-reading results still come from all visible libraries"
    );
    assert_eq!(
        publication.get("@context").and_then(Value::as_str),
        Some("https://readium.org/webpub-manifest/context.jsonld")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("identifier"))
            .and_then(Value::as_str),
        Some("urn:isbn:9780000000004")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("published"))
            .and_then(Value::as_str),
        Some("2024-01-15")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("belongsTo"))
            .and_then(|belongs_to| belongs_to.get("series"))
            .and_then(Value::as_array)
            .and_then(|series| series.first())
            .and_then(|series| series.get("name"))
            .and_then(Value::as_str),
        Some("Series 2")
    );
    let publication_links = publication
        .get("links")
        .and_then(Value::as_array)
        .expect("keep-reading publication links should be present");
    assert!(
        publication_links.iter().any(|link| {
            link.get("rel").and_then(Value::as_str)
                == Some("http://www.cantook.com/api/progression")
        }),
        "keep-reading publication should expose progression link"
    );
    assert!(
        publication_links.iter().all(|link| {
            link.get("properties")
                .and_then(|properties| properties.get("authenticate"))
                .and_then(|authenticate| authenticate.get("href"))
                .and_then(Value::as_str)
                == Some("http://localhost/opds/v2/auth")
        }),
        "all keep-reading publication links should carry authenticate properties"
    );
    assert!(
        publication
            .get("images")
            .and_then(Value::as_array)
            .is_some_and(|images| !images.is_empty()),
        "keep-reading publication should expose images"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_readlist_unauthorized_returns_opds_auth_document() {
    let paths = new_router_fixture("router-opds-v2-readlist-unauthorized-auth-doc").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/readlists/readlist-1")
                .body(Body::empty())
                .expect("opds v2 readlist unauthorized request should build"),
        )
        .await
        .expect("opds v2 readlist unauthorized request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"Realm\"")
    );
    assert!(
        response
            .headers()
            .get(header::LINK)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value.contains("/opds/v2/auth")
                    && value.contains("http://opds-spec.org/auth/document")
                    && value.contains("application/opds-authentication+json")
            })
    );

    let payload = response_json(response).await;
    assert!(
        payload
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|value| value.contains("/opds/v2/auth"))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_readlist_returns_not_found_for_missing_or_out_of_scope_readlist() {
    let paths = new_router_fixture("router-opds-v2-readlist-scope").await;
    seed_router_contract_data(&paths).await;
    seed_router_library(&paths, "library-2", "Library 2").await;
    seed_router_custom_series(&paths, "series-2", "Series 2", "library-2").await;
    seed_catalog_book(
        &paths,
        "book-library-2-readlist",
        "series-2",
        "library-2",
        "Book Library 2 Readlist",
        1,
        "2024-03-01 00:00:00",
    )
    .await;
    seed_router_readlist(
        &paths,
        "readlist-2",
        "ReadList 2",
        "book-library-2-readlist",
    )
    .await;
    seed_router_library_restricted_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "restricted-pass-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "restricted-pass-123",
    )
    .await;

    let hidden_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/readlists/readlist-2")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("hidden readlist request should build"),
        )
        .await
        .expect("hidden readlist request should complete");
    assert_eq!(hidden_response.status(), StatusCode::NOT_FOUND);

    let missing_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/readlists/missing-readlist")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing readlist request should build"),
        )
        .await
        .expect("missing readlist request should complete");
    assert_eq!(missing_response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_readlist_keeps_books_without_sharing_labels() {
    let paths = new_router_fixture("router-opds-v2-readlist-without-sharing-labels").await;
    seed_router_contract_data(&paths).await;
    clear_router_series_sharing_labels(&paths, "series-1").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/readlists/readlist-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 readlist request should build"),
        )
        .await
        .expect("opds v2 readlist request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let publications = payload
        .get("publications")
        .and_then(Value::as_array)
        .expect("readlist publications should be present");
    assert_eq!(publications.len(), 1);
    assert_eq!(
        publications[0]
            .get("metadata")
            .and_then(|metadata| metadata.get("title"))
            .and_then(Value::as_str),
        Some("Book 1")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_readlist_uses_kotlin_shape_and_publications() {
    let paths = new_router_fixture("router-opds-v2-readlist-shape").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(&paths, "book-pdf", "series-1", "book-pdf.pdf", "Book PDF").await;
    seed_router_readlist_book_entry(&paths, "readlist-1", "book-pdf", -1).await;
    update_router_readlist_ordered(&paths, "readlist-1", false).await;
    update_router_readlist_last_modified(&paths, "readlist-1", "2024-02-03 04:05:06").await;
    update_router_book_isbn(&paths, "book-1", "9780000000005").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/readlists/readlist-1?size=1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 readlist request should build"),
        )
        .await
        .expect("opds v2 readlist request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert!(payload.get("navigation").is_none());
    assert!(payload.get("groups").is_none());

    let metadata = payload
        .get("metadata")
        .expect("readlist metadata should be present");
    assert_eq!(
        metadata.get("title").and_then(Value::as_str),
        Some("ReadList 1")
    );
    assert_eq!(
        metadata.get("modified").and_then(Value::as_str),
        Some("2024-02-03T04:05:06Z")
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
        .expect("readlist links should be present");
    let self_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("self"))
        .expect("readlist self link should be present");
    assert_eq!(
        self_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/readlists/readlist-1")
    );
    assert!(self_link.get("type").is_none());
    let start_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("start"))
        .expect("readlist start link should be present");
    assert_eq!(
        start_link.get("title").and_then(Value::as_str),
        Some("Home")
    );
    let search_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("search"))
        .expect("readlist search link should be present");
    assert_eq!(
        search_link.get("title").and_then(Value::as_str),
        Some("Search")
    );
    assert_eq!(
        search_link.get("templated").and_then(Value::as_bool),
        Some(true)
    );
    let next_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("next"))
        .expect("readlist next link should be present");
    assert_eq!(
        next_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/readlists/readlist-1?page=1")
    );

    let publications = payload
        .get("publications")
        .and_then(Value::as_array)
        .expect("readlist publications should be present");
    assert_eq!(publications.len(), 1);
    let publication = &publications[0];
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("title"))
            .and_then(Value::as_str),
        Some("Book 1"),
        "readlist is unordered in the fixture, so Kotlin sorts by releaseDate ascending rather than READLIST_BOOK.NUMBER"
    );
    assert_eq!(
        publication.get("@context").and_then(Value::as_str),
        Some("https://readium.org/webpub-manifest/context.jsonld")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("identifier"))
            .and_then(Value::as_str),
        Some("urn:isbn:9780000000005")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("published"))
            .and_then(Value::as_str),
        Some("2024-01-15")
    );
    assert_eq!(
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("belongsTo"))
            .and_then(|belongs_to| belongs_to.get("series"))
            .and_then(Value::as_array)
            .and_then(|series| series.first())
            .and_then(|series| series.get("name"))
            .and_then(Value::as_str),
        Some("Series 1")
    );
    let publication_links = publication
        .get("links")
        .and_then(Value::as_array)
        .expect("readlist publication links should be present");
    assert!(
        publication_links.iter().any(|link| {
            link.get("rel").and_then(Value::as_str)
                == Some("http://www.cantook.com/api/progression")
        }),
        "readlist publication should expose progression link"
    );
    assert!(
        publication_links.iter().all(|link| {
            link.get("properties")
                .and_then(|properties| properties.get("authenticate"))
                .and_then(|authenticate| authenticate.get("href"))
                .and_then(Value::as_str)
                == Some("http://localhost/opds/v2/auth")
        }),
        "all readlist publication links should carry authenticate properties"
    );
    assert!(
        publication
            .get("images")
            .and_then(Value::as_array)
            .is_some_and(|images| !images.is_empty()),
        "readlist publication should expose images"
    );

    cleanup_router_fixture(paths);
}

async fn seed_router_read_progress_entry(
    paths: &RuntimeDbPaths,
    book_id: &str,
    user_id: &str,
    page: i64,
    completed: bool,
    read_date: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds read-progress seed db should open");

    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED, READ_DATE) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind(user_id)
    .bind(page)
    .bind(completed)
    .bind(read_date)
    .execute(&pool)
    .await
    .expect("read progress row should be inserted");

    pool.close().await;
}

async fn upsert_router_series_read_progress(
    paths: &RuntimeDbPaths,
    series_id: &str,
    user_id: &str,
    read_count: i64,
    in_progress_count: i64,
    most_recent_read_date: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds series read-progress seed db should open");

    sqlx::query(
        "INSERT INTO READ_PROGRESS_SERIES (SERIES_ID, USER_ID, READ_COUNT, IN_PROGRESS_COUNT, MOST_RECENT_READ_DATE) VALUES (?, ?, ?, ?, ?) \
         ON CONFLICT (SERIES_ID, USER_ID) DO UPDATE SET READ_COUNT = excluded.READ_COUNT, IN_PROGRESS_COUNT = excluded.IN_PROGRESS_COUNT, MOST_RECENT_READ_DATE = excluded.MOST_RECENT_READ_DATE",
    )
    .bind(series_id)
    .bind(user_id)
    .bind(read_count)
    .bind(in_progress_count)
    .bind(most_recent_read_date)
    .execute(&pool)
    .await
    .expect("series read progress row should be upserted");

    pool.close().await;
}

async fn update_router_book_isbn(paths: &RuntimeDbPaths, book_id: &str, isbn: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds book isbn update db should open");

    sqlx::query("UPDATE BOOK_METADATA SET ISBN = ? WHERE BOOK_ID = ?")
        .bind(isbn)
        .bind(book_id)
        .execute(&pool)
        .await
        .expect("book metadata isbn should be updated");

    pool.close().await;
}

async fn update_router_book_number_metadata(
    paths: &RuntimeDbPaths,
    book_id: &str,
    number: &str,
    number_sort: f64,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds book number metadata update db should open");

    sqlx::query("UPDATE BOOK_METADATA SET NUMBER = ?, NUMBER_SORT = ? WHERE BOOK_ID = ?")
        .bind(number)
        .bind(number_sort)
        .bind(book_id)
        .execute(&pool)
        .await
        .expect("book metadata number fields should be updated");

    pool.close().await;
}

async fn update_router_book_created_date(
    paths: &RuntimeDbPaths,
    book_id: &str,
    created_date: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds book created_date update db should open");

    sqlx::query("UPDATE BOOK SET CREATED_DATE = ?, LAST_MODIFIED_DATE = ? WHERE ID = ?")
        .bind(created_date)
        .bind(created_date)
        .bind(book_id)
        .execute(&pool)
        .await
        .expect("book created date should be updated");

    pool.close().await;
}

async fn update_router_series_catalog_fields(
    paths: &RuntimeDbPaths,
    series_id: &str,
    one_shot: bool,
    last_modified_date: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds series catalog update db should open");

    sqlx::query(
        "UPDATE SERIES SET ONESHOT = ?, LAST_MODIFIED_DATE = ?, CREATED_DATE = ? WHERE ID = ?",
    )
    .bind(one_shot)
    .bind(last_modified_date)
    .bind(last_modified_date)
    .bind(series_id)
    .execute(&pool)
    .await
    .expect("series catalog fields should be updated");

    pool.close().await;
}

async fn seed_catalog_book(
    paths: &RuntimeDbPaths,
    book_id: &str,
    series_id: &str,
    library_id: &str,
    title: &str,
    number: i64,
    created_date: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds catalog book seed db should open");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind(0_i64)
    .bind(format!("{book_id}.epub"))
    .bind(format!("books/{book_id}.epub"))
    .bind(series_id)
    .bind(2_048_i64)
    .bind(number)
    .bind(library_id)
    .execute(&pool)
    .await
    .expect("catalog book row should be inserted");

    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/epub+zip")
        .bind("READY")
        .bind(book_id)
        .bind(10_i64)
        .execute(&pool)
        .await
        .expect("catalog book media should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(number.to_string())
    .bind(number as f64)
    .bind(title)
    .bind("2024-01-15")
    .bind(book_id)
    .execute(&pool)
    .await
    .expect("catalog book metadata should be inserted");

    sqlx::query("UPDATE BOOK SET CREATED_DATE = ?, LAST_MODIFIED_DATE = ? WHERE ID = ?")
        .bind(created_date)
        .bind(created_date)
        .bind(book_id)
        .execute(&pool)
        .await
        .expect("catalog book created date should be updated");

    pool.close().await;
}

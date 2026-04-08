use super::*;

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

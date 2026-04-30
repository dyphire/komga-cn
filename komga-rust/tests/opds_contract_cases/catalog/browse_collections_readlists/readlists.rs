use super::*;

#[tokio::test]
async fn router_opds_v2_library_readlists_respect_kotlin_library_scope_statuses() {
    let ctx = TestFixture::new("router-opds-v2-library-readlists-scope").await;
    seed_router_library(ctx.paths(), "library-2", "Library 2").await;
    seed_router_library_restricted_user(
        ctx.paths(),
        "restricted-user",
        "restricted@example.org",
        "restricted-pass-123",
        &["library-1"],
    )
    .await;

    let auth_token = ctx
        .login_with_credentials("restricted@example.org", "restricted-pass-123")
        .await;

    let forbidden_response = ctx
        .app()
        .clone()
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

    let missing_response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_opds_v2_readlists_use_kotlin_grouped_feed_shape() {
    let ctx = TestFixture::new("router-opds-v2-readlists-shape").await;
    seed_router_readlist(ctx.paths(), "readlist-0", "Alpha ReadList", "book-1").await;
    update_router_library_last_modified(ctx.paths(), "library-1", "2024-02-03 04:05:06").await;

    let auth_token = ctx.login_admin().await;

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
        let response = ctx
            .app()
            .clone()
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

    for route in [
        "/opds/v2/libraries/readlists?page=2&size=1",
        "/opds/v2/libraries/library-1/readlists?page=2&size=1",
    ] {
        let response = ctx
            .app()
            .clone()
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("opds v2 empty-page readlists request should build"),
            )
            .await
            .expect("opds v2 empty-page readlists request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
        let payload = response_json(response).await;
        let groups = payload
            .get("groups")
            .and_then(Value::as_array)
            .expect("readlists groups should be present on empty pages too");
        let readlists_group = groups
            .iter()
            .find(|group| {
                group
                    .get("metadata")
                    .and_then(|metadata| metadata.get("title"))
                    .and_then(Value::as_str)
                    == Some("Read Lists")
            })
            .expect("readlists group should be present on empty pages too");
        assert_eq!(
            readlists_group
                .get("navigation")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(0),
            "route: {route}, Kotlin keeps empty group navigation arrays"
        );
    }
}

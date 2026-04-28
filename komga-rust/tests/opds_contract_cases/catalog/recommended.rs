use super::*;

#[tokio::test]
async fn router_opds_v2_library_recommended_unauthorized_returns_opds_auth_document() {
    let paths =
        new_router_fixture("router-opds-v2-library-recommended-unauthorized-auth-doc").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;

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

    assert_unauthorized_opds_auth_document(response).await;

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_library_recommended_uses_kotlin_shape() {
    let paths = new_router_fixture("router-opds-v2-library-recommended-shape").await;
    seed_router_contract_data(&paths).await;
    update_router_library_last_modified(&paths, "library-1", "2024-02-03 04:05:06").await;
    update_router_book_isbn(&paths, "book-1", "9781234567890").await;
    update_router_book_number_metadata(&paths, "book-1", "Special", 10.0).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
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

    cleanup_router_fixture(paths);
}

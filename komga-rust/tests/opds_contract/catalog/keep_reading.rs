use super::*;

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

    assert_unauthorized_opds_auth_document(response).await;

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

    assert_unauthorized_opds_auth_document(response).await;

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

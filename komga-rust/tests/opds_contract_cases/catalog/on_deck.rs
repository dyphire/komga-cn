use super::*;

#[tokio::test]
async fn router_opds_v2_on_deck_unauthorized_returns_opds_auth_document() {
    let ctx = TestFixture::new("router-opds-v2-on-deck-unauthorized-auth-doc").await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/on-deck")
                .body(Body::empty())
                .expect("opds v2 on-deck unauthorized request should build"),
        )
        .await
        .expect("opds v2 on-deck unauthorized request should complete");

    assert_unauthorized_opds_auth_document(response).await;
}

#[tokio::test]
async fn router_opds_v2_on_deck_uses_kotlin_shape_and_visible_results() {
    let ctx = TestFixture::new("router-opds-v2-on-deck-shape").await;
    seed_router_pdf_book(
        ctx.paths(),
        "book-pdf",
        "series-1",
        "book-pdf.pdf",
        "Book PDF",
    )
    .await;
    seed_router_library(ctx.paths(), "library-2", "Library 2").await;
    seed_router_custom_series(ctx.paths(), "series-2", "Series 2", "library-2").await;
    seed_catalog_book(
        ctx.paths(),
        "book-library-2-read",
        "series-2",
        "library-2",
        "Book Library 2 Read",
        1,
        "2024-03-01 00:00:00",
    )
    .await;
    seed_catalog_book(
        ctx.paths(),
        "book-library-2-on-deck",
        "series-2",
        "library-2",
        "Book Library 2 On Deck",
        2,
        "2024-03-01 00:00:00",
    )
    .await;
    update_router_book_isbn(ctx.paths(), "book-pdf", "9780000000002").await;
    update_router_book_isbn(ctx.paths(), "book-library-2-on-deck", "9780000000006").await;
    seed_router_read_progress_entry(
        ctx.paths(),
        "book-1",
        "admin-user",
        10,
        true,
        "2024-03-01 00:00:00",
    )
    .await;
    seed_router_read_progress_entry(
        ctx.paths(),
        "book-library-2-read",
        "admin-user",
        10,
        true,
        "2024-03-02 00:00:00",
    )
    .await;
    upsert_router_series_read_progress(
        ctx.paths(),
        "series-1",
        "admin-user",
        1,
        0,
        "2024-03-01 00:00:00",
    )
    .await;
    upsert_router_series_read_progress(
        ctx.paths(),
        "series-2",
        "admin-user",
        1,
        0,
        "2024-03-02 00:00:00",
    )
    .await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_opds_v2_on_deck_filters_results_for_restricted_user() {
    let ctx = TestFixture::new("router-opds-v2-on-deck-restricted-results").await;
    seed_router_pdf_book(
        ctx.paths(),
        "book-pdf",
        "series-1",
        "book-pdf.pdf",
        "Book PDF",
    )
    .await;
    seed_router_library(ctx.paths(), "library-2", "Library 2").await;
    seed_router_custom_series(ctx.paths(), "series-2", "Series 2", "library-2").await;
    seed_catalog_book(
        ctx.paths(),
        "book-library-2-read",
        "series-2",
        "library-2",
        "Book Library 2 Read",
        1,
        "2024-03-01 00:00:00",
    )
    .await;
    seed_catalog_book(
        ctx.paths(),
        "book-library-2-on-deck",
        "series-2",
        "library-2",
        "Book Library 2 On Deck",
        2,
        "2024-03-01 00:00:00",
    )
    .await;
    seed_router_library_restricted_user(
        ctx.paths(),
        "restricted-user",
        "restricted@example.org",
        "restricted-pass-123",
        &["library-1"],
    )
    .await;
    seed_router_read_progress_entry(
        ctx.paths(),
        "book-1",
        "restricted-user",
        10,
        true,
        "2024-03-01 00:00:00",
    )
    .await;
    seed_router_read_progress_entry(
        ctx.paths(),
        "book-library-2-read",
        "restricted-user",
        10,
        true,
        "2024-03-02 00:00:00",
    )
    .await;
    upsert_router_series_read_progress(
        ctx.paths(),
        "series-1",
        "restricted-user",
        1,
        0,
        "2024-03-01 00:00:00",
    )
    .await;
    upsert_router_series_read_progress(
        ctx.paths(),
        "series-2",
        "restricted-user",
        1,
        0,
        "2024-03-02 00:00:00",
    )
    .await;

    let auth_token = ctx
        .login_with_credentials("restricted@example.org", "restricted-pass-123")
        .await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_opds_v2_library_on_deck_unauthorized_returns_opds_auth_document() {
    let ctx = TestFixture::new("router-opds-v2-library-on-deck-unauthorized-auth-doc").await;
    seed_router_read_progress(ctx.paths(), true).await;
    seed_router_pdf_book(
        ctx.paths(),
        "book-pdf",
        "series-1",
        "book-pdf.pdf",
        "Book PDF",
    )
    .await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/library-1/on-deck")
                .body(Body::empty())
                .expect("opds v2 library on-deck unauthorized request should build"),
        )
        .await
        .expect("opds v2 library on-deck unauthorized request should complete");

    assert_unauthorized_opds_auth_document(response).await;
}

#[tokio::test]
async fn router_opds_v2_library_on_deck_respects_kotlin_library_scope_statuses() {
    let ctx = TestFixture::new("router-opds-v2-library-on-deck-scope").await;
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
                .uri("/opds/v2/libraries/library-2/on-deck")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("forbidden on-deck request should build"),
        )
        .await
        .expect("forbidden on-deck request should complete");
    assert_eq!(forbidden_response.status(), StatusCode::FORBIDDEN);

    let missing_response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_opds_v2_library_on_deck_uses_kotlin_shape() {
    let ctx = TestFixture::new("router-opds-v2-library-on-deck-shape").await;
    seed_router_read_progress(ctx.paths(), true).await;
    seed_router_pdf_book(
        ctx.paths(),
        "book-pdf",
        "series-1",
        "book-pdf.pdf",
        "Book PDF",
    )
    .await;
    update_router_library_last_modified(ctx.paths(), "library-1", "2024-02-03 04:05:06").await;
    update_router_book_isbn(ctx.paths(), "book-pdf", "9780000000002").await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
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
}

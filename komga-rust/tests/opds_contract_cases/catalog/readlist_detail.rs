use super::*;

#[tokio::test]
async fn router_opds_v2_readlist_unauthorized_returns_opds_auth_document() {
    let ctx = TestFixture::new("router-opds-v2-readlist-unauthorized-auth-doc").await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/readlists/readlist-1")
                .body(Body::empty())
                .expect("opds v2 readlist unauthorized request should build"),
        )
        .await
        .expect("opds v2 readlist unauthorized request should complete");

    assert_unauthorized_opds_auth_document(response).await;
}

#[tokio::test]
async fn router_opds_v2_readlist_returns_not_found_for_missing_or_out_of_scope_readlist() {
    let ctx = TestFixture::new("router-opds-v2-readlist-scope").await;
    seed_router_library(ctx.paths(), "library-2", "Library 2").await;
    seed_router_custom_series(ctx.paths(), "series-2", "Series 2", "library-2").await;
    seed_catalog_book(
        ctx.paths(),
        "book-library-2-readlist",
        "series-2",
        "library-2",
        "Book Library 2 Readlist",
        1,
        "2024-03-01 00:00:00",
    )
    .await;
    seed_router_readlist(
        ctx.paths(),
        "readlist-2",
        "ReadList 2",
        "book-library-2-readlist",
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

    let auth_token = ctx
        .login_with_credentials("restricted@example.org", "restricted-pass-123")
        .await;

    let hidden_response = ctx
        .app()
        .clone()
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

    let missing_response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_opds_v2_readlist_keeps_books_without_sharing_labels() {
    let ctx = TestFixture::new("router-opds-v2-readlist-without-sharing-labels").await;
    clear_router_series_sharing_labels(ctx.paths(), "series-1").await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_opds_v2_readlist_uses_kotlin_shape_and_publications() {
    let ctx = TestFixture::new("router-opds-v2-readlist-shape").await;
    seed_router_pdf_book(
        ctx.paths(),
        "book-pdf",
        "series-1",
        "book-pdf.pdf",
        "Book PDF",
    )
    .await;
    seed_router_readlist_book_entry(ctx.paths(), "readlist-1", "book-pdf", -1).await;
    update_router_readlist_ordered(ctx.paths(), "readlist-1", false).await;
    update_router_readlist_last_modified(ctx.paths(), "readlist-1", "2024-02-03 04:05:06").await;
    update_router_book_isbn(ctx.paths(), "book-1", "9780000000005").await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
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
        publication
            .get("metadata")
            .and_then(|metadata| metadata.get("identifier"))
            .and_then(Value::as_str),
        Some("urn:isbn:9780000000005")
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
}

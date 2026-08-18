use super::*;

#[tokio::test]
async fn router_download_routes_do_not_get_shallow_etag_headers() {
    let ctx = TestFixture::builder("router-download-routes-no-shallow-etag")
        .with_seed(|paths| async move {
            seed_kobo_sync_api_key(&paths, "any-token", "admin-user").await;
        })
        .build()
        .await;
    let books_dir = ctx.paths().config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for download exclusion test");
    std::fs::write(books_dir.join("book-1.epub"), b"download-exclusion-body")
        .expect("book fixture file should be written for download exclusion test");

    let auth_token = ctx.login_admin().await;

    for route in [
        "/api/v1/books/book-1/file/book-1.epub",
        "/opds/v2/books/book-1/file/book-1.epub",
        "/kobo/any-token/v1/books/book-1/file/epub",
    ] {
        let response = ctx
            .app()
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .header(header::IF_NONE_MATCH, "\"unrelated-etag\"")
                    .body(Body::empty())
                    .expect("download exclusion request should build"),
            )
            .await
            .expect("download exclusion request should complete");

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "download route should not turn into 304: {route}",
        );
        assert!(
            !response.headers().contains_key(header::ETAG),
            "download route should not receive shallow etag: {route}",
        );
    }
}

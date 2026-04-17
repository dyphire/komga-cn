use super::*;

#[tokio::test]
async fn router_download_routes_do_not_get_shallow_etag_headers() {
    let paths = new_router_fixture("router-download-routes-no-shallow-etag").await;
    seed_router_contract_data(&paths).await;
    seed_kobo_sync_api_key(&paths, "any-token", "admin-user").await;
    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for download exclusion test");
    std::fs::write(books_dir.join("book-1.epub"), b"download-exclusion-body")
        .expect("book fixture file should be written for download exclusion test");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let libraries_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/libraries")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("libraries request should build for exclusion control"),
        )
        .await
        .expect("libraries request should complete for exclusion control");
    let cache_etag = libraries_response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("non-download route should expose etag for exclusion control");

    for route in [
        "/api/v1/books/book-1/file/book-1.epub",
        "/opds/v2/books/book-1/file/book-1.epub",
        "/kobo/any-token/v1/books/book-1/file/epub",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .header(header::IF_NONE_MATCH, &cache_etag)
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

    cleanup_router_fixture(paths);
}

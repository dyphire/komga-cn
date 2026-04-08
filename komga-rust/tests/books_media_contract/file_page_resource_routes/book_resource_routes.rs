use super::*;

#[tokio::test]
async fn router_book_resource_supports_not_modified_and_inline_content_disposition() {
    let paths = new_router_fixture("router-book-resource-inline-not-modified").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_resource(
        &paths,
        "books/book-1.epub",
        "OEBPS/chapter.xhtml",
        br#"<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><p>Hello</p></body></html>"#,
    );

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for route in [
        "/api/v1/books/book-1/resource/OEBPS/chapter.xhtml",
        "/opds/v2/books/book-1/resource/OEBPS/chapter.xhtml",
    ] {
        let initial = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("resource request should build"),
            )
            .await
            .expect("resource request should complete");

        assert_eq!(initial.status(), StatusCode::OK, "route: {route}");
        let last_modified = initial
            .headers()
            .get(header::LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .expect("resource response should expose last-modified")
            .to_string();
        let disposition = initial
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .and_then(|value| value.to_str().ok())
            .expect("resource response should expose content-disposition");
        assert!(
            disposition.starts_with("inline;"),
            "route: {route}, disposition: {disposition}"
        );
        assert!(
            disposition.contains("chapter.xhtml"),
            "route: {route}, disposition: {disposition}"
        );
        assert_eq!(
            initial
                .headers()
                .get(header::CONTENT_SECURITY_POLICY)
                .and_then(|value| value.to_str().ok()),
            Some("script-src 'none'; object-src 'none';"),
            "route: {route}"
        );

        let not_modified = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .header(header::IF_MODIFIED_SINCE, &last_modified)
                    .body(Body::empty())
                    .expect("conditional resource request should build"),
            )
            .await
            .expect("conditional resource request should complete");

        assert_eq!(
            not_modified.status(),
            StatusCode::NOT_MODIFIED,
            "route: {route}"
        );
        assert_eq!(
            not_modified
                .headers()
                .get(header::LAST_MODIFIED)
                .and_then(|value| value.to_str().ok()),
            Some(last_modified.as_str()),
            "route: {route}"
        );
    }

    cleanup_router_fixture(paths);
}

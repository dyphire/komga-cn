use super::*;

#[tokio::test]
async fn router_opds_v2_book_file_unauthorized_returns_opds_auth_document() {
    let ctx = TestFixture::new("router-opds-v2-book-file-unauthorized-auth-doc").await;

    for route in [
        "/opds/v2/books/book-1/file",
        "/opds/v2/books/book-1/file/book-1.epub",
    ] {
        let response = ctx
            .app()
            .clone()
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
}

#[tokio::test]
async fn router_opds_v2_book_page_routes_unauthorized_return_opds_auth_document() {
    let ctx = TestFixture::new("router-opds-v2-book-page-unauthorized-auth-doc").await;

    for route in [
        "/opds/v2/books/book-1/pages/1",
        "/opds/v2/books/book-1/pages/1/raw",
    ] {
        let response = ctx
            .app()
            .clone()
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .body(Body::empty())
                    .expect("opds v2 book page unauthorized request should build"),
            )
            .await
            .expect("opds v2 book page unauthorized request should complete");

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
}

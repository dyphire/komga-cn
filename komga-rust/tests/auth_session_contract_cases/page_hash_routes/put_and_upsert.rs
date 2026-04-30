use super::*;

#[tokio::test]
async fn router_put_page_hash_normalizes_negative_size_to_null() {
    let ctx = TestFixture::new("router-put-page-hash-negative-size-null").await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"hash":"negative-size-hash","size":-1,"action":"IGNORE"}"#,
                ))
                .expect("page hash put request should build"),
        )
        .await
        .expect("page hash put request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        load_page_hash_size(ctx.paths(), "negative-size-hash").await,
        None
    );
}

#[tokio::test]
async fn router_put_page_hash_preserves_whitespace_padded_hash() {
    let ctx = TestFixture::new("router-put-page-hash-whitespace-hash").await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"hash":" negative-size-hash ","size":1,"action":"IGNORE"}"#,
                ))
                .expect("page hash put request with padded hash should build"),
        )
        .await
        .expect("page hash put request with padded hash should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        load_page_hash_size(ctx.paths(), " negative-size-hash ").await,
        Some(1)
    );
}

#[tokio::test]
async fn router_put_page_hash_rejects_blank_only_hash() {
    let ctx = TestFixture::new("router-put-page-hash-blank-only-hash").await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"hash":"   ","size":1,"action":"IGNORE"}"#))
                .expect("page hash put request with blank-only hash should build"),
        )
        .await
        .expect("page hash put request with blank-only hash should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn router_put_page_hash_rejects_whitespace_padded_action() {
    let ctx = TestFixture::new("router-put-page-hash-whitespace-action").await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"hash":"negative-size-hash","size":1,"action":" IGNORE "}"#,
                ))
                .expect("page hash put request with whitespace action should build"),
        )
        .await
        .expect("page hash put request with whitespace action should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn router_put_page_hash_rejects_non_integer_size_values() {
    let ctx = TestFixture::new("router-put-page-hash-non-integer-size").await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"hash":"typed-size-hash","size":true,"action":"IGNORE"}"#,
                ))
                .expect("page hash put request with non-integer size should build"),
        )
        .await
        .expect("page hash put request with non-integer size should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        load_page_hash_record(ctx.paths(), "typed-size-hash")
            .await
            .is_none()
    );
}

#[tokio::test]
async fn router_put_page_hash_preserves_existing_size_on_update() {
    let ctx = TestFixture::new("router-put-page-hash-preserve-existing-size").await;
    seed_page_hash_row(ctx.paths(), "existing-size-hash", Some(5), "IGNORE").await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"hash":"existing-size-hash","size":99,"action":"DELETE_AUTO"}"#,
                ))
                .expect("page hash update request should build"),
        )
        .await
        .expect("page hash update request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        load_page_hash_record(ctx.paths(), "existing-size-hash").await,
        Some((Some(5), "DELETE_AUTO".to_string()))
    );
}

#[tokio::test]
async fn router_put_page_hash_persists_known_thumbnail_so_it_survives_source_removal() {
    let ctx = TestFixture::new("router-put-page-hash-persists-thumbnail").await;
    let source_path = seed_page_hash_image_source(
        ctx.paths(),
        "book-page-hash-thumb",
        "known-thumb-hash",
        "images/known-thumb-source.png",
        "known-thumb-source.png",
    )
    .await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"hash":"known-thumb-hash","size":64,"action":"IGNORE"}"#,
                ))
                .expect("page hash put request for thumbnail persistence should build"),
        )
        .await
        .expect("page hash put request for thumbnail persistence should complete");

    assert_eq!(put_response.status(), StatusCode::ACCEPTED);
    std::fs::remove_file(&source_path).expect("source image should be removable after put");

    let thumbnail_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/known-thumb-hash/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("known page hash thumbnail request should build"),
        )
        .await
        .expect("known page hash thumbnail request should complete");

    assert_eq!(thumbnail_response.status(), StatusCode::OK);
    assert_eq!(
        thumbnail_response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    let body = to_bytes(thumbnail_response.into_body(), usize::MAX)
        .await
        .expect("known page hash thumbnail response body should be readable");
    assert!(body.starts_with(&[0xFF, 0xD8]));
}

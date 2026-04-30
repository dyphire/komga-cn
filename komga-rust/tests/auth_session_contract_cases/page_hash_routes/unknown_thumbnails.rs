use super::*;

#[tokio::test]
async fn router_get_page_hash_unknown_thumbnail_returns_original_image_without_resize_like_kotlin()
{
    let ctx = TestFixture::new("router-page-hash-unknown-thumbnail-original-image").await;
    let expected = large_png_bytes(640, 320);
    seed_unknown_page_hash_source(
        ctx.paths(),
        "book-unknown-thumb-image",
        "unknown-thumb-image-hash",
        "images/unknown-thumb-image.png",
        "unknown-thumb-image.png",
        "image/png",
        &expected,
    )
    .await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/unknown/unknown-thumb-image-hash/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("unknown page hash thumbnail request should build"),
        )
        .await
        .expect("unknown page hash thumbnail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/png")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("unknown page hash thumbnail body should be readable");
    assert_eq!(body.as_ref(), expected.as_slice());
}

#[tokio::test]
async fn router_get_page_hash_unknown_thumbnail_renders_pdf_page_without_resize_like_kotlin() {
    let ctx = TestFixture::new("router-page-hash-unknown-thumbnail-pdf-original").await;
    seed_unknown_page_hash_pdf_match(
        ctx.paths(),
        "book-unknown-thumb-pdf-original",
        "unknown-thumb-pdf-original-hash",
    )
    .await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/unknown/unknown-thumb-pdf-original-hash/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("pdf unknown page hash thumbnail request should build"),
        )
        .await
        .expect("pdf unknown page hash thumbnail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("pdf unknown page hash thumbnail body should be readable");
    let image = image::load_from_memory(&body)
        .expect("pdf unknown page hash thumbnail should decode as image");
    assert!(image.width().max(image.height()) > 300);
}

#[tokio::test]
async fn router_get_page_hash_unknown_thumbnail_honors_resize_and_renders_jpeg_for_pdf_like_kotlin()
{
    let ctx = TestFixture::new("router-page-hash-unknown-thumbnail-pdf-resize").await;
    seed_unknown_page_hash_pdf_match(
        ctx.paths(),
        "book-unknown-thumb-pdf",
        "unknown-thumb-pdf-hash",
    )
    .await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/unknown/unknown-thumb-pdf-hash/thumbnail?resize=300")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("resized unknown page hash pdf thumbnail request should build"),
        )
        .await
        .expect("resized unknown page hash pdf thumbnail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("resized unknown page hash pdf thumbnail body should be readable");
    let image = image::load_from_memory(&body)
        .expect("resized unknown page hash pdf thumbnail should decode as image");
    assert_eq!(image.width().max(image.height()), 300);
}

#[tokio::test]
async fn router_get_page_hash_unknown_thumbnail_returns_not_found_when_match_is_missing_like_kotlin()
 {
    let ctx = TestFixture::new("router-page-hash-unknown-thumbnail-missing-match").await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/unknown/missing-match-hash/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing-match unknown page hash thumbnail request should build"),
        )
        .await
        .expect("missing-match unknown page hash thumbnail request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn router_get_page_hash_unknown_thumbnail_returns_not_found_when_page_source_is_missing_like_kotlin()
 {
    let ctx = TestFixture::new("router-page-hash-unknown-thumbnail-missing-source").await;
    let source_path = seed_unknown_page_hash_source(
        ctx.paths(),
        "book-missing-source",
        "missing-source-hash",
        "images/missing-source.png",
        "missing-source.png",
        "image/png",
        &large_png_bytes(64, 64),
    )
    .await;
    std::fs::remove_file(&source_path).expect("missing-source fixture should be removable");

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/unknown/missing-source-hash/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing-source unknown page hash thumbnail request should build"),
        )
        .await
        .expect("missing-source unknown page hash thumbnail request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

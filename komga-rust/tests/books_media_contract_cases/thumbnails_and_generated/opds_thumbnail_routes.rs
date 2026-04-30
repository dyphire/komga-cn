use super::*;

#[tokio::test]
async fn router_opds_book_thumbnail_routes_convert_selected_png_to_jpeg() {
    let ctx = TestFixture::builder("router-opds-book-thumbnail-jpeg")
        .with_seed(|paths| async move {
            seed_book_thumbnail_bytes(&paths, "thumb-book-1", "image/png", &fixture_png_bytes())
                .await;
        })
        .build()
        .await;

    let auth_token = ctx.login_admin().await;

    for route in [
        "/opds/v1.2/books/book-1/thumbnail",
        "/opds/v2/books/book-1/thumbnail",
    ] {
        let response = ctx
            .app()
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("opds book thumbnail request should build"),
            )
            .await
            .expect("opds book thumbnail request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("image/jpeg"),
            "route: {route}"
        );
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("opds book thumbnail response body should be readable");
        assert_eq!(
            image::guess_format(body.as_ref()).expect("opds book thumbnail body should decode"),
            image::ImageFormat::Jpeg,
            "route: {route}"
        );
    }
}

#[tokio::test]
async fn router_opds_v2_book_thumbnail_unauthorized_returns_opds_auth_document() {
    let ctx = TestFixture::new("router-opds-v2-book-thumbnail-auth-doc").await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/books/book-1/thumbnail")
                .body(Body::empty())
                .expect("opds v2 book thumbnail unauthorized request should build"),
        )
        .await
        .expect("opds v2 book thumbnail unauthorized request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"Realm\"")
    );
    assert!(
        response
            .headers()
            .get(header::LINK)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.contains("application/opds-authentication+json"))
    );
    assert!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.contains("application/opds-authentication+json"))
    );
}

#[tokio::test]
async fn router_opds_v1_book_thumbnail_routes_unauthorized_include_basic_challenge() {
    let ctx = TestFixture::new("router-opds-v1-book-thumbnail-basic-challenge").await;

    for route in [
        "/opds/v1.2/books/book-1/thumbnail",
        "/opds/v1.2/books/book-1/thumbnail/small",
    ] {
        let response = ctx
            .app()
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .body(Body::empty())
                    .expect("opds v1 book thumbnail unauthorized request should build"),
            )
            .await
            .expect("opds v1 book thumbnail unauthorized request should complete");

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
            response.headers().get(header::LINK).is_none(),
            "route: {route}"
        );
    }
}

#[tokio::test]
async fn router_opds_v1_book_thumbnail_small_resizes_selected_png_to_jpeg() {
    let ctx = TestFixture::new("router-opds-v1-book-thumbnail-small-resize").await;
    let large_png = distinct_png_bytes(640, 480, 0, 0, 255);
    seed_book_thumbnail_bytes(ctx.paths(), "thumb-book-1", "image/png", &large_png).await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/books/book-1/thumbnail/small")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 book thumbnail small request should build"),
        )
        .await
        .expect("opds v1 book thumbnail small request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    let small_body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("opds v1 book thumbnail small body should be readable");
    let small_image =
        image::load_from_memory(small_body.as_ref()).expect("small thumbnail body should decode");
    assert_eq!(
        image::guess_format(small_body.as_ref())
            .expect("small thumbnail should expose image format"),
        image::ImageFormat::Jpeg
    );
    assert_eq!(small_image.width().max(small_image.height()), 300);

    let full_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/books/book-1/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 book thumbnail request should build"),
        )
        .await
        .expect("opds v1 book thumbnail request should complete");
    assert_eq!(full_response.status(), StatusCode::OK);
    let full_body = to_bytes(full_response.into_body(), usize::MAX)
        .await
        .expect("opds v1 book thumbnail body should be readable");
    let full_image =
        image::load_from_memory(full_body.as_ref()).expect("full thumbnail body should decode");

    assert!(
        full_image.width().max(full_image.height()) > small_image.width().max(small_image.height())
    );
}

#[tokio::test]
async fn router_opds_v1_book_thumbnail_small_returns_selected_generated_thumbnail_bytes() {
    let ctx = TestFixture::new("router-opds-v1-book-thumbnail-small-generated").await;
    write_router_epub_with_cover(ctx.paths(), "books/book-1.epub");

    let cleanup_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for small generated thumbnail cleanup");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&cleanup_pool)
        .await
        .expect("existing thumbnails should be deleted before small generated test");
    cleanup_pool.close().await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("pool for generate_book_thumbnail");
    generate_book_thumbnail(&pool, "book-1")
        .await
        .expect("generate_book_thumbnail should succeed before small generated test");

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for generated thumbnail lookup");
    let generated_thumbnail_id = sqlx::query(
        "SELECT ID FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? AND TYPE = 'GENERATED' LIMIT 1",
    )
    .bind("book-1")
    .fetch_one(&verify_pool)
    .await
    .expect("generated thumbnail row should be queryable")
    .get::<String, _>("ID");
    verify_pool.close().await;

    let tampered_jpeg = distinct_jpeg_bytes(4, 3, 255, 0, 0);
    seed_book_thumbnail_bytes(
        ctx.paths(),
        &generated_thumbnail_id,
        "image/jpeg",
        &tampered_jpeg,
    )
    .await;

    let auth_token = ctx.login_admin().await;

    let small = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/books/book-1/thumbnail/small")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 book thumbnail small generated request should build"),
        )
        .await
        .expect("opds v1 book thumbnail small generated request should complete");
    assert_eq!(small.status(), StatusCode::OK);
    assert_eq!(
        small
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    let small_body = to_bytes(small.into_body(), usize::MAX)
        .await
        .expect("opds v1 book thumbnail small generated body should be readable")
        .to_vec();
    assert_eq!(small_body, tampered_jpeg);

    let full = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/books/book-1/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 book thumbnail full generated request should build"),
        )
        .await
        .expect("opds v1 book thumbnail full generated request should complete");
    assert_eq!(full.status(), StatusCode::OK);
    let full_body = to_bytes(full.into_body(), usize::MAX)
        .await
        .expect("opds v1 book thumbnail full generated body should be readable")
        .to_vec();

    assert_ne!(full_body, tampered_jpeg);
}

#[tokio::test]
async fn router_opds_v1_book_thumbnail_small_falls_back_to_original_bytes_when_resize_fails() {
    let ctx = TestFixture::new("router-opds-v1-book-thumbnail-small-resize-fallback").await;
    let invalid_png = b"not-a-real-png-thumbnail".to_vec();
    seed_book_thumbnail_bytes(ctx.paths(), "thumb-book-1", "image/png", &invalid_png).await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/books/book-1/thumbnail/small")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 book thumbnail small fallback request should build"),
        )
        .await
        .expect("opds v1 book thumbnail small fallback request should complete");

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
        .expect("opds v1 book thumbnail small fallback body should be readable")
        .to_vec();
    assert_eq!(body, invalid_png);
}

#[tokio::test]
async fn router_opds_v2_book_thumbnail_ignores_mutated_generated_thumbnail_bytes() {
    for source in ["epub", "pdf"] {
        let (fixture_name, book_id, route) = match source {
            "epub" => (
                "router-opds-v2-book-thumbnail-epub-generated-source",
                "book-1",
                "/opds/v2/books/book-1/thumbnail",
            ),
            "pdf" => (
                "router-opds-v2-book-thumbnail-pdf-generated-source",
                "book-pdf-1",
                "/opds/v2/books/book-pdf-1/thumbnail",
            ),
            _ => unreachable!("unsupported source case"),
        };

        let ctx = TestFixture::new(fixture_name).await;

        match source {
            "epub" => write_router_epub_with_cover(ctx.paths(), "books/book-1.epub"),
            "pdf" => {
                seed_router_pdf_book(
                    ctx.paths(),
                    "book-pdf-1",
                    "series-1",
                    "fixture-page.pdf",
                    "Fixture PDF",
                )
                .await;
            }
            _ => unreachable!("unsupported source case"),
        }

        let cleanup_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
            .await
            .expect("main db should open for generated thumbnail cleanup");
        sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
            .bind(book_id)
            .execute(&cleanup_pool)
            .await
            .expect("existing thumbnails should be deleted before generated-source test");
        cleanup_pool.close().await;

        let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
            .await
            .expect("pool for generate_book_thumbnail");
        generate_book_thumbnail(&pool, book_id)
            .await
            .expect("generate_book_thumbnail should succeed before generated-source test");

        let auth_token = ctx.login_admin().await;

        let before = ctx
            .app()
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("opds v2 generated thumbnail baseline request should build"),
            )
            .await
            .expect("opds v2 generated thumbnail baseline request should complete");
        assert_eq!(before.status(), StatusCode::OK, "source: {source}");
        let before_body = to_bytes(before.into_body(), usize::MAX)
            .await
            .expect("opds v2 generated thumbnail baseline body should be readable")
            .to_vec();

        let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
            .await
            .expect("main db should open for generated thumbnail lookup");
        let generated_thumbnail_id = sqlx::query(
            "SELECT ID FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? AND TYPE = 'GENERATED' LIMIT 1",
        )
        .bind(book_id)
        .fetch_one(&verify_pool)
        .await
        .expect("generated thumbnail row should be queryable")
        .get::<String, _>("ID");
        verify_pool.close().await;

        let tampered_png = match source {
            "epub" => distinct_png_bytes(3, 2, 0, 255, 0),
            "pdf" => distinct_png_bytes(4, 3, 255, 0, 0),
            _ => unreachable!("unsupported source case"),
        };
        seed_book_thumbnail_bytes(
            ctx.paths(),
            &generated_thumbnail_id,
            "image/png",
            &tampered_png,
        )
        .await;

        let after = ctx
            .app()
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("opds v2 generated thumbnail request should build after tampering"),
            )
            .await
            .expect("opds v2 generated thumbnail request should complete after tampering");
        assert_eq!(after.status(), StatusCode::OK, "source: {source}");
        let after_body = to_bytes(after.into_body(), usize::MAX)
            .await
            .expect("opds v2 generated thumbnail response body should be readable after tampering")
            .to_vec();

        assert_eq!(after_body, before_body, "source: {source}");
    }
}

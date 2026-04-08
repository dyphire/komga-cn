use super::*;

async fn seed_book_thumbnail_bytes(
    paths: &RuntimeDbPaths,
    thumbnail_id: &str,
    media_type: &str,
    bytes: &[u8],
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for book thumbnail seed");
    sqlx::query("UPDATE THUMBNAIL_BOOK SET MEDIA_TYPE = ?, THUMBNAIL = ? WHERE ID = ?")
        .bind(media_type)
        .bind(bytes)
        .bind(thumbnail_id)
        .execute(&pool)
        .await
        .expect("book thumbnail row should be updated");
    pool.close().await;
}

fn distinct_png_bytes(width: u32, height: u32, red: u8, green: u8, blue: u8) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba([red, green, blue, 255]));
    let mut output = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut output, image::ImageFormat::Png)
        .expect("distinct png fixture should encode");
    output.into_inner()
}

fn distinct_jpeg_bytes(width: u32, height: u32, red: u8, green: u8, blue: u8) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba([red, green, blue, 255]));
    let mut output = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut output, image::ImageFormat::Jpeg)
        .expect("distinct jpeg fixture should encode");
    output.into_inner()
}

#[tokio::test]
async fn router_opds_book_thumbnail_routes_convert_selected_png_to_jpeg() {
    let paths = new_router_fixture("router-opds-book-thumbnail-jpeg").await;
    seed_router_contract_data(&paths).await;
    seed_book_thumbnail_bytes(&paths, "thumb-book-1", "image/png", &fixture_png_bytes()).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for route in [
        "/opds/v1.2/books/book-1/thumbnail",
        "/opds/v2/books/book-1/thumbnail",
    ] {
        let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_book_thumbnail_unauthorized_returns_opds_auth_document() {
    let paths = new_router_fixture("router-opds-v2-book-thumbnail-auth-doc").await;
    seed_router_contract_data(&paths).await;
    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_book_thumbnail_routes_unauthorized_include_basic_challenge() {
    let paths = new_router_fixture("router-opds-v1-book-thumbnail-basic-challenge").await;
    seed_router_contract_data(&paths).await;
    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    for route in [
        "/opds/v1.2/books/book-1/thumbnail",
        "/opds/v1.2/books/book-1/thumbnail/small",
    ] {
        let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_book_thumbnail_small_resizes_selected_png_to_jpeg() {
    let paths = new_router_fixture("router-opds-v1-book-thumbnail-small-resize").await;
    seed_router_contract_data(&paths).await;
    let large_png = distinct_png_bytes(640, 480, 0, 0, 255);
    seed_book_thumbnail_bytes(&paths, "thumb-book-1", "image/png", &large_png).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    let full_response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_book_thumbnail_small_returns_selected_generated_thumbnail_bytes() {
    let paths = new_router_fixture("router-opds-v1-book-thumbnail-small-generated").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_with_cover(&paths, "books/book-1.epub");

    let cleanup_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for small generated thumbnail cleanup");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&cleanup_pool)
        .await
        .expect("existing thumbnails should be deleted before small generated test");
    cleanup_pool.close().await;

    generate_book_thumbnail(paths.main_db.as_path(), "book-1")
        .expect("generate_book_thumbnail should succeed before small generated test");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
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
        &paths,
        &generated_thumbnail_id,
        "image/jpeg",
        &tampered_jpeg,
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let small = app
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

    let full = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_book_thumbnail_small_falls_back_to_original_bytes_when_resize_fails() {
    let paths = new_router_fixture("router-opds-v1-book-thumbnail-small-resize-fallback").await;
    seed_router_contract_data(&paths).await;
    let invalid_png = b"not-a-real-png-thumbnail".to_vec();
    seed_book_thumbnail_bytes(&paths, "thumb-book-1", "image/png", &invalid_png).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
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

        let paths = new_router_fixture(fixture_name).await;
        seed_router_contract_data(&paths).await;

        match source {
            "epub" => write_router_epub_with_cover(&paths, "books/book-1.epub"),
            "pdf" => {
                seed_router_pdf_book(
                    &paths,
                    "book-pdf-1",
                    "series-1",
                    "fixture-page.pdf",
                    "Fixture PDF",
                )
                .await;
            }
            _ => unreachable!("unsupported source case"),
        }

        let cleanup_pool = connect_pool(paths.main_db.as_path(), 1)
            .await
            .expect("main db should open for generated thumbnail cleanup");
        sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
            .bind(book_id)
            .execute(&cleanup_pool)
            .await
            .expect("existing thumbnails should be deleted before generated-source test");
        cleanup_pool.close().await;

        generate_book_thumbnail(paths.main_db.as_path(), book_id)
            .expect("generate_book_thumbnail should succeed before generated-source test");

        let app = build_router_with_config(&runtime_config_for_paths(&paths));
        let auth_token = login_with_basic_and_get_token(app.clone()).await;

        let before = app
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

        let verify_pool = connect_pool(paths.main_db.as_path(), 1)
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
        seed_book_thumbnail_bytes(&paths, &generated_thumbnail_id, "image/png", &tampered_png)
            .await;

        let after = app
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

        cleanup_router_fixture(paths);
    }
}

#[tokio::test]
async fn router_book_thumbnail_upload_parses_multipart_image_and_selected_flag() {
    let paths = new_router_fixture("router-book-thumbnail-upload-multipart").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "cover.png", "image/png", false, &image_bytes);

    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/book-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("book thumbnail upload request should build"),
        )
        .await
        .expect("book thumbnail upload request should complete");

    assert_eq!(upload.status(), StatusCode::OK);
    let payload = response_json(upload).await;
    assert_eq!(
        payload.get("bookId"),
        Some(&Value::String("book-1".to_string()))
    );
    assert_eq!(
        payload.get("type"),
        Some(&Value::String("USER_UPLOADED".to_string()))
    );
    assert!(
        payload.get("id").and_then(Value::as_str).is_some(),
        "book thumbnail upload should return thumbnail id"
    );
    assert_eq!(payload.get("selected"), Some(&Value::Bool(false)));
    assert_eq!(
        payload.get("mediaType"),
        Some(&Value::String("image/png".to_string()))
    );
    assert_eq!(
        payload.get("fileSize"),
        Some(&json!(image_bytes.len() as i64))
    );
    assert_eq!(payload.get("width"), Some(&json!(1)));
    assert_eq!(payload.get("height"), Some(&json!(1)));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_thumbnail_upload_selects_thumbnail_when_none_was_selected() {
    let paths = new_router_fixture("router-book-thumbnail-upload-auto-selects-first").await;
    seed_router_contract_data(&paths).await;

    let cleanup_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for book thumbnail cleanup");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&cleanup_pool)
        .await
        .expect("existing book-1 thumbnails should be deleted before upload test");
    cleanup_pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "cover.png", "image/png", false, &image_bytes);

    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/book-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("book thumbnail upload request should build"),
        )
        .await
        .expect("book thumbnail upload request should complete");

    assert_eq!(upload.status(), StatusCode::OK);

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for selected thumbnail verification");
    let selected_count = sqlx::query(
        "SELECT COUNT(*) AS COUNT FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? AND SELECTED = 1",
    )
    .bind("book-1")
    .fetch_one(&verify_pool)
    .await
    .expect("selected book thumbnails should be queryable")
    .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(selected_count, 1);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_thumbnail_by_id_allows_missing_path_book_when_thumbnail_exists() {
    let paths = new_router_fixture("router-book-thumbnail-by-id-missing-path-book").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "cover.png", "image/png", false, &image_bytes);

    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/book-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("book thumbnail upload request should build"),
        )
        .await
        .expect("book thumbnail upload request should complete");

    let thumbnail_id = response_json(upload)
        .await
        .get("id")
        .and_then(Value::as_str)
        .expect("book thumbnail upload should return thumbnail id")
        .to_string();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/v1/books/missing-book/thumbnails/{thumbnail_id}"
                ))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book thumbnail missing path request should build"),
        )
        .await
        .expect("book thumbnail missing path request should complete");

    assert_eq!(response.status(), StatusCode::OK);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_thumbnail_delete_allows_missing_path_book_when_thumbnail_exists() {
    let paths = new_router_fixture("router-book-thumbnail-delete-missing-path-book").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "cover.png", "image/png", false, &image_bytes);

    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/book-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("book thumbnail upload request should build"),
        )
        .await
        .expect("book thumbnail upload request should complete");
    assert_eq!(upload.status(), StatusCode::OK);
    let thumbnail_id = response_json(upload)
        .await
        .get("id")
        .and_then(Value::as_str)
        .expect("book thumbnail upload should return thumbnail id")
        .to_string();

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!(
                    "/api/v1/books/missing-book/thumbnails/{thumbnail_id}"
                ))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book thumbnail missing path delete request should build"),
        )
        .await
        .expect("book thumbnail missing path delete request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for missing path delete verification");
    let remaining = sqlx::query("SELECT COUNT(*) AS COUNT FROM THUMBNAIL_BOOK WHERE ID = ?")
        .bind(&thumbnail_id)
        .fetch_one(&verify_pool)
        .await
        .expect("book thumbnail delete should be queryable")
        .get::<i64, _>("COUNT");
    verify_pool.close().await;
    assert_eq!(remaining, 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_thumbnail_delete_rejects_generated_thumbnail() {
    let paths = new_router_fixture("router-book-thumbnail-delete-generated").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_with_cover(&paths, "books/book-1.epub");

    let cleanup_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for generated thumbnail cleanup");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&cleanup_pool)
        .await
        .expect("existing thumbnails should be deleted before generated delete test");
    cleanup_pool.close().await;

    generate_book_thumbnail(paths.main_db.as_path(), "book-1")
        .expect("generate_book_thumbnail should succeed before delete test");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!(
                    "/api/v1/books/book-1/thumbnails/{generated_thumbnail_id}"
                ))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("generated book thumbnail delete request should build"),
        )
        .await
        .expect("generated book thumbnail delete request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_thumbnail_delete_reselects_remaining_thumbnail_when_selected_one_is_removed() {
    let paths = new_router_fixture("router-book-thumbnail-delete-reselects-remaining").await;
    seed_router_contract_data(&paths).await;

    let cleanup_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for book thumbnail delete cleanup");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&cleanup_pool)
        .await
        .expect("existing book-1 thumbnails should be deleted before delete reselect test");
    cleanup_pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();

    let mut selected_thumbnail_id = String::new();
    for (selected, name) in [(true, "selected.png"), (false, "other.png")] {
        let (content_type, body) =
            multipart_image_upload_body("file", name, "image/png", selected, &image_bytes);
        let upload = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/books/book-1/thumbnails")
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, content_type)
                    .body(Body::from(body))
                    .expect("book thumbnail upload request should build"),
            )
            .await
            .expect("book thumbnail upload request should complete");
        assert_eq!(upload.status(), StatusCode::OK);
        let thumbnail_id = response_json(upload)
            .await
            .get("id")
            .and_then(Value::as_str)
            .expect("uploaded book thumbnail should expose id")
            .to_string();
        if selected {
            selected_thumbnail_id = thumbnail_id;
        }
    }

    let delete = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!(
                    "/api/v1/books/book-1/thumbnails/{selected_thumbnail_id}"
                ))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book selected thumbnail delete request should build"),
        )
        .await
        .expect("book selected thumbnail delete request should complete");
    assert_eq!(delete.status(), StatusCode::ACCEPTED);

    let list = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book thumbnail list request should build"),
        )
        .await
        .expect("book thumbnail list request should complete");
    assert_eq!(list.status(), StatusCode::OK);
    let rows = response_json(list).await;
    let rows = rows
        .as_array()
        .expect("book thumbnail list response should be an array");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("selected"), Some(&Value::Bool(true)));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn generate_book_thumbnail_persists_generated_thumbnail_for_epub_cover() {
    let paths = new_router_fixture("router-generate-book-thumbnail-epub").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_with_cover(&paths, "books/book-1.epub");

    let cleanup_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for epub thumbnail cleanup");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&cleanup_pool)
        .await
        .expect("existing book-1 thumbnails should be deleted before epub cover test");
    cleanup_pool.close().await;

    let media = BookMediaRecord {
        library_id: "library-1".to_string(),
        media_type: "application/epub+zip".to_string(),
        file_path: paths.config_dir.join("books/book-1.epub"),
        file_name: "book-1.epub".to_string(),
        page_count: 10,
    };
    let (cover_bytes, cover_media_type) =
        load_epub_cover_bytes(&media).expect("epub cover bytes should be extractable");
    assert!(!cover_bytes.is_empty());
    assert_eq!(cover_media_type, "image/png");

    generate_book_thumbnail(paths.main_db.as_path(), "book-1")
        .expect("generate_book_thumbnail should execute successfully for epub cover");

    let main_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for epub generated thumbnail verification");
    let generated = sqlx::query(
        "SELECT TYPE, MEDIA_TYPE, WIDTH, HEIGHT FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? ORDER BY ID ASC",
    )
    .bind("book-1")
    .fetch_all(&main_pool)
    .await
    .expect("epub generated thumbnail rows should be queryable");
    main_pool.close().await;
    assert_eq!(generated.len(), 1);
    let generated_row = generated
        .iter()
        .find(|row| row.get::<String, _>("TYPE") == "GENERATED")
        .expect("epub generated thumbnail row should exist");
    assert_eq!(generated_row.get::<String, _>("MEDIA_TYPE"), "image/jpeg");

    let runtime_config = runtime_config_for_paths(&paths);
    let app = build_router_with_config(&runtime_config);
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let after = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("epub book thumbnail request should build after generate task"),
        )
        .await
        .expect("epub book thumbnail request should complete after generate task");
    assert_eq!(after.status(), StatusCode::OK);
    assert_eq!(
        after
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );

    let thumbnails = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("epub book thumbnails request should build after generate task"),
        )
        .await
        .expect("epub book thumbnails request should complete after generate task");
    assert_eq!(thumbnails.status(), StatusCode::OK);
    let payload = response_json(thumbnails).await;
    assert_eq!(
        payload
            .as_array()
            .and_then(|items| items.first())
            .and_then(|item| item.get("type")),
        Some(&Value::String("GENERATED".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn generate_book_thumbnail_persists_generated_thumbnail_for_pdf() {
    let paths = new_router_fixture("router-generate-book-thumbnail-pdf").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Fixture PDF",
    )
    .await;

    generate_book_thumbnail(paths.main_db.as_path(), "book-pdf-1")
        .expect("generate_book_thumbnail should execute successfully for pdf");

    let main_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for pdf generated thumbnail verification");
    let generated = sqlx::query(
        "SELECT TYPE, MEDIA_TYPE, WIDTH, HEIGHT FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? ORDER BY ID ASC",
    )
    .bind("book-pdf-1")
    .fetch_all(&main_pool)
    .await
    .expect("pdf generated thumbnail rows should be queryable");
    main_pool.close().await;
    assert_eq!(generated.len(), 1);
    let generated_row = generated
        .iter()
        .find(|row| row.get::<String, _>("TYPE") == "GENERATED")
        .expect("pdf generated thumbnail row should exist");
    assert_eq!(generated_row.get::<String, _>("MEDIA_TYPE"), "image/jpeg");
    assert!(generated_row.get::<i64, _>("WIDTH") > 0);
    assert!(generated_row.get::<i64, _>("HEIGHT") > 0);

    let runtime_config = runtime_config_for_paths(&paths);
    let app = build_router_with_config(&runtime_config);
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let after = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("pdf book thumbnail request should build after generate task"),
        )
        .await
        .expect("pdf book thumbnail request should complete after generate task");
    assert_eq!(after.status(), StatusCode::OK);
    assert_eq!(
        after
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );

    let thumbnails = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("pdf book thumbnails request should build after generate task"),
        )
        .await
        .expect("pdf book thumbnails request should complete after generate task");
    assert_eq!(thumbnails.status(), StatusCode::OK);
    let payload = response_json(thumbnails).await;
    assert_eq!(
        payload
            .as_array()
            .and_then(|items| items.first())
            .and_then(|item| item.get("type")),
        Some(&Value::String("GENERATED".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_thumbnails_returns_empty_array_for_existing_book_without_posters() {
    let paths = new_router_fixture("router-book-thumbnails-empty-array").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-2/thumbnails")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book thumbnails empty request should build"),
        )
        .await
        .expect("book thumbnails empty request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload, json!([]));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_thumbnail_returns_not_found_for_existing_single_image_without_poster() {
    let paths = new_router_fixture("router-book-thumbnail-single-image-no-poster").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for single-image thumbnail fixture");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-image-1")
    .bind(0_i64)
    .bind("cover.png")
    .bind("books/cover.png")
    .bind("series-1")
    .bind(1_i64)
    .bind(5_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("single-image book row should be inserted");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("image/png")
        .bind("READY")
        .bind("book-image-1")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("single-image media row should be inserted");
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("5")
    .bind(5.0_f64)
    .bind("Cover Book")
    .bind("2024-02-02")
    .bind("book-image-1")
    .execute(&pool)
    .await
    .expect("single-image book metadata row should be inserted");
    pool.close().await;

    let image_path = paths.config_dir.join("books/cover.png");
    if let Some(parent) = image_path.parent() {
        std::fs::create_dir_all(parent).expect("single-image parent directory should be created");
    }
    std::fs::write(&image_path, fixture_png_bytes())
        .expect("single-image fixture should be written");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-image-1/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("single-image thumbnail request should build"),
        )
        .await
        .expect("single-image thumbnail request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_media_asset_routes_forbid_age_restricted_user() {
    let paths = new_router_fixture("router-book-media-asset-restricted-user").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
        &["USER", "PAGE_STREAMING", "FILE_DOWNLOAD"],
    )
    .await;
    write_router_epub_resource(
        &paths,
        "books/book-1.epub",
        "OEBPS/chapter.xhtml",
        br#"<html xmlns='http://www.w3.org/1999/xhtml'><body>Restricted</body></html>"#,
    );

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;

    for route in [
        "/api/v1/books/book-1/file",
        "/api/v1/books/book-1/thumbnails",
        "/api/v1/books/book-1/manifest",
        "/api/v1/books/book-1/resource/OEBPS/chapter.xhtml",
        "/api/v1/books/book-1/progression",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("restricted media asset get request should build"),
            )
            .await
            .expect("restricted media asset get request should complete");

        assert_eq!(response.status(), StatusCode::FORBIDDEN, "route: {route}");
    }

    for route in ["/api/v1/books/book-1/progression"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "locator": {
                                "locations": {
                                    "progression": 0.25
                                }
                            }
                        })
                        .to_string(),
                    ))
                    .expect("restricted media asset put request should build"),
            )
            .await
            .expect("restricted media asset put request should complete");

        assert_eq!(response.status(), StatusCode::FORBIDDEN, "route: {route}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_file_delete_enqueues_delete_book_even_when_book_is_missing() {
    let paths = new_router_fixture("router-book-file-delete-missing-book").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/books/missing-book/file")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing book file delete request should build"),
        )
        .await
        .expect("missing book file delete request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for missing book file delete verification");
    let rows = sqlx::query("SELECT ID, SIMPLE_TYPE, GROUP_ID FROM TASK ORDER BY ID ASC")
        .fetch_all(&tasks_pool)
        .await
        .expect("missing book delete task rows should be queryable");
    tasks_pool.close().await;

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<String, _>("ID"), "DELETE_BOOK:missing-book");
    assert_eq!(rows[0].get::<String, _>("SIMPLE_TYPE"), "DELETE_BOOK");
    assert_eq!(
        rows[0].get::<Option<String>, _>("GROUP_ID"),
        Some("missing-book".to_string())
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_thumbnail_upload_rejects_invalid_selected_flag() {
    let paths = new_router_fixture("router-book-thumbnail-upload-invalid-selected").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let boundary = "komga-rust-invalid-selected-boundary";
    let mut body = Vec::new();
    use std::io::Write as _;
    write!(
        &mut body,
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"cover.png\"\r\nContent-Type: image/png\r\n\r\n"
    )
    .expect("multipart invalid-selected file prelude should be written");
    body.extend_from_slice(&image_bytes);
    write!(
        &mut body,
        "\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"selected\"\r\n\r\nmaybe\r\n--{boundary}--\r\n"
    )
    .expect("multipart invalid-selected field should be written");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/book-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .expect("invalid selected thumbnail upload request should build"),
        )
        .await
        .expect("invalid selected thumbnail upload request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "book thumbnail selected field must be true or false".to_string()
        ))
    );

    cleanup_router_fixture(paths);
}

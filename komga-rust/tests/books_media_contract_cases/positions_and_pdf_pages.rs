use super::*;

#[tokio::test]
async fn router_book_pages_single_image_fallback_includes_dimensions() {
    let ctx = TestFixture::new("router-book-pages-single-image-dimensions").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for single-image page fixture");
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

    let image_path = ctx.paths().config_dir.join("books/cover.png");
    if let Some(parent) = image_path.parent() {
        std::fs::create_dir_all(parent).expect("single-image parent directory should be created");
    }
    std::fs::write(&image_path, fixture_png_bytes())
        .expect("single-image fixture should be written");

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-image-1/pages")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("single-image pages request should build"),
        )
        .await
        .expect("single-image pages request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let rows = payload
        .as_array()
        .expect("single-image pages payload should be an array");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("width"), Some(&json!(1)));
    assert_eq!(rows[0].get("height"), Some(&json!(1)));
}

#[tokio::test]
async fn router_book_pages_single_image_fallback_reports_non_file_media_paths() {
    let ctx = TestFixture::new("router-book-pages-single-image-directory").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for single-image directory fixture");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-image-directory-1")
    .bind(0_i64)
    .bind("cover-directory.png")
    .bind("books/cover-directory.png")
    .bind("series-1")
    .bind(1_i64)
    .bind(6_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("single-image directory book row should be inserted");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("image/png")
        .bind("READY")
        .bind("book-image-directory-1")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("single-image directory media row should be inserted");
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("6")
    .bind(6.0_f64)
    .bind("Directory Cover Book")
    .bind("2024-02-03")
    .bind("book-image-directory-1")
    .execute(&pool)
    .await
    .expect("single-image directory metadata row should be inserted");
    pool.close().await;

    let image_path = ctx.paths().config_dir.join("books/cover-directory.png");
    std::fs::create_dir_all(&image_path).expect("single-image directory should be created");

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-image-directory-1/pages")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("single-image directory pages request should build"),
        )
        .await
        .expect("single-image directory pages request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn router_book_positions_follow_direct_basic_auth_and_book_visibility() {
    let ctx = TestFixture::new("router-book-positions-basic-auth-visibility").await;
    seed_router_age_exclude_user_with_roles(
        ctx.paths(),
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
        &["USER", "PAGE_STREAMING"],
    )
    .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for basic-auth positions seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub positions extension should be seeded for positions auth test");
    pool.close().await;

    let admin_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/positions")
                .header(
                    header::AUTHORIZATION,
                    basic_authorization_header_value(
                        "admin@example.org",
                        "router-contract-admin-123",
                    ),
                )
                .header("x-auth-token", "")
                .body(Body::empty())
                .expect("admin basic-auth positions request should build"),
        )
        .await
        .expect("admin basic-auth positions request should complete");
    assert_eq!(admin_response.status(), StatusCode::OK);

    let restricted_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/positions")
                .header(
                    header::AUTHORIZATION,
                    basic_authorization_header_value(
                        "restricted@example.org",
                        "router-contract-restricted-123",
                    ),
                )
                .header("x-auth-token", "")
                .body(Body::empty())
                .expect("restricted basic-auth positions request should build"),
        )
        .await
        .expect("restricted basic-auth positions request should complete");
    assert_eq!(restricted_response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn router_book_positions_requires_page_streaming_role_even_for_admins() {
    let ctx = TestFixture::new("router-book-positions-page-streaming-role").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for positions role seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub positions extension should be seeded for positions role test");

    let password = "router-contract-admin-only-123";
    sqlx::query(
        "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES) \
         VALUES (?, ?, ?, ?)",
    )
    .bind("admin-only-user")
    .bind("admin-only@example.org")
    .bind(hash_router_contract_password(password))
    .bind(true)
    .execute(&pool)
    .await
    .expect("admin-only user should be inserted");
    for role in ["USER", "ADMIN"] {
        sqlx::query("INSERT INTO USER_ROLE (USER_ID, ROLE) VALUES (?, ?)")
            .bind("admin-only-user")
            .bind(role)
            .execute(&pool)
            .await
            .expect("admin-only role should be inserted");
    }
    pool.close().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/positions")
                .header(
                    header::AUTHORIZATION,
                    basic_authorization_header_value("admin-only@example.org", password),
                )
                .header("x-auth-token", "")
                .body(Body::empty())
                .expect("admin-only positions request should build"),
        )
        .await
        .expect("admin-only positions request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn router_book_raw_page_returns_bad_request_with_message_for_non_pdf_media() {
    let ctx = TestFixture::new("router-book-raw-page-single-image").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for single-image raw fixture");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-image-raw-1")
    .bind(0_i64)
    .bind("cover.png")
    .bind("books/cover-raw.png")
    .bind("series-1")
    .bind(1_i64)
    .bind(6_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("single-image raw book row should be inserted");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("image/png")
        .bind("READY")
        .bind("book-image-raw-1")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("single-image raw media row should be inserted");
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("6")
    .bind(6.0_f64)
    .bind("Cover Raw Book")
    .bind("2024-02-03")
    .bind("book-image-raw-1")
    .execute(&pool)
    .await
    .expect("single-image raw metadata row should be inserted");
    pool.close().await;

    let image_path = ctx.paths().config_dir.join("books/cover-raw.png");
    if let Some(parent) = image_path.parent() {
        std::fs::create_dir_all(parent)
            .expect("single-image raw parent directory should be created");
    }
    let image_bytes = fixture_png_bytes();
    std::fs::write(&image_path, &image_bytes).expect("single-image raw fixture should be written");

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-image-raw-1/pages/1/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("single-image raw page request should build"),
        )
        .await
        .expect("single-image raw page request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "Extractor does not support raw extraction of pages".to_string()
        ))
    );
}

#[tokio::test]
async fn router_book_raw_page_returns_bad_request_for_non_pdf_media_even_when_not_ready() {
    let ctx = TestFixture::new("router-book-raw-page-single-image-not-ready").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for single-image raw not-ready fixture");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-image-raw-not-ready-1")
    .bind(0_i64)
    .bind("cover.png")
    .bind("books/cover-raw-not-ready.png")
    .bind("series-1")
    .bind(1_i64)
    .bind(7_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("single-image raw not-ready book row should be inserted");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("image/png")
        .bind("OUTDATED")
        .bind("book-image-raw-not-ready-1")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("single-image raw not-ready media row should be inserted");
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("7")
    .bind(7.0_f64)
    .bind("Cover Raw Not Ready Book")
    .bind("2024-02-04")
    .bind("book-image-raw-not-ready-1")
    .execute(&pool)
    .await
    .expect("single-image raw not-ready metadata row should be inserted");
    pool.close().await;

    let image_path = ctx.paths().config_dir.join("books/cover-raw-not-ready.png");
    if let Some(parent) = image_path.parent() {
        std::fs::create_dir_all(parent)
            .expect("single-image raw not-ready parent directory should be created");
    }
    std::fs::write(&image_path, fixture_png_bytes())
        .expect("single-image raw not-ready fixture should be written");

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-image-raw-not-ready-1/pages/1/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("single-image raw not-ready page request should build"),
        )
        .await
        .expect("single-image raw not-ready page request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "Extractor does not support raw extraction of pages".to_string()
        ))
    );
}

#[tokio::test]
async fn router_book_raw_page_returns_bad_request_for_non_pdf_media_before_missing_file() {
    let ctx = TestFixture::new("router-book-raw-page-single-image-file-missing").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for single-image raw missing-file fixture");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-image-raw-missing-file-1")
    .bind(0_i64)
    .bind("cover.png")
    .bind("books/cover-raw-missing.png")
    .bind("series-1")
    .bind(1_i64)
    .bind(8_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("single-image raw missing-file book row should be inserted");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("image/png")
        .bind("READY")
        .bind("book-image-raw-missing-file-1")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("single-image raw missing-file media row should be inserted");
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("8")
    .bind(8.0_f64)
    .bind("Cover Raw Missing File Book")
    .bind("2024-02-05")
    .bind("book-image-raw-missing-file-1")
    .execute(&pool)
    .await
    .expect("single-image raw missing-file metadata row should be inserted");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-image-raw-missing-file-1/pages/1/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("single-image raw missing-file page request should build"),
        )
        .await
        .expect("single-image raw missing-file page request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "Extractor does not support raw extraction of pages".to_string()
        ))
    );
}

#[tokio::test]
async fn router_book_pages_without_persisted_pdf_rows_returns_empty_like_kotlin() {
    let ctx = TestFixture::new("router-book-pages-pdf-dimensions").await;
    seed_router_pdf_book(
        ctx.paths(),
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Fixture PDF",
    )
    .await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("pdf pages request should build"),
        )
        .await
        .expect("pdf pages request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let rows = payload
        .as_array()
        .expect("pdf pages payload should be an array");
    assert!(rows.is_empty());
}

#[tokio::test]
async fn router_persisted_pdf_page_uses_jpeg_delivery_metadata() {
    let ctx = TestFixture::builder("router-persisted-pdf-page-jpeg")
        .with_seed(|paths| async move {
            seed_router_pdf_book(
                &paths,
                "book-pdf-1",
                "series-1",
                "fixture-page.pdf",
                "Fixture PDF",
            )
            .await;
            seed_router_persisted_pdf_page(
                &paths,
                "book-pdf-1",
                0,
                "page-0000.pdf",
                595,
                842,
                None,
            )
            .await;
        })
        .build()
        .await;

    let auth_token = ctx.login_admin().await;
    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("persisted pdf page request should build"),
        )
        .await
        .expect("persisted pdf page request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    assert!(
        response
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.contains(".jpeg")),
        "pdf page should advertise a jpeg filename"
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("persisted pdf page body should be readable");
    let image =
        image::load_from_memory(&body).expect("persisted pdf page body should decode as image");
    assert_eq!(image.width(), 2_479);
    assert_eq!(image.height(), 3_508);
}

#[tokio::test]
async fn router_book_page_pdf_negotiation_returns_pdf_only_when_requested() {
    let ctx = TestFixture::new("router-book-page-pdf-negotiation").await;
    seed_router_pdf_book(
        ctx.paths(),
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Fixture PDF",
    )
    .await;

    let auth_token = ctx.login_admin().await;
    let pdf_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1")
                .header("x-auth-token", &auth_token)
                .header(header::ACCEPT, "application/pdf")
                .body(Body::empty())
                .expect("negotiated pdf page request should build"),
        )
        .await
        .expect("negotiated pdf page request should complete");
    assert_eq!(pdf_response.status(), StatusCode::OK);
    assert_eq!(
        pdf_response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/pdf")
    );
    let pdf_body = to_bytes(pdf_response.into_body(), usize::MAX)
        .await
        .expect("negotiated pdf page body should be readable");
    lopdf::Document::load_mem(&pdf_body).expect("negotiated pdf page should be valid pdf");

    let image_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1?contentNegotiation=false")
                .header("x-auth-token", &auth_token)
                .header(header::ACCEPT, "application/pdf")
                .body(Body::empty())
                .expect("disabled pdf negotiation request should build"),
        )
        .await
        .expect("disabled pdf negotiation request should complete");
    assert_eq!(image_response.status(), StatusCode::OK);
    assert_eq!(
        image_response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    let image_body = to_bytes(image_response.into_body(), usize::MAX)
        .await
        .expect("disabled pdf negotiation body should be readable");
    image::load_from_memory(&image_body)
        .expect("disabled pdf negotiation body should decode as image");
}

#[tokio::test]
async fn router_book_page_returns_bad_request_with_message_for_missing_pdf_page_number() {
    let ctx = TestFixture::new("router-book-page-missing-pdf-page-nonraw").await;
    seed_router_pdf_book(
        ctx.paths(),
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/2")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing nonraw pdf page request should build"),
        )
        .await
        .expect("missing nonraw pdf page request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Page number does not exist".to_string()))
    );
}

#[tokio::test]
async fn router_book_page_pdf_negotiation_returns_bad_request_with_message_for_missing_pdf_page_number()
 {
    let ctx = TestFixture::new("router-book-page-missing-pdf-page-nonraw-pdf-negotiation").await;
    seed_router_pdf_book(
        ctx.paths(),
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/2")
                .header("x-auth-token", &auth_token)
                .header(header::ACCEPT, "application/pdf")
                .body(Body::empty())
                .expect("missing negotiated nonraw pdf page request should build"),
        )
        .await
        .expect("missing negotiated nonraw pdf page request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Page number does not exist".to_string()))
    );
}

#[tokio::test]
async fn router_book_positions_returns_not_found_without_epub_extension_positions() {
    let ctx = TestFixture::new("router-book-positions-no-extension").await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/positions")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book positions request should build"),
        )
        .await
        .expect("book positions request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn router_book_positions_does_not_return_not_modified_when_positions_are_missing() {
    let ctx = TestFixture::new("router-book-positions-no-extension-not-modified").await;
    write_router_epub_with_cover(ctx.paths(), "books/book-1.epub");

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/positions")
                .header("x-auth-token", &auth_token)
                .header(header::IF_MODIFIED_SINCE, "Wed, 31 Dec 2099 23:59:59 GMT")
                .body(Body::empty())
                .expect("book positions conditional missing request should build"),
        )
        .await
        .expect("book positions conditional missing request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn router_book_progression_get_returns_full_r2_progression_shape() {
    let ctx = TestFixture::new("router-book-progression-get-full-shape").await;

    let locator = json!({
        "href": "/book-1.xhtml#kobo.2.1",
        "type": "application/xhtml+xml",
        "title": "Chapter 2",
        "locations": {
            "position": 2,
            "progression": 0.5,
            "totalProgression": 0.2
        },
        "text": {
            "highlight": "Some text"
        },
        "koboSpan": "kobo-span-2"
    });

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for progression shape seed");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED, READ_DATE, DEVICE_ID, DEVICE_NAME, LOCATOR) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(2_i64)
    .bind(false)
    .bind("2024-01-02 03:04:05")
    .bind("reader-1")
    .bind("KOReader")
    .bind(serde_json::to_vec(&locator).expect("locator should serialize"))
    .execute(&pool)
    .await
    .expect("read progress row for progression shape should insert");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book progression request should build"),
        )
        .await
        .expect("book progression request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("modified"),
        Some(&Value::String("2024-01-02T03:04:05Z".to_string()))
    );
    assert_eq!(
        payload.get("device"),
        Some(&json!({
            "id": "reader-1",
            "name": "KOReader"
        }))
    );
    assert_eq!(payload.get("locator"), Some(&locator));
}

#[tokio::test]
async fn router_book_positions_returns_epub_extension_positions_and_supports_not_modified() {
    let ctx = TestFixture::new("router-book-positions-epub-extension").await;
    write_router_epub_with_cover(ctx.paths(), "books/book-1.epub");

    let positions = json!([
        {
            "href": "/book-1.xhtml#kobo.1.1",
            "type": "application/xhtml+xml",
            "locations": {
                "position": 1,
                "progression": 0.0,
                "totalProgression": 0.1
            }
        },
        {
            "href": "/book-1.xhtml#kobo.2.1",
            "type": "application/xhtml+xml",
            "locations": {
                "position": 2,
                "progression": 0.5,
                "totalProgression": 0.2
            }
        }
    ]);
    let extension_blob = fixture_epub_positions_extension_blob();

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for epub extension positions seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(extension_blob)
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let initial = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/positions")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book positions initial request should build"),
        )
        .await
        .expect("book positions initial request should complete");

    assert_eq!(initial.status(), StatusCode::OK);
    assert_eq!(
        initial
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/vnd.readium.position-list+json")
    );
    let last_modified = initial
        .headers()
        .get(header::LAST_MODIFIED)
        .and_then(|value| value.to_str().ok())
        .expect("book positions response should expose last-modified")
        .to_string();
    let payload = response_json(initial).await;
    assert_eq!(payload.get("total"), Some(&Value::from(2)));
    assert_eq!(payload.get("positions"), Some(&positions));

    let not_modified = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/positions")
                .header("x-auth-token", &auth_token)
                .header(header::IF_MODIFIED_SINCE, &last_modified)
                .body(Body::empty())
                .expect("book positions conditional request should build"),
        )
        .await
        .expect("book positions conditional request should complete");

    assert_eq!(not_modified.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(
        not_modified
            .headers()
            .get(header::LAST_MODIFIED)
            .and_then(|value| value.to_str().ok()),
        Some(last_modified.as_str())
    );
}

#[tokio::test]
async fn router_book_pages_persisted_pdf_rows_match_kotlin_dynamic_page_shape() {
    let ctx = TestFixture::builder("router-book-pages-persisted-pdf-dynamic-shape")
        .with_seed(|paths| async move {
            seed_router_pdf_book(
                &paths,
                "book-pdf-1",
                "series-1",
                "fixture-page.pdf",
                "Fixture PDF",
            )
            .await;
            seed_router_persisted_pdf_page(&paths, "book-pdf-1", 1, "page-1.pdf", 612, 866, None)
                .await;
        })
        .build()
        .await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("persisted pdf pages request should build"),
        )
        .await
        .expect("persisted pdf pages request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let rows = payload
        .as_array()
        .expect("persisted pdf pages payload should be an array");
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("fileName"),
        Some(&Value::String("page-1.pdf".to_string()))
    );
    assert_eq!(
        rows[0].get("mediaType"),
        Some(&Value::String("image/jpeg".to_string()))
    );
    assert_eq!(rows[0].get("width"), Some(&json!(3200)));
    assert_eq!(rows[0].get("height"), Some(&json!(4528)));
    assert!(rows[0].get("sizeBytes").is_some_and(Value::is_null));
    assert_eq!(rows[0].get("size"), Some(&Value::String(String::new())));
}

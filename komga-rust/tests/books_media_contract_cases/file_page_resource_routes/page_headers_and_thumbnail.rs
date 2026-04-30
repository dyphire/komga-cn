use super::*;

fn solid_png_bytes(rgb: [u8; 3]) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(8, 8, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
    let mut cursor = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut cursor, image::ImageFormat::Png)
        .expect("solid png fixture should encode");
    cursor.into_inner()
}

fn legacy_file_url(path: &std::path::Path) -> String {
    format!("file:{}", path.to_string_lossy().replace(' ', "%20"))
}

async fn seed_router_persisted_two_page_cbz_book(
    paths: &RuntimeDbPaths,
    book_id: &str,
    series_id: &str,
    file_name: &str,
    title: &str,
) -> (Vec<u8>, Vec<u8>) {
    let first_page = solid_png_bytes([255, 0, 0]);
    let second_page = solid_png_bytes([0, 255, 0]);

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("persisted cbz book seed db should open");

    let relative_path = format!("books/{file_name}");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind(0_i64)
    .bind(file_name)
    .bind(&relative_path)
    .bind(series_id)
    .bind(8_192_i64)
    .bind(98_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("persisted cbz book row should be inserted");

    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/vnd.comicbook+zip")
        .bind("READY")
        .bind(book_id)
        .bind(2_i64)
        .execute(&pool)
        .await
        .expect("persisted cbz media row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("98")
    .bind(98.0_f64)
    .bind(title)
    .bind("2024-02-02")
    .bind(book_id)
    .execute(&pool)
    .await
    .expect("persisted cbz metadata row should be inserted");

    for (number, file_name, bytes) in [
        (0_i64, "page-1.png", first_page.as_slice()),
        (1_i64, "page-2.png", second_page.as_slice()),
    ] {
        sqlx::query(
            "INSERT INTO MEDIA_PAGE (BOOK_ID, NUMBER, FILE_HASH, FILE_NAME, MEDIA_TYPE, WIDTH, HEIGHT, FILE_SIZE) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(book_id)
        .bind(number)
        .bind("")
        .bind(file_name)
        .bind("image/png")
        .bind(8_i64)
        .bind(8_i64)
        .bind(i64::try_from(bytes.len()).expect("fixture page length should fit in i64"))
        .execute(&pool)
        .await
        .expect("persisted cbz media page row should be inserted");
    }

    pool.close().await;

    let archive_path = paths.config_dir.join(relative_path);
    if let Some(parent) = archive_path.parent() {
        std::fs::create_dir_all(parent).expect("persisted cbz parent directory should be created");
    }
    let file = File::create(&archive_path).expect("persisted cbz fixture file should be created");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o644);
    for (entry_name, bytes) in [
        ("page-1.png", first_page.as_slice()),
        ("page-2.png", second_page.as_slice()),
    ] {
        zip.start_file(entry_name, options)
            .expect("persisted cbz page entry should be created");
        zip.write_all(bytes)
            .expect("persisted cbz page payload should be written");
    }
    zip.finish()
        .expect("persisted cbz fixture should finish successfully");

    (first_page, second_page)
}

async fn rewrite_router_book_to_legacy_file_urls(
    paths: &RuntimeDbPaths,
    book_id: &str,
    file_name: &str,
) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("legacy file url rewrite db should open");

    let archive_path = paths.config_dir.join("books").join(file_name);
    let library_root = legacy_file_url(paths.config_dir.as_path());
    let book_url = legacy_file_url(archive_path.as_path());

    sqlx::query("UPDATE LIBRARY SET ROOT = ? WHERE ID = ?")
        .bind(&library_root)
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("legacy library root should update");

    sqlx::query("UPDATE BOOK SET URL = ? WHERE ID = ?")
        .bind(&book_url)
        .bind(book_id)
        .execute(&pool)
        .await
        .expect("legacy book url should update");

    pool.close().await;
}

#[tokio::test]
async fn router_book_pages_and_raw_pages_include_inline_content_disposition() {
    let ctx = TestFixture::new("router-book-pages-inline-disposition").await;
    seed_router_pdf_book(
        ctx.paths(),
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;
    update_router_book_name(ctx.paths(), "book-pdf-1", "Readable Page Title").await;

    let auth_token = ctx.login_admin().await;

    for route in [
        "/api/v1/books/book-pdf-1/pages/1",
        "/api/v1/books/book-pdf-1/pages/1/raw",
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
                    .expect("page request should build"),
            )
            .await
            .expect("page request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
        let disposition = response
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .and_then(|value| value.to_str().ok())
            .expect("page response should expose content-disposition");
        assert!(
            disposition.starts_with("inline;"),
            "route: {route}, disposition: {disposition}"
        );
        assert!(
            disposition.contains("Readable Page Title-1"),
            "route: {route}, disposition: {disposition}"
        );
    }
}

#[tokio::test]
async fn router_book_page_routes_accept_basic_auth_like_kotlin_clients() {
    let ctx = TestFixture::new("router-book-page-routes-basic-auth-compat").await;
    seed_router_pdf_book(
        ctx.paths(),
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let authorization =
        basic_authorization_header_value("admin@example.org", "router-contract-admin-123");

    for route in [
        "/api/v1/books/book-pdf-1/pages",
        "/api/v1/books/book-pdf-1/pages/1",
        "/api/v1/books/book-pdf-1/pages/1/thumbnail",
    ] {
        let response = ctx
            .app()
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header(header::AUTHORIZATION, authorization.as_str())
                    .header("x-auth-token", "")
                    .body(Body::empty())
                    .expect("book page basic-auth request should build"),
            )
            .await
            .expect("book page basic-auth request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
    }
}

#[tokio::test]
async fn router_persisted_cbz_pages_follow_kotlin_one_based_numbering() {
    let ctx = TestFixture::new("router-persisted-cbz-pages-one-based").await;
    let (first_page, second_page) = seed_router_persisted_two_page_cbz_book(
        ctx.paths(),
        "book-cbz-persisted-1",
        "series-1",
        "persisted-pages.cbz",
        "Persisted Pages Book",
    )
    .await;

    let auth_token = ctx.login_admin().await;

    let pages_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-cbz-persisted-1/pages")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("persisted cbz pages request should build"),
        )
        .await
        .expect("persisted cbz pages request should complete");
    assert_eq!(pages_response.status(), StatusCode::OK);
    let pages_payload = response_json(pages_response).await;
    let rows = pages_payload
        .as_array()
        .expect("persisted cbz pages payload should be an array");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get("number"), Some(&json!(1)));
    assert_eq!(rows[1].get("number"), Some(&json!(2)));

    for (page_number, expected_bytes) in [(1_u64, first_page), (2_u64, second_page)] {
        let response = ctx
            .app()
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/api/v1/books/book-cbz-persisted-1/pages/{page_number}"
                    ))
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("persisted cbz page request should build"),
            )
            .await
            .expect("persisted cbz page request should complete");

        assert_eq!(response.status(), StatusCode::OK, "page: {page_number}");
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("persisted cbz page body should read");
        assert_eq!(
            body.as_ref(),
            expected_bytes.as_slice(),
            "page: {page_number}"
        );
    }

    let invalid_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-cbz-persisted-1/pages/0")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("persisted cbz invalid page request should build"),
        )
        .await
        .expect("persisted cbz invalid page request should complete");
    assert_eq!(invalid_response.status(), StatusCode::BAD_REQUEST);
    let invalid_payload = response_json(invalid_response).await;
    assert_eq!(
        invalid_payload.get("error"),
        Some(&Value::String("Page number does not exist".to_string()))
    );
}

#[tokio::test]
async fn router_persisted_cbz_page_route_reads_legacy_file_urls() {
    let ctx = TestFixture::new("router-persisted-cbz-legacy-file-url-page-route").await;
    let (_, second_page) = seed_router_persisted_two_page_cbz_book(
        ctx.paths(),
        "book-cbz-legacy-url-1",
        "series-1",
        "persisted legacy pages.cbz",
        "Persisted Legacy URL Book",
    )
    .await;
    rewrite_router_book_to_legacy_file_urls(
        ctx.paths(),
        "book-cbz-legacy-url-1",
        "persisted legacy pages.cbz",
    )
    .await;

    let auth_token = ctx.login_admin().await;

    let pages_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-cbz-legacy-url-1/pages")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("legacy file url pages request should build"),
        )
        .await
        .expect("legacy file url pages request should complete");
    assert_eq!(pages_response.status(), StatusCode::OK);

    let page_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-cbz-legacy-url-1/pages/2")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("legacy file url single page request should build"),
        )
        .await
        .expect("legacy file url single page request should complete");

    assert_eq!(page_response.status(), StatusCode::OK);
    let body = to_bytes(page_response.into_body(), usize::MAX)
        .await
        .expect("legacy file url single page body should read");
    assert_eq!(body.as_ref(), second_page.as_slice());
}

#[tokio::test]
async fn router_book_page_thumbnail_resizes_largest_dimension_to_300px() {
    let ctx = TestFixture::new("router-book-page-thumbnail-300px").await;
    seed_router_pdf_book(
        ctx.paths(),
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let auth_token = ctx.login_admin().await;

    let pages_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book pages request should build"),
        )
        .await
        .expect("book pages request should complete");
    assert_eq!(pages_response.status(), StatusCode::OK);
    let pages_payload = response_json(pages_response).await;
    let rows = pages_payload
        .as_array()
        .expect("book pages payload should be an array");
    let page_max_dimension = rows[0]
        .get("width")
        .and_then(Value::as_u64)
        .expect("book page metadata should expose width")
        .max(
            rows[0]
                .get("height")
                .and_then(Value::as_u64)
                .expect("book page metadata should expose height"),
        );

    let thumbnail_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book page thumbnail request should build"),
        )
        .await
        .expect("book page thumbnail request should complete");
    assert_eq!(thumbnail_response.status(), StatusCode::OK);
    let thumbnail_bytes = axum::body::to_bytes(thumbnail_response.into_body(), usize::MAX)
        .await
        .expect("book page thumbnail body should read");
    let thumbnail_image = image::load_from_memory(&thumbnail_bytes)
        .expect("book page thumbnail should decode as image");
    let thumbnail_max_dimension = thumbnail_image.width().max(thumbnail_image.height());

    assert!(
        page_max_dimension > 300,
        "page max dimension was {page_max_dimension}"
    );
    assert_eq!(thumbnail_max_dimension, 300);
    assert!(u64::from(thumbnail_image.width()) <= page_max_dimension);
    assert!(u64::from(thumbnail_image.height()) <= page_max_dimension);
}

#[tokio::test]
async fn router_book_page_thumbnail_returns_bad_request_for_missing_page_number() {
    let ctx = TestFixture::new("router-book-page-thumbnail-missing-page-number").await;
    seed_router_pdf_book(
        ctx.paths(),
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let auth_token = ctx.login_admin().await;

    for route in [
        "/api/v1/books/book-pdf-1/pages/0/thumbnail",
        "/api/v1/books/book-pdf-1/pages/2/thumbnail",
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
                    .expect("book page thumbnail missing-page request should build"),
            )
            .await
            .expect("book page thumbnail missing-page request should complete");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "route: {route}");
        let payload = response_json(response).await;
        assert_eq!(
            payload.get("error"),
            Some(&Value::String("Page number does not exist".to_string())),
            "route: {route}"
        );
    }
}

use super::*;

#[tokio::test]
async fn router_book_pages_single_image_fallback_includes_dimensions() {
    let paths = new_router_fixture("router-book-pages-single-image-dimensions").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_returns_bad_request_with_message_for_non_pdf_media() {
    let paths = new_router_fixture("router-book-raw-page-single-image").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
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

    let image_path = paths.config_dir.join("books/cover-raw.png");
    if let Some(parent) = image_path.parent() {
        std::fs::create_dir_all(parent)
            .expect("single-image raw parent directory should be created");
    }
    let image_bytes = fixture_png_bytes();
    std::fs::write(&image_path, &image_bytes).expect("single-image raw fixture should be written");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_pages_generated_pdf_fallback_matches_kotlin_page_shape() {
    let paths = new_router_fixture("router-book-pages-pdf-dimensions").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Fixture PDF",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("fileName"),
        Some(&Value::String("1".to_string()))
    );
    assert_eq!(
        rows[0].get("mediaType"),
        Some(&Value::String("image/jpeg".to_string()))
    );
    assert!(rows[0].get("width").is_some_and(|value| !value.is_null()));
    assert!(rows[0].get("height").is_some_and(|value| !value.is_null()));
    assert!(rows[0].get("sizeBytes").is_some_and(Value::is_null));
    assert_eq!(rows[0].get("size"), Some(&Value::String(String::new())));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_page_returns_bad_request_with_message_for_missing_pdf_page_number() {
    let paths = new_router_fixture("router-book-page-missing-pdf-page-nonraw").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_page_pdf_negotiation_returns_bad_request_with_message_for_missing_pdf_page_number()
 {
    let paths =
        new_router_fixture("router-book-page-missing-pdf-page-nonraw-pdf-negotiation").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}
#[tokio::test]
async fn router_book_positions_returns_not_found_without_epub_extension_positions() {
    let paths = new_router_fixture("router-book-positions-no-extension").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_positions_does_not_return_not_modified_when_positions_are_missing() {
    let paths = new_router_fixture("router-book-positions-no-extension-not-modified").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_with_cover(&paths, "books/book-1.epub");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_get_returns_full_r2_progression_shape() {
    let paths = new_router_fixture("router-book-progression-get-full-shape").await;
    seed_router_contract_data(&paths).await;

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

    let pool = connect_pool(paths.main_db.as_path(), 1)
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_positions_returns_epub_extension_positions_and_supports_not_modified() {
    let paths = new_router_fixture("router-book-positions-epub-extension").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_with_cover(&paths, "books/book-1.epub");

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

    let pool = connect_pool(paths.main_db.as_path(), 1)
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let initial = app
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

    let not_modified = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_pages_persisted_pdf_rows_match_kotlin_dynamic_page_shape() {
    let paths = new_router_fixture("router-book-pages-persisted-pdf-dynamic-shape").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Fixture PDF",
    )
    .await;
    seed_router_persisted_pdf_page(&paths, "book-pdf-1", 1, "page-1.pdf", 612, 866, None).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

use super::*;

#[tokio::test]
async fn generate_book_thumbnail_persists_generated_thumbnail_for_epub_cover() {
    let paths = new_router_fixture("router-generate-book-thumbnail-epub").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_with_cover(&paths, "books/book-1.epub");

    let cleanup_pool = connect_test_pool(paths.main_db.as_path(), 1)
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
    let (cover_bytes, cover_media_type) = load_epub_cover_bytes(&media)
        .await
        .expect("epub cover bytes should be extractable");
    assert!(!cover_bytes.is_empty());
    assert_eq!(cover_media_type, "image/png");

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for thumbnail generation");
    generate_book_thumbnail(&pool, "book-1")
        .await
        .expect("generate_book_thumbnail should execute successfully for epub cover");
    pool.close().await;

    let main_pool = connect_test_pool(paths.main_db.as_path(), 1)
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
    let app = build_router_with_config(&runtime_config).await;
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

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("pool for generate_book_thumbnail");
    generate_book_thumbnail(&pool, "book-pdf-1")
        .await
        .expect("generate_book_thumbnail should execute successfully for pdf");

    let main_pool = connect_test_pool(paths.main_db.as_path(), 1)
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
    let app = build_router_with_config(&runtime_config).await;
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
async fn generate_book_thumbnail_emits_thumbnail_book_added_event() {
    let _guard = thumbnail_runtime_sse_guard().await;
    let book_id = "book-generated-thumbnail-sse";
    let paths = new_router_fixture("router-generate-book-thumbnail-sse").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_with_cover(&paths, "books/book-generated-thumbnail-sse.epub");

    let cleanup_pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for generated thumbnail sse cleanup");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind(0_i64)
    .bind("book-generated-thumbnail-sse.epub")
    .bind("books/book-generated-thumbnail-sse.epub")
    .bind("series-1")
    .bind(1_024_i64)
    .bind(42_i64)
    .bind("library-1")
    .execute(&cleanup_pool)
    .await
    .expect("unique generated-thumbnail SSE book row should be inserted");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/epub+zip")
        .bind("READY")
        .bind(book_id)
        .bind(10_i64)
        .execute(&cleanup_pool)
        .await
        .expect("unique generated-thumbnail SSE media row should be inserted");
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("42")
    .bind(42.0_f64)
    .bind("Generated Thumbnail SSE Book")
    .bind("2024-01-15")
    .bind(book_id)
    .execute(&cleanup_pool)
    .await
    .expect("unique generated-thumbnail SSE book metadata row should be inserted");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind(book_id)
        .execute(&cleanup_pool)
        .await
        .expect("existing book thumbnails should be deleted before generated thumbnail sse test");
    cleanup_pool.close().await;

    let cursor = komga_application::runtime_sse::current_runtime_sse_event_cursor();
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("pool for generate_book_thumbnail");
    generate_book_thumbnail(&pool, book_id)
        .await
        .expect("generate_book_thumbnail should execute successfully for sse contract");

    let (_, events) = komga_application::runtime_sse::pending_runtime_sse_events(
        cursor,
        "runtime-contract-admin",
        true,
    );
    let thumbnail_events = events
        .iter()
        .filter(|event| event.name == "ThumbnailBookAdded")
        .filter(|event| {
            event.payload.get("bookId").and_then(|value| value.as_str()) == Some(book_id)
        })
        .filter(|event| {
            event
                .payload
                .get("seriesId")
                .and_then(|value| value.as_str())
                == Some("series-1")
        })
        .collect::<Vec<_>>();

    assert_eq!(
        thumbnail_events.len(),
        1,
        "generated book thumbnail creation should emit one ThumbnailBookAdded event",
    );
    assert_eq!(
        thumbnail_events[0].payload.get("selected"),
        Some(&Value::Bool(true)),
        "generated book thumbnail event should reflect the selected generated thumbnail",
    );

    cleanup_router_fixture(paths);
}

use super::*;
use komga_application::task_processing::ThumbnailRegenerationPolicy;

#[tokio::test]
async fn generate_book_thumbnail_persists_generated_thumbnail_for_epub_cover() {
    let ctx = TestFixture::new("router-generate-book-thumbnail-epub").await;
    write_router_epub_with_cover(ctx.paths(), "books/book-1.epub");

    let cleanup_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for epub thumbnail cleanup");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&cleanup_pool)
        .await
        .expect("existing book-1 thumbnails should be deleted before epub cover test");
    cleanup_pool.close().await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for thumbnail generation");
    generate_book_thumbnail_with_isolated_events(&pool, "book-1")
        .await
        .expect("generate_book_thumbnail should execute successfully for epub cover");
    pool.close().await;

    let main_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
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

    let auth_token = ctx.login_admin().await;

    let after = ctx
        .app()
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

    let thumbnails = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn generate_book_thumbnail_reports_invalid_epub_archive_errors() {
    let ctx = TestFixture::new("router-generate-book-thumbnail-invalid-epub").await;
    let epub_path = ctx.paths().config_dir.join("books/book-1.epub");
    std::fs::create_dir_all(
        epub_path
            .parent()
            .expect("invalid epub fixture should have a parent directory"),
    )
    .expect("invalid epub parent directory should be created");
    std::fs::write(&epub_path, b"not a zip").expect("invalid epub fixture should be written");

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for invalid epub thumbnail generation");
    let error = generate_book_thumbnail_with_isolated_events(&pool, "book-1")
        .await
        .expect_err("invalid EPUB archive should fail thumbnail generation");
    pool.close().await;

    assert!(
        error.to_string().contains("open EPUB archive"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn generate_book_thumbnail_persists_generated_thumbnail_for_pdf() {
    let ctx = TestFixture::new("router-generate-book-thumbnail-pdf").await;
    seed_router_pdf_book(
        ctx.paths(),
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Fixture PDF",
    )
    .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("pool for generate_book_thumbnail");
    generate_book_thumbnail_with_isolated_events(&pool, "book-pdf-1")
        .await
        .expect("generate_book_thumbnail should execute successfully for pdf");

    let main_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
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

    let auth_token = ctx.login_admin().await;

    let after = ctx
        .app()
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

    let thumbnails = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn generate_book_thumbnail_uses_callers_thumbnail_policy() {
    let ctx = TestFixture::new("router-generate-book-thumbnail-policy").await;
    write_router_epub_with_cover(ctx.paths(), "books/book-1.epub");

    let cleanup_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for generated thumbnail policy cleanup");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&cleanup_pool)
        .await
        .expect("existing book-1 thumbnails should be deleted before policy test");
    cleanup_pool.close().await;

    let runtime_events = RuntimeSseEventStore::default();
    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for generated thumbnail policy test");
    generate_book_thumbnail(
        &pool,
        &runtime_events,
        "book-1",
        ThumbnailRegenerationPolicy {
            generated_thumbnail_max_edge: 64,
        },
    )
    .await
    .expect("generate_book_thumbnail should use caller thumbnail policy");
    pool.close().await;

    let main_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for generated thumbnail policy verification");
    let generated = sqlx::query(
        "SELECT WIDTH, HEIGHT FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? AND TYPE = ? LIMIT 1",
    )
    .bind("book-1")
    .bind("GENERATED")
    .fetch_one(&main_pool)
    .await
    .expect("generated thumbnail row should exist after policy test");
    main_pool.close().await;

    assert!(
        generated.get::<i64, _>("WIDTH") <= 64 && generated.get::<i64, _>("HEIGHT") <= 64,
        "generated thumbnail dimensions must be capped by the caller policy",
    );
}

#[tokio::test]
async fn generate_book_thumbnail_emits_thumbnail_book_added_event() {
    let book_id = "book-generated-thumbnail-sse";
    let ctx = TestFixture::new("router-generate-book-thumbnail-sse").await;
    write_router_epub_with_cover(ctx.paths(), "books/book-generated-thumbnail-sse.epub");

    let cleanup_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
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

    let cursor = ctx.runtime_events().current_cursor();
    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("pool for generate_book_thumbnail");
    generate_book_thumbnail(
        &pool,
        ctx.runtime_events(),
        book_id,
        ThumbnailRegenerationPolicy::default(),
    )
    .await
    .expect("generate_book_thumbnail should execute successfully for sse contract");

    let events = ctx
        .runtime_events()
        .pending_events(cursor, "runtime-contract-admin", true)
        .events;
    let thumbnail_events = events
        .iter()
        .filter_map(|event| match &event.event {
            RuntimeSseEvent::ThumbnailBookAdded {
                book_id: event_book_id,
                series_id,
                selected,
            } if event_book_id == book_id && series_id == "series-1" => Some(*selected),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        thumbnail_events.len(),
        1,
        "generated book thumbnail creation should emit one ThumbnailBookAdded event",
    );
    assert!(
        thumbnail_events[0],
        "generated book thumbnail event should reflect the selected generated thumbnail",
    );
}

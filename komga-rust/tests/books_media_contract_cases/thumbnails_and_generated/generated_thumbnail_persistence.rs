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
    let (cover_bytes, cover_media_type) =
        load_epub_cover_bytes(&media).expect("epub cover bytes should be extractable");
    assert!(!cover_bytes.is_empty());
    assert_eq!(cover_media_type, "image/png");

    generate_book_thumbnail(paths.main_db.as_path(), "book-1")
        .expect("generate_book_thumbnail should execute successfully for epub cover");

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

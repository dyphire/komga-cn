use super::*;

#[tokio::test]
async fn router_book_thumbnail_by_id_allows_missing_path_book_when_thumbnail_exists() {
    let ctx = TestFixture::new("router-book-thumbnail-by-id-missing-path-book").await;

    let auth_token = ctx.login_admin().await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "cover.png", "image/png", false, &image_bytes);

    let upload = ctx
        .app()
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

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_book_thumbnail_delete_allows_missing_path_book_when_thumbnail_exists() {
    let ctx = TestFixture::new("router-book-thumbnail-delete-missing-path-book").await;

    let auth_token = ctx.login_admin().await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "cover.png", "image/png", false, &image_bytes);

    let upload = ctx
        .app()
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

    let response = ctx
        .app()
        .clone()
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

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
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
}

#[tokio::test]
async fn router_book_thumbnail_delete_rejects_generated_thumbnail() {
    let ctx = TestFixture::new("router-book-thumbnail-delete-generated").await;
    write_router_epub_with_cover(ctx.paths(), "books/book-1.epub");

    let cleanup_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for generated thumbnail cleanup");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&cleanup_pool)
        .await
        .expect("existing thumbnails should be deleted before generated delete test");
    cleanup_pool.close().await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("pool for generate_book_thumbnail");
    generate_book_thumbnail(&pool, "book-1")
        .await
        .expect("generate_book_thumbnail should succeed before delete test");

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

    let auth_token = ctx.login_admin().await;
    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_book_thumbnail_delete_reselects_remaining_thumbnail_when_selected_one_is_removed() {
    let ctx = TestFixture::new("router-book-thumbnail-delete-reselects-remaining").await;

    let cleanup_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for book thumbnail delete cleanup");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&cleanup_pool)
        .await
        .expect("existing book-1 thumbnails should be deleted before delete reselect test");
    cleanup_pool.close().await;

    let auth_token = ctx.login_admin().await;
    let image_bytes = fixture_png_bytes();

    let mut selected_thumbnail_id = String::new();
    for (selected, name) in [(true, "selected.png"), (false, "other.png")] {
        let (content_type, body) =
            multipart_image_upload_body("file", name, "image/png", selected, &image_bytes);
        let upload = ctx
            .app()
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

    let delete = ctx
        .app()
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

    let list = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_book_thumbnail_select_emits_thumbnail_book_added_event() {
    let _guard = thumbnail_runtime_sse_guard().await;
    let ctx = TestFixture::new("router-book-thumbnail-select-sse").await;

    let auth_token = ctx.login_admin().await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "cover.png", "image/png", false, &image_bytes);
    let upload = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/book-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("book thumbnail select sse upload request should build"),
        )
        .await
        .expect("book thumbnail select sse upload request should complete");
    let thumbnail_id = response_json(upload)
        .await
        .get("id")
        .and_then(Value::as_str)
        .expect("uploaded book thumbnail should expose id")
        .to_string();

    let cursor = komga_application::runtime_sse::current_runtime_sse_event_cursor();
    let select = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!(
                    "/api/v1/books/book-1/thumbnails/{thumbnail_id}/selected"
                ))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book thumbnail select sse request should build"),
        )
        .await
        .expect("book thumbnail select sse request should complete");
    assert_eq!(select.status(), StatusCode::ACCEPTED);

    let (_, events) = komga_application::runtime_sse::pending_runtime_sse_events(
        cursor,
        "runtime-contract-admin",
        true,
    );
    let thumbnail_event = events
        .iter()
        .find(|event| event.name == "ThumbnailBookAdded")
        .expect("book thumbnail select should emit ThumbnailBookAdded SSE");
    assert_eq!(
        thumbnail_event.payload.get("selected"),
        Some(&Value::Bool(true))
    );
    assert_eq!(
        thumbnail_event.payload.get("bookId"),
        Some(&Value::String("book-1".to_string()))
    );
    assert_eq!(
        thumbnail_event.payload.get("seriesId"),
        Some(&Value::String("series-1".to_string()))
    );
}

#[tokio::test]
async fn router_book_thumbnail_delete_emits_thumbnail_book_deleted_event() {
    let _guard = thumbnail_runtime_sse_guard().await;
    let book_id = "book-thumbnail-delete-sse";
    let ctx = TestFixture::new("router-book-thumbnail-delete-sse").await;
    seed_router_primary_series_cbz_book(
        ctx.paths(),
        book_id,
        "book-thumbnail-delete-sse.cbz",
        "Book Thumbnail Delete SSE",
    )
    .await;

    let auth_token = ctx.login_admin().await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "cover.png", "image/png", true, &image_bytes);
    let upload = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/books/{book_id}/thumbnails"))
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("book thumbnail delete sse upload request should build"),
        )
        .await
        .expect("book thumbnail delete sse upload request should complete");
    let thumbnail_id = response_json(upload)
        .await
        .get("id")
        .and_then(Value::as_str)
        .expect("uploaded book thumbnail should expose id")
        .to_string();

    let cursor = komga_application::runtime_sse::current_runtime_sse_event_cursor();
    let delete = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/books/{book_id}/thumbnails/{thumbnail_id}"))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book thumbnail delete sse request should build"),
        )
        .await
        .expect("book thumbnail delete sse request should complete");
    assert_eq!(delete.status(), StatusCode::ACCEPTED);

    let (_, events) = komga_application::runtime_sse::pending_runtime_sse_events(
        cursor,
        "runtime-contract-admin",
        true,
    );
    let thumbnail_event = events
        .iter()
        .find(|event| {
            event.name == "ThumbnailBookDeleted"
                && event.payload.get("bookId").and_then(Value::as_str) == Some(book_id)
                && event.payload.get("seriesId").and_then(Value::as_str) == Some("series-1")
        })
        .expect("book thumbnail delete should emit ThumbnailBookDeleted SSE");
    assert_eq!(
        thumbnail_event.payload.get("selected"),
        Some(&Value::Bool(true))
    );
    assert_eq!(
        thumbnail_event.payload.get("bookId"),
        Some(&Value::String(book_id.to_string()))
    );
    assert_eq!(
        thumbnail_event.payload.get("seriesId"),
        Some(&Value::String("series-1".to_string()))
    );
}

#[tokio::test]
async fn router_book_thumbnails_returns_empty_array_for_existing_book_without_posters() {
    let ctx = TestFixture::builder("router-book-thumbnails-empty-array")
        .with_seed(|paths| async move {
            seed_router_authors_scope_variants(&paths).await;
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
}

#[tokio::test]
async fn router_book_thumbnail_returns_not_found_for_existing_single_image_without_poster() {
    let ctx = TestFixture::new("router-book-thumbnail-single-image-no-poster").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
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
                .uri("/api/v1/books/book-image-1/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("single-image thumbnail request should build"),
        )
        .await
        .expect("single-image thumbnail request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

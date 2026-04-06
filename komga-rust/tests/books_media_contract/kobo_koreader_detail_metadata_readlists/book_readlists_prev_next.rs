use super::*;

#[tokio::test]
async fn router_discovery_book_readlists_returns_existing_persisted_readlists() {
    let paths = new_router_fixture("router-discovery-book-readlists-persisted").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/readlists")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book readlists request should build"),
        )
        .await
        .expect("book readlists request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    let content = payload
        .as_array()
        .expect("book readlists payload should be an array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id"),
        Some(&Value::String("readlist-1".to_string())),
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_previous_uses_metadata_number_sort_instead_of_book_number() {
    let paths = new_router_fixture("router-book-previous-number-sort").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book previous number-sort db should open");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-prev-1")
    .bind(0_i64)
    .bind("book-prev-1.cbz")
    .bind("books/book-prev-1.cbz")
    .bind("series-1")
    .bind(4_096_i64)
    .bind(99_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("previous sibling book row should be inserted");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/vnd.comicbook+zip")
        .bind("READY")
        .bind("book-prev-1")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("previous sibling media row should be inserted");
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("99")
    .bind(0.5_f64)
    .bind("Previous by Number Sort")
    .bind("2024-01-01")
    .bind("book-prev-1")
    .execute(&pool)
    .await
    .expect("previous sibling metadata row should be inserted");
    pool.close().await;

    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for previous sibling fixture");
    let file = File::create(books_dir.join("book-prev-1.cbz"))
        .expect("previous sibling cbz fixture should be created");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o644);
    zip.start_file("page-1.png", options)
        .expect("previous sibling page entry should be created");
    zip.write_all(&fixture_png_bytes())
        .expect("previous sibling page payload should be written");
    zip.finish()
        .expect("previous sibling cbz fixture should finish successfully");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/previous")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book previous request should build"),
        )
        .await
        .expect("book previous request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("id"),
        Some(&Value::String("book-prev-1".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_previous_excludes_deleted_books_even_when_they_sort_closer() {
    let paths = new_router_fixture("router-book-previous-excludes-deleted").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book previous deleted db should open");
    sqlx::query("UPDATE BOOK_METADATA SET NUMBER_SORT = ? WHERE BOOK_ID = ?")
        .bind(1.0_f64)
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book-1 number_sort should update");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, DELETED_DATE) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-prev-active")
    .bind(0_i64)
    .bind("book-prev-active.cbz")
    .bind("books/book-prev-active.cbz")
    .bind("series-1")
    .bind(4_096_i64)
    .bind(98_i64)
    .bind("library-1")
    .bind(Option::<String>::None)
    .execute(&pool)
    .await
    .expect("active previous sibling book row should be inserted");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, DELETED_DATE) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-prev-deleted")
    .bind(0_i64)
    .bind("book-prev-deleted.cbz")
    .bind("books/book-prev-deleted.cbz")
    .bind("series-1")
    .bind(4_096_i64)
    .bind(97_i64)
    .bind("library-1")
    .bind("2025-04-01 00:00:00")
    .execute(&pool)
    .await
    .expect("deleted previous sibling book row should be inserted");
    for book_id in ["book-prev-active", "book-prev-deleted"] {
        sqlx::query(
            "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)",
        )
        .bind("application/vnd.comicbook+zip")
        .bind("READY")
        .bind(book_id)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("previous sibling media row should be inserted");
    }
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("98")
    .bind(0.5_f64)
    .bind("Active Previous")
    .bind("2024-01-01")
    .bind("book-prev-active")
    .execute(&pool)
    .await
    .expect("active previous sibling metadata row should be inserted");
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("97")
    .bind(0.75_f64)
    .bind("Deleted Previous")
    .bind("2024-01-01")
    .bind("book-prev-deleted")
    .execute(&pool)
    .await
    .expect("deleted previous sibling metadata row should be inserted");
    pool.close().await;

    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for deleted previous fixture");
    for file_name in ["book-prev-active.cbz", "book-prev-deleted.cbz"] {
        let file = File::create(books_dir.join(file_name))
            .expect("previous sibling cbz fixture should be created");
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o644);
        zip.start_file("page-1.png", options)
            .expect("previous sibling page entry should be created");
        zip.write_all(&fixture_png_bytes())
            .expect("previous sibling page payload should be written");
        zip.finish()
            .expect("previous sibling cbz fixture should finish successfully");
    }

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/previous")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book previous deleted-filter request should build"),
        )
        .await
        .expect("book previous deleted-filter request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("id"),
        Some(&Value::String("book-prev-active".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_previous_breaks_number_sort_ties_by_book_id() {
    let paths = new_router_fixture("router-book-previous-number-sort-tie").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book previous tie db should open");
    sqlx::query("UPDATE BOOK_METADATA SET NUMBER_SORT = ? WHERE BOOK_ID = ?")
        .bind(1.0_f64)
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book-1 number_sort should update for tie test");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-0a")
    .bind(0_i64)
    .bind("book-0a.cbz")
    .bind("books/book-0a.cbz")
    .bind("series-1")
    .bind(4_096_i64)
    .bind(50_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("tie previous sibling book row should be inserted");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/vnd.comicbook+zip")
        .bind("READY")
        .bind("book-0a")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("tie previous sibling media row should be inserted");
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("50")
    .bind(1.0_f64)
    .bind("Previous Tie")
    .bind("2024-01-01")
    .bind("book-0a")
    .execute(&pool)
    .await
    .expect("tie previous sibling metadata row should be inserted");
    pool.close().await;

    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for tie previous fixture");
    let file = File::create(books_dir.join("book-0a.cbz"))
        .expect("tie previous sibling cbz fixture should be created");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o644);
    zip.start_file("page-1.png", options)
        .expect("tie previous sibling page entry should be created");
    zip.write_all(&fixture_png_bytes())
        .expect("tie previous sibling page payload should be written");
    zip.finish()
        .expect("tie previous sibling cbz fixture should finish successfully");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/previous")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book previous tie request should build"),
        )
        .await
        .expect("book previous tie request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("id"),
        Some(&Value::String("book-0a".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_next_uses_metadata_number_sort_instead_of_book_number() {
    let paths = new_router_fixture("router-book-next-number-sort").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book next number-sort db should open");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-next-1")
    .bind(0_i64)
    .bind("book-next-1.cbz")
    .bind("books/book-next-1.cbz")
    .bind("series-1")
    .bind(4_096_i64)
    .bind(0_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("next sibling book row should be inserted");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/vnd.comicbook+zip")
        .bind("READY")
        .bind("book-next-1")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("next sibling media row should be inserted");
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("0")
    .bind(1.5_f64)
    .bind("Next by Number Sort")
    .bind("2024-01-01")
    .bind("book-next-1")
    .execute(&pool)
    .await
    .expect("next sibling metadata row should be inserted");
    pool.close().await;

    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for next sibling fixture");
    let file = File::create(books_dir.join("book-next-1.cbz"))
        .expect("next sibling cbz fixture should be created");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o644);
    zip.start_file("page-1.png", options)
        .expect("next sibling page entry should be created");
    zip.write_all(&fixture_png_bytes())
        .expect("next sibling page payload should be written");
    zip.finish()
        .expect("next sibling cbz fixture should finish successfully");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/next")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book next request should build"),
        )
        .await
        .expect("book next request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("id"),
        Some(&Value::String("book-next-1".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_next_excludes_deleted_books_even_when_they_sort_closer() {
    let paths = new_router_fixture("router-book-next-excludes-deleted").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book next deleted db should open");
    sqlx::query("UPDATE BOOK_METADATA SET NUMBER_SORT = ? WHERE BOOK_ID = ?")
        .bind(1.0_f64)
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book-1 number_sort should update");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, DELETED_DATE) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-next-active")
    .bind(0_i64)
    .bind("book-next-active.cbz")
    .bind("books/book-next-active.cbz")
    .bind("series-1")
    .bind(4_096_i64)
    .bind(2_i64)
    .bind("library-1")
    .bind(Option::<String>::None)
    .execute(&pool)
    .await
    .expect("active next sibling book row should be inserted");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, DELETED_DATE) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-next-deleted")
    .bind(0_i64)
    .bind("book-next-deleted.cbz")
    .bind("books/book-next-deleted.cbz")
    .bind("series-1")
    .bind(4_096_i64)
    .bind(3_i64)
    .bind("library-1")
    .bind("2025-04-01 00:00:00")
    .execute(&pool)
    .await
    .expect("deleted next sibling book row should be inserted");
    for book_id in ["book-next-active", "book-next-deleted"] {
        sqlx::query(
            "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)",
        )
        .bind("application/vnd.comicbook+zip")
        .bind("READY")
        .bind(book_id)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("next sibling media row should be inserted");
    }
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("2")
    .bind(2.0_f64)
    .bind("Active Next")
    .bind("2024-01-01")
    .bind("book-next-active")
    .execute(&pool)
    .await
    .expect("active next sibling metadata row should be inserted");
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("3")
    .bind(1.5_f64)
    .bind("Deleted Next")
    .bind("2024-01-01")
    .bind("book-next-deleted")
    .execute(&pool)
    .await
    .expect("deleted next sibling metadata row should be inserted");
    pool.close().await;

    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for deleted next fixture");
    for file_name in ["book-next-active.cbz", "book-next-deleted.cbz"] {
        let file = File::create(books_dir.join(file_name))
            .expect("next sibling cbz fixture should be created");
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o644);
        zip.start_file("page-1.png", options)
            .expect("next sibling page entry should be created");
        zip.write_all(&fixture_png_bytes())
            .expect("next sibling page payload should be written");
        zip.finish()
            .expect("next sibling cbz fixture should finish successfully");
    }

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/next")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book next deleted-filter request should build"),
        )
        .await
        .expect("book next deleted-filter request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("id"),
        Some(&Value::String("book-next-active".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_next_breaks_number_sort_ties_by_book_id() {
    let paths = new_router_fixture("router-book-next-number-sort-tie").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book next tie db should open");
    sqlx::query("UPDATE BOOK_METADATA SET NUMBER_SORT = ? WHERE BOOK_ID = ?")
        .bind(1.0_f64)
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book-1 number_sort should update for tie test");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-1z")
    .bind(0_i64)
    .bind("book-1z.cbz")
    .bind("books/book-1z.cbz")
    .bind("series-1")
    .bind(4_096_i64)
    .bind(50_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("tie next sibling book row should be inserted");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/vnd.comicbook+zip")
        .bind("READY")
        .bind("book-1z")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("tie next sibling media row should be inserted");
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("50")
    .bind(1.0_f64)
    .bind("Next Tie")
    .bind("2024-01-01")
    .bind("book-1z")
    .execute(&pool)
    .await
    .expect("tie next sibling metadata row should be inserted");
    pool.close().await;

    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for tie next fixture");
    let file = File::create(books_dir.join("book-1z.cbz"))
        .expect("tie next sibling cbz fixture should be created");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o644);
    zip.start_file("page-1.png", options)
        .expect("tie next sibling page entry should be created");
    zip.write_all(&fixture_png_bytes())
        .expect("tie next sibling page payload should be written");
    zip.finish()
        .expect("tie next sibling cbz fixture should finish successfully");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/next")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book next tie request should build"),
        )
        .await
        .expect("book next tie request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("id"),
        Some(&Value::String("book-1z".to_string()))
    );

    cleanup_router_fixture(paths);
}

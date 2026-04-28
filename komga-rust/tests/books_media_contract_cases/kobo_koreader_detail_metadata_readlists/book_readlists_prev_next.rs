use super::*;

#[tokio::test]
async fn router_discovery_book_readlists_returns_existing_persisted_readlists() {
    let paths = new_router_fixture("router-discovery-book-readlists-persisted").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
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
async fn router_book_readlists_and_siblings_accept_basic_auth_like_kotlin_clients() {
    let paths = new_router_fixture("router-book-readlists-siblings-basic-auth-compat").await;
    seed_router_contract_data(&paths).await;
    seed_router_primary_series_cbz_book(
        &paths,
        "book-prev-basic-auth",
        "book-prev-basic-auth.cbz",
        "Previous Basic Auth Book",
    )
    .await;
    seed_router_primary_series_cbz_book(
        &paths,
        "book-next-basic-auth",
        "book-next-basic-auth.cbz",
        "Next Basic Auth Book",
    )
    .await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book sibling basic-auth db should open");
    sqlx::query("UPDATE BOOK_METADATA SET NUMBER_SORT = ? WHERE BOOK_ID = ?")
        .bind(0.5_f64)
        .bind("book-prev-basic-auth")
        .execute(&pool)
        .await
        .expect("previous basic-auth sibling number sort should update");
    sqlx::query("UPDATE BOOK_METADATA SET NUMBER_SORT = ? WHERE BOOK_ID = ?")
        .bind(1.0_f64)
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book-1 number sort should update for basic-auth sibling test");
    sqlx::query("UPDATE BOOK_METADATA SET NUMBER_SORT = ? WHERE BOOK_ID = ?")
        .bind(1.5_f64)
        .bind("book-next-basic-auth")
        .execute(&pool)
        .await
        .expect("next basic-auth sibling number sort should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let authorization =
        basic_authorization_header_value("admin@example.org", "router-contract-admin-123");

    for route in [
        "/api/v1/books/book-1/readlists",
        "/api/v1/books/book-1/previous",
        "/api/v1/books/book-1/next",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header(header::AUTHORIZATION, authorization.as_str())
                    .header("x-auth-token", "")
                    .body(Body::empty())
                    .expect("book readlists/siblings basic-auth request should build"),
            )
            .await
            .expect("book readlists/siblings basic-auth request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_book_readlists_applies_content_restrictions_to_book_ids_and_filtered_like_kotlin()
 {
    let paths = new_router_fixture("router-discovery-book-readlists-content-restrictions").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        18,
    )
    .await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book readlists restricted db should open");
    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-2")
    .bind(0_i64)
    .bind("Series 2")
    .bind("series/series-2")
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("restricted secondary series should be inserted");
    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind("Series 2")
    .bind("Series 2")
    .bind("PubHouse")
    .bind("EN")
    .bind(21_i64)
    .bind("series-2")
    .execute(&pool)
    .await
    .expect("restricted secondary series metadata should be inserted");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-2")
    .bind(0_i64)
    .bind("book-2.epub")
    .bind("books/book-2.epub")
    .bind("series-2")
    .bind(2_048_i64)
    .bind(2_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("restricted secondary book should be inserted");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/epub+zip")
        .bind("READY")
        .bind("book-2")
        .bind(10_i64)
        .execute(&pool)
        .await
        .expect("restricted secondary media should be inserted");
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("2")
    .bind(2.0_f64)
    .bind("Book 2")
    .bind("2024-01-16")
    .bind("book-2")
    .execute(&pool)
    .await
    .expect("restricted secondary book metadata should be inserted");
    sqlx::query("INSERT INTO READLIST (ID, NAME, BOOK_COUNT) VALUES (?, ?, ?)")
        .bind("readlist-2")
        .bind("Filtered ReadList")
        .bind(2_i64)
        .execute(&pool)
        .await
        .expect("filtered readlist row should be inserted");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-2")
        .bind("book-1")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("filtered readlist visible book should be inserted");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-2")
        .bind("book-2")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("filtered readlist restricted book should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/readlists")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("restricted book readlists request should build"),
        )
        .await
        .expect("restricted book readlists request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    let content = payload
        .as_array()
        .expect("restricted book readlists payload should be an array");
    assert_eq!(content.len(), 2);

    let allowed = content
        .iter()
        .find(|entry| entry.get("id") == Some(&json!("readlist-1")))
        .expect("allowed readlist should be returned for visible target book");
    assert_eq!(allowed.get("filtered"), Some(&Value::Bool(false)));
    assert_eq!(allowed.get("bookIds"), Some(&json!(["book-1"])));
    assert!(allowed.get("name").is_some());
    assert!(allowed.get("summary").is_some());
    assert!(allowed.get("ordered").is_some());
    assert!(allowed.get("createdDate").is_some());
    assert!(allowed.get("lastModifiedDate").is_some());

    let filtered = content
        .iter()
        .find(|entry| entry.get("id") == Some(&json!("readlist-2")))
        .expect("partially visible readlist should still be returned");
    assert_eq!(filtered.get("filtered"), Some(&Value::Bool(true)));
    assert_eq!(filtered.get("bookIds"), Some(&json!(["book-1"])));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_previous_uses_metadata_number_sort_instead_of_book_number() {
    let paths = new_router_fixture("router-book-previous-number-sort").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
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
async fn router_book_previous_returns_deleted_books_when_they_sort_closer() {
    let paths = new_router_fixture("router-book-previous-excludes-deleted").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
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
        Some(&Value::String("book-prev-deleted".to_string()))
    );
    assert_eq!(payload.get("deleted"), Some(&Value::Bool(true)));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_previous_skips_equal_number_sort_ties() {
    let paths = new_router_fixture("router-book-previous-number-sort-tie").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
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

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_next_uses_metadata_number_sort_instead_of_book_number() {
    let paths = new_router_fixture("router-book-next-number-sort").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
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
async fn router_book_next_returns_deleted_books_when_they_sort_closer() {
    let paths = new_router_fixture("router-book-next-excludes-deleted").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
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
        Some(&Value::String("book-next-deleted".to_string()))
    );
    assert_eq!(payload.get("deleted"), Some(&Value::Bool(true)));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_next_skips_equal_number_sort_ties() {
    let paths = new_router_fixture("router-book-next-number-sort-tie").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
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

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_next_reuses_book_detail_payload_fields() {
    let paths = new_router_fixture("router-book-next-detail-payload").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book next detail payload db should open");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-next-detail")
    .bind(0_i64)
    .bind("book next detail.cbz")
    .bind("file:/library%20root/books/book%20next%20detail.cbz")
    .bind("series-1")
    .bind(4_096_i64)
    .bind(2_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("next detail sibling book row should be inserted");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/vnd.comicbook+zip")
        .bind("READY")
        .bind("book-next-detail")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("next detail sibling media row should be inserted");
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("2")
    .bind(2.0_f64)
    .bind("Next Detail Payload")
    .bind("2024-01-16")
    .bind("book-next-detail")
    .execute(&pool)
    .await
    .expect("next detail sibling metadata row should be inserted");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED, DEVICE_ID, DEVICE_NAME) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("book-next-detail")
    .bind("admin-user")
    .bind(5_i64)
    .bind(false)
    .bind("device-next")
    .bind("Next Reader")
    .execute(&pool)
    .await
    .expect("next detail sibling read progress row should be inserted");
    pool.close().await;

    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for next detail payload fixture");
    let file = File::create(books_dir.join("book next detail.cbz"))
        .expect("next detail payload cbz fixture should be created");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o644);
    zip.start_file("page-1.png", options)
        .expect("next detail payload page entry should be created");
    zip.write_all(&fixture_png_bytes())
        .expect("next detail payload page payload should be written");
    zip.finish()
        .expect("next detail payload cbz fixture should finish successfully");

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/next")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book next detail payload request should build"),
        )
        .await
        .expect("book next detail payload request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("id"),
        Some(&Value::String("book-next-detail".to_string()))
    );
    assert_eq!(
        payload.get("url"),
        Some(&Value::String(
            "/library root/books/book next detail.cbz".to_string()
        ))
    );
    assert_eq!(
        payload.get("fileLastModified"),
        Some(&Value::String("1970-01-01T00:00:00Z".to_string()))
    );
    assert_eq!(
        payload
            .get("readProgress")
            .and_then(|progress| progress.get("deviceId")),
        Some(&Value::String("device-next".to_string()))
    );
    assert_eq!(
        payload
            .get("readProgress")
            .and_then(|progress| progress.get("deviceName")),
        Some(&Value::String("Next Reader".to_string()))
    );

    cleanup_router_fixture(paths);
}

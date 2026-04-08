use super::*;

#[tokio::test]
async fn router_opds_v1_ondeck_returns_atom_feed() {
    let paths = new_router_fixture("router-opds-v1-ondeck-feed").await;
    seed_router_contract_data(&paths).await;
    seed_router_read_progress(&paths, true).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/ondeck")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 ondeck request should build"),
        )
        .await
        .expect("opds v1 ondeck request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(content_type.contains("application/atom+xml"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_ondeck_uses_kotlin_acquisition_entry_shape() {
    let paths = new_router_fixture("router-opds-v1-ondeck-entry-shape").await;
    seed_router_contract_data(&paths).await;
    seed_router_read_progress(&paths, true).await;
    seed_router_pdf_book(&paths, "book-pdf", "series-1", "book-pdf.pdf", "Book PDF").await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 ondeck entry-shape db should open");
    sqlx::query("UPDATE BOOK_METADATA SET SUMMARY = ? WHERE BOOK_ID = ?")
        .bind("Line one\nLine two")
        .bind("book-pdf")
        .execute(&pool)
        .await
        .expect("ondeck summary should update for entry-shape test");
    sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
        .bind("book-pdf")
        .bind("Jane Writer")
        .bind("writer")
        .execute(&pool)
        .await
        .expect("ondeck author should insert for entry-shape test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/ondeck")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 ondeck entry-shape request should build"),
        )
        .await
        .expect("opds v1 ondeck entry-shape request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains("<title>Series 1 99: Book PDF</title>"),
        "body={body}"
    );
    assert!(
        body.contains("<content>pdf - 4096<br/><br/>Line one<br/>Line two</content>"),
        "body={body}"
    );
    assert!(
        body.contains("<author><name>Jane Writer</name></author>"),
        "body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_ondeck_includes_page_streaming_link_without_read_progress_attributes() {
    let paths = new_router_fixture("router-opds-v1-ondeck-pse-link").await;
    seed_router_contract_data(&paths).await;
    seed_router_read_progress(&paths, true).await;
    seed_router_pdf_book(&paths, "book-pdf", "series-1", "book-pdf.pdf", "Book PDF").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/ondeck")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 ondeck PSE request should build"),
        )
        .await
        .expect("opds v1 ondeck PSE request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<id>book-pdf</id>"), "body={body}");
    assert!(
        body.contains("rel=\"http://vaemendis.net/opds-pse/stream\""),
        "body={body}"
    );
    assert!(body.contains("pse:count=\"1\""), "body={body}");
    assert!(!body.contains("pse:lastRead=\""), "body={body}");
    assert!(!body.contains("pse:lastReadDate=\""), "body={body}");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_ondeck_orders_series_by_most_recent_read_date() {
    let paths = new_router_fixture("router-opds-v1-ondeck-recent-order").await;
    seed_router_contract_data(&paths).await;
    seed_router_contract_nullable_samples(&paths).await;
    seed_router_read_progress(&paths, true).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "book-pdf-1.pdf",
        "Book PDF 1",
    )
    .await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-2",
        "series-2",
        "book-pdf-2.pdf",
        "Book PDF 2",
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 ondeck recent-order db should open");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) VALUES (?, ?, ?, ?)",
    )
    .bind("book-2")
    .bind("admin-user")
    .bind(12_i64)
    .bind(true)
    .execute(&pool)
    .await
    .expect("series-2 completed read progress should insert for recent-order test");
    sqlx::query(
        "INSERT INTO READ_PROGRESS_SERIES (SERIES_ID, USER_ID, READ_COUNT, IN_PROGRESS_COUNT, MOST_RECENT_READ_DATE) VALUES (?, ?, ?, ?, ?) \
         ON CONFLICT (SERIES_ID, USER_ID) DO UPDATE SET READ_COUNT = excluded.READ_COUNT, IN_PROGRESS_COUNT = excluded.IN_PROGRESS_COUNT, MOST_RECENT_READ_DATE = excluded.MOST_RECENT_READ_DATE",
    )
    .bind("series-1")
    .bind("admin-user")
    .bind(1_i64)
    .bind(0_i64)
    .bind("2024-03-01 00:00:00")
    .execute(&pool)
    .await
    .expect("series-1 series progress should upsert for recent-order test");
    sqlx::query(
        "INSERT INTO READ_PROGRESS_SERIES (SERIES_ID, USER_ID, READ_COUNT, IN_PROGRESS_COUNT, MOST_RECENT_READ_DATE) VALUES (?, ?, ?, ?, ?) \
         ON CONFLICT (SERIES_ID, USER_ID) DO UPDATE SET READ_COUNT = excluded.READ_COUNT, IN_PROGRESS_COUNT = excluded.IN_PROGRESS_COUNT, MOST_RECENT_READ_DATE = excluded.MOST_RECENT_READ_DATE",
    )
    .bind("series-2")
    .bind("admin-user")
    .bind(1_i64)
    .bind(0_i64)
    .bind("2024-03-02 00:00:00")
    .execute(&pool)
    .await
    .expect("series-2 series progress should upsert for recent-order test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/ondeck")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 ondeck recent-order request should build"),
        )
        .await
        .expect("opds v1 ondeck recent-order request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    let series_two_index = body.find("<id>book-pdf-2</id>").expect("body={body}");
    let series_one_index = body.find("<id>book-pdf-1</id>").expect("body={body}");
    assert!(series_two_index < series_one_index, "body={body}");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_keep_reading_filters_non_ready_books() {
    let paths = new_router_fixture("router-opds-v1-keep-reading-ready-only").await;
    seed_router_contract_data(&paths).await;
    seed_router_read_progress(&paths, false).await;
    seed_router_pdf_book(&paths, "book-pdf", "series-1", "book-pdf.pdf", "Book PDF").await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 keep-reading ready-only db should open");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) VALUES (?, ?, ?, ?)",
    )
    .bind("book-pdf")
    .bind("admin-user")
    .bind(7_i64)
    .bind(false)
    .execute(&pool)
    .await
    .expect("keep-reading read progress row should insert for ready-only test");
    sqlx::query("UPDATE MEDIA SET STATUS = ? WHERE BOOK_ID = ?")
        .bind("ERROR")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book-1 media status should update for keep-reading ready-only test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/keep-reading")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 keep-reading ready-only request should build"),
        )
        .await
        .expect("opds v1 keep-reading ready-only request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<id>book-pdf</id>"), "body={body}");
    assert!(!body.contains("<id>book-1</id>"), "body={body}");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_keep_reading_uses_kotlin_acquisition_entry_shape() {
    let paths = new_router_fixture("router-opds-v1-keep-reading-entry-shape").await;
    seed_router_contract_data(&paths).await;
    seed_router_read_progress(&paths, false).await;
    seed_router_pdf_book(&paths, "book-pdf", "series-1", "book-pdf.pdf", "Book PDF").await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 keep-reading entry-shape db should open");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) VALUES (?, ?, ?, ?)",
    )
    .bind("book-pdf")
    .bind("admin-user")
    .bind(7_i64)
    .bind(false)
    .execute(&pool)
    .await
    .expect("keep-reading read progress row should insert for entry-shape test");
    sqlx::query("UPDATE BOOK_METADATA SET SUMMARY = ? WHERE BOOK_ID = ?")
        .bind("Line one\nLine two")
        .bind("book-pdf")
        .execute(&pool)
        .await
        .expect("keep-reading summary should update for entry-shape test");
    sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
        .bind("book-pdf")
        .bind("Jane Writer")
        .bind("writer")
        .execute(&pool)
        .await
        .expect("keep-reading author should insert for entry-shape test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/keep-reading")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 keep-reading entry-shape request should build"),
        )
        .await
        .expect("opds v1 keep-reading entry-shape request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains("<title>Series 1 99: Book PDF</title>"),
        "body={body}"
    );
    assert!(
        body.contains("<content>pdf - 4096<br/><br/>Line one<br/>Line two</content>"),
        "body={body}"
    );
    assert!(
        body.contains("<author><name>Jane Writer</name></author>"),
        "body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_keep_reading_includes_page_streaming_link_with_read_progress_attributes() {
    let paths = new_router_fixture("router-opds-v1-keep-reading-pse-link").await;
    seed_router_contract_data(&paths).await;
    seed_router_read_progress(&paths, false).await;
    seed_router_pdf_book(&paths, "book-pdf", "series-1", "book-pdf.pdf", "Book PDF").await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 keep-reading PSE db should open");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) VALUES (?, ?, ?, ?)",
    )
    .bind("book-pdf")
    .bind("admin-user")
    .bind(7_i64)
    .bind(false)
    .execute(&pool)
    .await
    .expect("keep-reading read progress row should insert for PSE test");
    sqlx::query("UPDATE READ_PROGRESS SET READ_DATE = ? WHERE BOOK_ID = ? AND USER_ID = ?")
        .bind("2024-04-05 06:07:08")
        .bind("book-pdf")
        .bind("admin-user")
        .execute(&pool)
        .await
        .expect("keep-reading read progress timestamp should update for PSE test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/keep-reading")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 keep-reading PSE request should build"),
        )
        .await
        .expect("opds v1 keep-reading PSE request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<id>book-pdf</id>"), "body={body}");
    assert!(
        body.contains("rel=\"http://vaemendis.net/opds-pse/stream\""),
        "body={body}"
    );
    assert!(body.contains("pse:count=\"1\""), "body={body}");
    assert!(body.contains("pse:lastRead=\"7\""), "body={body}");
    assert!(
        body.contains("pse:lastReadDate=\"2024-04-05T06:07:08Z\""),
        "body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_keep_reading_preserves_zero_page_count_in_pse_link() {
    let paths = new_router_fixture("router-opds-v1-keep-reading-zero-page-count").await;
    seed_router_contract_data(&paths).await;
    seed_router_read_progress(&paths, false).await;
    seed_router_pdf_book(&paths, "book-pdf", "series-1", "book-pdf.pdf", "Book PDF").await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 keep-reading zero-page-count db should open");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) VALUES (?, ?, ?, ?)",
    )
    .bind("book-pdf")
    .bind("admin-user")
    .bind(7_i64)
    .bind(false)
    .execute(&pool)
    .await
    .expect("keep-reading read progress row should insert for zero-page-count test");
    sqlx::query("UPDATE MEDIA SET PAGE_COUNT = ? WHERE BOOK_ID = ?")
        .bind(0_i64)
        .bind("book-pdf")
        .execute(&pool)
        .await
        .expect("keep-reading page count should update for zero-page-count test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/keep-reading")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 keep-reading zero-page-count request should build"),
        )
        .await
        .expect("opds v1 keep-reading zero-page-count request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<id>book-pdf</id>"), "body={body}");
    assert!(
        body.contains("rel=\"http://vaemendis.net/opds-pse/stream\""),
        "body={body}"
    );
    assert!(body.contains("pse:count=\"0\""), "body={body}");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_keep_reading_orders_by_read_progress_date() {
    let paths = new_router_fixture("router-opds-v1-keep-reading-order").await;
    seed_router_contract_data(&paths).await;
    seed_router_read_progress(&paths, false).await;
    seed_router_pdf_book(&paths, "book-pdf", "series-1", "book-pdf.pdf", "Book PDF").await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 keep-reading order db should open");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) VALUES (?, ?, ?, ?)",
    )
    .bind("book-pdf")
    .bind("admin-user")
    .bind(7_i64)
    .bind(false)
    .execute(&pool)
    .await
    .expect("keep-reading read progress row should insert for ordering test");
    sqlx::query("UPDATE READ_PROGRESS SET READ_DATE = ? WHERE BOOK_ID = ? AND USER_ID = ?")
        .bind("2024-03-01 00:00:00")
        .bind("book-1")
        .bind("admin-user")
        .execute(&pool)
        .await
        .expect("book-1 read date should update for keep-reading ordering test");
    sqlx::query("UPDATE READ_PROGRESS SET READ_DATE = ? WHERE BOOK_ID = ? AND USER_ID = ?")
        .bind("2024-03-02 00:00:00")
        .bind("book-pdf")
        .bind("admin-user")
        .execute(&pool)
        .await
        .expect("book-pdf read date should update for keep-reading ordering test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/keep-reading")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 keep-reading ordering request should build"),
        )
        .await
        .expect("opds v1 keep-reading ordering request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    let pdf_index = body.find("<id>book-pdf</id>").expect("body={body}");
    let book_one_index = body.find("<id>book-1</id>").expect("body={body}");
    assert!(pdf_index < book_one_index, "body={body}");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_books_latest_uses_kotlin_acquisition_entry_shape() {
    let paths = new_router_fixture("router-opds-v1-books-latest-entry-shape").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/books/latest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 books latest entry-shape request should build"),
        )
        .await
        .expect("opds v1 books latest entry-shape request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<title>Series 1 1: Book 1</title>"));
    assert!(body.contains("<content>epub - 1024</content>"));
    assert!(body.contains("<author><name>Jane Writer</name></author>"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_books_latest_filters_non_ready_books() {
    let paths = new_router_fixture("router-opds-v1-books-latest-ready-only").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(&paths, "book-pdf", "series-1", "book-pdf.pdf", "Book PDF").await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 books latest ready-only db should open");
    sqlx::query("UPDATE MEDIA SET STATUS = ? WHERE BOOK_ID = ?")
        .bind("ERROR")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book-1 media status should update for latest books ready-only test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/books/latest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 books latest ready-only request should build"),
        )
        .await
        .expect("opds v1 books latest ready-only request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<id>book-pdf</id>"));
    assert!(!body.contains("<id>book-1</id>"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_books_latest_paginates_after_restriction_filtering() {
    let paths = new_router_fixture("router-opds-v1-books-latest-restriction-pagination").await;
    seed_router_contract_data(&paths).await;
    seed_router_contract_nullable_samples(&paths).await;
    seed_router_custom_series(&paths, "series-visible", "Visible Series", "library-1").await;
    seed_router_pdf_book(
        &paths,
        "book-visible",
        "series-visible",
        "book-visible.pdf",
        "Visible Book",
    )
    .await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 latest-books restriction-pagination db should open");
    sqlx::query("UPDATE SERIES_METADATA SET AGE_RATING = ? WHERE SERIES_ID = ?")
        .bind(0_i64)
        .bind("series-visible")
        .execute(&pool)
        .await
        .expect("visible series age rating should be updated for restriction-pagination test");
    sqlx::query("UPDATE BOOK SET CREATED_DATE = ? WHERE ID = ?")
        .bind("2024-03-03 00:00:00")
        .bind("book-2")
        .execute(&pool)
        .await
        .expect("book-2 created date should update for restriction-pagination test");
    sqlx::query("UPDATE BOOK SET CREATED_DATE = ? WHERE ID = ?")
        .bind("2024-03-02 00:00:00")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book-1 created date should update for restriction-pagination test");
    sqlx::query("UPDATE BOOK SET CREATED_DATE = ? WHERE ID = ?")
        .bind("2024-03-01 00:00:00")
        .bind("book-visible")
        .execute(&pool)
        .await
        .expect("visible book created date should update for restriction-pagination test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
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
                .uri("/opds/v1.2/books/latest?page=0&size=1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 latest-books restriction-pagination request should build"),
        )
        .await
        .expect("opds v1 latest-books restriction-pagination request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<id>book-visible</id>"), "body={body}");
    assert!(!body.contains("<id>book-1</id>"), "body={body}");
    assert!(!body.contains("<id>book-2</id>"), "body={body}");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_books_latest_includes_page_streaming_and_read_progress_for_pdf_books() {
    let paths = new_router_fixture("router-opds-v1-books-latest-pse-read-progress").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(&paths, "book-pdf", "series-1", "book-pdf.pdf", "Book PDF").await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 books latest PSE db should open");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) VALUES (?, ?, ?, ?)",
    )
    .bind("book-pdf")
    .bind("admin-user")
    .bind(7_i64)
    .bind(false)
    .execute(&pool)
    .await
    .expect("read progress row should be inserted for latest books PSE test");
    sqlx::query("UPDATE READ_PROGRESS SET READ_DATE = ? WHERE BOOK_ID = ? AND USER_ID = ?")
        .bind("2024-04-05 06:07:08")
        .bind("book-pdf")
        .bind("admin-user")
        .execute(&pool)
        .await
        .expect("read progress timestamp should update for latest books PSE test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/books/latest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 books latest PSE request should build"),
        )
        .await
        .expect("opds v1 books latest PSE request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<id>book-pdf</id>"));
    assert!(body.contains("rel=\"http://vaemendis.net/opds-pse/stream\""));
    assert!(body.contains("pse:count=\"1\""));
    assert!(body.contains("pse:lastRead=\"7\""));
    assert!(body.contains("pse:lastReadDate=\"2024-04-05T06:07:08Z\""));

    cleanup_router_fixture(paths);
}

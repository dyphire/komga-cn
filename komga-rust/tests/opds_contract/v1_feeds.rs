use super::*;
use icu::collator::{
    Collator,
    options::{CollatorOptions, Strength},
};
use icu::locale::locale;
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::{PrimitiveDateTime, UtcOffset};

fn kotlin_unicode_3_collator() -> icu::collator::CollatorBorrowed<'static> {
    let mut options = CollatorOptions::default();
    options.strength = Some(Strength::Tertiary);
    Collator::try_new(locale!("und").into(), options)
        .expect("ICU collator for OPDS contract ordering should construct")
}

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

#[tokio::test]
async fn router_opds_v1_publishers_returns_atom_feed() {
    let paths = new_router_fixture("router-opds-v1-publishers-feed").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/publishers")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 publishers request should build"),
        )
        .await
        .expect("opds v1 publishers request should complete");

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
async fn router_opds_v1_readlists_filters_out_of_scope_entries_sorts_by_name_and_uses_persisted_entry_updated()
 {
    let paths = new_router_fixture("router-opds-v1-readlists-visible-order-updated").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-user",
        "library-user@example.org",
        "router-contract-library-123",
        &["library-1"],
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 readlists db should open");
    sqlx::query(
        "INSERT INTO READLIST (ID, NAME, BOOK_COUNT, ORDERED, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("readlist-alpha")
    .bind("Alpha ReadList")
    .bind(1_i64)
    .bind(true)
    .bind("2024-01-24T01:02:03Z")
    .execute(&pool)
    .await
    .expect("visible readlist should be inserted");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-alpha")
        .bind("book-2")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("visible readlist book should be inserted");
    sqlx::query(
        "INSERT INTO READLIST (ID, NAME, BOOK_COUNT, ORDERED, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("readlist-zulu")
    .bind("Zulu ReadList")
    .bind(1_i64)
    .bind(true)
    .bind("2024-01-25T01:02:03Z")
    .execute(&pool)
    .await
    .expect("out-of-scope readlist should be inserted");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-zulu")
        .bind("book-3")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("out-of-scope readlist book should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library-user@example.org",
        "router-contract-library-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/readlists")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 readlists request should build"),
        )
        .await
        .expect("opds v1 readlists request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<id>allReadLists</id>"));
    assert!(body.contains("<title>All read lists</title>"));
    assert!(body.contains("<updated>2024-01-24T01:02:03Z</updated>"));
    assert!(body.contains("/opds/v1.2/readlists/readlist-alpha"));
    assert!(body.contains("/opds/v1.2/readlists/readlist-1"));
    assert!(!body.contains("/opds/v1.2/readlists/readlist-zulu"));
    let alpha_pos = body
        .find("/opds/v1.2/readlists/readlist-alpha")
        .expect("alpha readlist entry should be present");
    let default_pos = body
        .find("/opds/v1.2/readlists/readlist-1")
        .expect("default readlist entry should be present");
    assert!(
        alpha_pos < default_pos,
        "readlists list must preserve Kotlin name ordering, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_collections_filters_out_of_scope_entries_sorts_by_name_and_uses_persisted_entry_updated()
 {
    let paths = new_router_fixture("router-opds-v1-collections-visible-order-updated").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-user",
        "library-user@example.org",
        "router-contract-library-123",
        &["library-1"],
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 collections db should open");
    sqlx::query(
        "INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("collection-alpha")
    .bind("Alpha Collection")
    .bind(false)
    .bind(1_i64)
    .bind("2024-01-26T01:02:03Z")
    .execute(&pool)
    .await
    .expect("visible collection should be inserted");
    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind("collection-alpha")
    .bind("series-1")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("visible collection series should be inserted");
    sqlx::query(
        "INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("collection-zulu")
    .bind("Zulu Collection")
    .bind(false)
    .bind(1_i64)
    .bind("2024-01-27T01:02:03Z")
    .execute(&pool)
    .await
    .expect("out-of-scope collection should be inserted");
    sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
        .bind("library-2")
        .bind("Library 2")
        .bind(paths.config_dir.to_string_lossy().to_string())
        .execute(&pool)
        .await
        .expect("library-2 should be inserted for out-of-scope collection test");
    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-zulu")
    .bind(0_i64)
    .bind("Series Zulu")
    .bind("series/series-zulu")
    .bind("library-2")
    .execute(&pool)
    .await
    .expect("series-zulu should be inserted for out-of-scope collection test");
    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind("collection-zulu")
    .bind("series-zulu")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("out-of-scope collection series should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library-user@example.org",
        "router-contract-library-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/collections")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 collections request should build"),
        )
        .await
        .expect("opds v1 collections request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<id>allCollections</id>"));
    assert!(body.contains("<title>All collections</title>"));
    assert!(
        body.contains("<entry><title>Alpha Collection</title><updated>2024-01-26T01:02:03Z</updated><id>collection-alpha</id>"),
        "collections list should preserve persisted entry updated, body={body}"
    );
    assert!(body.contains("/opds/v1.2/collections/collection-alpha"));
    assert!(body.contains("/opds/v1.2/collections/collection-1"));
    assert!(!body.contains("/opds/v1.2/collections/collection-zulu"));
    let alpha_pos = body
        .find("/opds/v1.2/collections/collection-alpha")
        .expect("alpha collection entry should be present");
    let default_pos = body
        .find("/opds/v1.2/collections/collection-1")
        .expect("default collection entry should be present");
    assert!(
        alpha_pos < default_pos,
        "collections list must preserve Kotlin name ordering, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_collections_keeps_empty_collection_for_all_library_user() {
    let paths = new_router_fixture("router-opds-v1-collections-empty-visible").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 collections empty-visible db should open");
    sqlx::query(
        "INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("collection-empty")
    .bind("Empty Collection")
    .bind(false)
    .bind(0_i64)
    .bind("2024-02-01T01:02:03Z")
    .execute(&pool)
    .await
    .expect("empty collection should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/collections")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 collections empty-visible request should build"),
        )
        .await
        .expect("opds v1 collections empty-visible request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains("<entry><title>Empty Collection</title><updated>2024-02-01T01:02:03Z</updated><id>collection-empty</id>"),
        "all-libraries OPDS collections list should keep empty collections like Kotlin, body={body}"
    );
    assert!(body.contains("/opds/v1.2/collections/collection-empty"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_collections_formats_sqlite_naive_updated_like_kotlin() {
    let paths = new_router_fixture("router-opds-v1-collections-naive-updated").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 collections naive-updated db should open");
    sqlx::query(
        "INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("collection-naive-updated")
    .bind("Naive Updated Collection")
    .bind(false)
    .bind(0_i64)
    .bind("2024-01-26 01:02:03")
    .execute(&pool)
    .await
    .expect("naive-updated collection should be inserted");
    pool.close().await;

    let parsed = PrimitiveDateTime::parse(
        "2024-01-26 01:02:03",
        format_description!("[year]-[month]-[day] [hour]:[minute]:[second]"),
    )
    .expect("test naive timestamp should parse");
    let expected_updated = parsed
        .assume_utc()
        .to_offset(UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC))
        .format(&Rfc3339)
        .expect("expected OPDS updated timestamp should format");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/collections")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 collections naive-updated request should build"),
        )
        .await
        .expect("opds v1 collections naive-updated request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains(
            format!(
                "<entry><title>Naive Updated Collection</title><updated>{expected_updated}</updated><id>collection-naive-updated</id>"
            )
            .as_str()
        ),
        "OPDS v1 collections should format SQLite naive updated like Kotlin, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_collections_preserves_unicode_collation_order() {
    let paths = new_router_fixture("router-opds-v1-collections-unicode-order").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 collections unicode-order db should open");
    sqlx::query("UPDATE COLLECTION SET NAME = ? WHERE ID = ?")
        .bind("Éclair Collection")
        .bind("collection-1")
        .execute(&pool)
        .await
        .expect("default collection name should update for Unicode ordering test");
    sqlx::query("INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)")
        .bind("collection-alpha")
        .bind("Alpha Collection")
        .bind(false)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("alpha collection should insert for Unicode ordering test");
    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind("collection-alpha")
    .bind("series-1")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("alpha collection series should insert for Unicode ordering test");
    sqlx::query("INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)")
        .bind("collection-zulu")
        .bind("Zulu Collection")
        .bind(false)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("zulu collection should insert for Unicode ordering test");
    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind("collection-zulu")
    .bind("series-1")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("zulu collection series should insert for Unicode ordering test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/collections")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 collections Unicode ordering request should build"),
        )
        .await
        .expect("opds v1 collections Unicode ordering request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    let alpha_pos = body
        .find("/opds/v1.2/collections/collection-alpha")
        .expect("alpha collection entry should be present");
    let eclair_pos = body
        .find("/opds/v1.2/collections/collection-1")
        .expect("Éclair collection entry should be present");
    let zulu_pos = body
        .find("/opds/v1.2/collections/collection-zulu")
        .expect("zulu collection entry should be present");
    assert!(
        alpha_pos < eclair_pos && eclair_pos < zulu_pos,
        "OPDS v1 collections should keep Kotlin Unicode collation order, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_collections_preserves_kotlin_tertiary_case_order() {
    let paths = new_router_fixture("router-opds-v1-collections-tertiary-case-order").await;
    seed_router_contract_data(&paths).await;

    let collator = kotlin_unicode_3_collator();
    let mut names = vec![
        "eclair Collection".to_string(),
        "Eclair Collection".to_string(),
        "ECLAIR Collection".to_string(),
    ];
    names.sort_by(|left_name, right_name| collator.compare(left_name, right_name));

    let assigned = [
        ("collection-1", names[2].clone()),
        ("collection-a", names[1].clone()),
        ("collection-b", names[0].clone()),
    ];

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 collections tertiary-case-order db should open");
    sqlx::query("UPDATE COLLECTION SET NAME = ? WHERE ID = ?")
        .bind(&assigned[0].1)
        .bind(assigned[0].0)
        .execute(&pool)
        .await
        .expect("default collection name should update for tertiary case-order test");
    sqlx::query("INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)")
        .bind(assigned[1].0)
        .bind(&assigned[1].1)
        .bind(false)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("second collection should insert for tertiary case-order test");
    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind(assigned[1].0)
    .bind("series-1")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("second collection series should insert for tertiary case-order test");
    sqlx::query("INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)")
        .bind(assigned[2].0)
        .bind(&assigned[2].1)
        .bind(false)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("third collection should insert for tertiary case-order test");
    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind(assigned[2].0)
    .bind("series-1")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("third collection series should insert for tertiary case-order test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/collections")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 collections tertiary case-order request should build"),
        )
        .await
        .expect("opds v1 collections tertiary case-order request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;

    let expected_ids = vec![
        "/opds/v1.2/collections/collection-b",
        "/opds/v1.2/collections/collection-a",
        "/opds/v1.2/collections/collection-1",
    ];
    for pair in expected_ids.windows(2) {
        let left_pos = body
            .find(pair[0])
            .expect("expected left collection entry should be present");
        let right_pos = body
            .find(pair[1])
            .expect("expected right collection entry should be present");
        assert!(
            left_pos < right_pos,
            "OPDS v1 collections should keep Kotlin tertiary case order, body={body}"
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_detail_filters_non_ready_books() {
    let paths = new_router_fixture("router-opds-v1-series-detail-ready-only").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 series detail ready-only db should open");
    sqlx::query("UPDATE MEDIA SET STATUS = ? WHERE BOOK_ID = ?")
        .bind("ERROR")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book-1 media status should update to non-ready");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 series detail ready-only request should build"),
        )
        .await
        .expect("opds v1 series detail ready-only request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<id>series-1</id>"));
    assert!(!body.contains("<id>book-1</id>"));
    assert!(!body.contains("<entry>"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_detail_uses_kotlin_acquisition_entry_shape() {
    let paths = new_router_fixture("router-opds-v1-series-detail-entry-shape").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 series detail entry-shape request should build"),
        )
        .await
        .expect("opds v1 series detail entry-shape request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<title>Book 1</title>"));
    assert!(body.contains("<content>epub - 1024</content>"));
    assert!(body.contains("<author><name>Jane Writer</name></author>"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_detail_formats_summary_content_like_kotlin() {
    let paths = new_router_fixture("router-opds-v1-series-detail-summary-content").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 series detail summary-content db should open");
    sqlx::query("UPDATE BOOK_METADATA SET SUMMARY = ? WHERE BOOK_ID = ?")
        .bind("Line one\nLine two")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book summary should update for summary-content test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 series detail summary-content request should build"),
        )
        .await
        .expect("opds v1 series detail summary-content request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<content>epub - 1024<br/><br/>Line one<br/>Line two</content>"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_detail_includes_page_streaming_link_for_pdf_books() {
    let paths = new_router_fixture("router-opds-v1-series-detail-pdf-stream-link").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(&paths, "book-pdf", "series-1", "book-pdf.pdf", "Book PDF").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 series detail pdf-stream request should build"),
        )
        .await
        .expect("opds v1 series detail pdf-stream request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<id>book-pdf</id>"));
    assert!(body.contains("rel=\"http://vaemendis.net/opds-pse/stream\""));
    assert!(body.contains("/opds/v1.2/books/book-pdf/pages/"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_detail_declares_pse_namespace_for_page_streaming_links() {
    let paths = new_router_fixture("router-opds-v1-series-detail-pse-namespace").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(&paths, "book-pdf", "series-1", "book-pdf.pdf", "Book PDF").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 series detail pse-namespace request should build"),
        )
        .await
        .expect("opds v1 series detail pse-namespace request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("rel=\"http://vaemendis.net/opds-pse/stream\""));
    assert!(body.contains("pse:count=\""));
    assert!(body.contains("xmlns:pse=\"http://vaemendis.net/opds-pse/ns\""));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_detail_includes_read_progress_attributes_on_page_streaming_link() {
    let paths = new_router_fixture("router-opds-v1-series-detail-pse-read-progress").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(&paths, "book-pdf", "series-1", "book-pdf.pdf", "Book PDF").await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 series detail read-progress db should open");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) VALUES (?, ?, ?, ?)",
    )
    .bind("book-pdf")
    .bind("admin-user")
    .bind(7_i64)
    .bind(false)
    .execute(&pool)
    .await
    .expect("read progress row should be inserted for series detail PSE test");
    sqlx::query("UPDATE READ_PROGRESS SET READ_DATE = ? WHERE BOOK_ID = ? AND USER_ID = ?")
        .bind("2024-04-05 06:07:08")
        .bind("book-pdf")
        .bind("admin-user")
        .execute(&pool)
        .await
        .expect("read progress timestamp should update for series detail PSE test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 series detail PSE read-progress request should build"),
        )
        .await
        .expect("opds v1 series detail PSE read-progress request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<id>book-pdf</id>"));
    assert!(body.contains("rel=\"http://vaemendis.net/opds-pse/stream\""));
    assert!(body.contains("pse:count=\""));
    assert!(body.contains("pse:lastRead=\"7\""));
    assert!(body.contains("pse:lastReadDate=\"2024-04-05T06:07:08Z\""));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_detail_includes_page_streaming_link_for_cbz_books() {
    let paths = new_router_fixture("router-opds-v1-series-detail-cbz-stream-link").await;
    seed_router_contract_data(&paths).await;
    seed_router_cbz_book(&paths, "book-cbz", "series-1", "book-cbz.cbz", "Book CBZ").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 series detail cbz-stream request should build"),
        )
        .await
        .expect("opds v1 series detail cbz-stream request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<id>book-cbz</id>"));
    assert!(body.contains("rel=\"http://vaemendis.net/opds-pse/stream\""));
    assert!(body.contains("type=\"image/png\""));
    assert!(body.contains("/opds/v1.2/books/book-cbz/pages/{pageNumber}"));
    assert!(!body.contains("/opds/v1.2/books/book-cbz/pages/{pageNumber}?convert=jpeg"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_detail_includes_page_streaming_link_for_divina_compatible_epub() {
    let paths = new_router_fixture("router-opds-v1-series-detail-epub-stream-link").await;
    seed_router_contract_data(&paths).await;
    seed_router_epub_divina_book(
        &paths,
        "book-epub-divina",
        "series-1",
        "book-epub-divina.epub",
        "Book EPUB Divina",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 series detail epub-stream request should build"),
        )
        .await
        .expect("opds v1 series detail epub-stream request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<id>book-epub-divina</id>"));
    assert!(body.contains("rel=\"http://vaemendis.net/opds-pse/stream\""));
    assert!(body.contains("type=\"image/png\""));
    assert!(body.contains("/opds/v1.2/books/book-epub-divina/pages/{pageNumber}"));
    assert!(!body.contains("/opds/v1.2/books/book-epub-divina/pages/{pageNumber}?convert=jpeg"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_detail_formats_sqlite_naive_updated_like_kotlin() {
    let paths = new_router_fixture("router-opds-v1-series-detail-naive-updated").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 series detail naive-updated db should open");
    sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ? WHERE ID = ?")
        .bind("2024-03-03 00:00:00")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series last modified should update for naive-updated test");
    sqlx::query("UPDATE BOOK SET LAST_MODIFIED_DATE = ? WHERE ID = ?")
        .bind("2024-03-03 01:02:03")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book last modified should update for naive-updated test");
    pool.close().await;

    let series_updated = PrimitiveDateTime::parse(
        "2024-03-03 00:00:00",
        format_description!("[year]-[month]-[day] [hour]:[minute]:[second]"),
    )
    .expect("series naive timestamp should parse")
    .assume_utc()
    .to_offset(UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC))
    .format(&Rfc3339)
    .expect("series updated timestamp should format");
    let book_updated = PrimitiveDateTime::parse(
        "2024-03-03 01:02:03",
        format_description!("[year]-[month]-[day] [hour]:[minute]:[second]"),
    )
    .expect("book naive timestamp should parse")
    .assume_utc()
    .to_offset(UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC))
    .format(&Rfc3339)
    .expect("book updated timestamp should format");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 series detail naive-updated request should build"),
        )
        .await
        .expect("opds v1 series detail naive-updated request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains(format!("<updated>{series_updated}</updated>").as_str()));
    assert!(body.contains(format!("<updated>{book_updated}</updated>").as_str()));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_detail_returns_forbidden_for_age_restricted_user() {
    let paths = new_router_fixture("router-opds-v1-series-detail-age-forbidden").await;
    seed_router_contract_data(&paths).await;
    update_router_series_age_rating(&paths, "series-1", 21).await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;

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
                .uri("/opds/v1.2/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 series detail age-forbidden request should build"),
        )
        .await
        .expect("opds v1 series detail age-forbidden request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_detail_keeps_deleted_series_accessible_like_kotlin() {
    let paths = new_router_fixture("router-opds-v1-series-detail-deleted-series").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 series detail deleted-series db should open");
    sqlx::query("UPDATE SERIES SET DELETED_DATE = ? WHERE ID = ?")
        .bind("2024-03-03 00:00:00")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series deleted date should update for deleted-series test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 series detail deleted-series request should build"),
        )
        .await
        .expect("opds v1 series detail deleted-series request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<id>series-1</id>"));
    assert!(body.contains("<id>book-1</id>"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_collection_detail_returns_empty_feed_when_visible_page_is_empty() {
    let paths = new_router_fixture("router-opds-v1-collection-detail-empty-feed").await;
    seed_router_contract_data(&paths).await;
    update_router_series_age_rating(&paths, "series-1", 21).await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;

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
                .uri("/opds/v1.2/collections/collection-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 collection detail empty-feed request should build"),
        )
        .await
        .expect("opds v1 collection detail empty-feed request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<id>collection-1</id>"));
    assert!(body.contains("<title>Collection 1</title>"));
    assert!(body.contains("rel=\"self\""));
    assert!(body.contains("rel=\"start\""));
    assert!(!body.contains("/opds/v1.2/series/series-1"));
    assert!(!body.contains("<entry>"));
    assert!(!body.contains("rel=\"previous\""));
    assert!(!body.contains("rel=\"next\""));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_collection_detail_uses_collection_and_series_last_modified_timestamps() {
    let paths = new_router_fixture("router-opds-v1-collection-detail-updated").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 collection detail updated db should open");
    sqlx::query("UPDATE COLLECTION SET LAST_MODIFIED_DATE = ? WHERE ID = ?")
        .bind("2024-01-20T01:02:03Z")
        .bind("collection-1")
        .execute(&pool)
        .await
        .expect("collection last modified should update");
    sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ? WHERE ID = ?")
        .bind("2024-01-21T02:03:04Z")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series last modified should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/collections/collection-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 collection detail updated request should build"),
        )
        .await
        .expect("opds v1 collection detail updated request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<updated>2024-01-20T01:02:03Z</updated>"));
    assert!(body.contains("<updated>2024-01-21T02:03:04Z</updated>"));
    assert!(body.contains("/opds/v1.2/series/series-1"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_collection_detail_orders_unordered_entries_by_title_sort() {
    let paths = new_router_fixture("router-opds-v1-collection-detail-unordered-title-sort").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-2", "Alpha Display", "library-1").await;
    update_router_series_metadata_titles(&paths, "series-1", "Zeta Display", "Zulu Sort").await;
    update_router_series_metadata_titles(&paths, "series-2", "Alpha Display", "Alpha Sort").await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 collection detail order db should open");
    sqlx::query("UPDATE COLLECTION SET ORDERED = ? WHERE ID = ?")
        .bind(false)
        .bind("collection-1")
        .execute(&pool)
        .await
        .expect("collection ordered flag should update");
    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind("collection-1")
    .bind("series-2")
    .bind(99_i64)
    .execute(&pool)
    .await
    .expect("second series should be attached to collection");
    sqlx::query(
        "UPDATE COLLECTION_SERIES SET NUMBER = ? WHERE COLLECTION_ID = ? AND SERIES_ID = ?",
    )
    .bind(0_i64)
    .bind("collection-1")
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("first series collection number should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/collections/collection-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 collection detail unordered request should build"),
        )
        .await
        .expect("opds v1 collection detail unordered request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    let alpha_pos = body
        .find("/opds/v1.2/series/series-2")
        .expect("series-2 entry should be present");
    let zeta_pos = body
        .find("/opds/v1.2/series/series-1")
        .expect("series-1 entry should be present");
    assert!(
        alpha_pos < zeta_pos,
        "unordered OPDS v1 collection detail must order by Kotlin titleSort semantics, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_collection_detail_returns_not_found_when_collection_is_outside_shared_libraries()
 {
    let paths =
        new_router_fixture("router-opds-v1-collection-detail-library-scope-not-found").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-user",
        "library-user@example.org",
        "router-contract-library-123",
        &["library-1"],
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 collection detail library-scope db should open");
    sqlx::query("INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)")
        .bind("collection-2")
        .bind("Collection 2")
        .bind(false)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("library-scoped collection should be inserted");
    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind("collection-2")
    .bind("series-3")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("library-scoped collection series should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library-user@example.org",
        "router-contract-library-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/collections/collection-2")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 collection detail library-scope request should build"),
        )
        .await
        .expect("opds v1 collection detail library-scope request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_readlist_detail_returns_empty_feed_when_visible_page_is_empty() {
    let paths = new_router_fixture("router-opds-v1-readlist-detail-empty-feed").await;
    seed_router_contract_data(&paths).await;
    update_router_series_age_rating(&paths, "series-1", 21).await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;

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
                .uri("/opds/v1.2/readlists/readlist-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 readlist detail empty-feed request should build"),
        )
        .await
        .expect("opds v1 readlist detail empty-feed request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<id>readlist-1</id>"));
    assert!(body.contains("<title>ReadList 1</title>"));
    assert!(body.contains("rel=\"self\""));
    assert!(body.contains("rel=\"start\""));
    assert!(!body.contains("<entry>"));
    assert!(!body.contains("rel=\"previous\""));
    assert!(!body.contains("rel=\"next\""));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_readlist_detail_returns_not_found_when_readlist_is_outside_shared_libraries()
 {
    let paths = new_router_fixture("router-opds-v1-readlist-detail-library-scope-not-found").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-user",
        "library-user@example.org",
        "router-contract-library-123",
        &["library-1"],
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 readlist detail library-scope db should open");
    sqlx::query("INSERT INTO READLIST (ID, NAME, BOOK_COUNT, ORDERED) VALUES (?, ?, ?, ?)")
        .bind("readlist-2")
        .bind("ReadList 2")
        .bind(1_i64)
        .bind(false)
        .execute(&pool)
        .await
        .expect("library-scoped readlist should be inserted");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-2")
        .bind("book-3")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("library-scoped readlist book should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library-user@example.org",
        "router-contract-library-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/readlists/readlist-2")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 readlist detail library-scope request should build"),
        )
        .await
        .expect("opds v1 readlist detail library-scope request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_readlist_detail_orders_unordered_entries_by_release_date() {
    let paths = new_router_fixture("router-opds-v1-readlist-detail-unordered-release-date").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 readlist detail order db should open");
    sqlx::query("UPDATE READLIST SET ORDERED = ?, BOOK_COUNT = ? WHERE ID = ?")
        .bind(false)
        .bind(2_i64)
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist ordered flag should update");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-1")
        .bind("book-2")
        .bind(99_i64)
        .execute(&pool)
        .await
        .expect("second readlist book should be inserted");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/epub+zip")
        .bind("READY")
        .bind("book-2")
        .bind(10_i64)
        .execute(&pool)
        .await
        .expect("book-2 ready media row should be inserted for unordered readlist test");
    sqlx::query("UPDATE BOOK_METADATA SET RELEASE_DATE = ? WHERE BOOK_ID = ?")
        .bind("2024-01-16")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book-1 release date should update");
    sqlx::query("UPDATE BOOK_METADATA SET RELEASE_DATE = ? WHERE BOOK_ID = ?")
        .bind("2024-01-14")
        .bind("book-2")
        .execute(&pool)
        .await
        .expect("book-2 release date should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/readlists/readlist-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 readlist detail unordered request should build"),
        )
        .await
        .expect("opds v1 readlist detail unordered request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    let earlier_pos = body
        .find("<id>book-2</id>")
        .expect("book-2 entry should be present");
    let later_pos = body
        .find("<id>book-1</id>")
        .expect("book-1 entry should be present");
    assert!(
        earlier_pos < later_pos,
        "unordered OPDS v1 readlist detail must order by Kotlin releaseDate semantics, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_readlist_detail_uses_readlist_and_book_last_modified_timestamps() {
    let paths = new_router_fixture("router-opds-v1-readlist-detail-updated").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 readlist detail updated db should open");
    sqlx::query("UPDATE READLIST SET LAST_MODIFIED_DATE = ? WHERE ID = ?")
        .bind("2024-01-22T01:02:03Z")
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist last modified should update");
    sqlx::query("UPDATE BOOK SET LAST_MODIFIED_DATE = ? WHERE ID = ?")
        .bind("2024-01-23T02:03:04Z")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book last modified should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/readlists/readlist-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 readlist detail updated request should build"),
        )
        .await
        .expect("opds v1 readlist detail updated request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<updated>2024-01-22T01:02:03Z</updated>"));
    assert!(body.contains("<updated>2024-01-23T02:03:04Z</updated>"));
    assert!(body.contains("<id>book-1</id>"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_readlist_detail_uses_kotlin_acquisition_entry_shape() {
    let paths = new_router_fixture("router-opds-v1-readlist-detail-entry-shape").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/readlists/readlist-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 readlist detail entry-shape request should build"),
        )
        .await
        .expect("opds v1 readlist detail entry-shape request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<title>Series 1 1: Book 1</title>"));
    assert!(body.contains("<content>epub - 1024</content>"));
    assert!(body.contains("<author><name>Jane Writer</name></author>"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_readlist_detail_filters_non_ready_books_without_turning_empty_scope_into_not_found()
 {
    let paths = new_router_fixture("router-opds-v1-readlist-detail-ready-only").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 readlist detail ready-only db should open");
    sqlx::query("UPDATE READLIST SET ORDERED = ?, BOOK_COUNT = ? WHERE ID = ?")
        .bind(true)
        .bind(2_i64)
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist ordered flag should update for ready-only test");
    sqlx::query("UPDATE MEDIA SET STATUS = ? WHERE BOOK_ID = ?")
        .bind("ERROR")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book-1 media status should update to non-ready");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/epub+zip")
        .bind("READY")
        .bind("book-2")
        .bind(10_i64)
        .execute(&pool)
        .await
        .expect("book-2 ready media row should be inserted");
    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-1")
        .bind("book-2")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("book-2 should be attached to readlist-1");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/readlists/readlist-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 readlist detail ready-only request should build"),
        )
        .await
        .expect("opds v1 readlist detail ready-only request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<id>readlist-1</id>"));
    assert!(body.contains("<id>book-2</id>"));
    assert!(!body.contains("<id>book-1</id>"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_library_detail_orders_series_by_title_sort() {
    let paths = new_router_fixture("router-opds-v1-library-detail-title-sort").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-2", "Alpha Display", "library-1").await;
    update_router_series_metadata_titles(&paths, "series-1", "Zeta Display", "Alpha Sort").await;
    update_router_series_metadata_titles(&paths, "series-2", "Alpha Display", "Zeta Sort").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 library detail request should build"),
        )
        .await
        .expect("opds v1 library detail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    let series_1_pos = body
        .find("/opds/v1.2/series/series-1")
        .expect("series-1 entry should be present");
    let series_2_pos = body
        .find("/opds/v1.2/series/series-2")
        .expect("series-2 entry should be present");
    assert!(
        series_1_pos < series_2_pos,
        "OPDS v1 library detail should order by Kotlin titleSort semantics, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_library_detail_hides_age_restricted_series() {
    let paths = new_router_fixture("router-opds-v1-library-detail-age-restricted").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-0", "Visible Series", "library-1").await;
    update_router_series_age_rating(&paths, "series-0", 0).await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;

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
                .uri("/opds/v1.2/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 restricted library detail request should build"),
        )
        .await
        .expect("opds v1 restricted library detail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(!body.contains("/opds/v1.2/series/series-1"));
    assert!(body.contains("/opds/v1.2/series/series-0"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_library_detail_paginates_after_restrictions_filtering() {
    let paths = new_router_fixture("router-opds-v1-library-detail-filtered-pagination").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-0", "Visible Series", "library-1").await;
    seed_router_custom_series(&paths, "series-2", "Restricted Series 2", "library-1").await;
    update_router_series_metadata_titles(&paths, "series-2", "Restricted Series 2", "Alpha Sort")
        .await;
    update_router_series_metadata_titles(&paths, "series-1", "Restricted Series 1", "Beta Sort")
        .await;
    update_router_series_metadata_titles(&paths, "series-0", "Visible Series", "Gamma Sort").await;
    update_router_series_age_rating(&paths, "series-2", 18).await;
    update_router_series_age_rating(&paths, "series-1", 18).await;
    update_router_series_age_rating(&paths, "series-0", 0).await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;

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
                .uri("/opds/v1.2/libraries/library-1?page=0&size=1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 restricted library detail paged request should build"),
        )
        .await
        .expect("opds v1 restricted library detail paged request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("/opds/v1.2/series/series-0"));
    assert!(!body.contains("/opds/v1.2/series/series-1"));
    assert!(!body.contains("/opds/v1.2/series/series-2"));
    assert!(
        !body.contains("rel=\"next\""),
        "OPDS v1 library detail must paginate after restrictions filtering, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_publishers_preserves_unicode_collation_order() {
    let paths = new_router_fixture("router-opds-v1-publishers-unicode-order").await;
    seed_router_contract_data(&paths).await;
    update_router_series_publisher(&paths, "series-1", "Zulu House").await;
    seed_router_custom_series(&paths, "series-ang", "Series Å", "library-1").await;
    update_router_series_publisher(&paths, "series-ang", "Ångström Press").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/publishers")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 publishers request should build"),
        )
        .await
        .expect("opds v1 publishers request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    let angstrom_pos = body
        .find("publisher:%C3%85ngstr%C3%B6m%20Press")
        .expect("Ångström publisher entry should be present");
    let zulu_pos = body
        .find("publisher:Zulu%20House")
        .expect("Zulu publisher entry should be present");
    assert!(
        angstrom_pos < zulu_pos,
        "OPDS v1 publishers should keep Kotlin Unicode collation order, body={body}"
    );

    cleanup_router_fixture(paths);
}

use super::*;

#[tokio::test]
async fn router_opds_v1_series_detail_filters_non_ready_books() {
    let ctx = TestFixture::new("router-opds-v1-series-detail-ready-only").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("opds v1 series detail ready-only db should open");
    sqlx::query("UPDATE MEDIA SET STATUS = ? WHERE BOOK_ID = ?")
        .bind("ERROR")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book-1 media status should update to non-ready");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_opds_v1_series_detail_uses_kotlin_acquisition_entry_shape() {
    let ctx = TestFixture::new("router-opds-v1-series-detail-entry-shape").await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_opds_v1_series_detail_formats_summary_content_like_kotlin() {
    let ctx = TestFixture::new("router-opds-v1-series-detail-summary-content").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("opds v1 series detail summary-content db should open");
    sqlx::query("UPDATE BOOK_METADATA SET SUMMARY = ? WHERE BOOK_ID = ?")
        .bind("Line one\nLine two")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book summary should update for summary-content test");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_opds_v1_series_detail_includes_page_streaming_link_for_pdf_books() {
    let ctx = TestFixture::new("router-opds-v1-series-detail-pdf-stream-link").await;
    seed_router_pdf_book(
        ctx.paths(),
        "book-pdf",
        "series-1",
        "book-pdf.pdf",
        "Book PDF",
    )
    .await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_opds_v1_series_detail_declares_pse_namespace_for_page_streaming_links() {
    let ctx = TestFixture::new("router-opds-v1-series-detail-pse-namespace").await;
    seed_router_pdf_book(
        ctx.paths(),
        "book-pdf",
        "series-1",
        "book-pdf.pdf",
        "Book PDF",
    )
    .await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_opds_v1_series_detail_includes_read_progress_attributes_on_page_streaming_link() {
    let ctx = TestFixture::new("router-opds-v1-series-detail-pse-read-progress").await;
    seed_router_pdf_book(
        ctx.paths(),
        "book-pdf",
        "series-1",
        "book-pdf.pdf",
        "Book PDF",
    )
    .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
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

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_opds_v1_series_detail_includes_page_streaming_link_for_cbz_books() {
    let ctx = TestFixture::new("router-opds-v1-series-detail-cbz-stream-link").await;
    seed_router_cbz_book(
        ctx.paths(),
        "book-cbz",
        "series-1",
        "book-cbz.cbz",
        "Book CBZ",
    )
    .await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_opds_v1_series_detail_includes_page_streaming_link_for_divina_compatible_epub() {
    let ctx = TestFixture::new("router-opds-v1-series-detail-epub-stream-link").await;
    seed_router_epub_divina_book(
        ctx.paths(),
        "book-epub-divina",
        "series-1",
        "book-epub-divina.epub",
        "Book EPUB Divina",
    )
    .await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_opds_v1_series_detail_formats_sqlite_naive_updated_like_kotlin() {
    let ctx = TestFixture::new("router-opds-v1-series-detail-naive-updated").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
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

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_opds_v1_series_detail_returns_forbidden_for_age_restricted_user() {
    let ctx = TestFixture::new("router-opds-v1-series-detail-age-forbidden").await;
    update_router_series_age_rating(ctx.paths(), "series-1", 21).await;
    seed_router_age_exclude_user(
        ctx.paths(),
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;

    let auth_token = ctx
        .login_with_credentials("restricted@example.org", "router-contract-restricted-123")
        .await;

    let response = ctx
        .app()
        .clone()
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
}

#[tokio::test]
async fn router_opds_v1_series_detail_keeps_deleted_series_accessible_like_kotlin() {
    let ctx = TestFixture::new("router-opds-v1-series-detail-deleted-series").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("opds v1 series detail deleted-series db should open");
    sqlx::query("UPDATE SERIES SET DELETED_DATE = ? WHERE ID = ?")
        .bind("2024-03-03 00:00:00")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series deleted date should update for deleted-series test");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
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
}

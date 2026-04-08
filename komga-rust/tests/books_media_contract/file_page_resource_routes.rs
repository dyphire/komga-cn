use super::*;

#[tokio::test]
async fn router_book_pages_and_raw_pages_include_inline_content_disposition() {
    let paths = new_router_fixture("router-book-pages-inline-disposition").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;
    update_router_book_name(&paths, "book-pdf-1", "Readable Page Title").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for route in [
        "/api/v1/books/book-pdf-1/pages/1",
        "/api/v1/books/book-pdf-1/pages/1/raw",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("page request should build"),
            )
            .await
            .expect("page request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
        let disposition = response
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .and_then(|value| value.to_str().ok())
            .expect("page response should expose content-disposition");
        assert!(
            disposition.starts_with("inline;"),
            "route: {route}, disposition: {disposition}"
        );
        assert!(
            disposition.contains("Readable Page Title-1"),
            "route: {route}, disposition: {disposition}"
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_page_thumbnail_resizes_largest_dimension_to_300px() {
    let paths = new_router_fixture("router-book-page-thumbnail-300px").await;
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

    let pages_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book pages request should build"),
        )
        .await
        .expect("book pages request should complete");
    assert_eq!(pages_response.status(), StatusCode::OK);
    let pages_payload = response_json(pages_response).await;
    let rows = pages_payload
        .as_array()
        .expect("book pages payload should be an array");
    let page_max_dimension = rows[0]
        .get("width")
        .and_then(Value::as_u64)
        .expect("book page metadata should expose width")
        .max(
            rows[0]
                .get("height")
                .and_then(Value::as_u64)
                .expect("book page metadata should expose height"),
        );

    let thumbnail_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book page thumbnail request should build"),
        )
        .await
        .expect("book page thumbnail request should complete");
    assert_eq!(thumbnail_response.status(), StatusCode::OK);
    let thumbnail_bytes = axum::body::to_bytes(thumbnail_response.into_body(), usize::MAX)
        .await
        .expect("book page thumbnail body should read");
    let thumbnail_image = image::load_from_memory(&thumbnail_bytes)
        .expect("book page thumbnail should decode as image");
    let thumbnail_max_dimension = thumbnail_image.width().max(thumbnail_image.height());

    assert!(
        page_max_dimension > 300,
        "page max dimension was {page_max_dimension}"
    );
    assert_eq!(thumbnail_max_dimension, 300);
    assert!(u64::from(thumbnail_image.width()) <= page_max_dimension);
    assert!(u64::from(thumbnail_image.height()) <= page_max_dimension);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_page_thumbnail_returns_bad_request_for_missing_page_number() {
    let paths = new_router_fixture("router-book-page-thumbnail-missing-page-number").await;
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

    for route in [
        "/api/v1/books/book-pdf-1/pages/0/thumbnail",
        "/api/v1/books/book-pdf-1/pages/2/thumbnail",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("book page thumbnail missing-page request should build"),
            )
            .await
            .expect("book page thumbnail missing-page request should complete");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "route: {route}");
        let payload = response_json(response).await;
        assert_eq!(
            payload.get("error"),
            Some(&Value::String("Page number does not exist".to_string())),
            "route: {route}"
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_book_page_uses_zero_based_external_page_numbers() {
    let paths = new_router_fixture("router-opds-v1-book-page-zero-based").await;
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

    let zero_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/books/book-pdf-1/pages/0")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 zero-based page request should build"),
        )
        .await
        .expect("opds v1 zero-based page request should complete");

    assert_eq!(zero_response.status(), StatusCode::OK);

    let one_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/books/book-pdf-1/pages/1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 one-based page request should build"),
        )
        .await
        .expect("opds v1 one-based page request should complete");

    assert_eq!(one_response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(one_response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Page number does not exist".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_book_page_routes_do_not_negotiate_pdf_for_pdf_books() {
    let paths = new_router_fixture("router-opds-book-page-no-pdf-negotiation").await;
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

    for route in [
        "/opds/v1.2/books/book-pdf-1/pages/0",
        "/opds/v2/books/book-pdf-1/pages/1",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .header(header::ACCEPT, "application/pdf")
                    .body(Body::empty())
                    .expect("opds pdf negotiation request should build"),
            )
            .await
            .expect("opds pdf negotiation request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .expect("opds page should expose content-type");
        assert!(
            content_type.starts_with("image/"),
            "route: {route}, content-type was: {content_type}"
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_does_not_return_not_modified_before_page_streaming_check() {
    let paths = new_router_fixture("router-book-raw-page-no-304-before-role-check").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "admin-access-no-page-streaming",
        "admin-access-no-page-streaming@example.org",
        "router-contract-admin-access-123",
        0,
        &["USER", "ADMIN"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "admin-access-no-page-streaming@example.org",
        "router-contract-admin-access-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1/raw")
                .header("x-auth-token", &auth_token)
                .header(header::IF_MODIFIED_SINCE, "Wed, 01 Jan 2099 00:00:00 +0000")
                .body(Body::empty())
                .expect("book raw page role-order request should build"),
        )
        .await
        .expect("book raw page role-order request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_returns_bad_request_for_negative_page_number() {
    let paths = new_router_fixture("router-book-raw-page-negative-page-number").await;
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
                .uri("/api/v1/books/book-pdf-1/pages/-1/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("negative raw page request should build"),
        )
        .await
        .expect("negative raw page request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Page number does not exist".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_returns_bad_request_for_non_integer_page_number() {
    let paths = new_router_fixture("router-book-raw-page-non-integer-page-number").await;
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
                .uri("/api/v1/books/book-pdf-1/pages/abc/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("non-integer raw page request should build"),
        )
        .await
        .expect("non-integer raw page request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_content_disposition_uses_book_name_not_metadata_title() {
    let paths = new_router_fixture("router-book-raw-page-uses-book-name").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Metadata Title",
    )
    .await;
    update_router_book_name(&paths, "book-pdf-1", "Filesystem Shelf Name").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book raw page request should build"),
        )
        .await
        .expect("book raw page request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let disposition = response
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .expect("raw page response should expose content-disposition");
    assert!(
        disposition.contains("Filesystem Shelf Name-1"),
        "disposition was: {disposition}"
    );
    assert!(
        !disposition.contains("Metadata Title-1"),
        "disposition was: {disposition}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_returns_bad_request_for_missing_pdf_page_number() {
    let paths = new_router_fixture("router-book-page-missing-pdf-page").await;
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
                .uri("/api/v1/books/book-pdf-1/pages/2/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing raw pdf page request should build"),
        )
        .await
        .expect("missing raw pdf page request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Page number does not exist".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_returns_not_found_with_message_when_media_not_ready() {
    let paths = new_router_fixture("router-book-raw-page-media-not-ready").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book raw page not-ready db should open");
    sqlx::query("UPDATE MEDIA SET STATUS = ? WHERE BOOK_ID = ?")
        .bind("OUTDATED")
        .bind("book-pdf-1")
        .execute(&pool)
        .await
        .expect("media status should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("not-ready raw pdf page request should build"),
        )
        .await
        .expect("not-ready raw pdf page request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Book analysis failed".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_forbidden_before_not_ready_for_restricted_user() {
    let paths = new_router_fixture("router-book-raw-page-restricted-before-not-ready").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "restricted-page-user",
        "restricted-page-user@example.org",
        "router-contract-restricted-page-123",
        16,
        &["USER", "PAGE_STREAMING"],
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book raw page restricted db should open");
    sqlx::query("UPDATE MEDIA SET STATUS = ? WHERE BOOK_ID = ?")
        .bind("OUTDATED")
        .bind("book-pdf-1")
        .execute(&pool)
        .await
        .expect("media status should update");
    sqlx::query("UPDATE SERIES_METADATA SET AGE_RATING = ? WHERE SERIES_ID = ?")
        .bind(18_i64)
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series age rating should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted-page-user@example.org",
        "router-contract-restricted-page-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("restricted raw pdf page request should build"),
        )
        .await
        .expect("restricted raw pdf page request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_returns_not_found_with_message_when_file_is_missing() {
    let paths = new_router_fixture("router-book-raw-page-file-missing").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let pdf_path = paths.config_dir.join("books/fixture-page.pdf");
    std::fs::remove_file(&pdf_path).expect("pdf fixture should be removable");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("file-missing raw pdf page request should build"),
        )
        .await
        .expect("file-missing raw pdf page request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "File not found, it may have moved".to_string()
        ))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_returns_not_modified_before_not_ready_checks() {
    let paths = new_router_fixture("router-book-raw-page-not-modified-before-not-ready").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book raw page not-ready db should open");
    sqlx::query("UPDATE MEDIA SET STATUS = ? WHERE BOOK_ID = ?")
        .bind("OUTDATED")
        .bind("book-pdf-1")
        .execute(&pool)
        .await
        .expect("media status should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1/raw")
                .header("x-auth-token", &auth_token)
                .header(header::IF_MODIFIED_SINCE, "Wed, 01 Jan 2099 00:00:00 +0000")
                .body(Body::empty())
                .expect("not-modified raw pdf page request should build"),
        )
        .await
        .expect("not-modified raw pdf page request should complete");

    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_resource_supports_not_modified_and_inline_content_disposition() {
    let paths = new_router_fixture("router-book-resource-inline-not-modified").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_resource(
        &paths,
        "books/book-1.epub",
        "OEBPS/chapter.xhtml",
        br#"<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><p>Hello</p></body></html>"#,
    );

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for route in [
        "/api/v1/books/book-1/resource/OEBPS/chapter.xhtml",
        "/opds/v2/books/book-1/resource/OEBPS/chapter.xhtml",
    ] {
        let initial = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("resource request should build"),
            )
            .await
            .expect("resource request should complete");

        assert_eq!(initial.status(), StatusCode::OK, "route: {route}");
        let last_modified = initial
            .headers()
            .get(header::LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .expect("resource response should expose last-modified")
            .to_string();
        let disposition = initial
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .and_then(|value| value.to_str().ok())
            .expect("resource response should expose content-disposition");
        assert!(
            disposition.starts_with("inline;"),
            "route: {route}, disposition: {disposition}"
        );
        assert!(
            disposition.contains("chapter.xhtml"),
            "route: {route}, disposition: {disposition}"
        );
        assert_eq!(
            initial
                .headers()
                .get(header::CONTENT_SECURITY_POLICY)
                .and_then(|value| value.to_str().ok()),
            Some("script-src 'none'; object-src 'none';"),
            "route: {route}"
        );

        let not_modified = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .header(header::IF_MODIFIED_SINCE, &last_modified)
                    .body(Body::empty())
                    .expect("conditional resource request should build"),
            )
            .await
            .expect("conditional resource request should complete");

        assert_eq!(
            not_modified.status(),
            StatusCode::NOT_MODIFIED,
            "route: {route}"
        );
        assert_eq!(
            not_modified
                .headers()
                .get(header::LAST_MODIFIED)
                .and_then(|value| value.to_str().ok()),
            Some(last_modified.as_str()),
            "route: {route}"
        );
    }

    cleanup_router_fixture(paths);
}

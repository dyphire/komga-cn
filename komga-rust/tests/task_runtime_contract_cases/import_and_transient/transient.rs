use super::*;

#[tokio::test]
async fn router_transient_books_scan_and_analyze_returns_non_placeholder_payload() {
    let paths = new_router_fixture("router-transient-books-scan-analyze").await;
    seed_router_contract_data(&paths).await;

    let transient_dir = unique_transient_dir("scan-analyze");
    std::fs::create_dir_all(&transient_dir).expect("transient import directory should be created");
    let candidate_file = transient_dir.join("candidate.jpg");
    let candidate_bytes = b"transient-image-bytes";
    std::fs::write(&candidate_file, candidate_bytes)
        .expect("transient candidate file should be written");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let scan_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/transient-books")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(transient_scan_payload(&transient_dir)))
                .expect("transient scan request should build"),
        )
        .await
        .expect("transient scan request should complete");
    assert_eq!(scan_response.status(), StatusCode::OK);

    let scanned_payload = response_json(scan_response).await;
    let scanned_books = scanned_payload
        .as_array()
        .expect("transient scan payload should be an array");
    assert_eq!(scanned_books.len(), 1);
    let scanned_book = &scanned_books[0];
    assert!(scanned_book.get("path").is_none());
    assert_eq!(
        scanned_book.get("url"),
        Some(&Value::String(candidate_file.to_string_lossy().to_string())),
    );
    assert_eq!(
        scanned_book.get("status"),
        Some(&Value::String("UNKNOWN".to_string())),
    );
    assert!(
        scanned_book
            .get("size")
            .and_then(Value::as_str)
            .is_some_and(|size| !size.is_empty())
    );

    let transient_id = scanned_book
        .get("id")
        .and_then(Value::as_str)
        .expect("transient scan payload should include id")
        .to_string();

    let analyze_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/transient-books/{transient_id}/analyze"))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("transient analyze request should build"),
        )
        .await
        .expect("transient analyze request should complete");
    assert_eq!(analyze_response.status(), StatusCode::OK);

    let analyzed_payload = response_json(analyze_response).await;
    assert_eq!(
        analyzed_payload.get("status"),
        Some(&Value::String("READY".to_string())),
    );
    let analyzed_pages = analyzed_payload
        .get("pages")
        .and_then(Value::as_array)
        .expect("transient analyze payload should include pages");
    assert_eq!(analyzed_pages.len(), 1);
    assert_eq!(
        analyzed_pages[0].get("fileName"),
        Some(&Value::String("candidate.jpg".to_string())),
    );

    let page_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/transient-books/{transient_id}/pages/1"))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("transient page request should build"),
        )
        .await
        .expect("transient page request should complete");
    assert_eq!(page_response.status(), StatusCode::OK);
    assert_eq!(
        page_response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    let page_bytes = to_bytes(page_response.into_body(), usize::MAX)
        .await
        .expect("transient page response body should be readable");
    assert_eq!(page_bytes.as_ref(), candidate_bytes);

    let invalid_page_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/transient-books/{transient_id}/pages/2"))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("transient invalid page request should build"),
        )
        .await
        .expect("transient invalid page request should complete");
    assert_eq!(invalid_page_response.status(), StatusCode::BAD_REQUEST);
    let invalid_page_payload = response_json(invalid_page_response).await;
    assert_json_error(&invalid_page_payload, "Page number does not exist");

    cleanup_router_fixture(paths);
    let _ = std::fs::remove_dir_all(&transient_dir);
}

#[tokio::test]
async fn router_transient_book_page_returns_not_found_for_missing_id_like_kotlin() {
    let paths = new_router_fixture("router-transient-page-missing-id").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/transient-books/transient-missing/pages/1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing-id transient page request should build"),
        )
        .await
        .expect("missing-id transient page request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_transient_book_page_returns_not_found_with_message_when_media_not_ready_like_kotlin()
 {
    let paths = new_router_fixture("router-transient-page-not-ready").await;
    seed_router_contract_data(&paths).await;

    let transient_dir = unique_transient_dir("page-not-ready");
    std::fs::create_dir_all(&transient_dir).expect("not-ready transient directory should exist");
    let candidate_file = transient_dir.join("candidate.jpg");
    std::fs::write(&candidate_file, b"transient-image-bytes")
        .expect("not-ready transient candidate file should be written");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let scan_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/transient-books")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(transient_scan_payload(&transient_dir)))
                .expect("not-ready transient page scan request should build"),
        )
        .await
        .expect("not-ready transient page scan request should complete");
    let transient_id = transient_id_from_scan_payload(
        &response_json(scan_response).await,
        "not-ready transient page scan",
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/transient-books/{transient_id}/pages/1"))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("not-ready transient page request should build"),
        )
        .await
        .expect("not-ready transient page request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let payload = response_json(response).await;
    assert_json_error(&payload, "Book analysis failed");

    cleanup_router_fixture(paths);
    let _ = std::fs::remove_dir_all(&transient_dir);
}

#[tokio::test]
async fn router_transient_book_page_returns_not_found_with_message_when_file_is_missing_like_kotlin()
 {
    let paths = new_router_fixture("router-transient-page-file-missing").await;
    seed_router_contract_data(&paths).await;

    let transient_dir = unique_transient_dir("page-file-missing");
    std::fs::create_dir_all(&transient_dir).expect("file-missing transient directory should exist");
    let candidate_file = transient_dir.join("candidate.jpg");
    std::fs::write(&candidate_file, b"transient-image-bytes")
        .expect("file-missing transient candidate file should be written");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let scan_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/transient-books")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(transient_scan_payload(&transient_dir)))
                .expect("file-missing transient page scan request should build"),
        )
        .await
        .expect("file-missing transient page scan request should complete");
    let transient_id = transient_id_from_scan_payload(
        &response_json(scan_response).await,
        "file-missing transient page scan",
    );

    let analyze_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/transient-books/{transient_id}/analyze"))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("file-missing transient page analyze request should build"),
        )
        .await
        .expect("file-missing transient page analyze request should complete");
    assert_eq!(analyze_response.status(), StatusCode::OK);

    std::fs::remove_file(&candidate_file)
        .expect("file-missing transient candidate should be removable");

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/transient-books/{transient_id}/pages/1"))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("file-missing transient page request should build"),
        )
        .await
        .expect("file-missing transient page request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let payload = response_json(response).await;
    assert_json_error(&payload, "File not found, it may have moved");

    cleanup_router_fixture(paths);
    let _ = std::fs::remove_dir_all(&transient_dir);
}

#[tokio::test]
async fn router_transient_book_page_returns_rendered_jpeg_for_pdf_like_kotlin() {
    let paths = new_router_fixture("router-transient-page-pdf").await;
    seed_router_contract_data(&paths).await;

    let transient_dir = unique_transient_dir("page-pdf");
    std::fs::create_dir_all(&transient_dir).expect("pdf transient directory should exist");
    let candidate_file = transient_dir.join("candidate.pdf");
    write_single_page_pdf_fixture(&candidate_file);

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let scan_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/transient-books")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(transient_scan_payload(&transient_dir)))
                .expect("pdf transient page scan request should build"),
        )
        .await
        .expect("pdf transient page scan request should complete");
    let transient_id = transient_id_from_scan_payload(
        &response_json(scan_response).await,
        "pdf transient page scan",
    );

    let analyze_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/transient-books/{transient_id}/analyze"))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("pdf transient page analyze request should build"),
        )
        .await
        .expect("pdf transient page analyze request should complete");
    assert_eq!(analyze_response.status(), StatusCode::OK);

    let page_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/transient-books/{transient_id}/pages/1"))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("pdf transient page request should build"),
        )
        .await
        .expect("pdf transient page request should complete");

    assert_eq!(page_response.status(), StatusCode::OK);
    assert_eq!(
        page_response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    let page_bytes = to_bytes(page_response.into_body(), usize::MAX)
        .await
        .expect("pdf transient page response body should be readable");
    assert!(
        !page_bytes.is_empty(),
        "expected rendered jpeg bytes for transient pdf page"
    );

    cleanup_router_fixture(paths);
    let _ = std::fs::remove_dir_all(&transient_dir);
}

#[tokio::test]
async fn router_transient_book_analyze_returns_not_found_for_missing_id_like_kotlin() {
    let paths = new_router_fixture("router-transient-book-analyze-missing-id").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/transient-books/transient-missing/analyze")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing-id transient analyze request should build"),
        )
        .await
        .expect("missing-id transient analyze request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_transient_book_analyze_returns_error_dto_when_file_is_missing_like_kotlin() {
    let paths = new_router_fixture("router-transient-book-analyze-missing-file").await;
    seed_router_contract_data(&paths).await;

    let transient_dir = unique_transient_dir("analyze-missing-file");
    std::fs::create_dir_all(&transient_dir)
        .expect("missing-file analyze transient dir should exist");
    let candidate_file = transient_dir.join("candidate.jpg");
    std::fs::write(&candidate_file, b"transient-image-bytes")
        .expect("missing-file analyze candidate file should be written");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let scan_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/transient-books")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(transient_scan_payload(&transient_dir)))
                .expect("missing-file analyze scan request should build"),
        )
        .await
        .expect("missing-file analyze scan request should complete");
    assert_eq!(scan_response.status(), StatusCode::OK);
    let transient_id = transient_id_from_scan_payload(
        &response_json(scan_response).await,
        "missing-file analyze scan",
    );

    std::fs::remove_file(&candidate_file)
        .expect("missing-file analyze candidate should be removed");

    let analyze_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/transient-books/{transient_id}/analyze"))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing-file transient analyze request should build"),
        )
        .await
        .expect("missing-file transient analyze request should complete");

    assert_eq!(analyze_response.status(), StatusCode::OK);
    let payload = response_json(analyze_response).await;
    assert_eq!(
        payload.get("status"),
        Some(&Value::String("ERROR".to_string()))
    );
    assert_eq!(
        payload.get("mediaType"),
        Some(&Value::String(String::new()))
    );
    assert_eq!(
        payload.get("comment"),
        Some(&Value::String("ERR_1018".to_string()))
    );

    cleanup_router_fixture(paths);
    let _ = std::fs::remove_dir_all(&transient_dir);
}

#[tokio::test]
async fn router_transient_book_analyze_returns_error_dto_for_broken_epub_like_kotlin() {
    let paths = new_router_fixture("router-transient-book-analyze-broken-epub").await;
    seed_router_contract_data(&paths).await;

    let transient_dir = unique_transient_dir("analyze-broken-epub");
    std::fs::create_dir_all(&transient_dir)
        .expect("broken-epub analyze transient dir should exist");
    let candidate_file = transient_dir.join("Series 1 7.epub");
    write_zip_as_epub(&candidate_file);

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let scan_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/transient-books")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(transient_scan_payload(&transient_dir)))
                .expect("broken-epub analyze scan request should build"),
        )
        .await
        .expect("broken-epub analyze scan request should complete");
    assert_eq!(scan_response.status(), StatusCode::OK);
    let transient_id = transient_id_from_scan_payload(
        &response_json(scan_response).await,
        "broken-epub analyze scan",
    );

    let analyze_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/transient-books/{transient_id}/analyze"))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("broken-epub transient analyze request should build"),
        )
        .await
        .expect("broken-epub transient analyze request should complete");

    assert_eq!(analyze_response.status(), StatusCode::OK);
    let payload = response_json(analyze_response).await;
    assert_eq!(
        payload.get("status"),
        Some(&Value::String("ERROR".to_string()))
    );
    assert_eq!(
        payload.get("mediaType"),
        Some(&Value::String("application/zip".to_string()))
    );
    assert_eq!(
        payload.get("comment"),
        Some(&Value::String("ERR_1032".to_string()))
    );
    assert_eq!(payload.get("pages"), Some(&Value::Array(Vec::new())));
    assert_eq!(
        payload.get("seriesId"),
        Some(&Value::String("series-1".to_string()))
    );
    assert_eq!(payload.get("number"), Some(&Value::from(7.0)));

    cleanup_router_fixture(paths);
    let _ = std::fs::remove_dir_all(&transient_dir);
}

#[tokio::test]
async fn router_transient_book_analyze_supports_epub_resources() {
    let paths = new_router_fixture("router-transient-books-analyze-epub").await;
    seed_router_contract_data(&paths).await;

    let transient_dir = unique_transient_dir("analyze-epub");
    std::fs::create_dir_all(&transient_dir).expect("transient epub directory should be created");
    let external_paths = RuntimeDbPaths {
        config_dir: transient_dir.clone(),
        main_db: paths.main_db.clone(),
        tasks_db: paths.tasks_db.clone(),
    };
    write_router_epub_resource(
        &external_paths,
        "candidate.epub",
        "OEBPS/chapter.xhtml",
        br#"<html xmlns='http://www.w3.org/1999/xhtml'><body><p>Transient EPUB</p></body></html>"#,
    );

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let scan_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/transient-books")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(transient_scan_payload(&transient_dir)))
                .expect("transient epub scan request should build"),
        )
        .await
        .expect("transient epub scan request should complete");
    assert_eq!(scan_response.status(), StatusCode::OK);

    let scanned_payload = response_json(scan_response).await;
    let transient_id = transient_id_from_scan_payload(&scanned_payload, "transient epub scan");

    let analyze_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/transient-books/{transient_id}/analyze"))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("transient epub analyze request should build"),
        )
        .await
        .expect("transient epub analyze request should complete");
    assert_eq!(analyze_response.status(), StatusCode::OK);

    let analyzed_payload = response_json(analyze_response).await;
    assert_eq!(
        analyzed_payload.get("status"),
        Some(&Value::String("READY".to_string()))
    );
    assert_eq!(
        analyzed_payload.get("mediaType"),
        Some(&Value::String("application/epub+zip".to_string()))
    );
    let pages = analyzed_payload
        .get("pages")
        .and_then(Value::as_array)
        .expect("transient epub analyze payload should include pages");
    assert_eq!(pages.len(), 1);
    assert_eq!(
        pages[0].get("fileName"),
        Some(&Value::String("OEBPS/chapter.xhtml".to_string()))
    );
    assert_eq!(
        pages[0].get("mediaType"),
        Some(&Value::String("application/xhtml+xml".to_string()))
    );

    let page_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/transient-books/{transient_id}/pages/1"))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("transient epub page request should build"),
        )
        .await
        .expect("transient epub page request should complete");
    assert_eq!(page_response.status(), StatusCode::OK);
    assert_eq!(
        page_response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/xhtml+xml")
    );

    cleanup_router_fixture(paths);
    let _ = std::fs::remove_dir_all(&transient_dir);
}

#[tokio::test]
async fn router_transient_books_rejects_paths_inside_existing_library_with_err_1017_like_kotlin() {
    let paths = new_router_fixture("router-transient-books-library-contained").await;
    seed_router_contract_data(&paths).await;

    let contained_dir = paths.config_dir.join("transient-contained");
    std::fs::create_dir_all(&contained_dir).expect("contained transient directory should exist");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/transient-books")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "path": contained_dir.to_string_lossy().to_string(),
                    })
                    .to_string(),
                ))
                .expect("contained transient scan request should build"),
        )
        .await
        .expect("contained transient scan request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_spring_bad_request(&payload, "ERR_1017", "/api/v1/transient-books");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_transient_books_does_not_rebase_relative_paths_to_config_dir_like_kotlin() {
    let paths = new_router_fixture("router-transient-books-relative-path").await;
    seed_router_contract_data(&paths).await;

    let relative_name = format!(
        "transient-relative-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos()
    );
    let config_scoped_dir = paths.config_dir.join(&relative_name);
    std::fs::create_dir_all(&config_scoped_dir)
        .expect("config-scoped transient directory should exist");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/transient-books")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "path": relative_name,
                    })
                    .to_string(),
                ))
                .expect("relative transient scan request should build"),
        )
        .await
        .expect("relative transient scan request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_spring_bad_request(&payload, "ERR_1016", "/api/v1/transient-books");

    cleanup_router_fixture(paths);
}

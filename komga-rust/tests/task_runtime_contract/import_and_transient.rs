use super::*;

#[tokio::test]
async fn router_books_import_enqueues_individual_tasks_in_tasks_db() {
    let paths = new_router_fixture("router-books-import-individual-tasks").await;
    seed_router_contract_data(&paths).await;

    let source_a = paths.config_dir.join("incoming-a.cbz");
    let source_b = paths.config_dir.join("incoming-b.cbz");
    std::fs::write(&source_a, b"import-fixture-a").expect("source_a should be writable");
    std::fs::write(&source_b, b"import-fixture-b").expect("source_b should be writable");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/import")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "copyMode": "COPY",
                        "books": [
                            {
                                "sourceFile": source_a.to_string_lossy(),
                                "seriesId": "series-1"
                            },
                            {
                                "sourceFile": source_b.to_string_lossy(),
                                "seriesId": "series-1"
                            }
                        ]
                    })
                    .to_string(),
                ))
                .expect("books/import request should build"),
        )
        .await
        .expect("books/import request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open");
    let rows = sqlx::query(
        "SELECT SIMPLE_TYPE, GROUP_ID, PAYLOAD \
         FROM TASK \
         WHERE SIMPLE_TYPE = 'IMPORT_BOOK' \
         ORDER BY ID ASC",
    )
    .fetch_all(&tasks_pool)
    .await
    .expect("import task rows should be queryable");
    tasks_pool.close().await;

    assert_eq!(rows.len(), 2);
    for row in rows {
        assert_eq!(row.get::<String, _>("SIMPLE_TYPE"), "IMPORT_BOOK");
        assert_eq!(
            row.get::<Option<String>, _>("GROUP_ID"),
            Some("series-1".to_string())
        );
        let payload = row.get::<String, _>("PAYLOAD");
        let payload = serde_json::from_str::<Value>(&payload)
            .expect("import task payload should be valid JSON");
        assert!(payload.get("book").is_some());
        assert!(payload.get("books").is_none());
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_books_import_preserves_copy_mode_destination_and_upgrade_fields_per_task() {
    let paths = new_router_fixture("router-books-import-payload-shape").await;
    seed_router_contract_data(&paths).await;

    let source_a = paths.config_dir.join("incoming-a.cbz");
    let source_b = paths.config_dir.join("incoming-b.cbz");
    std::fs::write(&source_a, b"import-fixture-a").expect("source_a should be writable");
    std::fs::write(&source_b, b"import-fixture-b").expect("source_b should be writable");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/import")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "copyMode": "HARDLINK",
                        "books": [
                            {
                                "sourceFile": source_a.to_string_lossy(),
                                "seriesId": "series-1",
                                "destinationName": "dest-a",
                                "upgradeBookId": "book-1"
                            },
                            {
                                "sourceFile": source_b.to_string_lossy(),
                                "seriesId": "series-2",
                                "destinationName": "dest-b"
                            }
                        ]
                    })
                    .to_string(),
                ))
                .expect("books/import payload-shape request should build"),
        )
        .await
        .expect("books/import payload-shape request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for books/import payload-shape verification");
    let rows = sqlx::query(
        "SELECT GROUP_ID, PAYLOAD \
         FROM TASK \
         WHERE SIMPLE_TYPE = 'IMPORT_BOOK' \
         ORDER BY ID ASC",
    )
    .fetch_all(&tasks_pool)
    .await
    .expect("import task payload-shape rows should be queryable");
    tasks_pool.close().await;

    assert_eq!(rows.len(), 2);

    let first_payload = serde_json::from_str::<Value>(&rows[0].get::<String, _>("PAYLOAD"))
        .expect("first import payload should be valid JSON");
    assert_eq!(
        rows[0].get::<Option<String>, _>("GROUP_ID"),
        Some("series-1".to_string())
    );
    assert_eq!(
        first_payload.get("copy_mode"),
        Some(&Value::String("HARDLINK".to_string()))
    );
    assert_eq!(
        first_payload
            .get("book")
            .and_then(|value| value.get("series_id"))
            .and_then(Value::as_str),
        Some("series-1")
    );
    assert_eq!(
        first_payload
            .get("book")
            .and_then(|value| value.get("destination_name"))
            .and_then(Value::as_str),
        Some("dest-a")
    );
    assert_eq!(
        first_payload
            .get("book")
            .and_then(|value| value.get("upgrade_book_id"))
            .and_then(Value::as_str),
        Some("book-1")
    );

    let second_payload = serde_json::from_str::<Value>(&rows[1].get::<String, _>("PAYLOAD"))
        .expect("second import payload should be valid JSON");
    assert_eq!(
        rows[1].get::<Option<String>, _>("GROUP_ID"),
        Some("series-2".to_string())
    );
    assert_eq!(
        second_payload.get("copy_mode"),
        Some(&Value::String("HARDLINK".to_string()))
    );
    assert_eq!(
        second_payload
            .get("book")
            .and_then(|value| value.get("series_id"))
            .and_then(Value::as_str),
        Some("series-2")
    );
    assert_eq!(
        second_payload
            .get("book")
            .and_then(|value| value.get("destination_name"))
            .and_then(Value::as_str),
        Some("dest-b")
    );
    assert_eq!(
        second_payload
            .get("book")
            .and_then(|value| value.get("upgrade_book_id")),
        Some(&Value::Null)
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_transient_books_scan_and_analyze_returns_non_placeholder_payload() {
    let paths = new_router_fixture("router-transient-books-scan-analyze").await;
    seed_router_contract_data(&paths).await;

    let transient_dir = paths.config_dir.join("transient-import");
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
                .body(Body::from(
                    json!({
                        "path": transient_dir.to_string_lossy().to_string(),
                    })
                    .to_string(),
                ))
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_transient_book_analyze_supports_epub_resources() {
    let paths = new_router_fixture("router-transient-books-analyze-epub").await;
    seed_router_contract_data(&paths).await;

    let transient_dir = paths.config_dir.join("transient-epub");
    std::fs::create_dir_all(&transient_dir).expect("transient epub directory should be created");
    write_router_epub_resource(
        &paths,
        "transient-epub/candidate.epub",
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
                .body(Body::from(
                    json!({
                        "path": transient_dir.to_string_lossy().to_string(),
                    })
                    .to_string(),
                ))
                .expect("transient epub scan request should build"),
        )
        .await
        .expect("transient epub scan request should complete");
    assert_eq!(scan_response.status(), StatusCode::OK);

    let scanned_payload = response_json(scan_response).await;
    let transient_id = scanned_payload
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item.get("id"))
        .and_then(Value::as_str)
        .expect("transient epub scan should yield an id")
        .to_string();

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
}

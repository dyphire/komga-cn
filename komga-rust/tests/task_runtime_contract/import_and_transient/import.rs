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

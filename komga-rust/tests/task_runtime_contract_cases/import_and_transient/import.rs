use super::*;
use komga_application::media_assets::{
    BooksImportEntry, ImportBookOutcome, ImportCopyMode, MediaImportPort, MediaImportService,
};
use std::sync::{Arc, Mutex};

async fn enqueue_books_import(paths: &RuntimeDbPaths, payload: Value, context: &str) {
    // These route contracts assert the queued TASK rows themselves, so they must not race
    // the background worker that would claim and delete import jobs during the same test.
    let app =
        build_router_without_runtime_workers_for_contract(&runtime_config_for_paths(paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/import")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(payload.to_string()))
                .expect("books/import request should build"),
        )
        .await
        .expect(context);

    assert_eq!(response.status(), StatusCode::ACCEPTED);
}

async fn load_import_task_rows(
    paths: &RuntimeDbPaths,
    sql: &str,
    context: &str,
) -> Vec<sqlx::sqlite::SqliteRow> {
    let tasks_pool = connect_test_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect(context);
    let rows = sqlx::query(sql)
        .fetch_all(&tasks_pool)
        .await
        .expect(context);
    tasks_pool.close().await;
    rows
}

#[derive(Clone, Default)]
struct RecordingImportPort {
    calls: Arc<Mutex<Vec<(ImportCopyMode, BooksImportEntry)>>>,
    outcome: Option<ImportBookOutcome>,
}

impl MediaImportPort for RecordingImportPort {
    async fn import_book(
        &self,
        copy_mode: ImportCopyMode,
        book: BooksImportEntry,
    ) -> Result<Option<ImportBookOutcome>, String> {
        self.calls
            .lock()
            .expect("recording import port lock should not be poisoned")
            .push((copy_mode, book));
        Ok(self.outcome.clone())
    }
}

#[tokio::test]
async fn router_books_import_enqueues_individual_tasks_in_tasks_db() {
    let paths = new_router_fixture("router-books-import-individual-tasks").await;
    seed_router_contract_data(&paths).await;

    let source_a = paths.config_dir.join("incoming-a.cbz");
    let source_b = paths.config_dir.join("incoming-b.cbz");
    std::fs::write(&source_a, b"import-fixture-a").expect("source_a should be writable");
    std::fs::write(&source_b, b"import-fixture-b").expect("source_b should be writable");

    enqueue_books_import(
        &paths,
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
        }),
        "books/import request should complete",
    )
    .await;

    let rows = load_import_task_rows(
        &paths,
        "SELECT SIMPLE_TYPE, GROUP_ID, PAYLOAD \
         FROM TASK \
         WHERE SIMPLE_TYPE = 'ImportBook' \
         ORDER BY ID ASC",
        "import task rows should be queryable",
    )
    .await;

    assert_eq!(rows.len(), 2);
    for row in rows {
        assert_eq!(row.get::<String, _>("SIMPLE_TYPE"), "ImportBook");
        assert_eq!(
            row.get::<Option<String>, _>("GROUP_ID"),
            Some("series-1".to_string())
        );
        let payload = row.get::<String, _>("PAYLOAD");
        let payload = serde_json::from_str::<Value>(&payload)
            .expect("import task payload should be valid JSON");
        assert_eq!(
            payload.get("copyMode"),
            Some(&Value::String("COPY".to_string()))
        );
        assert_eq!(
            payload.get("seriesId"),
            Some(&Value::String("series-1".to_string()))
        );
        assert!(payload.get("sourceFile").is_some());
        assert_eq!(payload.get("destinationName"), Some(&Value::Null));
        assert_eq!(payload.get("upgradeBookId"), Some(&Value::Null));
        assert!(payload.get("book").is_none());
        assert!(payload.get("books").is_none());
    }

    enqueue_books_import(
        &paths,
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
        }),
        "books/import payload-shape request should complete",
    )
    .await;

    let rows = load_import_task_rows(
        &paths,
        "SELECT GROUP_ID, PAYLOAD \
         FROM TASK \
         WHERE SIMPLE_TYPE = 'ImportBook'",
        "import task payload-shape rows should be queryable",
    )
    .await;

    let mut parsed_rows = rows
        .iter()
        .map(|row| {
            let payload = serde_json::from_str::<Value>(&row.get::<String, _>("PAYLOAD"))
                .expect("import payload should be valid JSON");
            let source_file = payload
                .get("sourceFile")
                .and_then(Value::as_str)
                .expect("import payload should expose sourceFile")
                .to_string();
            (row, payload, source_file)
        })
        .filter(|(_, payload, _)| {
            payload.get("copyMode").and_then(Value::as_str) == Some("HARDLINK")
        })
        .collect::<Vec<_>>();
    assert_eq!(parsed_rows.len(), 2);
    parsed_rows.sort_by(|(_, _, left_source), (_, _, right_source)| left_source.cmp(right_source));

    for ((row, payload, _), expected_group, expected_destination, expected_upgrade) in [
        (&parsed_rows[0], "series-1", Some("dest-a"), Some("book-1")),
        (&parsed_rows[1], "series-2", Some("dest-b"), None),
    ] {
        assert_eq!(
            row.get::<Option<String>, _>("GROUP_ID"),
            Some(expected_group.to_string())
        );
        assert_eq!(
            payload.get("copyMode"),
            Some(&Value::String("HARDLINK".to_string()))
        );
        assert_eq!(
            payload.get("seriesId").and_then(Value::as_str),
            Some(expected_group)
        );
        assert_eq!(
            payload.get("destinationName").and_then(Value::as_str),
            expected_destination
        );
        assert_eq!(
            payload.get("upgradeBookId").and_then(Value::as_str),
            expected_upgrade
        );
        assert!(payload.get("sourceFile").is_some());
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_books_import_accepts_missing_books_like_kotlin() {
    let paths = new_router_fixture("router-books-import-missing-books").await;
    seed_router_contract_data(&paths).await;

    enqueue_books_import(
        &paths,
        json!({
            "copyMode": "COPY"
        }),
        "books/import request without books should complete",
    )
    .await;

    let rows = load_import_task_rows(
        &paths,
        "SELECT ID FROM TASK WHERE SIMPLE_TYPE = 'ImportBook'",
        "import task rows should be queryable after missing-books request",
    )
    .await;

    assert!(rows.is_empty());

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_books_import_reuses_kotlin_style_unique_id_for_duplicate_series_and_source() {
    let paths = new_router_fixture("router-books-import-deterministic-unique-id").await;
    seed_router_contract_data(&paths).await;
    let app =
        build_router_without_runtime_workers_for_contract(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let source = paths.config_dir.join("incoming-dedup.cbz");
    std::fs::write(&source, b"import-dedup-fixture").expect("dedup source should be writable");
    let expected_id = format!("ImportBook_series-1_{}", source.to_string_lossy());

    for (context, payload) in [
        (
            "books/import first deterministic-id request should complete",
            json!({
                "copyMode": "COPY",
                "books": [
                    {
                        "sourceFile": source.to_string_lossy(),
                        "seriesId": "series-1"
                    }
                ]
            }),
        ),
        (
            "books/import second deterministic-id request should complete",
            json!({
                "copyMode": "HARDLINK",
                "books": [
                    {
                        "sourceFile": source.to_string_lossy(),
                        "seriesId": "series-1",
                        "destinationName": "dedup-destination",
                        "upgradeBookId": "book-1"
                    }
                ]
            }),
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/books/import")
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(payload.to_string()))
                    .expect("deterministic books/import request should build"),
            )
            .await
            .expect(context);

        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }

    let rows = load_import_task_rows(
        &paths,
        "SELECT ID, PAYLOAD \
         FROM TASK \
         WHERE SIMPLE_TYPE = 'ImportBook'",
        "deterministic import task rows should be queryable",
    )
    .await;

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<String, _>("ID"), expected_id);

    let payload = serde_json::from_str::<Value>(&rows[0].get::<String, _>("PAYLOAD"))
        .expect("deterministic import payload should be valid JSON");
    assert_eq!(
        payload.get("uniqueId").and_then(Value::as_str),
        Some(expected_id.as_str())
    );
    assert_eq!(
        payload.get("copyMode").and_then(Value::as_str),
        Some("HARDLINK")
    );
    assert_eq!(
        payload.get("destinationName").and_then(Value::as_str),
        Some("dedup-destination")
    );
    assert_eq!(
        payload.get("upgradeBookId").and_then(Value::as_str),
        Some("book-1")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_books_import_runtime_follow_up_enqueues_analyze_book_instead_of_scan_library() {
    let paths = new_router_fixture("router-books-import-follow-up-shape").await;
    seed_router_contract_data(&paths).await;

    let source_root = std::env::temp_dir().join(format!(
        "komga-import-follow-up-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&source_root).expect("follow-up source root should be created");
    let source = source_root.join("incoming-follow-up.cbz");
    std::fs::write(&source, b"import-follow-up-fixture")
        .expect("follow-up source should be writable");

    enqueue_books_import(
        &paths,
        json!({
            "copyMode": "COPY",
            "books": [
                {
                    "sourceFile": source.to_string_lossy(),
                    "seriesId": "series-1"
                }
            ]
        }),
        "books/import follow-up request should complete",
    )
    .await;

    let mut rows = load_import_task_rows(
        &paths,
        "SELECT PRIORITY, PAYLOAD \
         FROM TASK \
         WHERE SIMPLE_TYPE = 'ImportBook' \
         LIMIT 1",
        "queued import task row should be queryable",
    )
    .await;
    let import_row = rows.pop().expect("queued import task row should exist");

    let calls = Arc::new(Mutex::new(Vec::new()));
    let follow_up_tasks = MediaImportService::new(RecordingImportPort {
        calls: calls.clone(),
        outcome: Some(ImportBookOutcome {
            library_id: "library-1".to_string(),
            imported_book_id: "book-imported-1".to_string(),
            sidecar_imported: false,
            artwork_sidecar_imported: false,
        }),
    })
    .process_queued_book_payload(
        &import_row.get::<String, _>("PAYLOAD"),
        import_row.get::<i64, _>("PRIORITY") as i32,
    )
    .await
    .expect("queued import payload should produce follow-up tasks");

    let recorded_calls = calls
        .lock()
        .expect("recorded import calls lock should not be poisoned");
    assert_eq!(recorded_calls.len(), 1);
    assert_eq!(recorded_calls[0].0, ImportCopyMode::Copy);
    assert_eq!(recorded_calls[0].1.series_id, "series-1");
    assert_eq!(recorded_calls[0].1.destination_name, None);
    assert_eq!(recorded_calls[0].1.upgrade_book_id, None);
    assert_eq!(recorded_calls[0].1.source_file, source);

    assert_eq!(follow_up_tasks.len(), 1);
    assert!(
        follow_up_tasks
            .iter()
            .all(|task| task.simple_type != "ScanLibrary"),
        "import runtime follow-up must not fall back to library scan tasks",
    );
    assert!(
        follow_up_tasks[0].id.starts_with("AnalyzeBook_"),
        "import runtime follow-up should enqueue analyze-book task ids",
    );
    assert_eq!(follow_up_tasks[0].simple_type, "AnalyzeBook");
    assert_eq!(follow_up_tasks[0].priority, 101);
    assert_eq!(follow_up_tasks[0].group.as_deref(), Some("series-1"));

    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_dir_all(&source_root);
    cleanup_router_fixture(paths);
}

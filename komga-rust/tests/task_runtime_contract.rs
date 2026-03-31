use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::application::task_processing::TaskRuntimeContext;
use komga_rust::config::{RuntimeMode, WriterOwnershipPolicy};
use komga_rust::infrastructure::sqlite::connect_pool;
use komga_rust::{SearchEntityType, SearchIndexLifecycle, TaskQueueRecord, TaskQueueScheduler};
use komga_server::app::build_router_with_config;
use serde_json::{Value, json};
use sqlx::Row;
use tower::util::ServiceExt;

#[path = "support/runtime_router_contract_support.rs"]
mod runtime_router_contract_support;

use runtime_router_contract_support::*;

fn runtime_task_context(paths: &RuntimeDbPaths) -> TaskRuntimeContext {
    TaskRuntimeContext {
        database_file: paths.main_db.clone(),
        tasks_db_file: paths.tasks_db.clone(),
        lucene_data_directory: paths.config_dir.join("lucene"),
        consumes_queue: true,
        owns_main_database: true,
        owns_filesystem_scan_output: true,
        owns_sidecar_output: true,
        owns_search_index: true,
    }
}

#[test]
fn task_runtime_contract_target_is_registered() {
    assert_required_target_declared("tasks/scanner", "task_runtime_contract");
}

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

#[tokio::test]
async fn isolated_runtime_keeps_search_index_external_owned() {
    let paths = new_router_fixture("isolated-runtime-external-search-index").await;
    seed_router_contract_data(&paths).await;

    let mut config = runtime_config_for_paths(&paths);
    config.mode = RuntimeMode::Isolated;
    config.writer_ownership_policy = WriterOwnershipPolicy {
        isolation_root: Some(paths.config_dir.clone()),
        allow_isolated_writes: true,
    };

    let mut scheduler = TaskQueueScheduler::for_runtime(config.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new("REBUILD_INDEX", 1_000, None));
    let processed = scheduler
        .process_available(&config)
        .expect("isolated runtime should process queued tasks without task-execution failure");
    assert_eq!(
        processed, 1,
        "fixture sanity: rebuild task should be consumed once"
    );

    let search = SearchIndexLifecycle::bootstrap(config.lucene_data_directory.as_path())
        .expect("search index should bootstrap for ownership assertions");
    let hits = search
        .search_ids("Book 1", SearchEntityType::Book, 10)
        .expect("search lookup should succeed for ownership assertions");
    assert!(
        hits.is_empty(),
        "isolated runtime should leave external-owned search index untouched",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_rejects_legacy_upgrade_index_task_contract() {
    let paths = new_router_fixture("runtime-rejects-legacy-upgrade-index-task").await;
    seed_router_contract_data(&paths).await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new("UPGRADE_INDEX", 1_000, None));

    let error = scheduler
        .process_available(&runtime)
        .expect_err("legacy upgrade index task should no longer be executable");

    assert!(
        error.is_unsupported_task(),
        "legacy upgrade index task must fail as unsupported instead of aliasing rebuild",
    );
    assert_eq!(
        error.to_string(),
        "unsupported runtime task type: UPGRADE_INDEX",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_incremental_index_sync_contract_covers_entity_lifecycle_and_metadata_refresh() {
    let paths = new_router_fixture("runtime-incremental-index-sync-contract").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for incremental index sync fixture setup");
    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, oneshot) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("series-oneshot")
    .bind(0_i64)
    .bind("OneShot Series")
    .bind("series/series-oneshot")
    .bind("library-1")
    .bind(true)
    .execute(&pool)
    .await
    .expect("oneshot series row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (\
             STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind("OneShot Series")
    .bind("OneShot Series")
    .bind("Oneshot Publisher")
    .bind("EN")
    .bind(16_i64)
    .bind("series-oneshot")
    .execute(&pool)
    .await
    .expect("oneshot series metadata row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK (\
             ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, oneshot) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-oneshot")
    .bind(0_i64)
    .bind("book-oneshot.cbz")
    .bind("books/book-oneshot.cbz")
    .bind("series-oneshot")
    .bind(1024_i64)
    .bind(2_i64)
    .bind("library-1")
    .bind(true)
    .execute(&pool)
    .await
    .expect("oneshot book row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, ISBN, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("2")
    .bind(2.0_f64)
    .bind("OneShot Book")
    .bind("978-oneshot")
    .bind("book-oneshot")
    .execute(&pool)
    .await
    .expect("oneshot book metadata row should be inserted");

    sqlx::query(
        "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) \
         VALUES (?, ?, ?, ?)",
    )
    .bind("application/epub+zip")
    .bind("READY")
    .bind("book-oneshot")
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("oneshot media row should be inserted");
    pool.close().await;

    let config = runtime_config_for_paths(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(config.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new("REBUILD_INDEX", 1_000, None));
    scheduler
        .process_available(&config)
        .expect("rebuild index task should succeed for incremental sync contract");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for series metadata update fixture");
    sqlx::query(
        "UPDATE SERIES_METADATA \
         SET PUBLISHER = ? \
         WHERE SERIES_ID = ?",
    )
    .bind("Oneshot Publisher Updated")
    .bind("series-oneshot")
    .execute(&pool)
    .await
    .expect("oneshot series publisher should be updated");
    pool.close().await;

    scheduler.enqueue(TaskQueueRecord::new(
        "REFRESH_SERIES_METADATA:series-oneshot",
        1_000,
        Some("series-oneshot".to_string()),
    ));
    scheduler
        .process_available(&config)
        .expect("refresh-series-metadata task should process for incremental sync contract");

    let app = build_router_with_config(&config);
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let search_hits = |query: &str, entity_type: SearchEntityType| -> Vec<String> {
        SearchIndexLifecycle::bootstrap(config.lucene_data_directory.as_path())
            .expect("search index should bootstrap for incremental sync contract")
            .search_ids(query, entity_type, 10)
            .expect("search lookup should succeed for incremental sync contract")
    };

    assert_eq!(
        search_hits("publisher:Updated", SearchEntityType::Book),
        vec!["book-oneshot".to_string()],
        "series metadata refresh task should update oneshot-derived book fields",
    );

    let create_collection_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": "Task6 Collection",
                        "ordered": true,
                        "seriesIds": ["series-1"]
                    })
                    .to_string(),
                ))
                .expect("collection create request should build"),
        )
        .await
        .expect("collection create request should complete");
    assert_eq!(create_collection_response.status(), StatusCode::OK);
    let collection_payload = response_json(create_collection_response).await;
    let collection_id = collection_payload
        .get("id")
        .and_then(Value::as_str)
        .expect("collection create payload should include id")
        .to_string();
    assert_eq!(
        search_hits("Task6 Collection", SearchEntityType::Collection),
        vec![collection_id.clone()],
        "collection create should upsert search document",
    );

    let update_collection_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/v1/collections/{collection_id}"))
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": "Task6 Collection Updated",
                        "ordered": true,
                        "seriesIds": ["series-1"]
                    })
                    .to_string(),
                ))
                .expect("collection update request should build"),
        )
        .await
        .expect("collection update request should complete");
    assert_eq!(update_collection_response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        search_hits("Task6 Collection Updated", SearchEntityType::Collection),
        vec![collection_id.clone()],
        "collection update should refresh search document",
    );

    let delete_collection_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/collections/{collection_id}"))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collection delete request should build"),
        )
        .await
        .expect("collection delete request should complete");
    assert_eq!(delete_collection_response.status(), StatusCode::NO_CONTENT);
    assert!(
        search_hits("Task6 Collection Updated", SearchEntityType::Collection).is_empty(),
        "collection delete should remove search document",
    );

    let create_readlist_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/readlists")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": "Task6 ReadList",
                        "summary": "task6",
                        "ordered": true,
                        "bookIds": ["book-1"]
                    })
                    .to_string(),
                ))
                .expect("readlist create request should build"),
        )
        .await
        .expect("readlist create request should complete");
    assert_eq!(create_readlist_response.status(), StatusCode::OK);
    let readlist_payload = response_json(create_readlist_response).await;
    let readlist_id = readlist_payload
        .get("id")
        .and_then(Value::as_str)
        .expect("readlist create payload should include id")
        .to_string();
    assert_eq!(
        search_hits("Task6 ReadList", SearchEntityType::ReadList),
        vec![readlist_id.clone()],
        "readlist create should upsert search document",
    );

    let update_readlist_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/v1/readlists/{readlist_id}"))
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": "Task6 ReadList Updated",
                        "summary": "task6-updated",
                        "ordered": true,
                        "bookIds": ["book-1"]
                    })
                    .to_string(),
                ))
                .expect("readlist update request should build"),
        )
        .await
        .expect("readlist update request should complete");
    assert_eq!(update_readlist_response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        search_hits("Task6 ReadList Updated", SearchEntityType::ReadList),
        vec![readlist_id.clone()],
        "readlist update should refresh search document",
    );

    let delete_readlist_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/readlists/{readlist_id}"))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist delete request should build"),
        )
        .await
        .expect("readlist delete request should complete");
    assert_eq!(delete_readlist_response.status(), StatusCode::NO_CONTENT);
    assert!(
        search_hits("Task6 ReadList Updated", SearchEntityType::ReadList).is_empty(),
        "readlist delete should remove search document",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_blocks_authentication_activity_cleanup_when_main_database_is_external_owned() {
    let paths = new_router_fixture("runtime-blocked-main-database-auth-cleanup").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for auth-cleanup fixture setup");
    sqlx::query(
        r#"
        INSERT INTO AUTHENTICATION_ACTIVITY (
            USER_ID,
            EMAIL,
            IP,
            USER_AGENT,
            SUCCESS,
            ERROR,
            DATE_TIME,
            SOURCE,
            API_KEY_ID,
            API_KEY_COMMENT
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind("admin-user")
    .bind("admin@example.org")
    .bind("127.0.0.1")
    .bind("test-agent")
    .bind(true)
    .bind(Option::<String>::None)
    .bind("2000-01-01 00:00:00")
    .bind("basic")
    .bind(Option::<String>::None)
    .bind(Option::<String>::None)
    .execute(&pool)
    .await
    .expect("authentication activity row should be inserted");
    pool.close().await;

    let runtime = TaskRuntimeContext {
        owns_main_database: false,
        ..runtime_task_context(&paths)
    };
    komga_rust::infrastructure::task_queue::cleanup_authentication_activity_once(&runtime).await;

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for auth-cleanup verification");
    let activity_rows = sqlx::query("SELECT COUNT(*) AS COUNT FROM AUTHENTICATION_ACTIVITY")
        .fetch_one(&verify_pool)
        .await
        .expect("authentication activity count should be queryable")
        .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(
        activity_rows, 1,
        "runtime must not delete authentication activity rows when main database is external-owned",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_blocks_book_media_analysis_when_main_database_is_external_owned() {
    let paths = new_router_fixture("runtime-blocked-main-database-analyze-book").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_resource(
        &paths,
        "books/book-1.epub",
        "OEBPS/chapter.xhtml",
        br#"<html xmlns='http://www.w3.org/1999/xhtml'><body><p>Analyze Fixture</p></body></html>"#,
    );

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for analyze-book fixture setup");
    sqlx::query(
        r#"
        UPDATE MEDIA
        SET STATUS = ?, PAGE_COUNT = ?
        WHERE BOOK_ID = ?
        "#,
    )
    .bind("ERROR")
    .bind(0_i64)
    .bind("book-1")
    .execute(&pool)
    .await
    .expect("media row should be downgraded for analyze-book fixture");
    sqlx::query(
        r#"
        INSERT INTO MEDIA_PAGE (
            FILE_NAME,
            MEDIA_TYPE,
            NUMBER,
            BOOK_ID,
            width,
            height,
            FILE_HASH,
            FILE_SIZE
        ) VALUES (?, ?, ?, ?, NULL, NULL, ?, ?)
        "#,
    )
    .bind("stale-page.xhtml")
    .bind("application/xhtml+xml")
    .bind(1_i64)
    .bind("book-1")
    .bind("stale-page-hash")
    .bind(123_i64)
    .execute(&pool)
    .await
    .expect("stale media page row should be inserted");
    pool.close().await;

    let runtime = TaskRuntimeContext {
        owns_main_database: false,
        owns_search_index: false,
        ..runtime_task_context(&paths)
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "ANALYZE_BOOK:book-1",
        1_000,
        Some("book-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("blocked main-database analyze-book should still drain cleanly");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for analyze-book verification");
    let media_row = sqlx::query(
        r#"
        SELECT STATUS, PAGE_COUNT
        FROM MEDIA
        WHERE BOOK_ID = ?
        LIMIT 1
        "#,
    )
    .bind("book-1")
    .fetch_one(&verify_pool)
    .await
    .expect("media row should be queryable");
    let stale_page_rows = sqlx::query(
        r#"
        SELECT COUNT(*) AS COUNT
        FROM MEDIA_PAGE
        WHERE BOOK_ID = ?
        AND FILE_NAME = ?
        AND FILE_HASH = ?
        "#,
    )
    .bind("book-1")
    .bind("stale-page.xhtml")
    .bind("stale-page-hash")
    .fetch_one(&verify_pool)
    .await
    .expect("stale media page rows should be queryable")
    .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(
        media_row.get::<String, _>("STATUS"),
        "ERROR",
        "runtime must not rewrite MEDIA status during analyze-book when main database is external-owned",
    );
    assert_eq!(
        media_row.get::<i64, _>("PAGE_COUNT"),
        0,
        "runtime must not rewrite MEDIA page count during analyze-book when main database is external-owned",
    );
    assert_eq!(
        stale_page_rows, 1,
        "runtime must not replace MEDIA_PAGE rows during analyze-book when main database is external-owned",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_blocks_sidecar_metadata_refresh_when_sidecar_output_is_external_owned() {
    let paths = new_router_fixture("runtime-blocked-sidecar-output").await;
    seed_router_contract_data(&paths).await;

    let sidecar_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&sidecar_dir).expect("book sidecar directory should be created");
    std::fs::write(
        sidecar_dir.join("book-1.xml"),
        br#"<ComicInfo><Title>Blocked Sidecar Title</Title></ComicInfo>"#,
    )
    .expect("book sidecar fixture should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for sidecar fixture setup");
    sqlx::query(
        "INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)",
    )
    .bind("books/book-1.xml")
    .bind("books/book-1.epub")
    .bind(1_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("book sidecar row should be inserted");
    pool.close().await;

    let runtime = TaskRuntimeContext {
        owns_sidecar_output: false,
        ..runtime_task_context(&paths)
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "REFRESH_BOOK_METADATA:book-1",
        1_000,
        Some("book-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("blocked sidecar metadata refresh should still drain cleanly");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for verification");
    let title = sqlx::query("SELECT TITLE FROM BOOK_METADATA WHERE BOOK_ID = ? LIMIT 1")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("book metadata title should be queryable")
        .get::<String, _>("TITLE");
    verify_pool.close().await;

    assert_eq!(
        title, "Book 1",
        "runtime must not apply sidecar metadata when sidecar output is external-owned",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_blocks_series_metadata_aggregation_when_main_database_is_external_owned() {
    let paths = new_router_fixture("runtime-blocked-main-database-aggregation").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for aggregation fixture setup");
    sqlx::query("UPDATE SERIES SET NAME = ? WHERE ID = ?")
        .bind("Renamed Series From Main DB")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series name should be updated for aggregation fixture");
    sqlx::query(
        "UPDATE SERIES_METADATA \
         SET TITLE = ?, TITLE_SORT = ? \
         WHERE SERIES_ID = ?",
    )
    .bind("Original Aggregation Title")
    .bind("Original Aggregation Title")
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("series metadata title should be updated for aggregation fixture");
    pool.close().await;

    let runtime = TaskRuntimeContext {
        owns_main_database: false,
        ..runtime_task_context(&paths)
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "AGGREGATE_SERIES_METADATA:series-1",
        1_000,
        Some("series-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("blocked main-database aggregation should still drain cleanly");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for aggregation verification");
    let row =
        sqlx::query("SELECT TITLE, TITLE_SORT FROM SERIES_METADATA WHERE SERIES_ID = ? LIMIT 1")
            .bind("series-1")
            .fetch_one(&verify_pool)
            .await
            .expect("series metadata aggregation row should be queryable");
    verify_pool.close().await;

    assert_eq!(
        row.get::<String, _>("TITLE"),
        "Original Aggregation Title",
        "runtime must not aggregate series metadata when main database is external-owned",
    );
    assert_eq!(
        row.get::<String, _>("TITLE_SORT"),
        "Original Aggregation Title",
        "runtime must not rewrite title sort when main database is external-owned",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_blocks_empty_trash_cleanup_when_main_database_is_external_owned() {
    let paths = new_router_fixture("runtime-blocked-main-database-empty-trash").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for cleanup fixture setup");
    sqlx::query("DELETE FROM COLLECTION_SERIES WHERE COLLECTION_ID = ?")
        .bind("collection-1")
        .execute(&pool)
        .await
        .expect("collection members should be removed for cleanup fixture");
    sqlx::query("DELETE FROM READLIST_BOOK WHERE READLIST_ID = ?")
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist members should be removed for cleanup fixture");
    sqlx::query("INSERT OR REPLACE INTO SERVER_SETTINGS (KEY, VALUE) VALUES (?, ?)")
        .bind("DELETE_EMPTY_COLLECTIONS")
        .bind("true")
        .execute(&pool)
        .await
        .expect("delete empty collections setting should be enabled");
    sqlx::query("INSERT OR REPLACE INTO SERVER_SETTINGS (KEY, VALUE) VALUES (?, ?)")
        .bind("DELETE_EMPTY_READLISTS")
        .bind("true")
        .execute(&pool)
        .await
        .expect("delete empty readlists setting should be enabled");
    pool.close().await;

    let runtime = TaskRuntimeContext {
        owns_main_database: false,
        ..runtime_task_context(&paths)
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "EMPTY_TRASH:library-1",
        1_000,
        Some("library-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("blocked main-database cleanup should still drain cleanly");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for cleanup verification");
    let collection_rows = sqlx::query("SELECT COUNT(*) AS COUNT FROM COLLECTION WHERE ID = ?")
        .bind("collection-1")
        .fetch_one(&verify_pool)
        .await
        .expect("collection row count should be queryable")
        .get::<i64, _>("COUNT");
    let readlist_rows = sqlx::query("SELECT COUNT(*) AS COUNT FROM READLIST WHERE ID = ?")
        .bind("readlist-1")
        .fetch_one(&verify_pool)
        .await
        .expect("readlist row count should be queryable")
        .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(
        collection_rows, 1,
        "runtime must not delete empty collections when main database is external-owned",
    );
    assert_eq!(
        readlist_rows, 1,
        "runtime must not delete empty readlists when main database is external-owned",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_blocks_book_thumbnail_generation_when_main_database_is_external_owned() {
    let paths = new_router_fixture("runtime-blocked-main-database-thumbnail").await;
    seed_router_contract_data(&paths).await;
    const GIF_1X1: &[u8] = &[
        0x47, 0x49, 0x46, 0x38, 0x37, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF, 0xFF, 0x2C, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x02,
        0x02, 0x44, 0x01, 0x00, 0x3B,
    ];
    write_router_epub_resource(&paths, "books/book-1.epub", "OEBPS/cover.gif", GIF_1X1);

    let runtime = TaskRuntimeContext {
        owns_main_database: false,
        ..runtime_task_context(&paths)
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "GENERATE_BOOK_THUMBNAIL:book-1",
        1_000,
        Some("book-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("blocked main-database thumbnail generation should still drain cleanly");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for thumbnail verification");
    let generated_count = sqlx::query(
        "SELECT COUNT(*) AS COUNT FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? AND TYPE = 'GENERATED'",
    )
    .bind("book-1")
    .fetch_one(&verify_pool)
    .await
    .expect("generated thumbnail rows should be queryable")
    .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(
        generated_count, 0,
        "runtime must not generate book thumbnails when main database is external-owned",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_blocks_book_delete_when_main_database_is_external_owned() {
    let paths = new_router_fixture("runtime-blocked-main-database-delete-book").await;
    seed_router_contract_data(&paths).await;

    let runtime = TaskRuntimeContext {
        owns_main_database: false,
        ..runtime_task_context(&paths)
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "DELETE_BOOK:book-1",
        1_000,
        Some("book-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("blocked main-database delete-book should still drain cleanly");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-book verification");
    let book_rows = sqlx::query("SELECT COUNT(*) AS COUNT FROM BOOK WHERE ID = ?")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("book row count should be queryable")
        .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(
        book_rows, 1,
        "runtime must not delete books when main database is external-owned",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_blocks_import_book_when_main_database_is_external_owned() {
    let paths = new_router_fixture("runtime-blocked-main-database-import").await;
    seed_router_contract_data(&paths).await;

    let source_root = std::env::temp_dir().join(format!(
        "komga-import-blocked-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&source_root).expect("blocked import source root should be created");
    let source_file = source_root.join("blocked-import.cbz");
    std::fs::write(&source_file, b"blocked-import-payload")
        .expect("blocked import source file should be written");

    let payload = json!({
        "copy_mode": "COPY",
        "book": {
            "source_file": source_file,
            "series_id": "series-1",
            "destination_name": null,
            "upgrade_book_id": null
        }
    })
    .to_string();

    let runtime = TaskRuntimeContext {
        owns_main_database: false,
        ..runtime_task_context(&paths)
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(
        TaskQueueRecord::new(
            "IMPORT_BOOK:blocked-import",
            1_000,
            Some("series-1".to_string()),
        )
        .with_simple_type("IMPORT_BOOK")
        .with_payload(payload),
    );
    scheduler
        .process_available(&runtime)
        .expect("blocked main-database import should still drain cleanly");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for import verification");
    let historical_events =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM HISTORICAL_EVENT WHERE TYPE = 'BookImported'")
            .fetch_one(&verify_pool)
            .await
            .expect("historical event rows should be queryable")
            .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(
        historical_events, 0,
        "runtime must not persist import historical events when main database is external-owned",
    );
    assert!(
        !paths
            .config_dir
            .join("series/series-1/blocked-import.cbz")
            .exists(),
        "runtime must not copy imported files into the library root when main database is external-owned",
    );

    let _ = std::fs::remove_file(&source_file);
    let _ = std::fs::remove_dir_all(&source_root);
    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_blocks_book_hash_when_main_database_is_external_owned() {
    let paths = new_router_fixture("runtime-blocked-main-database-hash-book").await;
    seed_router_contract_data(&paths).await;
    std::fs::create_dir_all(paths.config_dir.join("books"))
        .expect("book directory should exist for hash fixture");
    std::fs::write(
        paths.config_dir.join("books/book-1.epub"),
        b"hash-book-fixture",
    )
    .expect("book file should be written for hash fixture");

    let runtime = TaskRuntimeContext {
        owns_main_database: false,
        ..runtime_task_context(&paths)
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "HASH_BOOK:book-1",
        1_000,
        Some("book-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("blocked main-database hash-book should still drain cleanly");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for hash verification");
    let file_hash = sqlx::query("SELECT FILE_HASH FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("book hash should be queryable")
        .get::<Option<String>, _>("FILE_HASH");
    verify_pool.close().await;

    assert_eq!(
        file_hash,
        Some(String::new()),
        "runtime must not persist book hashes when main database is external-owned",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_blocks_book_page_hash_when_main_database_is_external_owned() {
    let paths = new_router_fixture("runtime-blocked-main-database-page-hash").await;
    seed_router_contract_data(&paths).await;
    const GIF_1X1: &[u8] = &[
        0x47, 0x49, 0x46, 0x38, 0x37, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF, 0xFF, 0x2C, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x02,
        0x02, 0x44, 0x01, 0x00, 0x3B,
    ];
    std::fs::create_dir_all(paths.config_dir.join("books"))
        .expect("book directory should exist for page-hash fixture");
    std::fs::write(paths.config_dir.join("books/hash-image.gif"), GIF_1X1)
        .expect("image file should be written for page-hash fixture");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for page-hash fixture setup");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-hash-1")
    .bind(0_i64)
    .bind("hash-image.gif")
    .bind("books/hash-image.gif")
    .bind("series-1")
    .bind(i64::try_from(GIF_1X1.len()).expect("gif size should fit in i64"))
    .bind(2_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("page-hash fixture book row should be inserted");
    sqlx::query(
        "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) \
         VALUES (?, ?, ?, ?)",
    )
    .bind("image/gif")
    .bind("READY")
    .bind("book-hash-1")
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("page-hash fixture media row should be inserted");
    sqlx::query(
        "INSERT INTO MEDIA_PAGE (FILE_NAME, MEDIA_TYPE, NUMBER, BOOK_ID, width, height, FILE_HASH, FILE_SIZE) VALUES (?, ?, ?, ?, NULL, NULL, '', ?)",
    )
    .bind("hash-image.gif")
    .bind("image/gif")
    .bind(1_i64)
    .bind("book-hash-1")
    .bind(i64::try_from(GIF_1X1.len()).expect("gif size should fit in i64"))
    .execute(&pool)
    .await
    .expect("page-hash fixture media page row should be inserted");
    pool.close().await;

    let runtime = TaskRuntimeContext {
        owns_main_database: false,
        ..runtime_task_context(&paths)
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "HASH_BOOK_PAGES:book-hash-1",
        1_000,
        Some("book-hash-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("blocked main-database page-hash should still drain cleanly");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for page-hash verification");
    let file_hash =
        sqlx::query("SELECT FILE_HASH FROM MEDIA_PAGE WHERE BOOK_ID = ? AND NUMBER = 1 LIMIT 1")
            .bind("book-hash-1")
            .fetch_one(&verify_pool)
            .await
            .expect("media page hash should be queryable")
            .get::<String, _>("FILE_HASH");
    verify_pool.close().await;

    assert_eq!(
        file_hash,
        String::new(),
        "runtime must not persist page hashes when main database is external-owned",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_blocks_extension_repair_when_main_database_is_external_owned() {
    let paths = new_router_fixture("runtime-blocked-main-database-extension-repair").await;
    seed_router_contract_data(&paths).await;
    std::fs::create_dir_all(paths.config_dir.join("books"))
        .expect("book directory should exist for extension-repair fixture");
    let source_path = paths.config_dir.join("books/repair-book.bin");
    std::fs::write(&source_path, b"repair-extension-fixture")
        .expect("book file should be written for extension-repair fixture");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for extension-repair fixture setup");
    sqlx::query("UPDATE LIBRARY SET REPAIR_EXTENSIONS = 1 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("repair extensions flag should be enabled");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-repair-1")
    .bind(0_i64)
    .bind("repair-book.bin")
    .bind("books/repair-book.bin")
    .bind("series-1")
    .bind(24_i64)
    .bind(3_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("extension-repair fixture book row should be inserted");
    sqlx::query(
        "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) \
         VALUES (?, ?, ?, ?)",
    )
    .bind("application/pdf")
    .bind("READY")
    .bind("book-repair-1")
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("extension-repair fixture media row should be inserted");
    pool.close().await;

    let runtime = TaskRuntimeContext {
        owns_main_database: false,
        ..runtime_task_context(&paths)
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "REPAIR_EXTENSIONS:library-1",
        1_000,
        Some("library-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("blocked main-database extension repair should still drain cleanly");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for extension-repair verification");
    let url = sqlx::query("SELECT URL FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-repair-1")
        .fetch_one(&verify_pool)
        .await
        .expect("book url should be queryable")
        .get::<String, _>("URL");
    verify_pool.close().await;

    assert_eq!(
        url, "books/repair-book.bin",
        "runtime must not rewrite book URLs during extension repair when main database is external-owned",
    );
    assert!(
        source_path.exists(),
        "runtime must not rename source files during extension repair when main database is external-owned",
    );
    assert!(
        !paths.config_dir.join("books/repair-book.gif").exists(),
        "runtime must not create repaired-extension files when main database is external-owned",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_blocks_find_books_to_convert_when_main_database_is_external_owned() {
    let paths = new_router_fixture("runtime-blocked-main-database-find-books-to-convert").await;
    seed_router_contract_data(&paths).await;
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for find-books-to-convert fixture setup");
    sqlx::query("UPDATE LIBRARY SET CONVERT_TO_CBZ = 1 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("convert-to-cbz flag should be enabled");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-convert-1")
    .bind(0_i64)
    .bind("convert-book.cbr")
    .bind("books/convert-book.cbr")
    .bind("series-1")
    .bind(32_i64)
    .bind(4_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("find-books-to-convert fixture book row should be inserted");
    sqlx::query(
        "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) \
         VALUES (?, ?, ?, ?)",
    )
    .bind("application/vnd.comicbook-rar")
    .bind("READY")
    .bind("book-convert-1")
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("find-books-to-convert fixture media row should be inserted");
    pool.close().await;

    let runtime = TaskRuntimeContext {
        owns_main_database: false,
        ..runtime_task_context(&paths)
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "FIND_BOOKS_TO_CONVERT:library-1",
        1_000,
        Some("library-1".to_string()),
    ));
    let processed = scheduler
        .process_available(&runtime)
        .expect("blocked main-database find-books-to-convert should still drain cleanly");

    assert_eq!(
        processed, 1,
        "runtime must not enqueue downstream convert-book tasks when find-books-to-convert is blocked by external-owned main database",
    );

    cleanup_router_fixture(paths);
}

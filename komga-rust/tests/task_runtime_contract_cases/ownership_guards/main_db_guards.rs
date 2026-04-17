use super::*;

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

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for blocked import follow-up verification");
    let analyze_follow_ups =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM TASK WHERE SIMPLE_TYPE = 'ANALYZE_BOOK'")
            .fetch_one(&tasks_pool)
            .await
            .expect("blocked import follow-up rows should be queryable")
            .get::<i64, _>("COUNT");
    tasks_pool.close().await;

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
    assert_eq!(
        analyze_follow_ups, 0,
        "blocked import must not enqueue analyze-book follow-up tasks when main database is external-owned",
    );

    let _ = std::fs::remove_file(&source_file);
    let _ = std::fs::remove_dir_all(&source_root);
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
    scheduler.enqueue(
        TaskQueueRecord::new(
            "REPAIR_EXTENSION_book-repair-1",
            1_000,
            Some("series-1".to_string()),
        )
        .with_simple_type("REPAIR_EXTENSION")
        .with_payload(
            json!({
                "bookId": "book-repair-1",
                "priority": 1000,
                "groupId": "series-1",
                "uniqueId": "REPAIR_EXTENSION_book-repair-1"
            })
            .to_string(),
        ),
    );
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
        "FIND_BOOKS_TO_CONVERT_library-1",
        1_000,
        Some("library-1".to_string()),
    ).with_simple_type("FIND_BOOKS_TO_CONVERT"));
    let processed = scheduler
        .process_available(&runtime)
        .expect("blocked main-database find-books-to-convert should still drain cleanly");

    assert_eq!(
        processed, 1,
        "runtime must not enqueue downstream convert-book tasks when find-books-to-convert is blocked by external-owned main database",
    );

    cleanup_router_fixture(paths);
}

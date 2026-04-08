use super::support::*;
use super::*;

#[tokio::test]
async fn scanner_runtime_blocks_scan_output_when_filesystem_scan_writer_is_external_owned() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-blocked-scan-output")
        .await
        .expect("scanner blocked scan-output fixture should be created");

    let _initial_runtime = komga_server::app::build_router_with_config(&fixture.config);

    let book_path = fixture.library_root.join("Series-A").join("Book-001.cbz");
    let book_url = book_path.to_string_lossy().to_string();
    let initial_size = load_book_file_size(&fixture.paths.main_db, &book_url).await;

    let updated_payload = b"book-001-blocked-scan-output";
    fs::write(&book_path, updated_payload)
        .expect("book payload rewrite should succeed for blocked scan-output contract");

    let runtime = TaskRuntimeContext {
        database_file: fixture.paths.main_db.clone(),
        tasks_db_file: fixture.paths.tasks_db.clone(),
        lucene_data_directory: fixture.config.lucene_data_directory.clone(),
        consumes_queue: true,
        owns_main_database: true,
        owns_filesystem_scan_output: false,
        owns_sidecar_output: true,
        owns_search_index: true,
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(
        TaskQueueRecord::new("SCAN_LIBRARY:library-1", 900, Some("library-1".to_string()))
            .with_simple_type("SCAN_LIBRARY"),
    );
    scheduler
        .process_available(&runtime)
        .expect("blocked scan-output task should still drain cleanly");

    let updated_size = load_book_file_size(&fixture.paths.main_db, &book_url).await;
    assert_eq!(
        updated_size, initial_size,
        "runtime must not persist scan-derived book updates when filesystem scan output is external-owned",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_unknown_task_type_is_not_completed_or_silently_skipped() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-unknown-task-skip")
        .await
        .expect("scanner unknown-task fixture should be created");

    let _initial_runtime = komga_server::app::build_router_with_config(&fixture.config);

    let book_path = fixture.library_root.join("Series-A").join("Book-001.cbz");
    let book_url = book_path.to_string_lossy().to_string();
    let updated_payload = b"book-001-after-unknown-task";
    fs::write(&book_path, updated_payload)
        .expect("book payload rewrite should succeed for unknown task contract");

    let mut scheduler = TaskQueueScheduler::for_runtime(fixture.config.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "UNSUPPORTED_TASK:book-1",
        1000,
        Some("book-1".to_string()),
    ));
    scheduler.enqueue(
        TaskQueueRecord::new("SCAN_LIBRARY:library-1", 900, Some("library-1".to_string()))
            .with_simple_type("SCAN_LIBRARY"),
    );

    let error = scheduler
        .process_available(&fixture.config)
        .expect_err("unknown task type should surface as runtime error instead of being completed");
    assert!(
        error
            .to_string()
            .contains("unsupported runtime task type: UNSUPPORTED_TASK"),
        "unsupported task error should identify the unimplemented task type, got: {error}",
    );

    let updated_size = load_book_file_size(&fixture.paths.main_db, &book_url).await;
    assert_ne!(
        updated_size,
        updated_payload.len() as i64,
        "supported task behind unsupported head task must not run after unsupported-task failure",
    );

    let tasks_pool = connect_pool(fixture.paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open after unknown-task processing");
    let remaining_tasks = sqlx::query("SELECT COUNT(*) AS COUNT FROM TASK")
        .fetch_one(&tasks_pool)
        .await
        .expect("remaining task rows should be queryable")
        .get::<i64, _>("COUNT");
    let owned_tasks = sqlx::query("SELECT COUNT(*) AS COUNT FROM TASK WHERE OWNER IS NOT NULL")
        .fetch_one(&tasks_pool)
        .await
        .expect("owned task rows should be queryable")
        .get::<i64, _>("COUNT");
    tasks_pool.close().await;
    assert_eq!(
        remaining_tasks, 1,
        "unsupported task flow must delete the failed head task while keeping the later unprocessed task in TASK",
    );
    assert_eq!(
        owned_tasks, 0,
        "unsupported task flow must leave no claimed rows behind after deleting the failed task",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_startup_releases_previously_claimed_persisted_tasks() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-startup-disown-all")
        .await
        .expect("scanner startup disown fixture should be created");

    let tasks_pool = connect_pool(fixture.paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for startup disown test");
    sqlx::query(
        "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("SCAN_LIBRARY:library-1")
    .bind(100_i64)
    .bind("library-1")
    .bind("org.gotson.komga.domain.task.TaskScanLibrary")
    .bind("SCAN_LIBRARY")
    .bind("{}")
    .bind("stale-owner")
    .execute(&tasks_pool)
    .await
    .expect("claimed task row should be inserted");
    tasks_pool.close().await;

    let _background =
        komga_rust::infrastructure::task_queue::prepare_task_queue(fixture.config.clone(), None);

    let verify_pool = connect_pool(fixture.paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should reopen for startup disown verification");
    let owner = sqlx::query("SELECT OWNER FROM TASK WHERE ID = ?")
        .bind("SCAN_LIBRARY:library-1")
        .fetch_one(&verify_pool)
        .await
        .expect("task owner row should be queryable")
        .get::<Option<String>, _>("OWNER");
    verify_pool.close().await;

    assert_eq!(
        owner, None,
        "runtime startup must disown previously claimed persisted task rows before processing",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_startup_does_not_disown_tasks_when_tasks_writer_is_external_owned() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-blocked-tasks-disown")
        .await
        .expect("scanner blocked tasks-disown fixture should be created");

    let tasks_pool = connect_pool(fixture.paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for blocked tasks-disown test");
    sqlx::query(
        r#"
        INSERT INTO TASK (
            ID,
            PRIORITY,
            GROUP_ID,
            CLASS,
            SIMPLE_TYPE,
            PAYLOAD,
            OWNER
        ) VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind("SCAN_LIBRARY:library-1")
    .bind(100_i64)
    .bind("library-1")
    .bind("org.gotson.komga.domain.task.TaskScanLibrary")
    .bind("SCAN_LIBRARY")
    .bind("{}")
    .bind("stale-owner")
    .execute(&tasks_pool)
    .await
    .expect("claimed task row should be inserted");
    tasks_pool.close().await;

    let runtime = TaskRuntimeContext {
        database_file: fixture.paths.main_db.clone(),
        tasks_db_file: fixture.paths.tasks_db.clone(),
        lucene_data_directory: fixture.config.lucene_data_directory.clone(),
        consumes_queue: false,
        owns_main_database: false,
        owns_filesystem_scan_output: false,
        owns_sidecar_output: false,
        owns_search_index: false,
    };

    let _background = komga_rust::infrastructure::task_queue::prepare_task_queue(runtime, None);

    let verify_pool = connect_pool(fixture.paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should reopen for blocked tasks-disown verification");
    let owner = sqlx::query("SELECT OWNER FROM TASK WHERE ID = ?")
        .bind("SCAN_LIBRARY:library-1")
        .fetch_one(&verify_pool)
        .await
        .expect("task owner row should be queryable")
        .get::<Option<String>, _>("OWNER");
    verify_pool.close().await;

    assert_eq!(
        owner,
        Some("stale-owner".to_string()),
        "startup must not rewrite persisted task ownership when tasks database writer is external-owned",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_startup_does_not_enqueue_search_tasks_when_tasks_writer_is_external_owned() {
    let fixture =
        ScannerPersistenceFixture::new("scanner-persistence-blocked-tasks-search-startup")
            .await
            .expect("scanner blocked tasks-search-startup fixture should be created");

    let runtime = TaskRuntimeContext {
        database_file: fixture.paths.main_db.clone(),
        tasks_db_file: fixture.paths.tasks_db.clone(),
        lucene_data_directory: fixture.config.lucene_data_directory.clone(),
        consumes_queue: false,
        owns_main_database: false,
        owns_filesystem_scan_output: false,
        owns_sidecar_output: false,
        owns_search_index: false,
    };

    let background =
        komga_rust::infrastructure::task_queue::prepare_task_queue(runtime, Some("REBUILD_INDEX"));

    let snapshot = load_task_snapshot(&fixture.paths.tasks_db).await;
    assert_eq!(
        snapshot.task_rows, 0,
        "startup must not enqueue persisted search tasks when tasks database writer is external-owned",
    );
    let queued_tasks = background
        .task_queue
        .lock()
        .expect("startup task queue lock should not be poisoned")
        .count_by_simple_type();
    assert!(
        queued_tasks.is_empty(),
        "startup must not enqueue in-memory search tasks when tasks database writer is external-owned",
    );

    fixture.cleanup();
}

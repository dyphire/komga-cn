use super::support::*;
use super::*;

#[tokio::test]
async fn scanner_runtime_blocks_scan_output_when_filesystem_scan_writer_is_external_owned() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-blocked-scan-output")
        .await
        .expect("scanner blocked scan-output fixture should be created");

    process_scan_library_task(fixture.config.clone(), "library-1", 900, false)
        .expect("blocked scan-output contract should seed initial persisted rows");

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
    scheduler.enqueue(scan_library_task("library-1", 900, false));
    scheduler
        .process_available(&runtime)
        .expect("blocked scan-output task should still drain cleanly");

    let updated_size = load_book_file_size(&fixture.paths.main_db, &book_url).await;
    assert_eq!(
        updated_size, initial_size,
        "runtime must not persist scan-derived book updates when filesystem scan output is external-owned",
    );

    let tasks_pool = connect_test_pool(fixture.paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open after blocked scan-output drain");
    let remaining_tasks = sqlx::query("SELECT COUNT(*) AS COUNT FROM TASK")
        .fetch_one(&tasks_pool)
        .await
        .expect("remaining blocked scan-output task rows should be queryable")
        .get::<i64, _>("COUNT");
    let owned_tasks = sqlx::query("SELECT COUNT(*) AS COUNT FROM TASK WHERE OWNER IS NOT NULL")
        .fetch_one(&tasks_pool)
        .await
        .expect("owned blocked scan-output task rows should be queryable")
        .get::<i64, _>("COUNT");
    tasks_pool.close().await;

    assert_eq!(
        remaining_tasks, 0,
        "external-owned filesystem scan execution must still drain the persisted SCAN_LIBRARY task instead of leaving it queued forever",
    );
    assert_eq!(
        owned_tasks, 0,
        "external-owned filesystem scan execution must not leak task ownership after the no-op drain path",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_unknown_task_type_is_not_completed_or_silently_skipped() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-unknown-task-skip")
        .await
        .expect("scanner unknown-task fixture should be created");

    process_scan_library_task(fixture.config.clone(), "library-1", 900, false)
        .expect("unknown-task contract should seed initial persisted rows");

    let book_path = fixture.library_root.join("Series-A").join("Book-001.cbz");
    let book_url = book_path.to_string_lossy().to_string();
    let updated_payload = b"book-001-after-unknown-task";
    fs::write(&book_path, updated_payload)
        .expect("book payload rewrite should succeed for unknown task contract");

    let runtime = runtime_task_context_from_config(&fixture.config);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "UNSUPPORTED_TASK:book-1",
        1000,
        Some("book-1".to_string()),
    ));
    scheduler.enqueue(scan_library_task("library-1", 900, false));

    let error = scheduler
        .process_available(&runtime)
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

    let tasks_pool = connect_test_pool(fixture.paths.tasks_db.as_path(), 1)
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

    let tasks_pool = connect_test_pool(fixture.paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for startup disown test");
    let task_id = scan_library_task_id("library-1", false);
    let task_payload = scan_library_task_payload("library-1", 100, false);
    sqlx::query(
        "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&task_id)
    .bind(100_i64)
    .bind(Option::<String>::None)
    .bind("org.gotson.komga.application.tasks.Task$ScanLibrary")
    .bind("ScanLibrary")
    .bind(task_payload)
    .bind("stale-owner")
    .execute(&tasks_pool)
    .await
    .expect("claimed task row should be inserted");
    tasks_pool.close().await;

    let _background = komga_infrastructure::task_queue::worker_runtime::prepare_task_queue(
        runtime_task_context_from_config(&fixture.config),
        None,
    );

    let verify_pool = connect_test_pool(fixture.paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should reopen for startup disown verification");
    let owner = sqlx::query("SELECT OWNER FROM TASK WHERE ID = ?")
        .bind(task_id)
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
async fn scanner_startup_leaves_tasks_untouched_when_tasks_writer_is_external_owned() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-blocked-tasks-disown")
        .await
        .expect("scanner blocked tasks-disown fixture should be created");

    let tasks_pool = connect_test_pool(fixture.paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for blocked tasks-disown test");
    let task_id = scan_library_task_id("library-1", false);
    let task_payload = scan_library_task_payload("library-1", 100, false);
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
    .bind(&task_id)
    .bind(100_i64)
    .bind(Option::<String>::None)
    .bind("org.gotson.komga.application.tasks.Task$ScanLibrary")
    .bind("ScanLibrary")
    .bind(task_payload)
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

    let background = komga_infrastructure::task_queue::worker_runtime::prepare_task_queue(
        runtime,
        Some("REBUILD_INDEX"),
    );

    let verify_pool = connect_test_pool(fixture.paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should reopen for blocked tasks-disown verification");
    let task_rows = sqlx::query("SELECT COUNT(*) AS COUNT FROM TASK")
        .fetch_one(&verify_pool)
        .await
        .expect("task rows should be queryable")
        .get::<i64, _>("COUNT");
    let owner = sqlx::query("SELECT OWNER FROM TASK WHERE ID = ?")
        .bind(task_id)
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
    assert_eq!(
        task_rows, 1,
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

#[tokio::test]
async fn scanner_persisted_scan_library_payload_overrides_legacy_id_target_and_deep_flag() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-scan-payload-precedence")
        .await
        .expect("scanner payload precedence fixture should be created");

    let book_path = fixture.library_root.join("Series-A").join("Book-001.cbz");
    let book_url = book_path.to_string_lossy().to_string();
    let initial_page_size = write_scannable_cbz_fixture(&book_path, b"page-before-payload-wins")
        .expect("initial scan-library payload precedence fixture should be written");

    let runtime = runtime_task_context_from_config(&fixture.config);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(scan_library_task("library-1", 900, false));
    scheduler
        .process_available(&runtime)
        .expect("initial scan should seed scanner persistence state");

    assert_eq!(
        load_media_page_file_size(&fixture.paths.main_db, &book_url).await,
        initial_page_size,
        "fixture sanity: initial scan should persist MEDIA_PAGE rows before payload precedence replay",
    );

    let main_pool = connect_test_pool(fixture.paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for scan-library payload precedence timestamp");
    let library_last_modified_before =
        sqlx::query_scalar::<_, String>("SELECT LAST_MODIFIED_DATE FROM LIBRARY WHERE ID = ?")
            .bind("library-1")
            .fetch_one(&main_pool)
            .await
            .expect("library row should be queryable before payload precedence replay");
    main_pool.close().await;

    tokio::time::sleep(Duration::from_millis(1100)).await;
    let updated_page_size = write_scannable_cbz_fixture(&book_path, b"page-after-payload-wins")
        .expect("updated scan-library payload precedence fixture should be written");

    let tasks_pool = connect_test_pool(fixture.paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for scan-library payload precedence seed");
    sqlx::query(
        "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) VALUES (?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind("SCAN_LIBRARY:missing-library:DEEP:true")
    .bind(900_i64)
    .bind(Option::<String>::None)
    .bind("org.gotson.komga.application.tasks.Task$ScanLibrary")
    .bind("ScanLibrary")
    .bind(
        json!({
            "libraryId": "library-1",
            "scanDeep": false,
            "priority": 900,
            "groupId": Value::Null,
            "uniqueId": "SCAN_LIBRARY:missing-library:DEEP:true"
        })
        .to_string(),
    )
    .execute(&tasks_pool)
    .await
    .expect("legacy scan-library task row should be inserted for payload precedence");
    tasks_pool.close().await;

    let runtime = runtime_task_context_from_config(&fixture.config);
    let mut replay = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    replay
        .process_available(&runtime)
        .expect("legacy scan-library payload precedence row should process successfully");

    let main_pool = connect_test_pool(fixture.paths.main_db.as_path(), 1)
        .await
        .expect("main db should reopen for scan-library payload precedence timestamp");
    let library_last_modified_after =
        sqlx::query_scalar::<_, String>("SELECT LAST_MODIFIED_DATE FROM LIBRARY WHERE ID = ?")
            .bind("library-1")
            .fetch_one(&main_pool)
            .await
            .expect("library row should be queryable after payload precedence replay");
    main_pool.close().await;
    assert_ne!(
        library_last_modified_after, library_last_modified_before,
        "scan-library replay must honor payload.libraryId over the legacy id target so the real library is scanned",
    );
    assert_eq!(
        load_media_page_file_size(&fixture.paths.main_db, &book_url).await,
        initial_page_size,
        "scan-library replay must honor payload.scanDeep over the legacy :DEEP: suffix so a false payload does not force deep reanalysis",
    );
    assert_ne!(
        initial_page_size, updated_page_size,
        "fixture sanity: changed archive content should produce a different deep-scan page size for precedence verification",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_persisted_scan_library_recovers_deep_flag_from_underscore_legacy_id() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-scan-underscore-deep")
        .await
        .expect("scanner underscore deep fixture should be created");

    let book_path = fixture.library_root.join("Series-A").join("Book-001.cbz");
    let book_url = book_path.to_string_lossy().to_string();
    let initial_page_size = write_scannable_cbz_fixture(&book_path, b"page-before-underscore")
        .expect("initial underscore scan fixture should be written");

    let runtime = runtime_task_context_from_config(&fixture.config);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(scan_library_task("library-1", 900, false));
    scheduler
        .process_available(&runtime)
        .expect("initial scan should seed underscore deep replay state");

    assert_eq!(
        load_media_page_file_size(&fixture.paths.main_db, &book_url).await,
        initial_page_size,
        "fixture sanity: initial scan should persist MEDIA_PAGE rows before underscore replay",
    );

    tokio::time::sleep(Duration::from_millis(1100)).await;
    let expected_updated_page_size =
        write_scannable_cbz_fixture(&book_path, b"page-after-underscore-deep")
            .expect("updated underscore scan fixture should be written");

    scheduler.enqueue(
        TaskQueueRecord::new("SCAN_LIBRARY_library-1_DEEP_true", 900, None)
            .with_simple_type("SCAN_LIBRARY"),
    );
    scheduler
        .process_available(&runtime)
        .expect("underscore legacy scan-library id should process successfully after canonical payload restoration");

    assert_eq!(
        load_media_page_file_size(&fixture.paths.main_db, &book_url).await,
        expected_updated_page_size,
        "scan-library enqueue must recover deep=true from the legacy _DEEP_ suffix so persisted execution still performs deep reanalysis",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_persisted_scan_library_recovers_deep_flag_from_colon_legacy_id() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-scan-colon-deep")
        .await
        .expect("scanner colon deep fixture should be created");

    let book_path = fixture.library_root.join("Series-A").join("Book-001.cbz");
    let book_url = book_path.to_string_lossy().to_string();
    let initial_page_size = write_scannable_cbz_fixture(&book_path, b"page-before-colon")
        .expect("initial colon scan fixture should be written");

    let runtime = runtime_task_context_from_config(&fixture.config);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(scan_library_task("library-1", 900, false));
    scheduler
        .process_available(&runtime)
        .expect("initial scan should seed colon deep replay state");

    assert_eq!(
        load_media_page_file_size(&fixture.paths.main_db, &book_url).await,
        initial_page_size,
        "fixture sanity: initial scan should persist MEDIA_PAGE rows before colon replay",
    );

    tokio::time::sleep(Duration::from_millis(1100)).await;
    let expected_updated_page_size =
        write_scannable_cbz_fixture(&book_path, b"page-after-colon-deep")
            .expect("updated colon scan fixture should be written");

    scheduler.enqueue(TaskQueueRecord::new(
        "SCAN_LIBRARY:library-1:DEEP:true",
        900,
        None,
    ));
    scheduler
        .process_available(&runtime)
        .expect("colon legacy scan-library id should process successfully after canonical payload restoration");

    assert_eq!(
        load_media_page_file_size(&fixture.paths.main_db, &book_url).await,
        expected_updated_page_size,
        "scan-library execution must recover deep=true from the legacy :DEEP: suffix when payload is absent so persisted replay still performs deep reanalysis",
    );

    fixture.cleanup();
}

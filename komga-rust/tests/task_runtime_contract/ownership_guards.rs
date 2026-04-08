use super::*;

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
async fn runtime_executes_kotlin_persisted_refresh_book_metadata_task() {
    let paths = new_router_fixture("runtime-executes-kotlin-refresh-book-metadata-task").await;
    seed_router_contract_data(&paths).await;

    let sidecar_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&sidecar_dir).expect("book metadata sidecar directory should exist");
    std::fs::write(
        sidecar_dir.join("book-1.xml"),
        br#"<ComicInfo><Title>Kotlin Refresh Title</Title><Summary>Kotlin Refresh Summary</Summary></ComicInfo>"#,
    )
    .expect("book metadata sidecar fixture should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for Kotlin persisted metadata fixture setup");
    sqlx::query("DELETE FROM SIDECAR WHERE PARENT_URL = ?")
        .bind("books/book-1.epub")
        .execute(&pool)
        .await
        .expect(
            "existing book metadata sidecars should be cleared before Kotlin persisted task test",
        );
    sqlx::query(
        "INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)",
    )
    .bind("books/book-1.xml")
    .bind("books/book-1.epub")
    .bind(1_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("book metadata sidecar row should be inserted for Kotlin persisted task test");
    pool.close().await;

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for Kotlin persisted metadata task setup");
    sqlx::query(
        "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) VALUES (?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind("REFRESH_BOOK_METADATA_book-1")
    .bind(80_i64)
    .bind("series-1")
    .bind("org.gotson.komga.application.tasks.Task$RefreshBookMetadata")
    .bind("RefreshBookMetadata")
    .bind(
        json!({
            "bookId": "book-1",
            "capabilities": [
                "TITLE",
                "SUMMARY",
                "NUMBER",
                "NUMBER_SORT",
                "RELEASE_DATE",
                "AUTHORS",
                "TAGS",
                "ISBN",
                "READ_LISTS",
                "THUMBNAILS",
                "LINKS"
            ],
            "priority": 80,
            "groupId": "series-1",
            "uniqueId": "REFRESH_BOOK_METADATA_book-1"
        })
        .to_string(),
    )
    .execute(&tasks_pool)
    .await
    .expect("Kotlin persisted metadata task row should be inserted");
    tasks_pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler
        .process_available(&runtime)
        .expect("runtime should execute Kotlin persisted RefreshBookMetadata tasks successfully");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for Kotlin persisted metadata verification");
    let metadata =
        sqlx::query("SELECT TITLE, SUMMARY FROM BOOK_METADATA WHERE BOOK_ID = ? LIMIT 1")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect("book metadata row should be queryable after Kotlin persisted task execution");
    verify_pool.close().await;

    assert_eq!(metadata.get::<String, _>("TITLE"), "Kotlin Refresh Title");
    assert_eq!(
        metadata.get::<String, _>("SUMMARY"),
        "Kotlin Refresh Summary"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_refresh_series_metadata_applies_oneshot_provider_fields() {
    let paths = new_router_fixture("runtime-refresh-series-metadata-oneshot-provider").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for oneshot series metadata fixture setup");
    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, ONESHOT) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("series-oneshot")
    .bind(0_i64)
    .bind("OneShot Series")
    .bind("series/series-oneshot")
    .bind("library-1")
    .bind(true)
    .execute(&pool)
    .await
    .expect("oneshot series row should be inserted for series metadata fixture");
    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, SUMMARY, SERIES_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind("Stale Series Title")
    .bind("Stale Series Title")
    .bind("Stale Series Summary")
    .bind("series-oneshot")
    .execute(&pool)
    .await
    .expect("oneshot series metadata row should be inserted for series metadata fixture");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, ONESHOT) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-oneshot")
    .bind(0_i64)
    .bind("oneshot-book.cbz")
    .bind("books/oneshot-book.cbz")
    .bind("series-oneshot")
    .bind(2_048_i64)
    .bind(1_i64)
    .bind("library-1")
    .bind(true)
    .execute(&pool)
    .await
    .expect("oneshot book row should be inserted for series metadata fixture");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/epub+zip")
        .bind("READY")
        .bind("book-oneshot")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("oneshot media row should be inserted for series metadata fixture");
    sqlx::query(
        "INSERT INTO BOOK_METADATA (TITLE, SUMMARY, NUMBER, NUMBER_SORT, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("OneShot Book Title")
    .bind("OneShot Book Summary")
    .bind("1")
    .bind(1.0_f64)
    .bind("book-oneshot")
    .execute(&pool)
    .await
    .expect("oneshot book metadata row should be inserted for series metadata fixture");
    pool.close().await;

    let runtime = TaskRuntimeContext {
        owns_search_index: false,
        ..runtime_task_context(&paths)
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "REFRESH_SERIES_METADATA:series-oneshot",
        1_000,
        Some("series-oneshot".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("oneshot refresh-series-metadata task should process successfully");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for oneshot series metadata verification");
    let metadata = sqlx::query(
        "SELECT STATUS, TITLE, TITLE_SORT, SUMMARY, TOTAL_BOOK_COUNT FROM SERIES_METADATA WHERE SERIES_ID = ? LIMIT 1",
    )
    .bind("series-oneshot")
    .fetch_one(&verify_pool)
    .await
    .expect("oneshot series metadata row should be queryable after refresh");
    verify_pool.close().await;

    assert_eq!(metadata.get::<String, _>("STATUS"), "ENDED");
    assert_eq!(metadata.get::<String, _>("TITLE"), "OneShot Book Title");
    assert_eq!(
        metadata.get::<String, _>("TITLE_SORT"),
        "OneShot Book Title"
    );
    assert_eq!(metadata.get::<String, _>("SUMMARY"), "OneShot Book Summary");
    assert_eq!(metadata.get::<i64, _>("TOTAL_BOOK_COUNT"), 1_i64);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_executes_kotlin_persisted_repair_extension_task() {
    let paths = new_router_fixture("runtime-executes-kotlin-repair-extension-task").await;
    seed_router_contract_data(&paths).await;

    std::fs::create_dir_all(paths.config_dir.join("books"))
        .expect("book directory should exist for Kotlin persisted repair-extension task");
    let source_path = paths.config_dir.join("books/repair-book.bin");
    std::fs::write(&source_path, b"kotlin-repair-extension")
        .expect("repair-extension source should be written for Kotlin persisted task");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for Kotlin persisted repair-extension fixture setup");
    sqlx::query("UPDATE LIBRARY SET REPAIR_EXTENSIONS = 1 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("repair extensions flag should be enabled for Kotlin persisted task");
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
    .expect("repair-extension fixture book row should be inserted for Kotlin persisted task");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/pdf")
        .bind("READY")
        .bind("book-repair-1")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("repair-extension fixture media row should be inserted for Kotlin persisted task");
    pool.close().await;

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for Kotlin persisted repair-extension task setup");
    sqlx::query(
        "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) VALUES (?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind("REPAIR_EXTENSION_book-repair-1")
    .bind(12_i64)
    .bind("series-1")
    .bind("org.gotson.komga.application.tasks.Task$RepairExtension")
    .bind("RepairExtension")
    .bind(
        json!({
            "bookId": "book-repair-1",
            "priority": 12,
            "groupId": "series-1",
            "uniqueId": "REPAIR_EXTENSION_book-repair-1"
        })
        .to_string(),
    )
    .execute(&tasks_pool)
    .await
    .expect("Kotlin persisted repair-extension task row should be inserted");
    tasks_pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler
        .process_available(&runtime)
        .expect("runtime should execute Kotlin persisted RepairExtension tasks successfully");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for Kotlin persisted repair-extension verification");
    let url = sqlx::query("SELECT URL FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-repair-1")
        .fetch_one(&verify_pool)
        .await
        .expect("repair-extension book row should be queryable after Kotlin persisted task")
        .get::<String, _>("URL");
    verify_pool.close().await;

    assert_eq!(url, "books/repair-book.pdf");
    assert!(paths.config_dir.join("books/repair-book.pdf").exists());
    assert!(!source_path.exists());

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_refresh_book_metadata_can_import_readlists_without_applying_book_fields() {
    let paths =
        new_router_fixture("runtime-refresh-book-metadata-readlists-without-book-fields").await;
    seed_router_contract_data(&paths).await;

    let sidecar_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&sidecar_dir).expect("book metadata sidecar directory should exist");
    std::fs::write(
        sidecar_dir.join("book-1.xml"),
        br#"<ComicInfo><Title>Should Stay Book 1</Title><AlternateSeries>Reading Order</AlternateSeries><AlternateNumber>7</AlternateNumber></ComicInfo>"#,
    )
    .expect("book metadata sidecar fixture with read list should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for readlist-only metadata fixture setup");
    sqlx::query(
        "UPDATE LIBRARY SET IMPORT_COMICINFO_BOOK = 0, IMPORT_COMICINFO_READLIST = 1 WHERE ID = ?",
    )
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("library ComicInfo import flags should be updated for readlist-only metadata test");
    sqlx::query("DELETE FROM SIDECAR WHERE PARENT_URL = ?")
        .bind("books/book-1.epub")
        .execute(&pool)
        .await
        .expect("existing book metadata sidecars should be cleared before readlist-only test");
    sqlx::query("DELETE FROM READLIST_BOOK")
        .execute(&pool)
        .await
        .expect("existing readlist memberships should be cleared before readlist-only test");
    sqlx::query("DELETE FROM READLIST")
        .execute(&pool)
        .await
        .expect("existing readlists should be cleared before readlist-only test");
    sqlx::query(
        "INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)",
    )
    .bind("books/book-1.xml")
    .bind("books/book-1.epub")
    .bind(1_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("book metadata sidecar row should be inserted for readlist-only test");
    pool.close().await;

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for readlist-only metadata task setup");
    sqlx::query(
        "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) VALUES (?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind("REFRESH_BOOK_METADATA_book-1")
    .bind(80_i64)
    .bind("series-1")
    .bind("org.gotson.komga.application.tasks.Task$RefreshBookMetadata")
    .bind("RefreshBookMetadata")
    .bind(
        json!({
            "bookId": "book-1",
            "capabilities": ["READ_LISTS"],
            "priority": 80,
            "groupId": "series-1",
            "uniqueId": "REFRESH_BOOK_METADATA_book-1"
        })
        .to_string(),
    )
    .execute(&tasks_pool)
    .await
    .expect("readlist-only metadata task row should be inserted");
    tasks_pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler
        .process_available(&runtime)
        .expect("runtime should process readlist-only RefreshBookMetadata tasks successfully");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for readlist-only metadata verification");
    let metadata = sqlx::query("SELECT TITLE FROM BOOK_METADATA WHERE BOOK_ID = ? LIMIT 1")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("book metadata row should be queryable after readlist-only task execution");
    let readlist =
        sqlx::query("SELECT ID, NAME, BOOK_COUNT, ORDERED FROM READLIST WHERE NAME = ? LIMIT 1")
            .bind("Reading Order")
            .fetch_one(&verify_pool)
            .await
            .expect("ComicInfo read list should be created when READ_LISTS capability is enabled");
    let readlist_book = sqlx::query(
        "SELECT NUMBER FROM READLIST_BOOK WHERE READLIST_ID = ? AND BOOK_ID = ? LIMIT 1",
    )
    .bind(readlist.get::<String, _>("ID"))
    .bind("book-1")
    .fetch_one(&verify_pool)
    .await
    .expect("ComicInfo read list should contain the refreshed book");
    verify_pool.close().await;

    assert_eq!(
        metadata.get::<String, _>("TITLE"),
        "Book 1",
        "READ_LISTS-only refresh must not apply ComicInfo book fields when importComicInfoBook is disabled",
    );
    assert_eq!(readlist.get::<String, _>("NAME"), "Reading Order");
    assert_eq!(readlist.get::<i64, _>("BOOK_COUNT"), 1);
    assert_eq!(readlist.get::<i64, _>("ORDERED"), 1);
    assert_eq!(readlist_book.get::<i64, _>("NUMBER"), 7);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_refresh_book_metadata_assigns_zero_to_readlists_without_explicit_number() {
    let paths =
        new_router_fixture("runtime-refresh-book-metadata-readlists-without-explicit-number").await;
    seed_router_contract_data(&paths).await;

    let sidecar_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&sidecar_dir).expect("book metadata sidecar directory should exist");
    std::fs::write(
        sidecar_dir.join("book-1.xml"),
        br#"<ComicInfo><StoryArc>Unnumbered Reading Order</StoryArc></ComicInfo>"#,
    )
    .expect("book metadata sidecar fixture without explicit readlist number should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for unnumbered readlist metadata fixture setup");
    sqlx::query("DELETE FROM SIDECAR WHERE PARENT_URL = ?")
        .bind("books/book-1.epub")
        .execute(&pool)
        .await
        .expect(
            "existing book metadata sidecars should be cleared before unnumbered readlist test",
        );
    sqlx::query("DELETE FROM READLIST_BOOK")
        .execute(&pool)
        .await
        .expect("existing readlist memberships should be cleared before unnumbered readlist test");
    sqlx::query("DELETE FROM READLIST")
        .execute(&pool)
        .await
        .expect("existing readlists should be cleared before unnumbered readlist test");
    sqlx::query(
        "UPDATE LIBRARY SET IMPORT_COMICINFO_BOOK = 0, IMPORT_COMICINFO_READLIST = 1 WHERE ID = ?",
    )
    .bind("library-1")
    .execute(&pool)
    .await
    .expect(
        "library ComicInfo import flags should be updated for unnumbered readlist metadata test",
    );
    sqlx::query(
        "INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)",
    )
    .bind("books/book-1.xml")
    .bind("books/book-1.epub")
    .bind(1_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("book metadata sidecar row should be inserted for unnumbered readlist test");
    pool.close().await;

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for unnumbered readlist metadata task setup");
    sqlx::query(
        "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) VALUES (?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind("REFRESH_BOOK_METADATA_book-1")
    .bind(80_i64)
    .bind("series-1")
    .bind("org.gotson.komga.application.tasks.Task$RefreshBookMetadata")
    .bind("RefreshBookMetadata")
    .bind(
        json!({
            "bookId": "book-1",
            "capabilities": ["READ_LISTS"],
            "priority": 80,
            "groupId": "series-1",
            "uniqueId": "REFRESH_BOOK_METADATA_book-1"
        })
        .to_string(),
    )
    .execute(&tasks_pool)
    .await
    .expect("unnumbered readlist metadata task row should be inserted");
    tasks_pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.process_available(&runtime).expect(
        "runtime should process unnumbered readlist RefreshBookMetadata tasks successfully",
    );

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for unnumbered readlist metadata verification");
    let readlist =
        sqlx::query("SELECT ID, NAME, BOOK_COUNT, ORDERED FROM READLIST WHERE NAME = ? LIMIT 1")
            .bind("Unnumbered Reading Order")
            .fetch_one(&verify_pool)
            .await
            .expect("ComicInfo read list without explicit number should be created");
    let readlist_book = sqlx::query(
        "SELECT NUMBER FROM READLIST_BOOK WHERE READLIST_ID = ? AND BOOK_ID = ? LIMIT 1",
    )
    .bind(readlist.get::<String, _>("ID"))
    .bind("book-1")
    .fetch_one(&verify_pool)
    .await
    .expect("ComicInfo read list membership without explicit number should be inserted");
    verify_pool.close().await;

    assert_eq!(readlist.get::<i64, _>("BOOK_COUNT"), 1);
    assert_eq!(readlist.get::<i64, _>("ORDERED"), 1);
    assert_eq!(readlist_book.get::<i64, _>("NUMBER"), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_refresh_book_metadata_applies_comicinfo_number_when_capability_requests_it() {
    let paths = new_router_fixture("runtime-refresh-book-metadata-applies-comicinfo-number").await;
    seed_router_contract_data(&paths).await;

    let sidecar_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&sidecar_dir).expect("book metadata sidecar directory should exist");
    std::fs::write(
        sidecar_dir.join("book-1.xml"),
        br#"<ComicInfo><Number>7</Number></ComicInfo>"#,
    )
    .expect("book metadata sidecar fixture with number should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for number metadata fixture setup");
    sqlx::query("DELETE FROM SIDECAR WHERE PARENT_URL = ?")
        .bind("books/book-1.epub")
        .execute(&pool)
        .await
        .expect("existing book metadata sidecars should be cleared before number capability test");
    sqlx::query(
        "INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)",
    )
    .bind("books/book-1.xml")
    .bind("books/book-1.epub")
    .bind(1_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("book metadata sidecar row should be inserted for number capability test");
    pool.close().await;

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for number metadata task setup");
    sqlx::query(
        "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) VALUES (?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind("REFRESH_BOOK_METADATA_book-1")
    .bind(80_i64)
    .bind("series-1")
    .bind("org.gotson.komga.application.tasks.Task$RefreshBookMetadata")
    .bind("RefreshBookMetadata")
    .bind(
        json!({
            "bookId": "book-1",
            "capabilities": ["NUMBER"],
            "priority": 80,
            "groupId": "series-1",
            "uniqueId": "REFRESH_BOOK_METADATA_book-1"
        })
        .to_string(),
    )
    .execute(&tasks_pool)
    .await
    .expect("number-only metadata task row should be inserted");
    tasks_pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler
        .process_available(&runtime)
        .expect("runtime should process number-only RefreshBookMetadata tasks successfully");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for number metadata verification");
    let metadata =
        sqlx::query("SELECT NUMBER, NUMBER_SORT FROM BOOK_METADATA WHERE BOOK_ID = ? LIMIT 1")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect("book metadata number row should be queryable after number capability task");
    verify_pool.close().await;

    assert_eq!(metadata.get::<String, _>("NUMBER"), "7");
    assert_eq!(metadata.get::<f64, _>("NUMBER_SORT"), 7.0_f64);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_refresh_book_metadata_applies_remaining_comicinfo_fields_with_lock_semantics() {
    let paths =
        new_router_fixture("runtime-refresh-book-metadata-applies-remaining-comicinfo-fields")
            .await;
    seed_router_contract_data(&paths).await;

    let sidecar_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&sidecar_dir).expect("book metadata sidecar directory should exist");
    std::fs::write(
        sidecar_dir.join("book-1.xml"),
        br#"<ComicInfo><Year>2025</Year><Month>3</Month><Day>4</Day><Writer>Alice Writer, Bob Writer</Writer><Penciller>Cara Pencil</Penciller><Web>https://example.com/series https://komga.org/docs invalid-url</Web><Tags>Sci-Fi, Adventure, sci-fi</Tags><GTIN>9780306406157</GTIN></ComicInfo>"#,
    )
    .expect("book metadata sidecar fixture with remaining ComicInfo fields should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for remaining ComicInfo metadata fixture setup");
    sqlx::query("DELETE FROM SIDECAR WHERE PARENT_URL = ?")
        .bind("books/book-1.epub")
        .execute(&pool)
        .await
        .expect(
            "existing book metadata sidecars should be cleared before remaining ComicInfo test",
        );
    sqlx::query(
        "INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)",
    )
    .bind("books/book-1.xml")
    .bind("books/book-1.epub")
    .bind(1_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("book metadata sidecar row should be inserted for remaining ComicInfo test");
    sqlx::query(
        "UPDATE BOOK_METADATA SET RELEASE_DATE = ?, RELEASE_DATE_LOCK = 1, ISBN = ?, ISBN_LOCK = 1, AUTHORS_LOCK = 0, TAGS_LOCK = 0, LINKS_LOCK = 0 WHERE BOOK_ID = ?",
    )
    .bind("2024-01-15")
    .bind("9789999999991")
    .bind("book-1")
    .execute(&pool)
    .await
    .expect("book metadata lock state should be updated for remaining ComicInfo test");
    sqlx::query("DELETE FROM BOOK_METADATA_LINK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("existing metadata links should be cleared before remaining ComicInfo test");
    sqlx::query("INSERT INTO BOOK_METADATA_LINK (BOOK_ID, LABEL, URL) VALUES (?, ?, ?)")
        .bind("book-1")
        .bind("old.example")
        .bind("https://old.example/link")
        .execute(&pool)
        .await
        .expect("seed metadata link should be inserted before remaining ComicInfo test");
    pool.close().await;

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for remaining ComicInfo metadata task setup");
    sqlx::query(
        "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) VALUES (?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind("REFRESH_BOOK_METADATA_book-1")
    .bind(80_i64)
    .bind("series-1")
    .bind("org.gotson.komga.application.tasks.Task$RefreshBookMetadata")
    .bind("RefreshBookMetadata")
    .bind(
        json!({
            "bookId": "book-1",
            "capabilities": ["RELEASE_DATE", "AUTHORS", "TAGS", "ISBN", "LINKS"],
            "priority": 80,
            "groupId": "series-1",
            "uniqueId": "REFRESH_BOOK_METADATA_book-1"
        })
        .to_string(),
    )
    .execute(&tasks_pool)
    .await
    .expect("remaining ComicInfo metadata task row should be inserted");
    tasks_pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler
        .process_available(&runtime)
        .expect("runtime should process remaining ComicInfo metadata fields successfully");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for remaining ComicInfo metadata verification");
    let metadata =
        sqlx::query("SELECT RELEASE_DATE, ISBN FROM BOOK_METADATA WHERE BOOK_ID = ? LIMIT 1")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect(
                "book metadata row should be queryable after remaining ComicInfo task execution",
            );
    let authors = sqlx::query(
        "SELECT NAME, ROLE FROM BOOK_METADATA_AUTHOR WHERE BOOK_ID = ? ORDER BY ROLE ASC, NAME ASC",
    )
    .bind("book-1")
    .fetch_all(&verify_pool)
    .await
    .expect("book metadata authors should be queryable after remaining ComicInfo task execution");
    let tags = sqlx::query(
        "SELECT TAG FROM BOOK_METADATA_TAG WHERE BOOK_ID = ? ORDER BY TAG COLLATE NOCASE ASC",
    )
    .bind("book-1")
    .fetch_all(&verify_pool)
    .await
    .expect("book metadata tags should be queryable after remaining ComicInfo task execution");
    let links = sqlx::query(
        "SELECT LABEL, URL FROM BOOK_METADATA_LINK WHERE BOOK_ID = ? ORDER BY LABEL COLLATE NOCASE ASC, URL ASC",
    )
    .bind("book-1")
    .fetch_all(&verify_pool)
    .await
    .expect("book metadata links should be queryable after remaining ComicInfo task execution");
    verify_pool.close().await;

    assert_eq!(metadata.get::<String, _>("RELEASE_DATE"), "2024-01-15");
    assert_eq!(metadata.get::<String, _>("ISBN"), "9789999999991");
    assert_eq!(
        authors
            .iter()
            .map(|row| (row.get::<String, _>("NAME"), row.get::<String, _>("ROLE")))
            .collect::<Vec<_>>(),
        vec![
            ("Cara Pencil".to_string(), "penciller".to_string()),
            ("Alice Writer".to_string(), "writer".to_string()),
            ("Bob Writer".to_string(), "writer".to_string()),
        ],
        "unlocked authors should be replaced from ComicInfo using Kotlin author-role semantics",
    );
    assert_eq!(
        tags.iter()
            .map(|row| row.get::<String, _>("TAG"))
            .collect::<Vec<_>>(),
        vec!["adventure".to_string(), "sci-fi".to_string()],
        "unlocked tags should be lowercased and deduplicated from ComicInfo",
    );
    assert_eq!(
        links
            .iter()
            .map(|row| (row.get::<String, _>("LABEL"), row.get::<String, _>("URL")))
            .collect::<Vec<_>>(),
        vec![
            (
                "example.com".to_string(),
                "https://example.com/series".to_string(),
            ),
            (
                "komga.org".to_string(),
                "https://komga.org/docs".to_string(),
            ),
        ],
        "unlocked links should be replaced from valid ComicInfo Web URIs only",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_refresh_book_metadata_does_not_run_comicinfo_for_isbn_or_tags_only_capabilities() {
    let paths =
        new_router_fixture("runtime-refresh-book-metadata-skips-comicinfo-for-isbn-tags-only")
            .await;
    seed_router_contract_data(&paths).await;
    seed_router_cbz_book(
        &paths,
        "book-comicinfo-gate-1",
        "series-1",
        "comicinfo-gate.cbz",
        "ComicInfo Gate Book",
    )
    .await;

    let sidecar_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&sidecar_dir).expect("book metadata sidecar directory should exist");
    std::fs::write(
        sidecar_dir.join("comicinfo-gate.xml"),
        br#"<ComicInfo><Tags>Sci-Fi, Mystery</Tags><GTIN>9780306406157</GTIN></ComicInfo>"#,
    )
    .expect("ComicInfo ISBN/tags-only sidecar fixture should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for ComicInfo gate fixture setup");
    sqlx::query("DELETE FROM SIDECAR WHERE PARENT_URL = ?")
        .bind("books/comicinfo-gate.cbz")
        .execute(&pool)
        .await
        .expect("existing ComicInfo gate sidecars should be cleared before test");
    sqlx::query(
        "INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)",
    )
    .bind("books/comicinfo-gate.xml")
    .bind("books/comicinfo-gate.cbz")
    .bind(1_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("ComicInfo gate sidecar row should be inserted");
    sqlx::query(
        "UPDATE LIBRARY SET IMPORT_COMICINFO_BOOK = 1, IMPORT_EPUB_BOOK = 0, IMPORT_BARCODE_ISBN = 0 WHERE ID = ?",
    )
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("library metadata import flags should isolate ComicInfo gate behavior for refresh test");
    sqlx::query("UPDATE BOOK_METADATA SET ISBN = ?, ISBN_LOCK = 0 WHERE BOOK_ID = ?")
        .bind("")
        .bind("book-comicinfo-gate-1")
        .execute(&pool)
        .await
        .expect("book metadata isbn should be reset before ComicInfo gate test");
    sqlx::query("DELETE FROM BOOK_METADATA_TAG WHERE BOOK_ID = ?")
        .bind("book-comicinfo-gate-1")
        .execute(&pool)
        .await
        .expect("book metadata tags should be cleared before ComicInfo gate test");
    pool.close().await;

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for ComicInfo gate metadata task setup");
    sqlx::query(
        "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) VALUES (?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind("REFRESH_BOOK_METADATA_book-comicinfo-gate-1")
    .bind(80_i64)
    .bind("series-1")
    .bind("org.gotson.komga.application.tasks.Task$RefreshBookMetadata")
    .bind("RefreshBookMetadata")
    .bind(
        json!({
            "bookId": "book-comicinfo-gate-1",
            "capabilities": ["ISBN", "TAGS"],
            "priority": 80,
            "groupId": "series-1",
            "uniqueId": "REFRESH_BOOK_METADATA_book-comicinfo-gate-1"
        })
        .to_string(),
    )
    .execute(&tasks_pool)
    .await
    .expect("ComicInfo gate metadata task row should be inserted");
    tasks_pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler
        .process_available(&runtime)
        .expect("runtime should process ComicInfo gate metadata tasks successfully");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for ComicInfo gate metadata verification");
    let metadata = sqlx::query("SELECT ISBN FROM BOOK_METADATA WHERE BOOK_ID = ? LIMIT 1")
        .bind("book-comicinfo-gate-1")
        .fetch_one(&verify_pool)
        .await
        .expect("book metadata row should be queryable after ComicInfo gate task execution");
    let tags = sqlx::query("SELECT TAG FROM BOOK_METADATA_TAG WHERE BOOK_ID = ?")
        .bind("book-comicinfo-gate-1")
        .fetch_all(&verify_pool)
        .await
        .expect("book metadata tags should be queryable after ComicInfo gate task execution");
    verify_pool.close().await;

    assert_eq!(
        metadata.get::<String, _>("ISBN"),
        "",
        "ISBN-only refresh must not trigger ComicInfo provider when Kotlin capability gate would skip it",
    );
    assert!(
        tags.is_empty(),
        "TAGS-only refresh must not trigger ComicInfo provider when Kotlin capability gate would skip it",
    );

    cleanup_router_fixture(paths);
}

fn write_router_epub_with_package_document(
    paths: &RuntimeDbPaths,
    relative_book_path: &str,
    package_document: &str,
) {
    write_router_epub_with_package_document_and_entries(
        paths,
        relative_book_path,
        package_document,
        &[],
    );
}

fn write_router_epub_with_package_document_and_entries(
    paths: &RuntimeDbPaths,
    relative_book_path: &str,
    package_document: &str,
    extra_entries: &[(&str, &[u8])],
) {
    let epub_path = paths.config_dir.join(relative_book_path);
    if let Some(parent) = epub_path.parent() {
        std::fs::create_dir_all(parent).expect("epub metadata parent directory should be created");
    }

    let file = std::fs::File::create(&epub_path).expect("epub metadata fixture file should exist");
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o644);

    zip.start_file("mimetype", options)
        .expect("epub metadata mimetype entry should be created");
    use std::io::Write;
    zip.write_all(b"application/epub+zip")
        .expect("epub metadata mimetype should be written");

    zip.start_file("META-INF/container.xml", options)
        .expect("epub metadata container entry should be created");
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
    )
    .expect("epub metadata container should be written");

    zip.start_file("OEBPS/content.opf", options)
        .expect("epub metadata package entry should be created");
    zip.write_all(package_document.as_bytes())
        .expect("epub metadata package should be written");

    zip.start_file("OEBPS/chapter.xhtml", options)
        .expect("epub metadata chapter entry should be created");
    zip.write_all(
        br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>EPUB metadata fixture</p></body></html>"#,
    )
    .expect("epub metadata chapter should be written");

    for (entry_name, entry_bytes) in extra_entries {
        zip.start_file(*entry_name, options)
            .expect("epub metadata extra entry should be created");
        zip.write_all(entry_bytes)
            .expect("epub metadata extra entry should be written");
    }

    zip.finish()
        .expect("epub metadata fixture should finish successfully");
}

fn write_router_cbz_with_single_page(
    paths: &RuntimeDbPaths,
    relative_book_path: &str,
    page_file_name: &str,
    page_bytes: &[u8],
) {
    let archive_path = paths.config_dir.join(relative_book_path);
    if let Some(parent) = archive_path.parent() {
        std::fs::create_dir_all(parent).expect("cbz barcode parent directory should be created");
    }

    let file = std::fs::File::create(&archive_path).expect("cbz barcode fixture file should exist");
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o644);

    use std::io::Write;
    zip.start_file(page_file_name, options)
        .expect("cbz barcode page entry should be created");
    zip.write_all(page_bytes)
        .expect("cbz barcode page should be written");
    zip.finish()
        .expect("cbz barcode fixture should finish successfully");
}

fn render_ean13_png_bytes(digits: &str) -> Vec<u8> {
    const MODULE_WIDTH: u32 = 4;
    const BAR_HEIGHT: u32 = 140;
    const TOP_MARGIN: u32 = 10;
    const QUIET_ZONE: &str = "0000000000";
    const START_GUARD: &str = "101";
    const MIDDLE_GUARD: &str = "01010";
    const END_GUARD: &str = "101";
    const LEFT_ODD: [&str; 10] = [
        "0001101", "0011001", "0010011", "0111101", "0100011", "0110001", "0101111", "0111011",
        "0110111", "0001011",
    ];
    const LEFT_EVEN: [&str; 10] = [
        "0100111", "0110011", "0011011", "0100001", "0011101", "0111001", "0000101", "0010001",
        "0001001", "0010111",
    ];
    const RIGHT: [&str; 10] = [
        "1110010", "1100110", "1101100", "1000010", "1011100", "1001110", "1010000", "1000100",
        "1001000", "1110100",
    ];
    const PARITY: [&str; 10] = [
        "LLLLLL", "LLGLGG", "LLGGLG", "LLGGGL", "LGLLGG", "LGGLLG", "LGGGLL", "LGLGLG", "LGLGGL",
        "LGGLGL",
    ];

    assert_eq!(digits.len(), 13, "EAN-13 fixture must contain 13 digits");
    let digits = digits
        .chars()
        .map(|digit| digit.to_digit(10).expect("EAN-13 fixture must be numeric") as usize)
        .collect::<Vec<_>>();

    let mut bits = String::from(QUIET_ZONE);
    bits.push_str(START_GUARD);
    let parity = PARITY[digits[0]].as_bytes();
    for (index, digit) in digits[1..7].iter().enumerate() {
        let pattern = if parity[index] == b'L' {
            LEFT_ODD[*digit]
        } else {
            LEFT_EVEN[*digit]
        };
        bits.push_str(pattern);
    }
    bits.push_str(MIDDLE_GUARD);
    for digit in &digits[7..13] {
        bits.push_str(RIGHT[*digit]);
    }
    bits.push_str(END_GUARD);
    bits.push_str(QUIET_ZONE);

    let width = bits.len() as u32 * MODULE_WIDTH;
    let height = BAR_HEIGHT + TOP_MARGIN * 2;
    let mut image = image::GrayImage::from_pixel(width, height, image::Luma([255]));
    for (index, bit) in bits.bytes().enumerate() {
        if bit != b'1' {
            continue;
        }
        let start_x = index as u32 * MODULE_WIDTH;
        for x in start_x..start_x + MODULE_WIDTH {
            for y in TOP_MARGIN..TOP_MARGIN + BAR_HEIGHT {
                image.put_pixel(x, y, image::Luma([0]));
            }
        }
    }

    let mut output = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageLuma8(image)
        .write_to(&mut output, image::ImageFormat::Png)
        .expect("EAN-13 PNG fixture should encode");
    output.into_inner()
}

#[tokio::test]
async fn runtime_refresh_book_metadata_applies_epub_provider_patch_when_title_capability_matches() {
    let paths =
        new_router_fixture("runtime-refresh-book-metadata-applies-epub-provider-patch").await;
    seed_router_contract_data(&paths).await;

    write_router_epub_with_package_document(
        &paths,
        "books/book-1.epub",
        r##"<?xml version="1.0" encoding="UTF-8"?>
        <package version="3.0" xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf" unique-identifier="bookid">
          <metadata>
            <dc:identifier id="bookid">isbn:9780306406157</dc:identifier>
            <dc:title>EPUB Refresh Title</dc:title>
            <dc:description><![CDATA[<p>EPUB <b>Summary</b></p>]]></dc:description>
            <dc:date>2025-04-06T10:11:12Z</dc:date>
            <dc:creator id="creator-1">Alice Author</dc:creator>
            <meta refines="#creator-1" property="role" scheme="marc:relators">aut</meta>
            <dc:creator opf:role="trl">Bob Translator</dc:creator>
            <meta property="belongs-to-collection" id="series-collection">Series Collection</meta>
            <meta refines="#series-collection" property="group-position">4</meta>
          </metadata>
          <manifest>
            <item id="main" href="chapter.xhtml" media-type="application/xhtml+xml"/>
          </manifest>
          <spine>
            <itemref idref="main"/>
          </spine>
        </package>"##,
    );

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for EPUB metadata fixture setup");
    sqlx::query("DELETE FROM SIDECAR WHERE PARENT_URL = ?")
        .bind("books/book-1.epub")
        .execute(&pool)
        .await
        .expect("book sidecars should be cleared before EPUB metadata refresh test");
    sqlx::query("UPDATE LIBRARY SET IMPORT_COMICINFO_BOOK = 0, IMPORT_EPUB_BOOK = 1 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library metadata import flags should isolate EPUB provider for refresh test");
    sqlx::query(
        "UPDATE BOOK_METADATA SET RELEASE_DATE = ?, RELEASE_DATE_LOCK = 1, ISBN = ?, ISBN_LOCK = 0, NUMBER = ?, NUMBER_SORT = ? WHERE BOOK_ID = ?",
    )
    .bind("2024-01-15")
    .bind("")
    .bind("1")
    .bind(1.0_f64)
    .bind("book-1")
    .execute(&pool)
    .await
    .expect("book metadata locks should be updated before EPUB metadata refresh test");
    sqlx::query("DELETE FROM BOOK_METADATA_AUTHOR WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("existing metadata authors should be cleared before EPUB metadata refresh test");
    pool.close().await;

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for EPUB metadata task setup");
    sqlx::query(
        "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) VALUES (?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind("REFRESH_BOOK_METADATA_book-1")
    .bind(80_i64)
    .bind("series-1")
    .bind("org.gotson.komga.application.tasks.Task$RefreshBookMetadata")
    .bind("RefreshBookMetadata")
    .bind(
        json!({
            "bookId": "book-1",
            "capabilities": ["TITLE"],
            "priority": 80,
            "groupId": "series-1",
            "uniqueId": "REFRESH_BOOK_METADATA_book-1"
        })
        .to_string(),
    )
    .execute(&tasks_pool)
    .await
    .expect("EPUB metadata task row should be inserted");
    tasks_pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler
        .process_available(&runtime)
        .expect("runtime should process EPUB RefreshBookMetadata tasks successfully");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for EPUB metadata verification");
    let metadata = sqlx::query(
        "SELECT TITLE, SUMMARY, NUMBER, NUMBER_SORT, RELEASE_DATE, ISBN FROM BOOK_METADATA WHERE BOOK_ID = ? LIMIT 1",
    )
    .bind("book-1")
    .fetch_one(&verify_pool)
    .await
    .expect("book metadata row should be queryable after EPUB metadata refresh");
    let authors = sqlx::query(
        "SELECT NAME, ROLE FROM BOOK_METADATA_AUTHOR WHERE BOOK_ID = ? ORDER BY ROLE ASC, NAME ASC",
    )
    .bind("book-1")
    .fetch_all(&verify_pool)
    .await
    .expect("book metadata authors should be queryable after EPUB metadata refresh");
    verify_pool.close().await;

    assert_eq!(metadata.get::<String, _>("TITLE"), "EPUB Refresh Title");
    assert_eq!(metadata.get::<String, _>("SUMMARY"), "EPUB Summary");
    assert_eq!(metadata.get::<String, _>("NUMBER"), "4");
    assert_eq!(metadata.get::<f64, _>("NUMBER_SORT"), 4.0_f64);
    assert_eq!(
        metadata.get::<String, _>("RELEASE_DATE"),
        "2024-01-15",
        "EPUB provider refresh must still respect existing releaseDate locks",
    );
    assert_eq!(metadata.get::<String, _>("ISBN"), "9780306406157");
    assert_eq!(
        authors
            .iter()
            .map(|row| (row.get::<String, _>("NAME"), row.get::<String, _>("ROLE")))
            .collect::<Vec<_>>(),
        vec![
            ("Bob Translator".to_string(), "translator".to_string()),
            ("Alice Author".to_string(), "writer".to_string()),
        ],
        "EPUB provider should map OPF creator roles and replace authors when provider capabilities match",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_refresh_series_metadata_applies_epub_from_book_provider_patch() {
    let paths = new_router_fixture("runtime-refresh-series-metadata-applies-epub-provider").await;
    seed_router_contract_data(&paths).await;

    write_router_epub_with_package_document(
        &paths,
        "books/book-1.epub",
        r##"<?xml version="1.0" encoding="UTF-8"?>
        <package version="3.0" xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/" unique-identifier="bookid">
          <metadata>
            <dc:identifier id="bookid">book-1</dc:identifier>
            <dc:title>EPUB Series Metadata Fixture</dc:title>
            <dc:publisher>EPUB Provider House</dc:publisher>
            <dc:language>EN-us</dc:language>
            <dc:subject>Adventure</dc:subject>
            <dc:subject>Mystery</dc:subject>
            <meta property="belongs-to-collection">EPUB Provider Series</meta>
          </metadata>
          <manifest>
            <item id="main" href="chapter.xhtml" media-type="application/xhtml+xml"/>
          </manifest>
          <spine page-progression-direction="rtl">
            <itemref idref="main"/>
          </spine>
        </package>"##,
    );

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for EPUB series metadata fixture setup");
    sqlx::query(
        "UPDATE LIBRARY SET IMPORT_COMICINFO_SERIES = 0, IMPORT_COMICINFO_COLLECTION = 0, IMPORT_EPUB_SERIES = 1 WHERE ID = ?",
    )
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("library flags should isolate EPUB series metadata provider");
    sqlx::query(
        "UPDATE SERIES_METADATA SET TITLE = ?, TITLE_LOCK = 0, TITLE_SORT = ?, TITLE_SORT_LOCK = 0, READING_DIRECTION = NULL, READING_DIRECTION_LOCK = 0, PUBLISHER = ?, PUBLISHER_LOCK = 0, LANGUAGE = ?, LANGUAGE_LOCK = 0, GENRES_LOCK = 0 WHERE SERIES_ID = ?",
    )
    .bind("Stale EPUB Series")
    .bind("Stale EPUB Series")
    .bind("")
    .bind("")
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("series metadata should be reset before EPUB provider refresh test");
    sqlx::query("DELETE FROM SERIES_METADATA_GENRE WHERE SERIES_ID = ?")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("existing series genres should be cleared before EPUB provider refresh test");
    pool.close().await;

    let runtime = TaskRuntimeContext {
        owns_search_index: false,
        ..runtime_task_context(&paths)
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "REFRESH_SERIES_METADATA:series-1",
        1_000,
        Some("series-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("EPUB series metadata refresh task should process successfully");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for EPUB series metadata verification");
    let metadata = sqlx::query(
        "SELECT TITLE, TITLE_SORT, READING_DIRECTION, PUBLISHER, LANGUAGE FROM SERIES_METADATA WHERE SERIES_ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("series metadata row should be queryable after EPUB provider refresh");
    let genres = sqlx::query(
        "SELECT GENRE FROM SERIES_METADATA_GENRE WHERE SERIES_ID = ? ORDER BY GENRE COLLATE NOCASE ASC",
    )
    .bind("series-1")
    .fetch_all(&verify_pool)
    .await
    .expect("series genres should be queryable after EPUB provider refresh");
    verify_pool.close().await;

    assert_eq!(metadata.get::<String, _>("TITLE"), "EPUB Provider Series");
    assert_eq!(
        metadata.get::<String, _>("TITLE_SORT"),
        "EPUB Provider Series"
    );
    assert_eq!(
        metadata.get::<Option<String>, _>("READING_DIRECTION"),
        Some("RIGHT_TO_LEFT".to_string())
    );
    assert_eq!(
        metadata.get::<String, _>("PUBLISHER"),
        "EPUB Provider House"
    );
    assert_eq!(metadata.get::<String, _>("LANGUAGE"), "en-US");
    assert_eq!(
        genres
            .into_iter()
            .map(|row| row.get::<String, _>("GENRE"))
            .collect::<Vec<_>>(),
        vec!["Adventure".to_string(), "Mystery".to_string()],
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_refresh_series_metadata_ignores_non_iso_language_tags_from_book_providers() {
    let paths =
        new_router_fixture("runtime-refresh-series-metadata-ignores-non-iso-language").await;
    seed_router_contract_data(&paths).await;

    write_router_epub_with_package_document(
        &paths,
        "books/book-1.epub",
        r##"<?xml version="1.0" encoding="UTF-8"?>
        <package version="3.0" xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/" unique-identifier="bookid">
          <metadata>
            <dc:identifier id="bookid">book-1</dc:identifier>
            <dc:title>EPUB Invalid Language Fixture</dc:title>
            <dc:publisher>EPUB Invalid Language House</dc:publisher>
            <dc:language>zz-YY</dc:language>
            <meta property="belongs-to-collection">EPUB Invalid Language Series</meta>
          </metadata>
          <manifest>
            <item id="main" href="chapter.xhtml" media-type="application/xhtml+xml"/>
          </manifest>
          <spine>
            <itemref idref="main"/>
          </spine>
        </package>"##,
    );

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for invalid EPUB language series metadata fixture setup");
    sqlx::query(
        "UPDATE LIBRARY SET IMPORT_COMICINFO_SERIES = 0, IMPORT_COMICINFO_COLLECTION = 0, IMPORT_EPUB_SERIES = 1 WHERE ID = ?",
    )
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("library flags should isolate EPUB provider for invalid language test");
    sqlx::query(
        "UPDATE SERIES_METADATA SET TITLE = ?, TITLE_LOCK = 0, TITLE_SORT = ?, TITLE_SORT_LOCK = 0, PUBLISHER = ?, PUBLISHER_LOCK = 0, LANGUAGE = ?, LANGUAGE_LOCK = 0 WHERE SERIES_ID = ?",
    )
    .bind("Baseline Invalid Language Title")
    .bind("Baseline Invalid Language Title")
    .bind("")
    .bind("en-US")
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("series metadata should be reset before invalid language refresh test");
    pool.close().await;

    let runtime = TaskRuntimeContext {
        owns_search_index: false,
        ..runtime_task_context(&paths)
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "REFRESH_SERIES_METADATA:series-1",
        1_000,
        Some("series-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("invalid language series metadata refresh task should process successfully");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for invalid language series metadata verification");
    let metadata = sqlx::query(
        "SELECT TITLE, TITLE_SORT, PUBLISHER, LANGUAGE FROM SERIES_METADATA WHERE SERIES_ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("series metadata row should be queryable after invalid language refresh");
    verify_pool.close().await;

    assert_eq!(
        metadata.get::<String, _>("TITLE"),
        "EPUB Invalid Language Series"
    );
    assert_eq!(
        metadata.get::<String, _>("TITLE_SORT"),
        "EPUB Invalid Language Series"
    );
    assert_eq!(
        metadata.get::<String, _>("PUBLISHER"),
        "EPUB Invalid Language House"
    );
    assert_eq!(
        metadata.get::<String, _>("LANGUAGE"),
        "en-US",
        "non-ISO language tags should be ignored to match Kotlin BCP47TagValidator semantics",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_refresh_series_metadata_ignores_generic_series_xml_sidecar_without_matching_provider()
 {
    let paths =
        new_router_fixture("runtime-refresh-series-metadata-ignores-generic-series-xml").await;
    seed_router_contract_data(&paths).await;

    let series_sidecar_path = paths.config_dir.join("series/series-1.xml");
    if let Some(parent) = series_sidecar_path.parent() {
        std::fs::create_dir_all(parent).expect("series sidecar parent directory should be created");
    }
    std::fs::write(
        &series_sidecar_path,
        br#"<ComicInfo><Title>Unexpected Series Sidecar Title</Title><Summary>Unexpected Series Sidecar Summary</Summary></ComicInfo>"#,
    )
    .expect("series sidecar fixture should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for generic series sidecar fixture setup");
    sqlx::query(
        "INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)",
    )
    .bind("series/series-1.xml")
    .bind("series/series-1")
    .bind(1_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("generic series sidecar row should be inserted");
    sqlx::query(
        "UPDATE LIBRARY SET IMPORT_COMICINFO_SERIES = 0, IMPORT_COMICINFO_COLLECTION = 0, IMPORT_EPUB_SERIES = 0, IMPORT_MYLAR_SERIES = 0 WHERE ID = ?",
    )
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("library flags should disable matching series metadata providers");
    sqlx::query(
        "UPDATE SERIES_METADATA SET TITLE = ?, TITLE_LOCK = 0, TITLE_SORT = ?, TITLE_SORT_LOCK = 0, SUMMARY = ?, SUMMARY_LOCK = 0 WHERE SERIES_ID = ?",
    )
    .bind("Series Baseline Title")
    .bind("Series Baseline Title")
    .bind("Series Baseline Summary")
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("series metadata should be reset before generic sidecar refresh test");
    pool.close().await;

    let runtime = TaskRuntimeContext {
        owns_search_index: false,
        ..runtime_task_context(&paths)
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "REFRESH_SERIES_METADATA:series-1",
        1_000,
        Some("series-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("generic series sidecar refresh task should process successfully");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for generic series sidecar verification");
    let metadata = sqlx::query(
        "SELECT TITLE, TITLE_SORT, SUMMARY FROM SERIES_METADATA WHERE SERIES_ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("series metadata row should be queryable after generic sidecar refresh");
    verify_pool.close().await;

    assert_eq!(metadata.get::<String, _>("TITLE"), "Series Baseline Title");
    assert_eq!(
        metadata.get::<String, _>("TITLE_SORT"),
        "Series Baseline Title"
    );
    assert_eq!(
        metadata.get::<String, _>("SUMMARY"),
        "Series Baseline Summary"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_refresh_series_metadata_applies_comicinfo_from_book_provider_and_collection_side_effects()
 {
    let paths =
        new_router_fixture("runtime-refresh-series-metadata-applies-comicinfo-provider").await;
    seed_router_contract_data(&paths).await;

    write_router_epub_with_package_document_and_entries(
        &paths,
        "books/book-1.epub",
        r##"<?xml version="1.0" encoding="UTF-8"?>
        <package version="3.0" xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/" unique-identifier="bookid">
          <metadata>
            <dc:identifier id="bookid">book-1</dc:identifier>
            <dc:title>ComicInfo Series Metadata Fixture</dc:title>
            <dc:language>en</dc:language>
          </metadata>
          <manifest>
            <item id="main" href="chapter.xhtml" media-type="application/xhtml+xml"/>
          </manifest>
          <spine>
            <itemref idref="main"/>
          </spine>
        </package>"##,
        &[(
            "ComicInfo.xml",
            br#"<ComicInfo><Series>ComicInfo Series</Series><Volume>2</Volume><Count>9</Count><Publisher>ComicInfo House</Publisher><LanguageISO>EN-us</LanguageISO><Genre>Drama, Action, Drama</Genre><Manga>YesAndRightToLeft</Manga><AgeRating>MA 15+</AgeRating><SeriesGroup>Collection 1, New Refresh Collection</SeriesGroup></ComicInfo>"#,
        )],
    );

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for ComicInfo series metadata fixture setup");
    sqlx::query(
        "UPDATE LIBRARY SET IMPORT_COMICINFO_SERIES = 1, IMPORT_COMICINFO_COLLECTION = 1, IMPORT_COMICINFO_SERIES_APPEND_VOLUME = 1, IMPORT_EPUB_SERIES = 0 WHERE ID = ?",
    )
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("library flags should isolate ComicInfo series metadata provider");
    sqlx::query(
        "UPDATE SERIES_METADATA SET TITLE = ?, TITLE_LOCK = 0, TITLE_SORT = ?, TITLE_SORT_LOCK = 0, READING_DIRECTION = NULL, READING_DIRECTION_LOCK = 0, PUBLISHER = ?, PUBLISHER_LOCK = 0, AGE_RATING = NULL, AGE_RATING_LOCK = 0, LANGUAGE = ?, LANGUAGE_LOCK = 0, TOTAL_BOOK_COUNT = NULL, TOTAL_BOOK_COUNT_LOCK = 0, GENRES_LOCK = 0 WHERE SERIES_ID = ?",
    )
    .bind("Stale ComicInfo Series")
    .bind("Stale ComicInfo Series")
    .bind("")
    .bind("")
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("series metadata should be reset before ComicInfo provider refresh test");
    sqlx::query("DELETE FROM SERIES_METADATA_GENRE WHERE SERIES_ID = ?")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("existing series genres should be cleared before ComicInfo provider refresh test");
    sqlx::query("INSERT INTO SERIES_METADATA_GENRE (SERIES_ID, GENRE) VALUES (?, ?)")
        .bind("series-1")
        .bind("Action")
        .execute(&pool)
        .await
        .expect("baseline Action genre should be inserted before ComicInfo provider refresh test");
    sqlx::query("INSERT INTO SERIES_METADATA_GENRE (SERIES_ID, GENRE) VALUES (?, ?)")
        .bind("series-1")
        .bind("Drama")
        .execute(&pool)
        .await
        .expect("baseline Drama genre should be inserted before ComicInfo provider refresh test");
    sqlx::query("DELETE FROM COLLECTION_SERIES WHERE COLLECTION_ID = ? AND SERIES_ID <> ?")
        .bind("collection-1")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("existing collection memberships should be normalized before ComicInfo provider refresh test");
    pool.close().await;

    let runtime = TaskRuntimeContext {
        owns_search_index: false,
        ..runtime_task_context(&paths)
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "REFRESH_SERIES_METADATA:series-1",
        1_000,
        Some("series-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("ComicInfo series metadata refresh task should process successfully");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for ComicInfo series metadata verification");
    let metadata = sqlx::query(
        "SELECT TITLE, TITLE_SORT, READING_DIRECTION, PUBLISHER, AGE_RATING, LANGUAGE, TOTAL_BOOK_COUNT FROM SERIES_METADATA WHERE SERIES_ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("series metadata row should be queryable after ComicInfo provider refresh");
    let genres = sqlx::query(
        "SELECT GENRE FROM SERIES_METADATA_GENRE WHERE SERIES_ID = ? ORDER BY ROWID ASC",
    )
    .bind("series-1")
    .fetch_all(&verify_pool)
    .await
    .expect("series genres should be queryable after ComicInfo provider refresh");
    let new_collection = sqlx::query(
        "SELECT ID, SERIES_COUNT FROM COLLECTION WHERE NAME = ? COLLATE NOCASE LIMIT 1",
    )
    .bind("New Refresh Collection")
    .fetch_one(&verify_pool)
    .await
    .expect("new ComicInfo collection should be created");
    let new_collection_members = sqlx::query(
        "SELECT SERIES_ID FROM COLLECTION_SERIES WHERE COLLECTION_ID = ? ORDER BY NUMBER ASC",
    )
    .bind(new_collection.get::<String, _>("ID"))
    .fetch_all(&verify_pool)
    .await
    .expect("new ComicInfo collection membership should be queryable");
    let existing_membership_count = sqlx::query(
        "SELECT COUNT(*) AS COUNT FROM COLLECTION_SERIES WHERE COLLECTION_ID = ? AND SERIES_ID = ?",
    )
    .bind("collection-1")
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("existing collection membership should be queryable")
    .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(metadata.get::<String, _>("TITLE"), "ComicInfo Series (2)");
    assert_eq!(
        metadata.get::<String, _>("TITLE_SORT"),
        "ComicInfo Series (2)"
    );
    assert_eq!(
        metadata.get::<Option<String>, _>("READING_DIRECTION"),
        Some("RIGHT_TO_LEFT".to_string())
    );
    assert_eq!(metadata.get::<String, _>("PUBLISHER"), "ComicInfo House");
    assert_eq!(metadata.get::<Option<i64>, _>("AGE_RATING"), Some(15_i64));
    assert_eq!(metadata.get::<String, _>("LANGUAGE"), "en-US");
    assert_eq!(
        metadata.get::<Option<i64>, _>("TOTAL_BOOK_COUNT"),
        Some(9_i64)
    );
    assert_eq!(
        genres
            .into_iter()
            .map(|row| row.get::<String, _>("GENRE"))
            .collect::<Vec<_>>(),
        vec!["Action".to_string(), "Drama".to_string()],
    );
    assert_eq!(new_collection.get::<i64, _>("SERIES_COUNT"), 1_i64);
    assert_eq!(
        new_collection_members
            .into_iter()
            .map(|row| row.get::<String, _>("SERIES_ID"))
            .collect::<Vec<_>>(),
        vec!["series-1".to_string()],
    );
    assert_eq!(existing_membership_count, 1_i64);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_refresh_series_metadata_applies_mylar_series_provider() {
    let paths = new_router_fixture("runtime-refresh-series-metadata-applies-mylar-provider").await;
    seed_router_contract_data(&paths).await;

    let series_json_path = paths.config_dir.join("series/series-1/series.json");
    if let Some(parent) = series_json_path.parent() {
        std::fs::create_dir_all(parent)
            .expect("mylar series sidecar parent directory should exist");
    }
    std::fs::write(
        &series_json_path,
        r#"{
  "metadata": {
    "type": "comicSeries",
    "publisher": "Mylar House",
    "imprint": null,
    "name": "Mylar Saga",
    "cid": "123",
    "year": 2005,
    "description_text": "Plain summary",
    "description_formatted": "Formatted summary",
    "volume": 2,
    "booktype": "Print",
    "age_rating": "17+",
    "comic_image": "cover.jpg",
    "total_issues": 13,
    "publication_run": "2005-present",
    "status": "Continuing"
  }
}"#,
    )
    .expect("mylar series sidecar fixture should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for Mylar series metadata fixture setup");
    sqlx::query(
        "UPDATE LIBRARY SET IMPORT_COMICINFO_SERIES = 0, IMPORT_COMICINFO_COLLECTION = 0, IMPORT_EPUB_SERIES = 0, IMPORT_MYLAR_SERIES = 1 WHERE ID = ?",
    )
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("library flags should isolate Mylar series metadata provider");
    sqlx::query(
        "UPDATE SERIES_METADATA SET TITLE = ?, TITLE_LOCK = 0, TITLE_SORT = ?, TITLE_SORT_LOCK = 0, STATUS = ?, STATUS_LOCK = 0, SUMMARY = ?, SUMMARY_LOCK = 0, PUBLISHER = ?, PUBLISHER_LOCK = 0, AGE_RATING = NULL, AGE_RATING_LOCK = 0, TOTAL_BOOK_COUNT = NULL, TOTAL_BOOK_COUNT_LOCK = 0 WHERE SERIES_ID = ?",
    )
    .bind("Stale Mylar Title")
    .bind("Stale Mylar Title")
    .bind("ENDED")
    .bind("Stale Mylar Summary")
    .bind("")
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("series metadata should be reset before Mylar refresh test");
    pool.close().await;

    let runtime = TaskRuntimeContext {
        owns_search_index: false,
        ..runtime_task_context(&paths)
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "REFRESH_SERIES_METADATA:series-1",
        1_000,
        Some("series-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("Mylar series metadata refresh task should process successfully");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for Mylar series metadata verification");
    let metadata = sqlx::query(
        "SELECT TITLE, TITLE_SORT, STATUS, SUMMARY, PUBLISHER, AGE_RATING, TOTAL_BOOK_COUNT FROM SERIES_METADATA WHERE SERIES_ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("series metadata row should be queryable after Mylar refresh");
    verify_pool.close().await;

    assert_eq!(metadata.get::<String, _>("TITLE"), "Mylar Saga (2005)");
    assert_eq!(metadata.get::<String, _>("TITLE_SORT"), "Mylar Saga (2005)");
    assert_eq!(metadata.get::<String, _>("STATUS"), "ONGOING");
    assert_eq!(metadata.get::<String, _>("SUMMARY"), "Formatted summary");
    assert_eq!(metadata.get::<String, _>("PUBLISHER"), "Mylar House");
    assert_eq!(metadata.get::<Option<i64>, _>("AGE_RATING"), Some(17_i64));
    assert_eq!(
        metadata.get::<Option<i64>, _>("TOTAL_BOOK_COUNT"),
        Some(13_i64)
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_refresh_series_metadata_ignores_mylar_series_json_when_library_gate_is_disabled() {
    let paths =
        new_router_fixture("runtime-refresh-series-metadata-ignores-mylar-when-disabled").await;
    seed_router_contract_data(&paths).await;

    let series_json_path = paths.config_dir.join("series/series-1/series.json");
    if let Some(parent) = series_json_path.parent() {
        std::fs::create_dir_all(parent)
            .expect("mylar series sidecar parent directory should exist");
    }
    std::fs::write(
        &series_json_path,
        r#"{
  "metadata": {
    "type": "comicSeries",
    "publisher": "Blocked Mylar House",
    "imprint": null,
    "name": "Blocked Mylar Saga",
    "cid": "456",
    "year": 2010,
    "description_text": "Blocked summary",
    "description_formatted": "Blocked formatted summary",
    "volume": 2,
    "booktype": "Print",
    "age_rating": "Adult",
    "comic_image": "cover.jpg",
    "total_issues": 22,
    "publication_run": "2010-present",
    "status": "Ended"
  }
}"#,
    )
    .expect("disabled Mylar series sidecar fixture should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for disabled Mylar fixture setup");
    sqlx::query(
        "UPDATE LIBRARY SET IMPORT_COMICINFO_SERIES = 0, IMPORT_COMICINFO_COLLECTION = 0, IMPORT_EPUB_SERIES = 0, IMPORT_MYLAR_SERIES = 0 WHERE ID = ?",
    )
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("library flags should disable Mylar series metadata provider");
    sqlx::query(
        "UPDATE SERIES_METADATA SET TITLE = ?, TITLE_LOCK = 0, TITLE_SORT = ?, TITLE_SORT_LOCK = 0, STATUS = ?, STATUS_LOCK = 0, SUMMARY = ?, SUMMARY_LOCK = 0, PUBLISHER = ?, PUBLISHER_LOCK = 0, AGE_RATING = ?, AGE_RATING_LOCK = 0, TOTAL_BOOK_COUNT = ?, TOTAL_BOOK_COUNT_LOCK = 0 WHERE SERIES_ID = ?",
    )
    .bind("Baseline Mylar Title")
    .bind("Baseline Mylar Title")
    .bind("ONGOING")
    .bind("Baseline Mylar Summary")
    .bind("Baseline Mylar Publisher")
    .bind(9_i64)
    .bind(5_i64)
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("series metadata should be reset before disabled Mylar refresh test");
    pool.close().await;

    let runtime = TaskRuntimeContext {
        owns_search_index: false,
        ..runtime_task_context(&paths)
    };
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "REFRESH_SERIES_METADATA:series-1",
        1_000,
        Some("series-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("disabled Mylar series metadata refresh task should process successfully");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for disabled Mylar verification");
    let metadata = sqlx::query(
        "SELECT TITLE, TITLE_SORT, STATUS, SUMMARY, PUBLISHER, AGE_RATING, TOTAL_BOOK_COUNT FROM SERIES_METADATA WHERE SERIES_ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("series metadata row should be queryable after disabled Mylar refresh");
    verify_pool.close().await;

    assert_eq!(metadata.get::<String, _>("TITLE"), "Baseline Mylar Title");
    assert_eq!(
        metadata.get::<String, _>("TITLE_SORT"),
        "Baseline Mylar Title"
    );
    assert_eq!(metadata.get::<String, _>("STATUS"), "ONGOING");
    assert_eq!(
        metadata.get::<String, _>("SUMMARY"),
        "Baseline Mylar Summary"
    );
    assert_eq!(
        metadata.get::<String, _>("PUBLISHER"),
        "Baseline Mylar Publisher"
    );
    assert_eq!(metadata.get::<Option<i64>, _>("AGE_RATING"), Some(9_i64));
    assert_eq!(
        metadata.get::<Option<i64>, _>("TOTAL_BOOK_COUNT"),
        Some(5_i64)
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_refresh_book_metadata_applies_barcode_isbn_for_non_epub_books() {
    let paths = new_router_fixture("runtime-refresh-book-metadata-applies-barcode-isbn").await;
    seed_router_contract_data(&paths).await;
    seed_router_cbz_book(
        &paths,
        "book-barcode-1",
        "series-1",
        "barcode-book.cbz",
        "Barcode Book",
    )
    .await;
    write_router_cbz_with_single_page(
        &paths,
        "books/barcode-book.cbz",
        "page-1.png",
        &render_ean13_png_bytes("9780306406157"),
    );

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for barcode metadata fixture setup");
    sqlx::query(
        "UPDATE LIBRARY SET IMPORT_COMICINFO_BOOK = 0, IMPORT_EPUB_BOOK = 0, IMPORT_BARCODE_ISBN = 1 WHERE ID = ?",
    )
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("library metadata import flags should isolate barcode provider for refresh test");
    sqlx::query("UPDATE BOOK_METADATA SET ISBN = ?, ISBN_LOCK = 0 WHERE BOOK_ID = ?")
        .bind("")
        .bind("book-barcode-1")
        .execute(&pool)
        .await
        .expect("book metadata isbn should be reset before barcode refresh test");
    pool.close().await;

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for barcode metadata task setup");
    sqlx::query(
        "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) VALUES (?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind("REFRESH_BOOK_METADATA_book-barcode-1")
    .bind(80_i64)
    .bind("series-1")
    .bind("org.gotson.komga.application.tasks.Task$RefreshBookMetadata")
    .bind("RefreshBookMetadata")
    .bind(
        json!({
            "bookId": "book-barcode-1",
            "capabilities": ["ISBN"],
            "priority": 80,
            "groupId": "series-1",
            "uniqueId": "REFRESH_BOOK_METADATA_book-barcode-1"
        })
        .to_string(),
    )
    .execute(&tasks_pool)
    .await
    .expect("barcode metadata task row should be inserted");
    tasks_pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler
        .process_available(&runtime)
        .expect("runtime should process barcode RefreshBookMetadata tasks successfully");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for barcode metadata verification");
    let metadata = sqlx::query("SELECT ISBN FROM BOOK_METADATA WHERE BOOK_ID = ? LIMIT 1")
        .bind("book-barcode-1")
        .fetch_one(&verify_pool)
        .await
        .expect("book metadata row should be queryable after barcode metadata refresh");
    verify_pool.close().await;

    assert_eq!(metadata.get::<String, _>("ISBN"), "9780306406157");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_skips_book_local_artwork_refresh_when_library_import_local_artwork_is_disabled() {
    let paths = new_router_fixture("runtime-skip-book-local-artwork-when-import-disabled").await;
    seed_router_contract_data(&paths).await;

    let sidecar_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&sidecar_dir).expect("book artwork sidecar directory should exist");
    std::fs::write(sidecar_dir.join("book-1.png"), fixture_png_bytes())
        .expect("book artwork sidecar fixture should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for local artwork disabled fixture setup");
    sqlx::query("UPDATE LIBRARY SET IMPORT_LOCAL_ARTWORK = 0 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library import-local-artwork flag should be disabled");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("existing book thumbnails should be cleared before local artwork gating test");
    sqlx::query(
        "INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)",
    )
    .bind("books/book-1.png")
    .bind("books/book-1.epub")
    .bind(1_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("book artwork sidecar row should be inserted");
    pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "REFRESH_BOOK_LOCAL_ARTWORK:book-1",
        1_000,
        Some("book-1".to_string()),
    ));
    scheduler.process_available(&runtime).expect(
        "book local artwork refresh should skip cleanly when library.importLocalArtwork is disabled",
    );

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for local artwork disabled verification");
    let sidecar_thumbnail_count = sqlx::query(
        "SELECT COUNT(*) AS COUNT FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? AND TYPE = 'SIDECAR'",
    )
    .bind("book-1")
    .fetch_one(&verify_pool)
    .await
    .expect("sidecar thumbnail rows should be queryable")
    .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(
        sidecar_thumbnail_count, 0,
        "runtime must not import book local artwork when library.importLocalArtwork is disabled",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_executes_kotlin_persisted_refresh_book_local_artwork_task() {
    let paths = new_router_fixture("runtime-executes-kotlin-refresh-book-local-artwork-task").await;
    seed_router_contract_data(&paths).await;

    let sidecar_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&sidecar_dir).expect("book artwork sidecar directory should exist");
    std::fs::write(sidecar_dir.join("book-1.png"), fixture_png_bytes())
        .expect("book artwork sidecar fixture should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for Kotlin persisted local artwork fixture setup");
    sqlx::query("UPDATE LIBRARY SET IMPORT_LOCAL_ARTWORK = 1 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library import-local-artwork flag should be enabled");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("existing book thumbnails should be cleared before Kotlin persisted task test");
    sqlx::query(
        "INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)",
    )
    .bind("books/book-1.png")
    .bind("books/book-1.epub")
    .bind(1_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("book artwork sidecar row should be inserted for Kotlin persisted task test");
    pool.close().await;

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for Kotlin persisted local artwork task setup");
    sqlx::query(
        "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) VALUES (?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind("REFRESH_BOOK_LOCAL_ARTWORK_book-1")
    .bind(80_i64)
    .bind(Option::<String>::None)
    .bind("org.gotson.komga.application.tasks.Task$RefreshBookLocalArtwork")
    .bind("RefreshBookLocalArtwork")
    .bind(
        json!({
            "bookId": "book-1",
            "priority": 80,
            "groupId": Value::Null,
            "uniqueId": "REFRESH_BOOK_LOCAL_ARTWORK_book-1"
        })
        .to_string(),
    )
    .execute(&tasks_pool)
    .await
    .expect("Kotlin persisted local artwork task row should be inserted");
    tasks_pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.process_available(&runtime).expect(
        "runtime should execute Kotlin persisted RefreshBookLocalArtwork tasks successfully",
    );

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for Kotlin persisted local artwork verification");
    let row = sqlx::query(
        "SELECT TYPE, URL, SELECTED FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? ORDER BY ID ASC LIMIT 1",
    )
    .bind("book-1")
    .fetch_one(&verify_pool)
    .await
    .expect("sidecar thumbnail row should be queryable after Kotlin persisted task execution");
    verify_pool.close().await;

    assert_eq!(row.get::<String, _>("TYPE"), "SIDECAR");
    assert_eq!(row.get::<String, _>("URL"), "books/book-1.png");
    assert!(
        row.get::<bool, _>("SELECTED"),
        "executed Kotlin persisted local artwork task should import a selected SIDECAR thumbnail",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_imports_multiple_filesystem_book_local_artworks_and_selects_only_one_when_none_exists()
 {
    let paths =
        new_router_fixture("runtime-imports-multiple-filesystem-book-local-artworks-none-selected")
            .await;
    seed_router_contract_data(&paths).await;

    let sidecar_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&sidecar_dir).expect("book artwork directory should exist");
    std::fs::write(sidecar_dir.join("book-1.png"), fixture_png_bytes())
        .expect("primary local artwork should be written");
    std::fs::write(sidecar_dir.join("book-1-1.jpg"), fixture_png_bytes())
        .expect("secondary local artwork should be written");
    std::fs::write(sidecar_dir.join("book-12.png"), fixture_png_bytes())
        .expect("non-matching artwork should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for multi-artwork fixture setup");
    sqlx::query("UPDATE LIBRARY SET IMPORT_LOCAL_ARTWORK = 1 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library import-local-artwork flag should be enabled");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("existing thumbnails should be cleared for multi-artwork import test");
    pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(
        TaskQueueRecord::new("REFRESH_BOOK_LOCAL_ARTWORK_book-1", 1_000, None)
            .with_simple_type("REFRESH_BOOK_LOCAL_ARTWORK"),
    );
    scheduler
        .process_available(&runtime)
        .expect("book local artwork refresh should import multiple filesystem candidates cleanly");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for multi-artwork verification");
    let rows = sqlx::query(
        "SELECT TYPE, URL, SELECTED FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? ORDER BY URL ASC",
    )
    .bind("book-1")
    .fetch_all(&verify_pool)
    .await
    .expect("book local artwork rows should be queryable after filesystem import");
    verify_pool.close().await;

    let urls = rows
        .iter()
        .map(|row| row.get::<Option<String>, _>("URL"))
        .collect::<Vec<_>>();
    let selected_count = rows
        .iter()
        .filter(|row| row.get::<bool, _>("SELECTED"))
        .count();

    assert_eq!(
        rows.len(),
        2,
        "runtime should import every matching local artwork file"
    );
    assert_eq!(
        urls,
        vec![
            Some("books/book-1-1.jpg".to_string()),
            Some("books/book-1.png".to_string())
        ],
        "runtime should only import basename and basename-<n> local artwork candidates",
    );
    assert!(
        rows.iter()
            .all(|row| row.get::<String, _>("TYPE") == "SIDECAR"),
        "runtime should import filesystem local artwork files as SIDECAR thumbnails",
    );
    assert_eq!(
        selected_count, 1,
        "runtime should select exactly one imported local artwork when no thumbnail was previously selected",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_preserves_existing_non_generated_selection_when_importing_book_local_artworks() {
    let paths =
        new_router_fixture("runtime-preserves-non-generated-selection-for-book-local-artworks")
            .await;
    seed_router_contract_data(&paths).await;

    let sidecar_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&sidecar_dir).expect("book artwork directory should exist");
    std::fs::write(sidecar_dir.join("book-1.png"), fixture_png_bytes())
        .expect("primary local artwork should be written");
    std::fs::write(sidecar_dir.join("book-1-1.jpg"), fixture_png_bytes())
        .expect("secondary local artwork should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for non-generated selection fixture setup");
    sqlx::query("UPDATE LIBRARY SET IMPORT_LOCAL_ARTWORK = 1 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library import-local-artwork flag should be enabled");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? AND TYPE = 'SIDECAR'")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect(
            "existing sidecar thumbnails should be cleared before non-generated selection test",
        );
    pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(
        TaskQueueRecord::new("REFRESH_BOOK_LOCAL_ARTWORK_book-1", 1_000, None)
            .with_simple_type("REFRESH_BOOK_LOCAL_ARTWORK"),
    );
    scheduler.process_available(&runtime).expect(
        "book local artwork refresh should preserve existing non-generated selections cleanly",
    );

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for non-generated selection verification");
    let rows = sqlx::query(
        "SELECT ID, TYPE, URL, SELECTED FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? ORDER BY TYPE ASC, URL ASC, ID ASC",
    )
    .bind("book-1")
    .fetch_all(&verify_pool)
    .await
    .expect("book thumbnail rows should be queryable after preserving non-generated selection");
    verify_pool.close().await;

    let selected_rows = rows
        .iter()
        .filter(|row| row.get::<bool, _>("SELECTED"))
        .collect::<Vec<_>>();
    let imported_sidecars = rows
        .iter()
        .filter(|row| row.get::<String, _>("TYPE") == "SIDECAR")
        .collect::<Vec<_>>();

    assert_eq!(
        imported_sidecars.len(),
        2,
        "runtime should still import all matching local artworks"
    );
    assert_eq!(
        selected_rows.len(),
        1,
        "runtime should keep exactly one selected thumbnail"
    );
    assert_eq!(selected_rows[0].get::<String, _>("ID"), "thumb-book-1");
    assert_eq!(selected_rows[0].get::<String, _>("TYPE"), "USER_UPLOADED");
    assert!(
        imported_sidecars
            .iter()
            .all(|row| !row.get::<bool, _>("SELECTED")),
        "runtime should not override an existing non-generated selected thumbnail when importing local artworks",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_replaces_generated_selection_when_importing_book_local_artworks() {
    let paths =
        new_router_fixture("runtime-replaces-generated-selection-for-book-local-artworks").await;
    seed_router_contract_data(&paths).await;

    let sidecar_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&sidecar_dir).expect("book artwork directory should exist");
    std::fs::write(sidecar_dir.join("book-1.png"), fixture_png_bytes())
        .expect("primary local artwork should be written");
    std::fs::write(sidecar_dir.join("book-1-1.jpg"), fixture_png_bytes())
        .expect("secondary local artwork should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for generated selection fixture setup");
    sqlx::query("UPDATE LIBRARY SET IMPORT_LOCAL_ARTWORK = 1 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library import-local-artwork flag should be enabled");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("existing thumbnails should be cleared before generated selection test");
    sqlx::query(
        "INSERT INTO THUMBNAIL_BOOK (ID, BOOK_ID, TYPE, SELECTED, THUMBNAIL, MEDIA_TYPE) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("thumb-generated-book-1")
    .bind("book-1")
    .bind("GENERATED")
    .bind(true)
    .bind(fixture_png_bytes())
    .bind("image/png")
    .execute(&pool)
    .await
    .expect("generated selected thumbnail should be seeded for local artwork selection test");
    pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(
        TaskQueueRecord::new("REFRESH_BOOK_LOCAL_ARTWORK_book-1", 1_000, None)
            .with_simple_type("REFRESH_BOOK_LOCAL_ARTWORK"),
    );
    scheduler
        .process_available(&runtime)
        .expect("book local artwork refresh should replace generated selection cleanly");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for generated selection verification");
    let rows = sqlx::query(
        "SELECT ID, TYPE, URL, SELECTED FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? ORDER BY TYPE ASC, URL ASC, ID ASC",
    )
    .bind("book-1")
    .fetch_all(&verify_pool)
    .await
    .expect("book thumbnail rows should be queryable after generated selection replacement");
    verify_pool.close().await;

    let selected_rows = rows
        .iter()
        .filter(|row| row.get::<bool, _>("SELECTED"))
        .collect::<Vec<_>>();
    let generated_rows = rows
        .iter()
        .filter(|row| row.get::<String, _>("TYPE") == "GENERATED")
        .collect::<Vec<_>>();
    let imported_sidecars = rows
        .iter()
        .filter(|row| row.get::<String, _>("TYPE") == "SIDECAR")
        .collect::<Vec<_>>();

    assert_eq!(
        imported_sidecars.len(),
        2,
        "runtime should import all matching local artworks"
    );
    assert_eq!(
        generated_rows.len(),
        1,
        "runtime should retain the pre-existing generated thumbnail row"
    );
    assert_eq!(
        selected_rows.len(),
        1,
        "runtime should keep exactly one selected thumbnail after import"
    );
    assert_eq!(selected_rows[0].get::<String, _>("TYPE"), "SIDECAR");
    assert!(
        !generated_rows[0].get::<bool, _>("SELECTED"),
        "runtime should unselect previously selected GENERATED thumbnails when the first local artwork is imported",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_skips_series_local_artwork_refresh_when_library_import_local_artwork_is_disabled()
{
    let paths = new_router_fixture("runtime-skip-series-local-artwork-when-import-disabled").await;
    seed_router_contract_data(&paths).await;

    let series_dir = paths.config_dir.join("series/series-1");
    std::fs::create_dir_all(&series_dir).expect("series artwork directory should exist");
    std::fs::write(series_dir.join("cover.png"), fixture_png_bytes())
        .expect("series artwork fixture should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for series local artwork disabled fixture setup");
    sqlx::query("UPDATE LIBRARY SET IMPORT_LOCAL_ARTWORK = 0 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library import-local-artwork flag should be disabled");
    sqlx::query("DELETE FROM THUMBNAIL_SERIES WHERE SERIES_ID = ?")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("existing series thumbnails should be cleared before local artwork gating test");
    pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "REFRESH_SERIES_LOCAL_ARTWORK:series-1",
        1_000,
        Some("series-1".to_string()),
    ));
    scheduler.process_available(&runtime).expect(
        "series local artwork refresh should skip cleanly when library.importLocalArtwork is disabled",
    );

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for series local artwork disabled verification");
    let sidecar_thumbnail_count = sqlx::query(
        "SELECT COUNT(*) AS COUNT FROM THUMBNAIL_SERIES WHERE SERIES_ID = ? AND TYPE = 'SIDECAR'",
    )
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("series sidecar thumbnail rows should be queryable")
    .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(
        sidecar_thumbnail_count, 0,
        "runtime must not import series local artwork when library.importLocalArtwork is disabled",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_skips_series_local_artwork_refresh_for_oneshot_series() {
    let paths = new_router_fixture("runtime-skip-series-local-artwork-for-oneshot").await;
    seed_router_contract_data(&paths).await;

    let series_dir = paths.config_dir.join("series/series-1");
    std::fs::create_dir_all(&series_dir).expect("series artwork directory should exist");
    std::fs::write(series_dir.join("cover.png"), fixture_png_bytes())
        .expect("series artwork fixture should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for series oneshot artwork fixture setup");
    sqlx::query("UPDATE LIBRARY SET IMPORT_LOCAL_ARTWORK = 1 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library import-local-artwork flag should be enabled");
    sqlx::query("UPDATE SERIES SET ONESHOT = 1 WHERE ID = ?")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series oneshot flag should be enabled");
    sqlx::query("DELETE FROM THUMBNAIL_SERIES WHERE SERIES_ID = ?")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("existing series thumbnails should be cleared before oneshot skip test");
    pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "REFRESH_SERIES_LOCAL_ARTWORK:series-1",
        1_000,
        Some("series-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("series local artwork refresh should skip oneshot series cleanly");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for oneshot series local artwork verification");
    let sidecar_thumbnail_count = sqlx::query(
        "SELECT COUNT(*) AS COUNT FROM THUMBNAIL_SERIES WHERE SERIES_ID = ? AND TYPE = 'SIDECAR'",
    )
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("series sidecar thumbnail rows should be queryable after oneshot skip")
    .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(
        sidecar_thumbnail_count, 0,
        "runtime must not import series local artwork for oneshot series",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_imports_multiple_filesystem_series_local_artworks_and_selects_only_one_when_none_exists()
 {
    let paths = new_router_fixture(
        "runtime-imports-multiple-filesystem-series-local-artworks-none-selected",
    )
    .await;
    seed_router_contract_data(&paths).await;

    let series_dir = paths.config_dir.join("series/series-1");
    std::fs::create_dir_all(&series_dir).expect("series artwork directory should exist");
    std::fs::write(series_dir.join("cover.png"), fixture_png_bytes())
        .expect("primary series local artwork should be written");
    std::fs::write(series_dir.join("poster.jpg"), fixture_png_bytes())
        .expect("secondary series local artwork should be written");
    std::fs::write(series_dir.join("banner.png"), fixture_png_bytes())
        .expect("non-matching series local artwork should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for multi-series-artwork fixture setup");
    sqlx::query("UPDATE LIBRARY SET IMPORT_LOCAL_ARTWORK = 1 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library import-local-artwork flag should be enabled");
    sqlx::query("UPDATE SERIES SET ONESHOT = 0 WHERE ID = ?")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series oneshot flag should be disabled");
    sqlx::query("DELETE FROM THUMBNAIL_SERIES WHERE SERIES_ID = ?")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("existing series thumbnails should be cleared for multi-artwork import test");
    pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "REFRESH_SERIES_LOCAL_ARTWORK:series-1",
        1_000,
        Some("series-1".to_string()),
    ));
    scheduler.process_available(&runtime).expect(
        "series local artwork refresh should import multiple filesystem candidates cleanly",
    );

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for multi-series-artwork verification");
    let rows = sqlx::query(
        "SELECT TYPE, URL, SELECTED FROM THUMBNAIL_SERIES WHERE SERIES_ID = ? ORDER BY URL ASC",
    )
    .bind("series-1")
    .fetch_all(&verify_pool)
    .await
    .expect("series local artwork rows should be queryable after filesystem import");
    verify_pool.close().await;

    let urls = rows
        .iter()
        .map(|row| row.get::<Option<String>, _>("URL"))
        .collect::<Vec<_>>();
    let selected_count = rows
        .iter()
        .filter(|row| row.get::<bool, _>("SELECTED"))
        .count();

    assert_eq!(
        rows.len(),
        2,
        "runtime should import every matching series local artwork file"
    );
    assert_eq!(
        urls,
        vec![
            Some("series/series-1/cover.png".to_string()),
            Some("series/series-1/poster.jpg".to_string()),
        ],
        "runtime should only import Kotlin-supported series local artwork basenames",
    );
    assert!(
        rows.iter()
            .all(|row| row.get::<String, _>("TYPE") == "SIDECAR"),
        "runtime should import filesystem series local artwork files as SIDECAR thumbnails",
    );
    assert_eq!(
        selected_count, 1,
        "runtime should select exactly one imported series local artwork when no thumbnail was previously selected",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_preserves_existing_non_generated_selection_when_importing_series_local_artworks() {
    let paths =
        new_router_fixture("runtime-preserves-non-generated-selection-for-series-local-artworks")
            .await;
    seed_router_contract_data(&paths).await;

    let series_dir = paths.config_dir.join("series/series-1");
    std::fs::create_dir_all(&series_dir).expect("series artwork directory should exist");
    std::fs::write(series_dir.join("cover.png"), fixture_png_bytes())
        .expect("primary series local artwork should be written");
    std::fs::write(series_dir.join("poster.jpg"), fixture_png_bytes())
        .expect("secondary series local artwork should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for non-generated series selection fixture setup");
    sqlx::query("UPDATE LIBRARY SET IMPORT_LOCAL_ARTWORK = 1 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library import-local-artwork flag should be enabled");
    sqlx::query("DELETE FROM THUMBNAIL_SERIES WHERE SERIES_ID = ? AND TYPE = 'SIDECAR'")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("existing series sidecar thumbnails should be cleared before selection preservation test");
    sqlx::query(
        "INSERT INTO THUMBNAIL_SERIES (ID, SERIES_ID, TYPE, SELECTED, THUMBNAIL, MEDIA_TYPE) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("thumb-series-user-selected")
    .bind("series-1")
    .bind("USER_UPLOADED")
    .bind(true)
    .bind(fixture_png_bytes())
    .bind("image/png")
    .execute(&pool)
    .await
    .expect("existing non-generated selected series thumbnail should be seeded");
    pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "REFRESH_SERIES_LOCAL_ARTWORK:series-1",
        1_000,
        Some("series-1".to_string()),
    ));
    scheduler.process_available(&runtime).expect(
        "series local artwork refresh should preserve existing non-generated selections cleanly",
    );

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for non-generated series selection verification");
    let rows = sqlx::query(
        "SELECT ID, TYPE, URL, SELECTED FROM THUMBNAIL_SERIES WHERE SERIES_ID = ? ORDER BY TYPE ASC, URL ASC, ID ASC",
    )
    .bind("series-1")
    .fetch_all(&verify_pool)
    .await
    .expect("series thumbnail rows should be queryable after preserving non-generated selection");
    verify_pool.close().await;

    let selected_rows = rows
        .iter()
        .filter(|row| row.get::<bool, _>("SELECTED"))
        .collect::<Vec<_>>();
    let imported_sidecars = rows
        .iter()
        .filter(|row| row.get::<String, _>("TYPE") == "SIDECAR")
        .collect::<Vec<_>>();

    assert_eq!(
        imported_sidecars.len(),
        2,
        "runtime should still import all matching series local artworks"
    );
    assert_eq!(
        selected_rows.len(),
        1,
        "runtime should keep exactly one selected series thumbnail"
    );
    assert_eq!(
        selected_rows[0].get::<String, _>("ID"),
        "thumb-series-user-selected"
    );
    assert_eq!(selected_rows[0].get::<String, _>("TYPE"), "USER_UPLOADED");
    assert!(
        imported_sidecars
            .iter()
            .all(|row| !row.get::<bool, _>("SELECTED")),
        "runtime should not override an existing non-generated selected series thumbnail when importing local artworks",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_replaces_generated_selection_when_importing_series_local_artworks() {
    let paths =
        new_router_fixture("runtime-replaces-generated-selection-for-series-local-artworks").await;
    seed_router_contract_data(&paths).await;

    let series_dir = paths.config_dir.join("series/series-1");
    std::fs::create_dir_all(&series_dir).expect("series artwork directory should exist");
    std::fs::write(series_dir.join("cover.png"), fixture_png_bytes())
        .expect("primary series local artwork should be written");
    std::fs::write(series_dir.join("poster.jpg"), fixture_png_bytes())
        .expect("secondary series local artwork should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for generated series selection fixture setup");
    sqlx::query("UPDATE LIBRARY SET IMPORT_LOCAL_ARTWORK = 1 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library import-local-artwork flag should be enabled");
    sqlx::query("DELETE FROM THUMBNAIL_SERIES WHERE SERIES_ID = ?")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("existing series thumbnails should be cleared before generated selection test");
    sqlx::query(
        "INSERT INTO THUMBNAIL_SERIES (ID, SERIES_ID, TYPE, SELECTED, THUMBNAIL, MEDIA_TYPE) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("thumb-generated-series-1")
    .bind("series-1")
    .bind("GENERATED")
    .bind(true)
    .bind(fixture_png_bytes())
    .bind("image/png")
    .execute(&pool)
    .await
    .expect("generated selected series thumbnail should be seeded");
    pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "REFRESH_SERIES_LOCAL_ARTWORK:series-1",
        1_000,
        Some("series-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("series local artwork refresh should replace generated selection cleanly");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for generated series selection verification");
    let rows = sqlx::query(
        "SELECT ID, TYPE, URL, SELECTED FROM THUMBNAIL_SERIES WHERE SERIES_ID = ? ORDER BY TYPE ASC, URL ASC, ID ASC",
    )
    .bind("series-1")
    .fetch_all(&verify_pool)
    .await
    .expect("series thumbnail rows should be queryable after generated selection replacement");
    verify_pool.close().await;

    let selected_rows = rows
        .iter()
        .filter(|row| row.get::<bool, _>("SELECTED"))
        .collect::<Vec<_>>();
    let generated_rows = rows
        .iter()
        .filter(|row| row.get::<String, _>("TYPE") == "GENERATED")
        .collect::<Vec<_>>();
    let imported_sidecars = rows
        .iter()
        .filter(|row| row.get::<String, _>("TYPE") == "SIDECAR")
        .collect::<Vec<_>>();

    assert_eq!(
        imported_sidecars.len(),
        2,
        "runtime should import all matching series local artworks"
    );
    assert_eq!(
        generated_rows.len(),
        1,
        "runtime should retain the pre-existing generated series thumbnail row"
    );
    assert_eq!(
        selected_rows.len(),
        1,
        "runtime should keep exactly one selected series thumbnail after import"
    );
    assert_eq!(selected_rows[0].get::<String, _>("TYPE"), "SIDECAR");
    assert!(
        !generated_rows[0].get::<bool, _>("SELECTED"),
        "runtime should unselect previously selected GENERATED series thumbnails when the first local artwork is imported",
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
async fn runtime_empty_trash_resorts_remaining_books_in_affected_series() {
    let paths = new_router_fixture("runtime-empty-trash-resorts-affected-series").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for empty-trash sort fixture setup");
    for (book_id, name, url, file_size, number) in [
        (
            "book-2",
            "book-2.epub",
            "books/book-2.epub",
            2_048_i64,
            2_i64,
        ),
        (
            "book-3",
            "book-3.epub",
            "books/book-3.epub",
            3_072_i64,
            3_i64,
        ),
    ] {
        sqlx::query(
            "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(book_id)
        .bind(0_i64)
        .bind(name)
        .bind(url)
        .bind("series-1")
        .bind(file_size)
        .bind(number)
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("book row should be inserted for empty-trash sort fixture");

        sqlx::query(
            "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)",
        )
        .bind("application/epub+zip")
        .bind("READY")
        .bind(book_id)
        .bind(10_i64)
        .execute(&pool)
        .await
        .expect("media row should be inserted for empty-trash sort fixture");

        sqlx::query(
            "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, BOOK_ID) VALUES (?, ?, ?, ?)",
        )
        .bind(number.to_string())
        .bind(number as f64)
        .bind(format!("Book {}", number))
        .bind(book_id)
        .execute(&pool)
        .await
        .expect("book metadata row should be inserted for empty-trash sort fixture");
    }

    sqlx::query("UPDATE BOOK SET DELETED_DATE = CURRENT_TIMESTAMP WHERE ID = ?")
        .bind("book-2")
        .execute(&pool)
        .await
        .expect("trashed middle book should be marked deleted");
    pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "EMPTY_TRASH:library-1",
        1_000,
        Some("library-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("empty-trash cleanup should process successfully");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for empty-trash sort verification");
    let deleted_rows = sqlx::query("SELECT COUNT(*) AS COUNT FROM BOOK WHERE ID = ?")
        .bind("book-2")
        .fetch_one(&verify_pool)
        .await
        .expect("deleted middle book count should be queryable")
        .get::<i64, _>("COUNT");
    let remaining = sqlx::query(
        "SELECT b.ID AS ID, b.NUMBER AS BOOK_NUMBER, bm.NUMBER AS METADATA_NUMBER, \
         bm.NUMBER_SORT AS METADATA_NUMBER_SORT \
         FROM BOOK b \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         WHERE b.SERIES_ID = ? \
           AND b.DELETED_DATE IS NULL \
         ORDER BY b.NUMBER ASC",
    )
    .bind("series-1")
    .fetch_all(&verify_pool)
    .await
    .expect("remaining books after empty-trash should be queryable");
    verify_pool.close().await;

    assert_eq!(
        deleted_rows, 0,
        "empty-trash must hard-delete trashed books"
    );
    assert_eq!(
        remaining.len(),
        2,
        "series should keep two non-deleted books"
    );
    assert_eq!(remaining[0].get::<String, _>("ID"), "book-1");
    assert_eq!(remaining[0].get::<i64, _>("BOOK_NUMBER"), 1);
    assert_eq!(remaining[0].get::<String, _>("METADATA_NUMBER"), "1");
    assert_eq!(remaining[0].get::<f64, _>("METADATA_NUMBER_SORT"), 1.0_f64);
    assert_eq!(remaining[1].get::<String, _>("ID"), "book-3");
    assert_eq!(remaining[1].get::<i64, _>("BOOK_NUMBER"), 2);
    assert_eq!(remaining[1].get::<String, _>("METADATA_NUMBER"), "2");
    assert_eq!(remaining[1].get::<f64, _>("METADATA_NUMBER_SORT"), 2.0_f64);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_empty_trash_uses_kotlin_like_natural_name_sort_for_remaining_series_books() {
    let paths = new_router_fixture("runtime-empty-trash-natural-name-sort").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for empty-trash natural-sort fixture setup");

    sqlx::query("UPDATE BOOK SET NAME = ?, URL = ? WHERE ID = ?")
        .bind("Vol 10.epub")
        .bind("books/Vol 10.epub")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book-1 name should be updated for natural-sort fixture");
    sqlx::query("UPDATE BOOK_METADATA SET TITLE = ? WHERE BOOK_ID = ?")
        .bind("Vol 10")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book-1 metadata title should be updated for natural-sort fixture");

    for (book_id, name, url, file_size, number) in [
        ("book-2", "Vol 1.epub", "books/Vol 1.epub", 2_048_i64, 2_i64),
        ("book-3", "Vol 2.epub", "books/Vol 2.epub", 3_072_i64, 3_i64),
    ] {
        sqlx::query(
            "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(book_id)
        .bind(0_i64)
        .bind(name)
        .bind(url)
        .bind("series-1")
        .bind(file_size)
        .bind(number)
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("book row should be inserted for natural-sort fixture");

        sqlx::query(
            "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)",
        )
        .bind("application/epub+zip")
        .bind("READY")
        .bind(book_id)
        .bind(10_i64)
        .execute(&pool)
        .await
        .expect("media row should be inserted for natural-sort fixture");

        sqlx::query(
            "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, BOOK_ID) VALUES (?, ?, ?, ?)",
        )
        .bind(number.to_string())
        .bind(number as f64)
        .bind(name.replace(".epub", ""))
        .bind(book_id)
        .execute(&pool)
        .await
        .expect("book metadata row should be inserted for natural-sort fixture");
    }

    sqlx::query("UPDATE BOOK SET DELETED_DATE = CURRENT_TIMESTAMP WHERE ID = ?")
        .bind("book-2")
        .execute(&pool)
        .await
        .expect("trashed book should be marked deleted for natural-sort fixture");
    pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "EMPTY_TRASH:library-1",
        1_000,
        Some("library-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("empty-trash natural-sort cleanup should process successfully");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for empty-trash natural-sort verification");
    let remaining = sqlx::query(
        "SELECT b.ID AS ID, b.NAME AS NAME, b.NUMBER AS BOOK_NUMBER, bm.NUMBER AS METADATA_NUMBER, \
         bm.NUMBER_SORT AS METADATA_NUMBER_SORT \
         FROM BOOK b \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         WHERE b.SERIES_ID = ? \
           AND b.DELETED_DATE IS NULL \
         ORDER BY b.NUMBER ASC",
    )
    .bind("series-1")
    .fetch_all(&verify_pool)
    .await
    .expect("remaining books after natural-sort empty-trash should be queryable");
    verify_pool.close().await;

    assert_eq!(
        remaining.len(),
        2,
        "series should keep two non-deleted books"
    );
    assert_eq!(remaining[0].get::<String, _>("ID"), "book-3");
    assert_eq!(remaining[0].get::<String, _>("NAME"), "Vol 2.epub");
    assert_eq!(remaining[0].get::<i64, _>("BOOK_NUMBER"), 1);
    assert_eq!(remaining[0].get::<String, _>("METADATA_NUMBER"), "1");
    assert_eq!(remaining[0].get::<f64, _>("METADATA_NUMBER_SORT"), 1.0_f64);
    assert_eq!(remaining[1].get::<String, _>("ID"), "book-1");
    assert_eq!(remaining[1].get::<String, _>("NAME"), "Vol 10.epub");
    assert_eq!(remaining[1].get::<i64, _>("BOOK_NUMBER"), 2);
    assert_eq!(remaining[1].get::<String, _>("METADATA_NUMBER"), "2");
    assert_eq!(remaining[1].get::<f64, _>("METADATA_NUMBER_SORT"), 2.0_f64);

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
async fn runtime_generate_book_thumbnail_replaces_invalid_selected_thumbnail_with_generated_selection()
 {
    let paths =
        new_router_fixture("runtime-generate-book-thumbnail-replaces-invalid-selected-thumbnail")
            .await;
    seed_router_contract_data(&paths).await;
    const GIF_1X1: &[u8] = &[
        0x47, 0x49, 0x46, 0x38, 0x37, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF, 0xFF, 0x2C, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x02,
        0x02, 0x44, 0x01, 0x00, 0x3B,
    ];
    write_router_epub_resource(&paths, "books/book-1.epub", "OEBPS/cover.gif", GIF_1X1);

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(
        TaskQueueRecord::new("GENERATE_BOOK_THUMBNAIL_book-1", 1_000, None)
            .with_simple_type("GENERATE_BOOK_THUMBNAIL"),
    );
    scheduler
        .process_available(&runtime)
        .expect("generate-book-thumbnail task should replace invalid selected thumbnail cleanly");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for generated thumbnail verification");
    let thumbnails = sqlx::query(
        "SELECT ID, TYPE, SELECTED FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? ORDER BY ID ASC",
    )
    .bind("book-1")
    .fetch_all(&verify_pool)
    .await
    .expect("book thumbnail rows should be queryable after generated thumbnail task");
    verify_pool.close().await;

    assert_eq!(
        thumbnails.len(),
        1,
        "kotlin parity requires invalid selected thumbnails to be cleaned up during generated thumbnail insert",
    );
    assert_eq!(thumbnails[0].get::<String, _>("TYPE"), "GENERATED");
    assert!(
        thumbnails[0].get::<bool, _>("SELECTED"),
        "generated thumbnail should become selected after housekeeping removes the invalid previous selection",
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
async fn runtime_delete_book_soft_deletes_rows_and_removes_book_sidecar_files() {
    let paths = new_router_fixture("runtime-delete-book-soft-delete-staging").await;
    seed_router_contract_data(&paths).await;

    let delete_dir = paths.config_dir.join("delete-book");
    std::fs::create_dir_all(&delete_dir).expect("delete-book fixture directory should exist");
    let book_file = delete_dir.join("book-1.epub");
    let sidecar_thumbnail = delete_dir.join("book-1.png");
    std::fs::write(&book_file, b"delete-book-fixture")
        .expect("delete-book fixture book file should be written");
    std::fs::write(&sidecar_thumbnail, fixture_png_bytes())
        .expect("delete-book fixture sidecar thumbnail should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-book fixture setup");
    sqlx::query("UPDATE BOOK SET URL = ? WHERE ID = ?")
        .bind("delete-book/book-1.epub")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("delete-book fixture book url should be updated");
    sqlx::query(
        "INSERT INTO THUMBNAIL_BOOK (ID, BOOK_ID, TYPE, URL, SELECTED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("thumb-book-sidecar-delete")
    .bind("book-1")
    .bind("SIDECAR")
    .bind("delete-book/book-1.png")
    .bind(false)
    .execute(&pool)
    .await
    .expect("delete-book fixture sidecar thumbnail row should be inserted");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) VALUES (?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(3_i64)
    .bind(false)
    .execute(&pool)
    .await
    .expect("delete-book fixture read progress row should be inserted");
    let series_old_last_modified = sqlx::query(
        "SELECT COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED FROM SERIES WHERE ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&pool)
    .await
    .expect("delete-book fixture series row should be queryable")
    .get::<String, _>("LAST_MODIFIED");
    pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "DELETE_BOOK:book-1",
        1_000,
        Some("book-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("delete-book runtime should stage soft deletion cleanly");

    assert!(
        !book_file.exists(),
        "delete-book runtime should remove the main book file from disk"
    );
    assert!(
        !sidecar_thumbnail.exists(),
        "delete-book runtime should remove book sidecar thumbnail files from disk"
    );
    assert!(
        !delete_dir.exists(),
        "delete-book runtime should remove the now-empty parent directory"
    );

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-book verification");
    let book_row = sqlx::query("SELECT DELETED_DATE FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("soft-deleted book row should still be queryable");
    let thumbnail_rows =
        sqlx::query("SELECT ID, TYPE, URL FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? ORDER BY ID ASC")
            .bind("book-1")
            .fetch_all(&verify_pool)
            .await
            .expect("soft-deleted book thumbnail rows should still be queryable");
    let read_progress_count =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM READ_PROGRESS WHERE BOOK_ID = ?")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect("soft-deleted book read-progress rows should be queryable")
            .get::<i64, _>("COUNT");
    let series_row = sqlx::query(
        "SELECT BOOK_COUNT, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED FROM SERIES WHERE ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("series row should be queryable after delete-book staging");
    verify_pool.close().await;

    assert!(
        book_row.get::<Option<String>, _>("DELETED_DATE").is_some(),
        "delete-book runtime should stage the book for trash instead of hard-deleting it"
    );
    assert_eq!(
        thumbnail_rows.len(),
        2,
        "delete-book runtime should preserve THUMBNAIL_BOOK rows until EmptyTrash performs hard cleanup"
    );
    assert_eq!(
        read_progress_count, 1,
        "delete-book runtime should preserve READ_PROGRESS rows until EmptyTrash performs hard cleanup"
    );
    assert_eq!(
        series_row.get::<i64, _>("BOOK_COUNT"),
        0,
        "delete-book runtime should immediately recompute active series book count excluding soft-deleted books"
    );
    assert_ne!(
        series_row.get::<String, _>("LAST_MODIFIED"),
        series_old_last_modified,
        "delete-book runtime should refresh series last-modified so series changes remain externally visible",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_delete_book_oneshot_soft_deletes_series_and_removes_series_sidecar_files() {
    let paths = new_router_fixture("runtime-delete-book-oneshot-soft-delete-staging").await;
    seed_router_contract_data(&paths).await;

    let series_dir = paths.config_dir.join("delete-oneshot/series-1");
    std::fs::create_dir_all(&series_dir)
        .expect("delete-book oneshot series directory should exist");
    let book_file = series_dir.join("book-1.epub");
    let book_sidecar_thumbnail = series_dir.join("book-1.png");
    let series_sidecar_thumbnail = series_dir.join("cover.png");
    std::fs::write(&book_file, b"delete-book-oneshot-fixture")
        .expect("delete-book oneshot book file should be written");
    std::fs::write(&book_sidecar_thumbnail, fixture_png_bytes())
        .expect("delete-book oneshot book sidecar should be written");
    std::fs::write(&series_sidecar_thumbnail, fixture_png_bytes())
        .expect("delete-book oneshot series sidecar should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-book oneshot fixture setup");
    sqlx::query("UPDATE SERIES SET URL = ? WHERE ID = ?")
        .bind("delete-oneshot/series-1")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("delete-book oneshot series url should be updated");
    sqlx::query("UPDATE BOOK SET URL = ?, ONESHOT = 1 WHERE ID = ?")
        .bind("delete-oneshot/series-1/book-1.epub")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("delete-book oneshot book row should be updated");
    sqlx::query(
        "INSERT INTO THUMBNAIL_BOOK (ID, BOOK_ID, TYPE, URL, SELECTED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("thumb-book-sidecar-oneshot")
    .bind("book-1")
    .bind("SIDECAR")
    .bind("delete-oneshot/series-1/book-1.png")
    .bind(false)
    .execute(&pool)
    .await
    .expect("delete-book oneshot book sidecar row should be inserted");
    sqlx::query(
        "INSERT INTO THUMBNAIL_SERIES (ID, SERIES_ID, TYPE, URL, SELECTED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("thumb-series-sidecar-oneshot")
    .bind("series-1")
    .bind("SIDECAR")
    .bind("delete-oneshot/series-1/cover.png")
    .bind(true)
    .execute(&pool)
    .await
    .expect("delete-book oneshot series sidecar row should be inserted");
    let series_old_last_modified = sqlx::query(
        "SELECT COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED FROM SERIES WHERE ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&pool)
    .await
    .expect("delete-book oneshot series row should be queryable")
    .get::<String, _>("LAST_MODIFIED");
    pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "DELETE_BOOK:book-1",
        1_000,
        Some("book-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("delete-book oneshot runtime should stage soft deletion cleanly");

    assert!(
        !book_file.exists(),
        "delete-book oneshot runtime should remove the oneshot book file from disk"
    );
    assert!(
        !book_sidecar_thumbnail.exists(),
        "delete-book oneshot runtime should remove book sidecar thumbnail files from disk"
    );
    assert!(
        !series_sidecar_thumbnail.exists(),
        "delete-book oneshot runtime should remove series sidecar thumbnail files from disk"
    );
    assert!(
        !series_dir.exists(),
        "delete-book oneshot runtime should remove the now-empty series directory"
    );

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-book oneshot verification");
    let book_row = sqlx::query("SELECT DELETED_DATE FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("soft-deleted oneshot book row should still be queryable");
    let series_row = sqlx::query(
        "SELECT DELETED_DATE, BOOK_COUNT, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED FROM SERIES WHERE ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("soft-deleted oneshot series row should still be queryable");
    let book_thumbnail_rows =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect("soft-deleted oneshot book thumbnail rows should be queryable")
            .get::<i64, _>("COUNT");
    let series_thumbnail_rows =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM THUMBNAIL_SERIES WHERE SERIES_ID = ?")
            .bind("series-1")
            .fetch_one(&verify_pool)
            .await
            .expect("soft-deleted oneshot series thumbnail rows should be queryable")
            .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert!(
        book_row.get::<Option<String>, _>("DELETED_DATE").is_some(),
        "delete-book oneshot runtime should still trash-stage the book row instead of hard-deleting it"
    );
    assert!(
        series_row
            .get::<Option<String>, _>("DELETED_DATE")
            .is_some(),
        "delete-book oneshot runtime should trash-stage the series row instead of hard-deleting it"
    );
    assert_eq!(
        series_row.get::<i64, _>("BOOK_COUNT"),
        0,
        "delete-book oneshot runtime should recompute active book count to zero"
    );
    assert_ne!(
        series_row.get::<String, _>("LAST_MODIFIED"),
        series_old_last_modified,
        "delete-book oneshot runtime should refresh series last-modified for downstream visibility",
    );
    assert_eq!(
        book_thumbnail_rows, 2,
        "delete-book oneshot runtime should preserve THUMBNAIL_BOOK rows until EmptyTrash performs hard cleanup"
    );
    assert_eq!(
        series_thumbnail_rows, 1,
        "delete-book oneshot runtime should preserve THUMBNAIL_SERIES rows until EmptyTrash performs hard cleanup"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_delete_book_skips_soft_delete_when_book_file_is_missing() {
    let paths = new_router_fixture("runtime-delete-book-missing-file-no-staging").await;
    seed_router_contract_data(&paths).await;

    let delete_dir = paths.config_dir.join("delete-book-missing");
    std::fs::create_dir_all(&delete_dir)
        .expect("delete-book missing fixture directory should exist");
    let missing_book_file = delete_dir.join("book-1.epub");
    let sidecar_thumbnail = delete_dir.join("book-1.png");
    std::fs::write(&sidecar_thumbnail, fixture_png_bytes())
        .expect("delete-book missing fixture sidecar thumbnail should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-book missing fixture setup");
    sqlx::query("UPDATE BOOK SET URL = ? WHERE ID = ?")
        .bind("delete-book-missing/book-1.epub")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("delete-book missing fixture book url should be updated");
    sqlx::query(
        "INSERT INTO THUMBNAIL_BOOK (ID, BOOK_ID, TYPE, URL, SELECTED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("thumb-book-sidecar-missing")
    .bind("book-1")
    .bind("SIDECAR")
    .bind("delete-book-missing/book-1.png")
    .bind(false)
    .execute(&pool)
    .await
    .expect("delete-book missing fixture sidecar thumbnail row should be inserted");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) VALUES (?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(3_i64)
    .bind(false)
    .execute(&pool)
    .await
    .expect("delete-book missing fixture read progress row should be inserted");
    pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "DELETE_BOOK:book-1",
        1_000,
        Some("book-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("delete-book missing-file runtime should still drain cleanly");

    assert!(
        !missing_book_file.exists(),
        "delete-book missing-file fixture intentionally keeps the main file absent"
    );
    assert!(
        sidecar_thumbnail.exists(),
        "delete-book missing-file should not delete sidecar thumbnails when the main file precondition fails"
    );

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-book missing-file verification");
    let book_deleted = sqlx::query("SELECT DELETED_DATE FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("book row should be queryable after missing-file delete attempt")
        .get::<Option<String>, _>("DELETED_DATE");
    let thumbnail_count =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect("thumbnail rows should be queryable after missing-file delete attempt")
            .get::<i64, _>("COUNT");
    let read_progress_count =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM READ_PROGRESS WHERE BOOK_ID = ?")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect("read progress rows should be queryable after missing-file delete attempt")
            .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert!(
        book_deleted.is_none(),
        "delete-book missing-file should not soft-delete the book when filesystem preconditions fail",
    );
    assert_eq!(thumbnail_count, 2);
    assert_eq!(read_progress_count, 1);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_delete_book_oneshot_skips_soft_delete_when_series_directory_is_readonly() {
    let paths = new_router_fixture("runtime-delete-book-oneshot-readonly-series-no-staging").await;
    seed_router_contract_data(&paths).await;

    let series_dir = paths.config_dir.join("delete-oneshot-readonly/series-1");
    std::fs::create_dir_all(&series_dir)
        .expect("delete-book oneshot readonly series directory should exist");
    let book_file = series_dir.join("book-1.epub");
    let book_sidecar_thumbnail = series_dir.join("book-1.png");
    let series_sidecar_thumbnail = series_dir.join("cover.png");
    std::fs::write(&book_file, b"delete-book-oneshot-readonly-fixture")
        .expect("delete-book oneshot readonly book file should be written");
    std::fs::write(&book_sidecar_thumbnail, fixture_png_bytes())
        .expect("delete-book oneshot readonly book sidecar should be written");
    std::fs::write(&series_sidecar_thumbnail, fixture_png_bytes())
        .expect("delete-book oneshot readonly series sidecar should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-book oneshot readonly fixture setup");
    sqlx::query("UPDATE SERIES SET URL = ? WHERE ID = ?")
        .bind("delete-oneshot-readonly/series-1")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("delete-book oneshot readonly series url should be updated");
    sqlx::query("UPDATE BOOK SET URL = ?, ONESHOT = 1 WHERE ID = ?")
        .bind("delete-oneshot-readonly/series-1/book-1.epub")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("delete-book oneshot readonly book row should be updated");
    sqlx::query(
        "INSERT INTO THUMBNAIL_BOOK (ID, BOOK_ID, TYPE, URL, SELECTED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("thumb-book-sidecar-oneshot-readonly")
    .bind("book-1")
    .bind("SIDECAR")
    .bind("delete-oneshot-readonly/series-1/book-1.png")
    .bind(false)
    .execute(&pool)
    .await
    .expect("delete-book oneshot readonly book sidecar row should be inserted");
    sqlx::query(
        "INSERT INTO THUMBNAIL_SERIES (ID, SERIES_ID, TYPE, URL, SELECTED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("thumb-series-sidecar-oneshot-readonly")
    .bind("series-1")
    .bind("SIDECAR")
    .bind("delete-oneshot-readonly/series-1/cover.png")
    .bind(true)
    .execute(&pool)
    .await
    .expect("delete-book oneshot readonly series sidecar row should be inserted");
    pool.close().await;

    let mut permissions = std::fs::metadata(&series_dir)
        .expect("delete-book oneshot readonly series metadata should be readable")
        .permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&series_dir, permissions)
        .expect("delete-book oneshot readonly series directory should become readonly");

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "DELETE_BOOK:book-1",
        1_000,
        Some("book-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("delete-book oneshot readonly runtime should still drain cleanly");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-book oneshot readonly verification");
    let book_deleted = sqlx::query("SELECT DELETED_DATE FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("oneshot readonly book row should be queryable")
        .get::<Option<String>, _>("DELETED_DATE");
    let series_deleted = sqlx::query("SELECT DELETED_DATE FROM SERIES WHERE ID = ? LIMIT 1")
        .bind("series-1")
        .fetch_one(&verify_pool)
        .await
        .expect("oneshot readonly series row should be queryable")
        .get::<Option<String>, _>("DELETED_DATE");
    verify_pool.close().await;

    assert!(
        book_file.exists() && book_sidecar_thumbnail.exists() && series_sidecar_thumbnail.exists(),
        "delete-book oneshot readonly should not delete files when the series directory precondition fails",
    );
    assert!(
        book_deleted.is_none() && series_deleted.is_none(),
        "delete-book oneshot readonly should not soft-delete book or series when filesystem preconditions fail",
    );

    let mut cleanup_permissions = std::fs::metadata(&series_dir)
        .expect("delete-book oneshot readonly series metadata should still be readable")
        .permissions();
    cleanup_permissions.set_readonly(false);
    std::fs::set_permissions(&series_dir, cleanup_permissions).expect(
        "delete-book oneshot readonly series directory permissions should reset for cleanup",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_delete_series_soft_deletes_rows_and_removes_series_sidecar_files() {
    let paths = new_router_fixture("runtime-delete-series-soft-delete-staging").await;
    seed_router_contract_data(&paths).await;

    let series_dir = paths.config_dir.join("delete-series/series-1");
    std::fs::create_dir_all(&series_dir)
        .expect("delete-series fixture series directory should exist");
    let book_file = series_dir.join("book-1.epub");
    let book_sidecar_thumbnail = series_dir.join("book-1.png");
    let series_sidecar_thumbnail = series_dir.join("cover.png");
    std::fs::write(&book_file, b"delete-series-fixture")
        .expect("delete-series fixture book file should be written");
    std::fs::write(&book_sidecar_thumbnail, fixture_png_bytes())
        .expect("delete-series fixture book sidecar should be written");
    std::fs::write(&series_sidecar_thumbnail, fixture_png_bytes())
        .expect("delete-series fixture series sidecar should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-series fixture setup");
    sqlx::query("UPDATE SERIES SET URL = ? WHERE ID = ?")
        .bind("delete-series/series-1")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("delete-series fixture series url should be updated");
    sqlx::query("UPDATE BOOK SET URL = ? WHERE ID = ?")
        .bind("delete-series/series-1/book-1.epub")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("delete-series fixture book url should be updated");
    sqlx::query(
        "INSERT INTO THUMBNAIL_BOOK (ID, BOOK_ID, TYPE, URL, SELECTED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("thumb-book-sidecar-delete-series")
    .bind("book-1")
    .bind("SIDECAR")
    .bind("delete-series/series-1/book-1.png")
    .bind(false)
    .execute(&pool)
    .await
    .expect("delete-series fixture book sidecar row should be inserted");
    sqlx::query(
        "INSERT INTO THUMBNAIL_SERIES (ID, SERIES_ID, TYPE, URL, SELECTED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("thumb-series-sidecar-delete-series")
    .bind("series-1")
    .bind("SIDECAR")
    .bind("delete-series/series-1/cover.png")
    .bind(true)
    .execute(&pool)
    .await
    .expect("delete-series fixture series sidecar row should be inserted");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) VALUES (?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(5_i64)
    .bind(false)
    .execute(&pool)
    .await
    .expect("delete-series fixture read progress row should be inserted");
    let series_old_last_modified = sqlx::query(
        "SELECT COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED FROM SERIES WHERE ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&pool)
    .await
    .expect("delete-series fixture series row should be queryable")
    .get::<String, _>("LAST_MODIFIED");
    pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "DELETE_SERIES:series-1",
        1_000,
        Some("series-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("delete-series runtime should stage soft deletion cleanly");

    assert!(
        !book_file.exists(),
        "delete-series runtime should remove the series book file from disk"
    );
    assert!(
        !book_sidecar_thumbnail.exists(),
        "delete-series runtime should remove book sidecar thumbnail files from disk"
    );
    assert!(
        !series_sidecar_thumbnail.exists(),
        "delete-series runtime should remove series sidecar thumbnail files from disk"
    );
    assert!(
        !series_dir.exists(),
        "delete-series runtime should remove the now-empty series directory"
    );

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-series verification");
    let book_row = sqlx::query("SELECT DELETED_DATE FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("soft-deleted series book row should still be queryable");
    let series_row = sqlx::query(
        "SELECT DELETED_DATE, BOOK_COUNT, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED FROM SERIES WHERE ID = ? LIMIT 1",
    )
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("soft-deleted series row should still be queryable");
    let book_thumbnail_count =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect("soft-deleted series book thumbnails should be queryable")
            .get::<i64, _>("COUNT");
    let series_thumbnail_count =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM THUMBNAIL_SERIES WHERE SERIES_ID = ?")
            .bind("series-1")
            .fetch_one(&verify_pool)
            .await
            .expect("soft-deleted series thumbnails should be queryable")
            .get::<i64, _>("COUNT");
    let read_progress_count =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM READ_PROGRESS WHERE BOOK_ID = ?")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect("soft-deleted series read progress rows should be queryable")
            .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert!(
        book_row.get::<Option<String>, _>("DELETED_DATE").is_some(),
        "delete-series runtime should soft-delete child book rows instead of hard-deleting them"
    );
    assert!(
        series_row
            .get::<Option<String>, _>("DELETED_DATE")
            .is_some(),
        "delete-series runtime should soft-delete the series row instead of hard-deleting it"
    );
    assert_eq!(
        series_row.get::<i64, _>("BOOK_COUNT"),
        0,
        "delete-series runtime should immediately recompute active book count to zero"
    );
    assert_ne!(
        series_row.get::<String, _>("LAST_MODIFIED"),
        series_old_last_modified,
        "delete-series runtime should refresh series last-modified for downstream visibility",
    );
    assert_eq!(book_thumbnail_count, 2);
    assert_eq!(series_thumbnail_count, 1);
    assert_eq!(read_progress_count, 1);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_delete_series_skips_soft_delete_when_series_directory_is_missing() {
    let paths = new_router_fixture("runtime-delete-series-missing-directory-no-staging").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-series missing-directory fixture setup");
    sqlx::query("UPDATE SERIES SET URL = ? WHERE ID = ?")
        .bind("missing-delete-series/series-1")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("delete-series missing-directory series url should be updated");
    sqlx::query("UPDATE BOOK SET URL = ? WHERE ID = ?")
        .bind("missing-delete-series/series-1/book-1.epub")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("delete-series missing-directory book url should be updated");
    pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(TaskQueueRecord::new(
        "DELETE_SERIES:series-1",
        1_000,
        Some("series-1".to_string()),
    ));
    scheduler
        .process_available(&runtime)
        .expect("delete-series missing-directory runtime should still drain cleanly");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for delete-series missing-directory verification");
    let book_deleted = sqlx::query("SELECT DELETED_DATE FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("delete-series missing-directory book row should be queryable")
        .get::<Option<String>, _>("DELETED_DATE");
    let series_deleted = sqlx::query("SELECT DELETED_DATE FROM SERIES WHERE ID = ? LIMIT 1")
        .bind("series-1")
        .fetch_one(&verify_pool)
        .await
        .expect("delete-series missing-directory series row should be queryable")
        .get::<Option<String>, _>("DELETED_DATE");
    verify_pool.close().await;

    assert!(
        book_deleted.is_none() && series_deleted.is_none(),
        "delete-series missing-directory should not soft-delete rows when series filesystem preconditions fail",
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
async fn runtime_skips_book_hash_when_library_hash_files_was_disabled_after_enqueue() {
    let paths = new_router_fixture("runtime-skip-hash-book-when-library-hash-files-disabled").await;
    seed_router_contract_data(&paths).await;
    std::fs::create_dir_all(paths.config_dir.join("books"))
        .expect("book directory should exist for hash-files disabled fixture");
    std::fs::write(
        paths.config_dir.join("books/hash-book.cbz"),
        b"hash-files-disabled",
    )
    .expect("book file should be written for hash-files disabled fixture");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for hash-files disabled fixture setup");
    sqlx::query("UPDATE LIBRARY SET HASH_FILES = 0 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library hash-files flag should be disabled for runtime hash skip test");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, FILE_HASH) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-hash-flag-1")
    .bind(0_i64)
    .bind("hash-book.cbz")
    .bind("books/hash-book.cbz")
    .bind("series-1")
    .bind(19_i64)
    .bind(2_i64)
    .bind("library-1")
    .bind("")
    .execute(&pool)
    .await
    .expect("hash-files disabled fixture book row should be inserted");
    pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(
        TaskQueueRecord::new("HASH_BOOK_book-hash-flag-1", 1_000, None)
            .with_simple_type("HASH_BOOK"),
    );
    scheduler.process_available(&runtime).expect(
        "hash-book task should skip cleanly when library hash-files was disabled after enqueue",
    );

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for hash-files disabled verification");
    let file_hash = sqlx::query("SELECT FILE_HASH FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-hash-flag-1")
        .fetch_one(&verify_pool)
        .await
        .expect("book hash should be queryable for disabled-flag verification")
        .get::<Option<String>, _>("FILE_HASH");
    verify_pool.close().await;

    assert_eq!(
        file_hash,
        Some(String::new()),
        "runtime must skip file hashing when library.hashFiles was disabled after the task was enqueued",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_skips_book_hash_when_book_already_has_hash() {
    let paths = new_router_fixture("runtime-skip-hash-book-when-already-present").await;
    seed_router_contract_data(&paths).await;
    std::fs::create_dir_all(paths.config_dir.join("books"))
        .expect("book directory should exist for existing hash fixture");
    std::fs::write(
        paths.config_dir.join("books/book-1.epub"),
        b"hash-should-not-overwrite",
    )
    .expect("book file should be written for existing hash fixture");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for existing hash fixture setup");
    sqlx::query("UPDATE BOOK SET FILE_HASH = ? WHERE ID = ?")
        .bind("hash-book-existing")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("existing book hash should be seeded for skip test");
    pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(
        TaskQueueRecord::new("HASH_BOOK_book-1", 1_000, None).with_simple_type("HASH_BOOK"),
    );
    scheduler
        .process_available(&runtime)
        .expect("hash-book task should skip cleanly when the book already has a hash");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for existing hash verification");
    let file_hash = sqlx::query("SELECT FILE_HASH FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("existing file hash should be queryable")
        .get::<Option<String>, _>("FILE_HASH");
    verify_pool.close().await;

    assert_eq!(
        file_hash,
        Some("hash-book-existing".to_string()),
        "runtime must not overwrite an existing file hash",
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
async fn runtime_skips_book_koreader_hash_when_library_hash_koreader_was_disabled_after_enqueue() {
    let paths =
        new_router_fixture("runtime-skip-koreader-hash-when-library-hash-koreader-disabled").await;
    seed_router_contract_data(&paths).await;
    std::fs::create_dir_all(paths.config_dir.join("books"))
        .expect("book directory should exist for koreader-hash disabled fixture");
    std::fs::write(
        paths.config_dir.join("books/koreader-book.cbz"),
        b"koreader-hash-disabled",
    )
    .expect("book file should be written for koreader-hash disabled fixture");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for koreader-hash disabled fixture setup");
    sqlx::query("UPDATE LIBRARY SET HASH_KOREADER = 0 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library hash-koreader flag should be disabled for runtime skip test");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, FILE_HASH_KOREADER) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-koreader-flag-1")
    .bind(0_i64)
    .bind("koreader-book.cbz")
    .bind("books/koreader-book.cbz")
    .bind("series-1")
    .bind(22_i64)
    .bind(2_i64)
    .bind("library-1")
    .bind("")
    .execute(&pool)
    .await
    .expect("koreader-hash disabled fixture book row should be inserted");
    pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(
        TaskQueueRecord::new("HASH_BOOK_KOREADER_book-koreader-flag-1", 1_000, None)
            .with_simple_type("HASH_BOOK_KOREADER"),
    );
    scheduler
        .process_available(&runtime)
        .expect("koreader-hash task should skip cleanly when library hash-koreader was disabled after enqueue");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for koreader-hash disabled verification");
    let file_hash = sqlx::query("SELECT FILE_HASH_KOREADER FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-koreader-flag-1")
        .fetch_one(&verify_pool)
        .await
        .expect("koreader hash should be queryable for disabled-flag verification")
        .get::<Option<String>, _>("FILE_HASH_KOREADER");
    verify_pool.close().await;

    assert_eq!(
        file_hash,
        Some(String::new()),
        "runtime must skip koreader hashing when library.hashKoreader was disabled after the task was enqueued",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_skips_book_koreader_hash_when_book_already_has_hash() {
    let paths = new_router_fixture("runtime-skip-koreader-hash-when-already-present").await;
    seed_router_contract_data(&paths).await;
    std::fs::create_dir_all(paths.config_dir.join("books"))
        .expect("book directory should exist for existing koreader hash fixture");
    std::fs::write(
        paths.config_dir.join("books/book-1.epub"),
        b"koreader-hash-should-not-overwrite",
    )
    .expect("book file should be written for existing koreader hash fixture");

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(
        TaskQueueRecord::new("HASH_BOOK_KOREADER_book-1", 1_000, None)
            .with_simple_type("HASH_BOOK_KOREADER"),
    );
    scheduler
        .process_available(&runtime)
        .expect("koreader-hash task should skip cleanly when the book already has a koreader hash");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for existing koreader hash verification");
    let file_hash = sqlx::query("SELECT FILE_HASH_KOREADER FROM BOOK WHERE ID = ? LIMIT 1")
        .bind("book-1")
        .fetch_one(&verify_pool)
        .await
        .expect("existing koreader hash should be queryable")
        .get::<Option<String>, _>("FILE_HASH_KOREADER");
    verify_pool.close().await;

    assert_eq!(
        file_hash,
        Some("hash-book-1".to_string()),
        "runtime must not overwrite an existing koreader hash",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn runtime_skips_book_page_hash_when_library_hash_pages_was_disabled_after_enqueue() {
    let paths = new_router_fixture("runtime-skip-page-hash-when-library-hash-pages-disabled").await;
    seed_router_contract_data(&paths).await;
    const GIF_1X1: &[u8] = &[
        0x47, 0x49, 0x46, 0x38, 0x37, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF, 0xFF, 0x2C, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x02,
        0x02, 0x44, 0x01, 0x00, 0x3B,
    ];
    std::fs::create_dir_all(paths.config_dir.join("books"))
        .expect("book directory should exist for page-hash disabled fixture");
    std::fs::write(paths.config_dir.join("books/hash-image.gif"), GIF_1X1)
        .expect("image file should be written for page-hash disabled fixture");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for page-hash disabled fixture setup");
    sqlx::query("UPDATE LIBRARY SET HASH_PAGES = 0 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library hash-pages flag should be disabled for runtime page-hash skip test");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-hash-flag-1")
    .bind(0_i64)
    .bind("hash-image.gif")
    .bind("books/hash-image.gif")
    .bind("series-1")
    .bind(i64::try_from(GIF_1X1.len()).expect("gif size should fit in i64"))
    .bind(2_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("page-hash disabled fixture book row should be inserted");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("image/gif")
        .bind("READY")
        .bind("book-hash-flag-1")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("page-hash disabled fixture media row should be inserted");
    sqlx::query(
        "INSERT INTO MEDIA_PAGE (FILE_NAME, MEDIA_TYPE, NUMBER, BOOK_ID, width, height, FILE_HASH, FILE_SIZE) VALUES (?, ?, ?, ?, NULL, NULL, '', ?)",
    )
    .bind("hash-image.gif")
    .bind("image/gif")
    .bind(1_i64)
    .bind("book-hash-flag-1")
    .bind(i64::try_from(GIF_1X1.len()).expect("gif size should fit in i64"))
    .execute(&pool)
    .await
    .expect("page-hash disabled fixture media page row should be inserted");
    pool.close().await;

    let runtime = runtime_task_context(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(
        TaskQueueRecord::new("HASH_BOOK_PAGES_book-hash-flag-1", 1_000, None)
            .with_simple_type("HASH_BOOK_PAGES"),
    );
    scheduler.process_available(&runtime).expect(
        "page-hash task should skip cleanly when library hash-pages was disabled after enqueue",
    );

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for page-hash disabled verification");
    let file_hash =
        sqlx::query("SELECT FILE_HASH FROM MEDIA_PAGE WHERE BOOK_ID = ? AND NUMBER = 1 LIMIT 1")
            .bind("book-hash-flag-1")
            .fetch_one(&verify_pool)
            .await
            .expect("page-hash disabled fixture media page hash should be queryable")
            .get::<String, _>("FILE_HASH");
    verify_pool.close().await;

    assert_eq!(
        file_hash,
        String::new(),
        "runtime must skip page hashing when library.hashPages was disabled after the task was enqueued",
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

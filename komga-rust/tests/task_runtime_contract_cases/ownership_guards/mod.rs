use super::*;

mod external_owned;

mod book_local_artwork;

mod series_local_artwork;

mod series_metadata_providers;

mod deletion;

mod hashing;

mod book_thumbnails;

mod metadata_refresh;

mod empty_trash;

mod main_db_guards;

use metadata_refresh::{
    write_router_epub_with_package_document, write_router_epub_with_package_document_and_entries,
};
use std::sync::OnceLock;
use tokio::sync::{Mutex, MutexGuard};

fn runtime_sse_contract_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

async fn runtime_sse_contract_guard() -> MutexGuard<'static, ()> {
    runtime_sse_contract_lock().lock().await
}

#[tokio::test]
async fn runtime_executes_kotlin_persisted_refresh_book_metadata_task() {
    let ctx = TestFixture::new("runtime-executes-kotlin-refresh-book-metadata-task").await;

    let sidecar_dir = ctx.paths().config_dir.join("books");
    std::fs::create_dir_all(&sidecar_dir).expect("book metadata sidecar directory should exist");
    std::fs::write(
        sidecar_dir.join("book-1.xml"),
        br#"<ComicInfo><Title>Kotlin Refresh Title</Title><Summary>Kotlin Refresh Summary</Summary></ComicInfo>"#,
    )
    .expect("book metadata sidecar fixture should be written");

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
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

    let tasks_pool = connect_test_pool(ctx.paths().tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for Kotlin persisted metadata task setup");
    sqlx::query(
        "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) VALUES (?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind("RefreshBookMetadata_book-1")
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
            "uniqueId": "RefreshBookMetadata_book-1"
        })
        .to_string(),
    )
    .execute(&tasks_pool)
    .await
    .expect("Kotlin persisted metadata task row should be inserted");
    tasks_pool.close().await;

    let runtime = runtime_task_context(ctx.paths()).await;
    let scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .process_available(&runtime.job())
        .await
        .expect("runtime should execute Kotlin persisted RefreshBookMetadata tasks successfully");

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
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
}

#[tokio::test]
async fn runtime_refresh_series_metadata_applies_oneshot_provider_fields() {
    let ctx = TestFixture::new("runtime-refresh-series-metadata-oneshot-provider").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
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

    let runtime = runtime_task_context_with_overrides(
        ctx.paths(),
        TaskRuntimeOwnershipOverrides {
            owns_search_index: Some(false),
            ..TaskRuntimeOwnershipOverrides::default()
        },
    )
    .await;
    let scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .enqueue(
            TaskQueueRecord::new(
                "RefreshSeriesMetadata_series-oneshot",
                1_000,
                Some("series-oneshot".to_string()),
            )
            .with_simple_type("RefreshSeriesMetadata"),
        )
        .await;
    scheduler
        .process_available(&runtime.job())
        .await
        .expect("oneshot refresh-series-metadata task should process successfully");

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
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
}

#[tokio::test]
async fn runtime_executes_kotlin_persisted_refresh_book_metadata_task_with_default_capabilities() {
    let ctx = TestFixture::new("runtime-executes-kotlin-refresh-book-metadata-defaults").await;

    let sidecar_dir = ctx.paths().config_dir.join("books");
    std::fs::create_dir_all(&sidecar_dir).expect("book metadata sidecar directory should exist");
    std::fs::write(
        sidecar_dir.join("book-1.xml"),
        br#"<ComicInfo><Title>Kotlin Default Title</Title><Summary>Kotlin Default Summary</Summary></ComicInfo>"#,
    )
    .expect("book metadata sidecar fixture should be written for default capability restore");

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for default-capability metadata fixture setup");
    sqlx::query("DELETE FROM SIDECAR WHERE PARENT_URL = ?")
        .bind("books/book-1.epub")
        .execute(&pool)
        .await
        .expect(
            "existing book metadata sidecars should be cleared before default-capability task test",
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
    .expect("book metadata sidecar row should be inserted for default-capability task test");
    pool.close().await;

    let tasks_pool = connect_test_pool(ctx.paths().tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for default-capability metadata task setup");
    sqlx::query(
        "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) VALUES (?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind("RefreshBookMetadata_book-1")
    .bind(80_i64)
    .bind("series-1")
    .bind("org.gotson.komga.application.tasks.Task$RefreshBookMetadata")
    .bind("RefreshBookMetadata")
    .bind(
        json!({
            "bookId": "book-1",
            "priority": 80,
            "groupId": "series-1",
            "uniqueId": "RefreshBookMetadata_book-1"
        })
        .to_string(),
    )
    .execute(&tasks_pool)
    .await
    .expect("Kotlin persisted metadata task row without capabilities should be inserted");
    tasks_pool.close().await;

    let runtime = runtime_task_context(ctx.paths()).await;
    let scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .process_available(&runtime.job()).await
        .expect("runtime should restore default RefreshBookMetadata capabilities for persisted Kotlin tasks");

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for default-capability metadata verification");
    let metadata =
        sqlx::query("SELECT TITLE, SUMMARY FROM BOOK_METADATA WHERE BOOK_ID = ? LIMIT 1")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect(
                "book metadata row should be queryable after default-capability task execution",
            );
    verify_pool.close().await;

    assert_eq!(metadata.get::<String, _>("TITLE"), "Kotlin Default Title");
    assert_eq!(
        metadata.get::<String, _>("SUMMARY"),
        "Kotlin Default Summary"
    );
}

#[tokio::test]
async fn runtime_executes_kotlin_persisted_repair_extension_task() {
    let ctx = TestFixture::new("runtime-executes-kotlin-repair-extension-task").await;

    std::fs::create_dir_all(ctx.paths().config_dir.join("books"))
        .expect("book directory should exist for Kotlin persisted repair-extension task");
    let source_path = ctx.paths().config_dir.join("books/repair-book.bin");
    std::fs::write(&source_path, b"kotlin-repair-extension")
        .expect("repair-extension source should be written for Kotlin persisted task");

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
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

    let tasks_pool = connect_test_pool(ctx.paths().tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for Kotlin persisted repair-extension task setup");
    sqlx::query(
        "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) VALUES (?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind("RepairExtension_book-repair-1")
    .bind(12_i64)
    .bind("series-1")
    .bind("org.gotson.komga.application.tasks.Task$RepairExtension")
    .bind("RepairExtension")
    .bind(
        json!({
            "bookId": "book-repair-1",
            "priority": 12,
            "groupId": "series-1",
            "uniqueId": "RepairExtension_book-repair-1"
        })
        .to_string(),
    )
    .execute(&tasks_pool)
    .await
    .expect("Kotlin persisted repair-extension task row should be inserted");
    tasks_pool.close().await;

    let runtime = runtime_task_context(ctx.paths()).await;
    let scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .process_available(&runtime.job())
        .await
        .expect("runtime should execute Kotlin persisted RepairExtension tasks successfully");

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
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
    assert!(
        ctx.paths()
            .config_dir
            .join("books/repair-book.pdf")
            .exists()
    );
    assert!(!source_path.exists());
}

use std::collections::BTreeMap;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::application::task_processing::TaskRuntimeContext;
use komga_rust::config::{RuntimeCli, RuntimeConfig};
use komga_rust::infrastructure::sqlite::connect_pool;
use komga_rust::scanner::{ScannerOptions, scan_root_folder};
use komga_rust::{SearchEntityType, SearchIndexLifecycle, TaskQueueRecord, TaskQueueScheduler};
use serde_json::{Value, json};
use sqlx::Row;
use zip::CompressionMethod;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

const MINIMAL_PNG_BYTES: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0,
    0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

#[path = "support/persistence_contract_fixture.rs"]
mod persistence_contract_fixture;

#[test]
fn scanner_persistence_contract_target_is_registered() {
    assert_required_target_declared("tasks/scanner", "scanner_persistence_contract");
}

#[tokio::test]
async fn scanner_scan_output_is_persisted_into_kotlin_compatible_library_series_book_and_sidecar_tables()
 {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-write-shape")
        .await
        .expect("scanner persistence fixture should be created");

    let scan_result = scan_root_folder(&fixture.library_root, &ScannerOptions::default())
        .expect("filesystem scan fixture should produce deterministic scanner output");
    assert_eq!(
        scan_result.series.len(),
        1,
        "fixture sanity: one series expected"
    );
    assert_eq!(
        scan_result.series[0].books.len(),
        1,
        "fixture sanity: one book expected"
    );
    assert_eq!(
        scan_result.sidecars.len(),
        2,
        "fixture sanity: one series sidecar and one book sidecar expected",
    );

    let _app = komga_server::app::build_router_with_config(&fixture.config);

    let snapshot = load_persistence_snapshot(&fixture.paths.main_db, "library-1").await;

    assert_eq!(
        snapshot.library_rows, 1,
        "fixture sanity: expected seeded LIBRARY row for scanner write contract",
    );
    assert!(
        snapshot.series_rows >= 1,
        "scanner contract requires scan output to persist SERIES rows compatible with Kotlin readers",
    );
    assert!(
        snapshot.book_rows >= 1,
        "scanner contract requires scan output to persist BOOK rows compatible with Kotlin readers",
    );
    assert!(
        snapshot.media_file_rows >= 1,
        "scanner contract requires scanned archive file names to persist in MEDIA_FILE",
    );
    assert!(
        snapshot.sidecar_rows >= 2,
        "scanner contract requires series/book sidecars to persist in SIDECAR with Kotlin-compatible shape",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_scan_persistence_emits_scan_and_analyze_tasks_into_persisted_runtime_flow() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-task-emission")
        .await
        .expect("scanner task-emission fixture should be created");

    let scan_result = scan_root_folder(&fixture.library_root, &ScannerOptions::default())
        .expect("filesystem scan fixture should produce deterministic scanner output");
    assert_eq!(
        scan_result.series.len(),
        1,
        "fixture sanity: one series expected"
    );
    assert_eq!(
        scan_result.series[0].books.len(),
        1,
        "fixture sanity: one book expected"
    );

    let _app = komga_server::app::build_router_with_config(&fixture.config);

    let content_snapshot = load_persistence_snapshot(&fixture.paths.main_db, "library-1").await;
    assert!(
        content_snapshot.series_rows >= 1 && content_snapshot.book_rows >= 1,
        "task-emission contract requires scanner content rows before asserting runtime task flow",
    );

    let task_snapshot = load_task_snapshot(&fixture.paths.tasks_db).await;
    assert_eq!(
        task_snapshot.task_rows, 0,
        "scanner-triggered runtime contract now requires queue worker to execute and complete queued scan/analyze tasks end to end",
    );

    let media_ready_rows = load_media_ready_count(&fixture.paths.main_db).await;
    assert!(
        media_ready_rows >= 1,
        "scanner-triggered runtime flow must execute analyze tasks and persist MEDIA status transitions",
    );

    let scheduler = TaskQueueScheduler::for_runtime(fixture.config.clone(), "rust-main");
    assert!(
        scheduler.count_by_simple_type().is_empty(),
        "runtime queue should be drained after worker execution instead of leaving persisted pending rows",
    );

    let search = SearchIndexLifecycle::bootstrap(fixture.config.lucene_data_directory.as_path())
        .expect("search index should bootstrap for scanner runtime assertions");
    let hits = search
        .search_ids("Book-001", SearchEntityType::Book, 10)
        .expect("search lookup should succeed after scanner/analyze worker execution");
    assert!(
        !hits.is_empty(),
        "scan/analyze runtime flow should update search index documents",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_deep_scan_reanalyzes_changed_existing_books() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-deep-scan-reanalyzes")
        .await
        .expect("scanner deep-scan fixture should be created");

    let book_path = fixture.library_root.join("Series-A").join("Book-001.cbz");
    write_scannable_cbz_fixture(&book_path, b"page-before")
        .expect("initial scannable cbz fixture should be written");
    let book_url = book_path.to_string_lossy().to_string();

    let mut scheduler = TaskQueueScheduler::for_runtime(fixture.config.clone(), "rust-main");
    scheduler.enqueue(
        TaskQueueRecord::new("SCAN_LIBRARY:library-1", 900, Some("library-1".to_string()))
            .with_simple_type("SCAN_LIBRARY"),
    );
    scheduler
        .process_available(&fixture.config)
        .expect("initial scan should analyze the seeded book successfully");

    let initial_page_size = load_media_page_file_size(&fixture.paths.main_db, &book_url).await;
    assert_eq!(
        initial_page_size,
        i64::try_from(b"page-before".len()).expect("initial page size should fit into i64"),
        "fixture sanity: initial scan must persist MEDIA_PAGE size from the archive entry",
    );

    tokio::time::sleep(Duration::from_millis(1100)).await;
    write_scannable_cbz_fixture(&book_path, b"page-after-deep-scan")
        .expect("updated scannable cbz fixture should be written");

    scheduler.enqueue(
        TaskQueueRecord::new("SCAN_LIBRARY:library-1", 900, Some("library-1".to_string()))
            .with_simple_type("SCAN_LIBRARY")
            .with_payload(r#"{"deep":true}"#),
    );
    scheduler
        .process_available(&fixture.config)
        .expect("deep scan should complete successfully after the book archive changes");

    let updated_page_size = load_media_page_file_size(&fixture.paths.main_db, &book_url).await;
    assert_eq!(
        updated_page_size,
        i64::try_from(b"page-after-deep-scan".len())
            .expect("updated page size should fit into i64"),
        "deep scan must re-trigger analyze for changed existing books so MEDIA_PAGE rows refresh",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_oneshot_rescan_reuses_existing_series_id_when_book_url_changes() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-oneshot-series-id-reuse")
        .await
        .expect("scanner oneshot series-id fixture should be created");

    let regular_series_dir = fixture.library_root.join("Series-A");
    fs::remove_dir_all(&regular_series_dir)
        .expect("default regular series directory should be removable for oneshot fixture");

    let oneshots_dir = fixture.library_root.join("OneShots");
    fs::create_dir_all(&oneshots_dir).expect("oneshots directory should be created");
    let existing_book_path = oneshots_dir.join("Existing.cbz");
    write_scannable_cbz_fixture(&existing_book_path, MINIMAL_PNG_BYTES)
        .expect("oneshot book fixture should be written");
    update_library_oneshots_directory(&fixture.paths.main_db, "library-1", Some("OneShots")).await;

    let mut scheduler = TaskQueueScheduler::for_runtime(fixture.config.clone(), "rust-main");
    scheduler.enqueue(
        TaskQueueRecord::new("SCAN_LIBRARY:library-1", 900, Some("library-1".to_string()))
            .with_simple_type("SCAN_LIBRARY"),
    );
    scheduler
        .process_available(&fixture.config)
        .expect("initial oneshot scan should complete successfully");

    let existing_book_url = existing_book_path.to_string_lossy().to_string();
    let original_series_id =
        load_active_series_id_for_book_url(&fixture.paths.main_db, &existing_book_url).await;

    tokio::time::sleep(Duration::from_millis(1100)).await;
    let renamed_book_path = oneshots_dir.join("Renamed.cbz");
    fs::rename(&existing_book_path, &renamed_book_path)
        .expect("oneshot book fixture should be renamed");
    let renamed_book_url = renamed_book_path.to_string_lossy().to_string();
    update_active_book_url(
        &fixture.paths.main_db,
        &existing_book_url,
        &renamed_book_url,
    )
    .await;

    scheduler.enqueue(
        TaskQueueRecord::new("SCAN_LIBRARY:library-1", 900, Some("library-1".to_string()))
            .with_simple_type("SCAN_LIBRARY"),
    );
    scheduler
        .process_available(&fixture.config)
        .expect("oneshot rescan should complete successfully after rename");

    let rescanned_series_id =
        load_active_series_id_for_book_url(&fixture.paths.main_db, &renamed_book_url).await;
    assert_eq!(
        rescanned_series_id, original_series_id,
        "oneshot rescan should reuse the existing series id instead of creating a new one after import-style rename",
    );
    assert_eq!(
        load_series_url_by_id(&fixture.paths.main_db, &original_series_id).await,
        renamed_book_url,
        "oneshot rescan should update SERIES.URL to the renamed book path while preserving the series identity",
    );
    assert_eq!(
        load_active_series_count(&fixture.paths.main_db, "library-1").await,
        1,
        "oneshot rescan should not leave behind a soft-deleted replacement series row for the renamed book",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_regular_scan_reanalyzes_changed_books_when_series_timestamp_changes() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-series-changed-reanalyzes")
        .await
        .expect("scanner series-changed fixture should be created");

    let book_path = fixture.library_root.join("Series-A").join("Book-001.cbz");
    write_scannable_cbz_fixture(&book_path, b"page-before")
        .expect("initial scannable cbz fixture should be written");
    let book_url = book_path.to_string_lossy().to_string();

    let mut scheduler = TaskQueueScheduler::for_runtime(fixture.config.clone(), "rust-main");
    scheduler.enqueue(
        TaskQueueRecord::new("SCAN_LIBRARY:library-1", 900, Some("library-1".to_string()))
            .with_simple_type("SCAN_LIBRARY"),
    );
    scheduler
        .process_available(&fixture.config)
        .expect("initial scan should analyze the seeded book successfully");

    tokio::time::sleep(Duration::from_millis(1100)).await;
    write_scannable_cbz_fixture(&book_path, b"page-after-regular-scan")
        .expect("updated scannable cbz fixture should be written");
    fs::write(
        fixture
            .library_root
            .join("Series-A")
            .join("scan-marker.tmp"),
        b"marker",
    )
    .expect("book sidecar rewrite should bump series directory timestamp");

    scheduler.enqueue(
        TaskQueueRecord::new("SCAN_LIBRARY:library-1", 900, Some("library-1".to_string()))
            .with_simple_type("SCAN_LIBRARY"),
    );
    scheduler
        .process_available(&fixture.config)
        .expect("regular scan should complete successfully after series timestamp changes");

    let updated_page_size = load_media_page_file_size(&fixture.paths.main_db, &book_url).await;
    assert_eq!(
        updated_page_size,
        i64::try_from(b"page-after-regular-scan".len())
            .expect("updated page size should fit into i64"),
        "regular scan must re-trigger analyze when seriesChanged makes Kotlin enter the book update branch",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_regular_scan_reanalyzes_changed_books_when_series_has_deleted_books_without_timestamp_change()
 {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-deleted-books-reanalyzes")
        .await
        .expect("scanner deleted-books fixture should be created");

    let series_dir = fixture.library_root.join("Series-A");
    let primary_book_path = series_dir.join("Book-001.cbz");
    let deleted_book_path = series_dir.join("Book-002.cbz");
    write_scannable_cbz_fixture(&primary_book_path, b"page-before")
        .expect("primary scannable cbz fixture should be written");
    write_scannable_cbz_fixture(&deleted_book_path, b"deleted-book-page")
        .expect("secondary scannable cbz fixture should be written");
    let primary_book_url = primary_book_path.to_string_lossy().to_string();

    let mut scheduler = TaskQueueScheduler::for_runtime(fixture.config.clone(), "rust-main");
    scheduler.enqueue(
        TaskQueueRecord::new("SCAN_LIBRARY:library-1", 900, Some("library-1".to_string()))
            .with_simple_type("SCAN_LIBRARY"),
    );
    scheduler
        .process_available(&fixture.config)
        .expect("initial scan should analyze both seeded books successfully");

    let initial_page_size =
        load_media_page_file_size(&fixture.paths.main_db, &primary_book_url).await;
    assert_eq!(
        initial_page_size,
        i64::try_from(b"page-before".len()).expect("initial page size should fit into i64"),
        "fixture sanity: initial scan must persist the primary book page size",
    );

    tokio::time::sleep(Duration::from_millis(1100)).await;
    fs::remove_file(&deleted_book_path)
        .expect("secondary book should be removed to simulate deleted-books seriesChanged path");
    write_scannable_cbz_fixture(&primary_book_path, b"page-after-deleted-book-regular-scan")
        .expect("primary book should be rewritten after deleted-books setup");

    let current_series_last_modified = fs::metadata(&series_dir)
        .expect("series directory metadata should stay queryable")
        .modified()
        .expect("series directory modified time should stay queryable")
        .duration_since(std::time::UNIX_EPOCH)
        .expect("series directory modified time should be after unix epoch")
        .as_secs() as i64;
    update_series_file_last_modified(
        &fixture.paths.main_db,
        &series_dir.to_string_lossy(),
        current_series_last_modified,
    )
    .await;

    scheduler.enqueue(
        TaskQueueRecord::new("SCAN_LIBRARY:library-1", 900, Some("library-1".to_string()))
            .with_simple_type("SCAN_LIBRARY"),
    );
    scheduler
        .process_available(&fixture.config)
        .expect("regular scan should complete successfully after deleting a sibling book");

    let updated_page_size =
        load_media_page_file_size(&fixture.paths.main_db, &primary_book_url).await;
    assert_eq!(
        updated_page_size,
        i64::try_from(b"page-after-deleted-book-regular-scan".len())
            .expect("updated page size should fit into i64"),
        "regular scan must re-trigger analyze when deleted books force Kotlin's seriesChanged fallback even without a timestamp delta",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_persists_hash_book_tasks_with_kotlin_task_shape() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-hash-book-shape")
        .await
        .expect("scanner hash-book task fixture should be created");

    let mut scheduler = TaskQueueScheduler::for_runtime(fixture.config.clone(), "rust-main");
    scheduler
        .enqueue(TaskQueueRecord::new("HASH_BOOK_book-1", 0, None).with_simple_type("HASH_BOOK"));

    let tasks_pool = connect_pool(fixture.paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for hash-book task verification");
    let row = sqlx::query(
        "SELECT ID, CLASS, SIMPLE_TYPE, GROUP_ID, PAYLOAD FROM TASK WHERE ID = ? LIMIT 1",
    )
    .bind("HASH_BOOK_book-1")
    .fetch_one(&tasks_pool)
    .await
    .expect("hash-book task row should be queryable");
    tasks_pool.close().await;

    assert_eq!(
        row.get::<String, _>("CLASS"),
        "org.gotson.komga.application.tasks.Task$HashBook"
    );
    assert_eq!(row.get::<String, _>("SIMPLE_TYPE"), "HashBook");
    assert_eq!(row.get::<Option<String>, _>("GROUP_ID"), None);
    assert_eq!(
        serde_json::from_str::<Value>(&row.get::<String, _>("PAYLOAD"))
            .expect("hash-book task payload should be valid json"),
        json!({
            "bookId": "book-1",
            "priority": 0,
            "groupId": Value::Null,
            "uniqueId": "HASH_BOOK_book-1"
        })
    );

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_persists_generate_book_thumbnail_tasks_with_kotlin_task_shape() {
    let fixture =
        ScannerPersistenceFixture::new("scanner-persistence-generate-book-thumbnail-shape")
            .await
            .expect("scanner generate-book-thumbnail task fixture should be created");

    let mut scheduler = TaskQueueScheduler::for_runtime(fixture.config.clone(), "rust-main");
    scheduler.enqueue(
        TaskQueueRecord::new("GENERATE_BOOK_THUMBNAIL_book-1", 12, None)
            .with_simple_type("GENERATE_BOOK_THUMBNAIL"),
    );

    let tasks_pool = connect_pool(fixture.paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for generate-book-thumbnail task verification");
    let row = sqlx::query(
        "SELECT ID, CLASS, SIMPLE_TYPE, GROUP_ID, PAYLOAD FROM TASK WHERE ID = ? LIMIT 1",
    )
    .bind("GENERATE_BOOK_THUMBNAIL_book-1")
    .fetch_one(&tasks_pool)
    .await
    .expect("generate-book-thumbnail task row should be queryable");
    tasks_pool.close().await;

    assert_eq!(
        row.get::<String, _>("CLASS"),
        "org.gotson.komga.application.tasks.Task$GenerateBookThumbnail"
    );
    assert_eq!(row.get::<String, _>("SIMPLE_TYPE"), "GenerateBookThumbnail");
    assert_eq!(row.get::<Option<String>, _>("GROUP_ID"), None);
    assert_eq!(
        serde_json::from_str::<Value>(&row.get::<String, _>("PAYLOAD"))
            .expect("generate-book-thumbnail task payload should be valid json"),
        json!({
            "bookId": "book-1",
            "priority": 12,
            "groupId": Value::Null,
            "uniqueId": "GENERATE_BOOK_THUMBNAIL_book-1"
        })
    );

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_persists_refresh_book_local_artwork_tasks_with_kotlin_task_shape() {
    let fixture =
        ScannerPersistenceFixture::new("scanner-persistence-refresh-book-local-artwork-shape")
            .await
            .expect("scanner refresh-book-local-artwork task fixture should be created");

    let mut scheduler = TaskQueueScheduler::for_runtime(fixture.config.clone(), "rust-main");
    scheduler.enqueue(
        TaskQueueRecord::new("REFRESH_BOOK_LOCAL_ARTWORK_book-1", 80, None)
            .with_simple_type("REFRESH_BOOK_LOCAL_ARTWORK"),
    );

    let tasks_pool = connect_pool(fixture.paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for refresh-book-local-artwork task verification");
    let row = sqlx::query(
        "SELECT ID, CLASS, SIMPLE_TYPE, GROUP_ID, PAYLOAD FROM TASK WHERE ID = ? LIMIT 1",
    )
    .bind("REFRESH_BOOK_LOCAL_ARTWORK_book-1")
    .fetch_one(&tasks_pool)
    .await
    .expect("refresh-book-local-artwork task row should be queryable");
    tasks_pool.close().await;

    assert_eq!(
        row.get::<String, _>("CLASS"),
        "org.gotson.komga.application.tasks.Task$RefreshBookLocalArtwork"
    );
    assert_eq!(
        row.get::<String, _>("SIMPLE_TYPE"),
        "RefreshBookLocalArtwork"
    );
    assert_eq!(row.get::<Option<String>, _>("GROUP_ID"), None);
    assert_eq!(
        serde_json::from_str::<Value>(&row.get::<String, _>("PAYLOAD"))
            .expect("refresh-book-local-artwork task payload should be valid json"),
        json!({
            "bookId": "book-1",
            "priority": 80,
            "groupId": Value::Null,
            "uniqueId": "REFRESH_BOOK_LOCAL_ARTWORK_book-1"
        })
    );

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_persists_refresh_book_metadata_tasks_with_kotlin_task_shape() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-refresh-book-metadata-shape")
        .await
        .expect("scanner refresh-book-metadata task fixture should be created");

    let mut scheduler = TaskQueueScheduler::for_runtime(fixture.config.clone(), "rust-main");
    scheduler.enqueue(
        TaskQueueRecord::new(
            "REFRESH_BOOK_METADATA_book-1",
            80,
            Some("series-1".to_string()),
        )
        .with_simple_type("REFRESH_BOOK_METADATA"),
    );

    let tasks_pool = connect_pool(fixture.paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for refresh-book-metadata task verification");
    let row = sqlx::query(
        "SELECT ID, CLASS, SIMPLE_TYPE, GROUP_ID, PAYLOAD FROM TASK WHERE ID = ? LIMIT 1",
    )
    .bind("REFRESH_BOOK_METADATA_book-1")
    .fetch_one(&tasks_pool)
    .await
    .expect("refresh-book-metadata task row should be queryable");
    tasks_pool.close().await;

    assert_eq!(
        row.get::<String, _>("CLASS"),
        "org.gotson.komga.application.tasks.Task$RefreshBookMetadata"
    );
    assert_eq!(row.get::<String, _>("SIMPLE_TYPE"), "RefreshBookMetadata");
    assert_eq!(
        row.get::<Option<String>, _>("GROUP_ID"),
        Some("series-1".to_string())
    );
    assert_eq!(
        serde_json::from_str::<Value>(&row.get::<String, _>("PAYLOAD"))
            .expect("refresh-book-metadata task payload should be valid json"),
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
    );

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_persists_find_duplicate_pages_to_delete_tasks_with_kotlin_task_shape() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-find-duplicate-pages-shape")
        .await
        .expect("scanner duplicate-pages task fixture should be created");

    let mut scheduler = TaskQueueScheduler::for_runtime(fixture.config.clone(), "rust-main");
    scheduler.enqueue(
        TaskQueueRecord::new("FIND_DUPLICATE_PAGES_TO_DELETE_library-1", 85, None)
            .with_simple_type("FIND_DUPLICATE_PAGES_TO_DELETE"),
    );

    let tasks_pool = connect_pool(fixture.paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for duplicate-pages task verification");
    let row = sqlx::query(
        "SELECT ID, CLASS, SIMPLE_TYPE, GROUP_ID, PAYLOAD FROM TASK WHERE ID = ? LIMIT 1",
    )
    .bind("FIND_DUPLICATE_PAGES_TO_DELETE_library-1")
    .fetch_one(&tasks_pool)
    .await
    .expect("duplicate-pages task row should be queryable");
    tasks_pool.close().await;

    assert_eq!(
        row.get::<String, _>("CLASS"),
        "org.gotson.komga.application.tasks.Task$FindDuplicatePagesToDelete"
    );
    assert_eq!(
        row.get::<String, _>("SIMPLE_TYPE"),
        "FindDuplicatePagesToDelete"
    );
    assert_eq!(row.get::<Option<String>, _>("GROUP_ID"), None);
    assert_eq!(
        serde_json::from_str::<Value>(&row.get::<String, _>("PAYLOAD"))
            .expect("duplicate-pages task payload should be valid json"),
        json!({
            "libraryId": "library-1",
            "priority": 85,
            "groupId": Value::Null,
            "uniqueId": "FIND_DUPLICATE_PAGES_TO_DELETE_library-1"
        })
    );

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_persists_repair_extension_tasks_with_kotlin_task_shape() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-repair-extension-shape")
        .await
        .expect("scanner repair-extension task fixture should be created");

    let mut scheduler = TaskQueueScheduler::for_runtime(fixture.config.clone(), "rust-main");
    scheduler.enqueue(
        TaskQueueRecord::new("REPAIR_EXTENSION_book-1", 12, Some("series-1".to_string()))
            .with_simple_type("REPAIR_EXTENSION"),
    );

    let tasks_pool = connect_pool(fixture.paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for repair-extension task verification");
    let row = sqlx::query(
        "SELECT ID, CLASS, SIMPLE_TYPE, GROUP_ID, PAYLOAD FROM TASK WHERE ID = ? LIMIT 1",
    )
    .bind("REPAIR_EXTENSION_book-1")
    .fetch_one(&tasks_pool)
    .await
    .expect("repair-extension task row should be queryable");
    tasks_pool.close().await;

    assert_eq!(
        row.get::<String, _>("CLASS"),
        "org.gotson.komga.application.tasks.Task$RepairExtension"
    );
    assert_eq!(row.get::<String, _>("SIMPLE_TYPE"), "RepairExtension");
    assert_eq!(
        row.get::<Option<String>, _>("GROUP_ID"),
        Some("series-1".to_string())
    );
    assert_eq!(
        serde_json::from_str::<Value>(&row.get::<String, _>("PAYLOAD"))
            .expect("repair-extension task payload should be valid json"),
        json!({
            "bookId": "book-1",
            "priority": 12,
            "groupId": "series-1",
            "uniqueId": "REPAIR_EXTENSION_book-1"
        })
    );

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_persisted_rows_remain_visible_after_runtime_rebuild() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-restart-visibility")
        .await
        .expect("scanner restart fixture should be created");

    let scan_result = scan_root_folder(&fixture.library_root, &ScannerOptions::default())
        .expect("filesystem scan fixture should produce deterministic scanner output");
    assert_eq!(
        scan_result.series.len(),
        1,
        "fixture sanity: one series expected"
    );
    assert_eq!(
        scan_result.series[0].books.len(),
        1,
        "fixture sanity: one book expected"
    );

    let _initial_runtime = komga_server::app::build_router_with_config(&fixture.config);
    let before_restart = load_persistence_snapshot(&fixture.paths.main_db, "library-1").await;
    let task_before_restart = load_task_snapshot(&fixture.paths.tasks_db).await;
    let media_ready_before_restart = load_media_ready_count(&fixture.paths.main_db).await;
    let runtime_before_restart =
        TaskQueueScheduler::for_runtime(fixture.config.clone(), "rust-main");

    assert!(
        before_restart.series_rows >= 1
            && before_restart.book_rows >= 1
            && before_restart.sidecar_rows >= 2,
        "restart contract requires scanner-derived rows to exist before runtime rebuild; memory-only scanner state is invalid",
    );
    assert_eq!(
        task_before_restart.task_rows, 0,
        "restart contract now requires queue worker to have drained persisted scanner/analyze tasks before runtime rebuild",
    );
    assert!(
        media_ready_before_restart >= 1,
        "restart contract requires analyze side effects before runtime rebuild",
    );
    assert!(
        runtime_before_restart.count_by_simple_type().is_empty(),
        "runtime pre-restart queue should be empty after worker completion",
    );

    let _restarted_runtime = komga_server::app::build_router_with_config(&fixture.config);
    let after_restart = load_persistence_snapshot(&fixture.paths.main_db, "library-1").await;
    let task_after_restart = load_task_snapshot(&fixture.paths.tasks_db).await;
    let media_ready_after_restart = load_media_ready_count(&fixture.paths.main_db).await;
    let runtime_after_restart =
        TaskQueueScheduler::for_runtime(fixture.config.clone(), "rust-main");

    assert_eq!(
        after_restart, before_restart,
        "scanner persistence rows must survive runtime rebuild; losing rows indicates scan state stayed in memory",
    );
    assert_eq!(
        task_after_restart, task_before_restart,
        "scanner-triggered queue state should remain drained after runtime rebuild",
    );
    assert_eq!(
        media_ready_after_restart, media_ready_before_restart,
        "analyze side effects must remain persisted across runtime rebuild",
    );
    assert!(
        runtime_after_restart.count_by_simple_type().is_empty(),
        "runtime post-restart queue should stay empty after persisted completion",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_rescan_updates_existing_persisted_book_file_size_rows() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-rescan-updates")
        .await
        .expect("scanner rescan fixture should be created");

    let _initial_runtime = komga_server::app::build_router_with_config(&fixture.config);

    let book_path = fixture.library_root.join("Series-A").join("Book-001.cbz");
    let book_url = book_path.to_string_lossy().to_string();

    let initial_size = load_book_file_size(&fixture.paths.main_db, &book_url).await;
    assert!(
        initial_size > 0,
        "fixture sanity: scanner startup should persist initial BOOK file size before rescan",
    );

    let updated_payload = b"book-001-updated-payload-content";
    fs::write(&book_path, updated_payload)
        .expect("book payload rewrite should succeed for rescan update contract");

    let mut scheduler = TaskQueueScheduler::for_runtime(fixture.config.clone(), "rust-main");
    scheduler.enqueue(
        TaskQueueRecord::new("SCAN_LIBRARY:library-1", 900, Some("library-1".to_string()))
            .with_simple_type("SCAN_LIBRARY"),
    );
    scheduler
        .process_available(&fixture.config)
        .expect("scanner rescan task should process successfully");

    let updated_size = load_book_file_size(&fixture.paths.main_db, &book_url).await;
    assert_eq!(
        updated_size,
        updated_payload.len() as i64,
        "scanner persistence contract requires rescan to update existing BOOK file size rows instead of leaving stale values",
    );

    fixture.cleanup();
}

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

#[derive(Debug, Eq, PartialEq)]
struct PersistenceSnapshot {
    library_rows: i64,
    series_rows: i64,
    book_rows: i64,
    media_file_rows: i64,
    sidecar_rows: i64,
}

#[derive(Debug, Eq, PartialEq)]
struct TaskSnapshot {
    task_rows: i64,
}

struct ScannerPersistenceFixture {
    paths: persistence_contract_fixture::RuntimeDbPaths,
    library_root: PathBuf,
    config: RuntimeConfig,
}

impl ScannerPersistenceFixture {
    async fn new(case_id: &str) -> anyhow::Result<Self> {
        let paths = persistence_contract_fixture::new_runtime_db_paths(case_id)?;
        persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db).await?;
        persistence_contract_fixture::seed_tasks_db_from_flyway(&paths.tasks_db).await?;

        let library_root = create_scannable_library_root(&paths.config_dir)?;
        seed_library_row(&paths.main_db, "library-1", &library_root).await?;

        let config = runtime_config_for_paths(&paths);
        Ok(Self {
            paths,
            library_root,
            config,
        })
    }

    fn cleanup(self) {
        persistence_contract_fixture::cleanup(self.paths);
    }
}

fn create_scannable_library_root(config_dir: &Path) -> anyhow::Result<PathBuf> {
    let root = config_dir.join("library-root");
    let series_dir = root.join("Series-A");

    fs::create_dir_all(&series_dir)?;
    fs::write(series_dir.join("Book-001.cbz"), b"book-001")?;
    fs::write(series_dir.join("Book-001.xml"), b"<ComicInfo></ComicInfo>")?;
    fs::write(series_dir.join("ComicInfo.xml"), b"<ComicInfo></ComicInfo>")?;

    Ok(root)
}

fn write_scannable_cbz_fixture(path: &Path, page_bytes: &[u8]) -> anyhow::Result<()> {
    let file = File::create(path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o644);

    zip.start_file("page-1.png", options)?;
    zip.write_all(page_bytes)?;
    zip.finish()?;

    Ok(())
}

async fn seed_library_row(main_db: &Path, library_id: &str, root: &Path) -> anyhow::Result<()> {
    let pool = connect_pool(main_db, 1).await?;
    sqlx::query(
        "INSERT INTO LIBRARY (ID, NAME, ROOT) \
                 VALUES (?, ?, ?)",
    )
    .bind(library_id)
    .bind("Scanner Persistence Contract Library")
    .bind(root.to_string_lossy().to_string())
    .execute(&pool)
    .await?;
    pool.close().await;
    Ok(())
}

async fn update_series_file_last_modified(
    main_db: &Path,
    series_url: &str,
    file_last_modified: i64,
) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for series last-modified update");
    sqlx::query("UPDATE SERIES SET FILE_LAST_MODIFIED = ? WHERE URL = ?")
        .bind(file_last_modified)
        .bind(series_url)
        .execute(&pool)
        .await
        .expect("series last-modified should be updated for deleted-books scan contract");
    pool.close().await;
}

async fn update_library_oneshots_directory(
    main_db: &Path,
    library_id: &str,
    oneshots_directory: Option<&str>,
) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for library oneshots-directory update");
    sqlx::query("UPDATE LIBRARY SET ONESHOTS_DIRECTORY = ? WHERE ID = ?")
        .bind(oneshots_directory)
        .bind(library_id)
        .execute(&pool)
        .await
        .expect("library oneshots-directory should be updated for scanner oneshot contract");
    pool.close().await;
}

async fn update_active_book_url(main_db: &Path, from_book_url: &str, to_book_url: &str) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for active book url update");
    sqlx::query(
        "UPDATE BOOK SET URL = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP WHERE URL = ? AND DELETED_DATE IS NULL",
    )
    .bind(to_book_url)
    .bind(from_book_url)
    .execute(&pool)
    .await
    .expect("active book url should be updated for scanner oneshot contract");
    pool.close().await;
}

async fn load_active_series_id_for_book_url(main_db: &Path, book_url: &str) -> String {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for active series id lookup");
    let series_id =
        sqlx::query("SELECT SERIES_ID FROM BOOK WHERE URL = ? AND DELETED_DATE IS NULL LIMIT 1")
            .bind(book_url)
            .fetch_one(&pool)
            .await
            .expect("active book row should be queryable for series id lookup")
            .get::<String, _>("SERIES_ID");
    pool.close().await;
    series_id
}

async fn load_series_url_by_id(main_db: &Path, series_id: &str) -> String {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for series url lookup");
    let series_url =
        sqlx::query("SELECT URL FROM SERIES WHERE ID = ? AND DELETED_DATE IS NULL LIMIT 1")
            .bind(series_id)
            .fetch_one(&pool)
            .await
            .expect("active series row should be queryable for url lookup")
            .get::<String, _>("URL");
    pool.close().await;
    series_url
}

async fn load_active_series_count(main_db: &Path, library_id: &str) -> i64 {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for active series count lookup");
    let series_count = sqlx::query(
        "SELECT COUNT(*) AS COUNT FROM SERIES WHERE LIBRARY_ID = ? AND DELETED_DATE IS NULL",
    )
    .bind(library_id)
    .fetch_one(&pool)
    .await
    .expect("active series count should be queryable")
    .get::<i64, _>("COUNT");
    pool.close().await;
    series_count
}

fn runtime_config_for_paths(paths: &persistence_contract_fixture::RuntimeDbPaths) -> RuntimeConfig {
    let mut env = BTreeMap::new();
    env.insert(
        "KOMGA_CONFIG_DIR".to_string(),
        paths.config_dir.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_DATABASE_FILE".to_string(),
        paths.main_db.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_TASKS_DB_FILE".to_string(),
        paths.tasks_db.to_string_lossy().to_string(),
    );

    RuntimeConfig::resolve_with_env(&RuntimeCli::default(), &env)
        .expect("runtime config should resolve scanner persistence fixture paths")
}

async fn load_persistence_snapshot(main_db: &Path, library_id: &str) -> PersistenceSnapshot {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for scanner persistence inspection");

    let library_rows = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
                                    FROM LIBRARY \
                                    WHERE ID = ?",
    )
    .bind(library_id)
    .fetch_one(&pool)
    .await
    .expect("library row count should be queryable")
    .get::<i64, _>("COUNT");

    let series_rows = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
                                   FROM SERIES \
                                   WHERE LIBRARY_ID = ?",
    )
    .bind(library_id)
    .fetch_one(&pool)
    .await
    .expect("series row count should be queryable")
    .get::<i64, _>("COUNT");

    let book_rows = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
                                 FROM BOOK \
                                 WHERE LIBRARY_ID = ?",
    )
    .bind(library_id)
    .fetch_one(&pool)
    .await
    .expect("book row count should be queryable")
    .get::<i64, _>("COUNT");

    let media_file_rows = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
         FROM MEDIA_FILE \
         WHERE BOOK_ID IN (SELECT ID \
         FROM BOOK \
         WHERE LIBRARY_ID = ?)",
    )
    .bind(library_id)
    .fetch_one(&pool)
    .await
    .expect("media_file row count should be queryable")
    .get::<i64, _>("COUNT");

    let sidecar_rows = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
                                    FROM SIDECAR \
                                    WHERE LIBRARY_ID = ?",
    )
    .bind(library_id)
    .fetch_one(&pool)
    .await
    .expect("sidecar row count should be queryable")
    .get::<i64, _>("COUNT");

    pool.close().await;

    PersistenceSnapshot {
        library_rows,
        series_rows,
        book_rows,
        media_file_rows,
        sidecar_rows,
    }
}

async fn load_task_snapshot(tasks_db: &Path) -> TaskSnapshot {
    let pool = connect_pool(tasks_db, 1)
        .await
        .expect("sqlite pool should open for scanner task inspection");

    let task_rows = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
                                 FROM TASK",
    )
    .fetch_one(&pool)
    .await
    .expect("task row count should be queryable")
    .get::<i64, _>("COUNT");

    pool.close().await;

    TaskSnapshot { task_rows }
}

async fn load_media_ready_count(main_db: &Path) -> i64 {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for media status inspection");
    let count = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
                             FROM MEDIA \
                             WHERE STATUS = 'READY'",
    )
    .fetch_one(&pool)
    .await
    .expect("media READY count should be queryable")
    .get::<i64, _>("COUNT");
    pool.close().await;
    count
}

async fn load_book_file_size(main_db: &Path, book_url: &str) -> i64 {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for book file size inspection");
    let file_size = sqlx::query(
        "SELECT FILE_SIZE \
                                 FROM BOOK \
                                 WHERE URL = ? \
                                 LIMIT 1",
    )
    .bind(book_url)
    .fetch_one(&pool)
    .await
    .expect("book row should be queryable by URL for rescan contract")
    .get::<i64, _>("FILE_SIZE");
    pool.close().await;
    file_size
}

async fn load_media_page_file_size(main_db: &Path, book_url: &str) -> i64 {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for media page size inspection");
    let file_size = sqlx::query(
        "SELECT mp.FILE_SIZE \
         FROM MEDIA_PAGE mp \
         JOIN BOOK b ON b.ID = mp.BOOK_ID \
         WHERE b.URL = ? \
         ORDER BY mp.NUMBER ASC \
         LIMIT 1",
    )
    .bind(book_url)
    .fetch_one(&pool)
    .await
    .expect("media page row should be queryable by book url for deep-scan contract")
    .get::<i64, _>("FILE_SIZE");
    pool.close().await;
    file_size
}

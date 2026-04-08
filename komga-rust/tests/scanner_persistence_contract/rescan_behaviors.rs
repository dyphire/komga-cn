use super::support::*;
use super::*;

#[tokio::test]
async fn scanner_deep_scan_reanalyzes_changed_existing_books() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-deep-scan-reanalyzes")
        .await
        .expect("scanner deep-scan fixture should be created");

    let book_path = fixture.library_root.join("Series-A").join("Book-001.cbz");
    let expected_initial_page_size = write_scannable_cbz_fixture(&book_path, b"page-before")
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
        initial_page_size, expected_initial_page_size,
        "fixture sanity: initial scan must persist MEDIA_PAGE size from the archive entry",
    );

    tokio::time::sleep(Duration::from_millis(1100)).await;
    let expected_updated_page_size =
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
        updated_page_size, expected_updated_page_size,
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
    let expected_updated_page_size =
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
        updated_page_size, expected_updated_page_size,
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
    let expected_initial_page_size =
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
        initial_page_size, expected_initial_page_size,
        "fixture sanity: initial scan must persist the primary book page size",
    );

    tokio::time::sleep(Duration::from_millis(1100)).await;
    fs::remove_file(&deleted_book_path)
        .expect("secondary book should be removed to simulate deleted-books seriesChanged path");
    let expected_updated_page_size =
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
        updated_page_size, expected_updated_page_size,
        "regular scan must re-trigger analyze when deleted books force Kotlin's seriesChanged fallback even without a timestamp delta",
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

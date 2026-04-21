use super::support::*;
use super::*;
use std::sync::OnceLock;
use tokio::sync::Mutex;

fn scanner_runtime_sse_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[tokio::test]
async fn scanner_deep_scan_reanalyzes_changed_existing_books() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-deep-scan-reanalyzes")
        .await
        .expect("scanner deep-scan fixture should be created");

    let book_path = fixture.library_root.join("Series-A").join("Book-001.cbz");
    let expected_initial_page_size = write_scannable_cbz_fixture(&book_path, b"page-before")
        .expect("initial scannable cbz fixture should be written");
    let book_url = book_path.to_string_lossy().to_string();

    let runtime = runtime_task_context_from_config(&fixture.config);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(scan_library_task("library-1", 900, false));
    scheduler
        .process_available(&runtime)
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

    scheduler.enqueue(scan_library_task("library-1", 900, true));
    scheduler
        .process_available(&runtime)
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

    let runtime = runtime_task_context_from_config(&fixture.config);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(scan_library_task("library-1", 900, false));
    scheduler
        .process_available(&runtime)
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

    scheduler.enqueue(scan_library_task("library-1", 900, false));
    scheduler
        .process_available(&runtime)
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
async fn scanner_scan_splits_configured_oneshots_directories_into_per_book_oneshot_series() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-oneshots-directory-shape")
        .await
        .expect("scanner oneshots-directory fixture should be created");

    let nested_oneshots_dir = fixture.library_root.join("Series-A").join("_oneshots");
    fs::create_dir_all(&nested_oneshots_dir).expect("nested oneshots directory should be created");
    write_scannable_cbz_fixture(
        &nested_oneshots_dir.join("Nested-001.cbz"),
        MINIMAL_PNG_BYTES,
    )
    .expect("nested oneshot fixture should be written");
    write_scannable_cbz_fixture(
        &nested_oneshots_dir.join("Nested-002.cbz"),
        MINIMAL_PNG_BYTES,
    )
    .expect("second nested oneshot fixture should be written");

    let root_oneshots_dir = fixture.library_root.join("_oneshots");
    fs::create_dir_all(&root_oneshots_dir).expect("root oneshots directory should be created");
    write_scannable_cbz_fixture(&root_oneshots_dir.join("Root-001.cbz"), MINIMAL_PNG_BYTES)
        .expect("root oneshot fixture should be written");

    update_library_oneshots_directory(&fixture.paths.main_db, "library-1", Some("_oneshots")).await;

    let runtime = runtime_task_context_from_config(&fixture.config);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(scan_library_task("library-1", 900, false));
    scheduler
        .process_available(&runtime)
        .expect("scan should treat configured oneshots directories like Kotlin does");

    let pool = connect_test_pool(fixture.paths.main_db.as_path(), 1)
        .await
        .expect("sqlite pool should open for oneshots-directory scan contract");
    let series_rows = sqlx::query(
        "SELECT NAME, oneshot AS ONESHOT_FLAG \
         FROM SERIES \
         WHERE LIBRARY_ID = ? AND DELETED_DATE IS NULL \
         ORDER BY NAME ASC",
    )
    .bind("library-1")
    .fetch_all(&pool)
    .await
    .expect("active series rows should be queryable after oneshots-directory scan");
    let book_rows = sqlx::query(
        "SELECT NAME, oneshot AS ONESHOT_FLAG \
         FROM BOOK \
         WHERE LIBRARY_ID = ? AND DELETED_DATE IS NULL \
         ORDER BY NAME ASC",
    )
    .bind("library-1")
    .fetch_all(&pool)
    .await
    .expect("active book rows should be queryable after oneshots-directory scan");
    pool.close().await;

    let persisted_series = series_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("NAME"),
                row.get::<bool, _>("ONESHOT_FLAG"),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        persisted_series,
        vec![
            ("Nested-001".to_string(), true),
            ("Nested-002".to_string(), true),
            ("Root-001".to_string(), true),
            ("Series-A".to_string(), false),
        ],
        "configured `_oneshots` directories should be flattened into one-shot series while regular directories stay regular",
    );

    let persisted_books = book_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("NAME"),
                row.get::<bool, _>("ONESHOT_FLAG"),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        persisted_books,
        vec![
            ("Book-001".to_string(), false),
            ("Nested-001".to_string(), true),
            ("Nested-002".to_string(), true),
            ("Root-001".to_string(), true),
        ],
        "books discovered under configured `_oneshots` directories must persist with the oneshot flag set",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_regular_scan_reanalyzes_changed_books_when_series_changed() {
    for (fixture_name, deleted_sibling) in [
        ("scanner-persistence-series-changed-reanalyzes", false),
        ("scanner-persistence-deleted-books-reanalyzes", true),
    ] {
        let fixture = ScannerPersistenceFixture::new(fixture_name)
            .await
            .expect("scanner seriesChanged fixture should be created");

        let series_dir = fixture.library_root.join("Series-A");
        let primary_book_path = series_dir.join("Book-001.cbz");
        let primary_book_url = primary_book_path.to_string_lossy().to_string();
        let deleted_book_path = series_dir.join("Book-002.cbz");

        let expected_initial_page_size =
            write_scannable_cbz_fixture(&primary_book_path, b"page-before")
                .expect("primary scannable cbz fixture should be written");
        if deleted_sibling {
            write_scannable_cbz_fixture(&deleted_book_path, b"deleted-book-page")
                .expect("secondary scannable cbz fixture should be written");
        }

        let runtime = runtime_task_context_from_config(&fixture.config);
        let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
        scheduler.enqueue(scan_library_task("library-1", 900, false));
        scheduler
            .process_available(&runtime)
            .expect("initial scan should analyze seeded books successfully");

        if deleted_sibling {
            let initial_page_size =
                load_media_page_file_size(&fixture.paths.main_db, &primary_book_url).await;
            assert_eq!(
                initial_page_size, expected_initial_page_size,
                "fixture={fixture_name} should persist the primary book page size before rescan",
            );
        }

        tokio::time::sleep(Duration::from_millis(1100)).await;
        let expected_updated_page_size = write_scannable_cbz_fixture(
            &primary_book_path,
            if deleted_sibling {
                b"page-after-deleted-book-regular-scan"
            } else {
                b"page-after-regular-scan"
            },
        )
        .expect("updated scannable cbz fixture should be written");

        if deleted_sibling {
            fs::remove_file(&deleted_book_path).expect(
                "secondary book should be removed to simulate deleted-books seriesChanged path",
            );
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
        } else {
            fs::write(series_dir.join("scan-marker.tmp"), b"marker")
                .expect("book sidecar rewrite should bump series directory timestamp");
        }

        scheduler.enqueue(scan_library_task("library-1", 900, false));
        scheduler
            .process_available(&runtime)
            .expect("regular scan should complete successfully after seriesChanged trigger");

        let updated_page_size =
            load_media_page_file_size(&fixture.paths.main_db, &primary_book_url).await;
        assert_eq!(
            updated_page_size, expected_updated_page_size,
            "fixture={fixture_name} should re-trigger analyze when seriesChanged is true",
        );

        fixture.cleanup();
    }
}

#[tokio::test]
async fn scanner_rescan_reapplies_provider_numbering_after_kotlin_like_resort() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-rescan-provider-numbering")
        .await
        .expect("scanner rescan provider-numbering fixture should be created");

    fs::write(
        fixture.library_root.join("Series-A").join("Book-001.xml"),
        br#"<ComicInfo><Number>7</Number></ComicInfo>"#,
    )
    .expect("book sidecar with provider number should be written for rescan fixture");

    let runtime = runtime_task_context_from_config(&fixture.config);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(scan_library_task("library-1", 900, false));
    scheduler
        .process_available(&runtime)
        .expect("initial scan should apply provider numbering successfully");

    let initial_pool = connect_test_pool(fixture.paths.main_db.as_path(), 1)
        .await
        .expect("sqlite pool should open for provider numbering verification");
    let initial = sqlx::query(
        "SELECT b.NUMBER AS BOOK_NUMBER, bm.NUMBER AS METADATA_NUMBER, bm.NUMBER_SORT AS METADATA_NUMBER_SORT \
         FROM BOOK b \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         WHERE b.NAME = ? LIMIT 1",
    )
    .bind("Book-001")
    .fetch_one(&initial_pool)
    .await
    .expect("provider-numbered book row should be queryable after initial scan");
    assert_eq!(initial.get::<i64, _>("BOOK_NUMBER"), 1);
    assert_eq!(initial.get::<String, _>("METADATA_NUMBER"), "7");
    assert_eq!(initial.get::<f64, _>("METADATA_NUMBER_SORT"), 7.0_f64);
    initial_pool.close().await;

    scheduler.enqueue(scan_library_task("library-1", 900, false));
    scheduler
        .process_available(&runtime)
        .expect("rescan should preserve provider numbering after Kotlin-like resort");

    let verify_pool = connect_test_pool(fixture.paths.main_db.as_path(), 1)
        .await
        .expect("sqlite pool should reopen for provider numbering rescan verification");
    let rescanned = sqlx::query(
        "SELECT b.NUMBER AS BOOK_NUMBER, bm.NUMBER AS METADATA_NUMBER, bm.NUMBER_SORT AS METADATA_NUMBER_SORT \
         FROM BOOK b \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         WHERE b.NAME = ? LIMIT 1",
    )
    .bind("Book-001")
    .fetch_one(&verify_pool)
    .await
    .expect("provider-numbered book row should be queryable after rescan");
    verify_pool.close().await;

    assert_eq!(rescanned.get::<i64, _>("BOOK_NUMBER"), 1);
    assert_eq!(rescanned.get::<String, _>("METADATA_NUMBER"), "7");
    assert_eq!(rescanned.get::<f64, _>("METADATA_NUMBER_SORT"), 7.0_f64);

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_rescan_updates_existing_persisted_book_file_size_rows() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-rescan-updates")
        .await
        .expect("scanner rescan fixture should be created");

    process_scan_library_task(fixture.config.clone(), "library-1", 900, false)
        .expect("rescan update contract should seed initial persisted rows");

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

    let runtime = runtime_task_context_from_config(&fixture.config);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(scan_library_task("library-1", 900, false));
    scheduler
        .process_available(&runtime)
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
async fn scanner_rescan_recreates_missing_metadata_seed_rows() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-recreates-missing-metadata")
        .await
        .expect("scanner metadata-repair fixture should be created");

    let runtime = runtime_task_context_from_config(&fixture.config);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(scan_library_task("library-1", 900, false));
    scheduler
        .process_available(&runtime)
        .expect("initial scan should create persisted metadata seeds");

    let pool = connect_test_pool(fixture.paths.main_db.as_path(), 1)
        .await
        .expect("scanner metadata-repair db should open");
    sqlx::query("DELETE FROM SERIES_METADATA WHERE SERIES_ID IN (SELECT ID FROM SERIES WHERE LIBRARY_ID = ?)")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("series metadata rows should delete for repair regression");
    sqlx::query(
        "DELETE FROM BOOK_METADATA WHERE BOOK_ID IN (SELECT ID FROM BOOK WHERE LIBRARY_ID = ?)",
    )
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("book metadata rows should delete for repair regression");
    sqlx::query("DELETE FROM BOOK_METADATA_AGGREGATION WHERE SERIES_ID IN (SELECT ID FROM SERIES WHERE LIBRARY_ID = ?)")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("book metadata aggregation rows should delete for repair regression");
    pool.close().await;

    let broken_snapshot = load_persistence_snapshot(&fixture.paths.main_db, "library-1").await;
    assert_eq!(broken_snapshot.series_metadata_rows, 0);
    assert_eq!(broken_snapshot.book_metadata_rows, 0);
    assert_eq!(broken_snapshot.book_metadata_aggregation_rows, 0);

    scheduler.enqueue(scan_library_task("library-1", 900, false));
    scheduler
        .process_available(&runtime)
        .expect("rescan should recreate missing metadata seed rows");

    let repaired_snapshot = load_persistence_snapshot(&fixture.paths.main_db, "library-1").await;
    assert!(repaired_snapshot.series_metadata_rows >= 1);
    assert!(repaired_snapshot.book_metadata_rows >= 1);
    assert!(repaired_snapshot.book_metadata_aggregation_rows >= 1);

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_rescan_soft_deletes_missing_series_while_leaving_sidecar_rows_visible() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-soft-delete-missing-series")
        .await
        .expect("scanner soft-delete fixture should be created");

    let series_dir = fixture.library_root.join("Series-A");

    let runtime = runtime_task_context_from_config(&fixture.config);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(scan_library_task("library-1", 900, false));
    scheduler
        .process_available(&runtime)
        .expect("initial scan should persist the seeded series before missing-series rescan");

    let initial_snapshot = load_persistence_snapshot(&fixture.paths.main_db, "library-1").await;
    assert_eq!(
        initial_snapshot.sidecar_rows, 2,
        "fixture sanity: initial scan should persist both seeded sidecar rows before removal",
    );

    fs::remove_dir_all(&series_dir)
        .expect("series directory should be removable for missing-series soft-delete contract");

    scheduler.enqueue(scan_library_task("library-1", 900, false));
    scheduler
        .process_available(&runtime)
        .expect("rescan should soft-delete missing persisted series successfully");

    let pool = connect_test_pool(fixture.paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for missing-series soft-delete verification");
    let series_total = sqlx::query("SELECT COUNT(*) AS COUNT FROM SERIES WHERE LIBRARY_ID = ?")
        .bind("library-1")
        .fetch_one(&pool)
        .await
        .expect("series total count should be queryable after missing-series rescan")
        .get::<i64, _>("COUNT");
    let active_series = sqlx::query(
        "SELECT COUNT(*) AS COUNT FROM SERIES WHERE LIBRARY_ID = ? AND DELETED_DATE IS NULL",
    )
    .bind("library-1")
    .fetch_one(&pool)
    .await
    .expect("active series count should be queryable after missing-series rescan")
    .get::<i64, _>("COUNT");
    let deleted_series = sqlx::query(
        "SELECT COUNT(*) AS COUNT FROM SERIES WHERE LIBRARY_ID = ? AND DELETED_DATE IS NOT NULL",
    )
    .bind("library-1")
    .fetch_one(&pool)
    .await
    .expect("deleted series count should be queryable after missing-series rescan")
    .get::<i64, _>("COUNT");
    let book_total = sqlx::query("SELECT COUNT(*) AS COUNT FROM BOOK WHERE LIBRARY_ID = ?")
        .bind("library-1")
        .fetch_one(&pool)
        .await
        .expect("book total count should be queryable after missing-series rescan")
        .get::<i64, _>("COUNT");
    let active_books = sqlx::query(
        "SELECT COUNT(*) AS COUNT FROM BOOK WHERE LIBRARY_ID = ? AND DELETED_DATE IS NULL",
    )
    .bind("library-1")
    .fetch_one(&pool)
    .await
    .expect("active book count should be queryable after missing-series rescan")
    .get::<i64, _>("COUNT");
    let deleted_books = sqlx::query(
        "SELECT COUNT(*) AS COUNT FROM BOOK WHERE LIBRARY_ID = ? AND DELETED_DATE IS NOT NULL",
    )
    .bind("library-1")
    .fetch_one(&pool)
    .await
    .expect("deleted book count should be queryable after missing-series rescan")
    .get::<i64, _>("COUNT");
    let remaining_sidecars =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM SIDECAR WHERE LIBRARY_ID = ?")
            .bind("library-1")
            .fetch_one(&pool)
            .await
            .expect("sidecar count should be queryable after missing-series rescan")
            .get::<i64, _>("COUNT");
    pool.close().await;

    assert_eq!(series_total, 1);
    assert_eq!(active_series, 0);
    assert_eq!(deleted_series, 1);
    assert_eq!(book_total, 1);
    assert_eq!(active_books, 0);
    assert_eq!(deleted_books, 1);
    assert_eq!(
        remaining_sidecars, 2,
        "current scanner rescan semantics soft-delete missing series/book rows without deleting the persisted sidecar records in the same pass",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_runtime_sse_scan_events_follow_kotlin_lifecycle_order() {
    let _guard = scanner_runtime_sse_lock().lock().await;
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-sse-order")
        .await
        .expect("scanner SSE order fixture should be created");
    let book_url = fixture
        .library_root
        .join("Series-A")
        .join("Book-001.cbz")
        .to_string_lossy()
        .to_string();

    let initial_cursor = komga_application::runtime_sse::current_runtime_sse_event_cursor();
    let runtime = runtime_task_context_from_config(&fixture.config);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(scan_library_task("library-1", 900, false));
    scheduler
        .process_available(&runtime)
        .expect("initial scan should publish runtime SSE events successfully");

    let expected_series_id =
        load_active_series_id_for_book_url(&fixture.paths.main_db, &book_url).await;
    let expected_book_id = load_active_book_id_by_url(&fixture.paths.main_db, &book_url).await;

    let (_, initial_events) = komga_application::runtime_sse::pending_runtime_sse_events(
        initial_cursor,
        "scanner-contract-admin",
        true,
    );
    let initial_scan_names = initial_events
        .iter()
        .filter_map(|event| match event.name.as_str() {
            "SeriesAdded"
                if event
                    .payload
                    .get("seriesId")
                    .and_then(|value| value.as_str())
                    == Some(expected_series_id.as_str()) =>
            {
                Some(event.name.as_str())
            }
            "BookAdded"
                if event.payload.get("bookId").and_then(|value| value.as_str())
                    == Some(expected_book_id.as_str())
                    && event
                        .payload
                        .get("seriesId")
                        .and_then(|value| value.as_str())
                        == Some(expected_series_id.as_str()) =>
            {
                Some(event.name.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        initial_scan_names,
        vec!["SeriesAdded", "BookAdded"],
        "scanner runtime SSE should mirror Kotlin createSeries->addBooks ordering during initial discovery",
    );

    let rescan_cursor = komga_application::runtime_sse::current_runtime_sse_event_cursor();
    fs::remove_dir_all(fixture.library_root.join("Series-A"))
        .expect("series directory should be removable for scanner SSE order contract");
    scheduler.enqueue(scan_library_task("library-1", 900, false));
    scheduler
        .process_available(&runtime)
        .expect("missing-series rescan should publish runtime SSE events successfully");

    let (_, rescan_events) = komga_application::runtime_sse::pending_runtime_sse_events(
        rescan_cursor,
        "scanner-contract-admin",
        true,
    );
    let rescan_names = rescan_events
        .iter()
        .filter_map(|event| match event.name.as_str() {
            "BookChanged"
                if event.payload.get("bookId").and_then(|value| value.as_str())
                    == Some(expected_book_id.as_str())
                    && event
                        .payload
                        .get("seriesId")
                        .and_then(|value| value.as_str())
                        == Some(expected_series_id.as_str()) =>
            {
                Some(event.name.as_str())
            }
            "SeriesChanged"
                if event
                    .payload
                    .get("seriesId")
                    .and_then(|value| value.as_str())
                    == Some(expected_series_id.as_str()) =>
            {
                Some(event.name.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        rescan_names.starts_with(&["BookChanged", "SeriesChanged"]),
        "scanner runtime SSE should soft-delete books before series before any later maintenance follow-up events: {rescan_names:?}",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn scanner_runtime_sse_mixed_rescan_deletes_missing_items_before_adding_new_ones() {
    let _guard = scanner_runtime_sse_lock().lock().await;
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-sse-mixed-order")
        .await
        .expect("scanner SSE mixed-order fixture should be created");
    let original_book_url = fixture
        .library_root
        .join("Series-A")
        .join("Book-001.cbz")
        .to_string_lossy()
        .to_string();

    let runtime = runtime_task_context_from_config(&fixture.config);
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
    scheduler.enqueue(scan_library_task("library-1", 900, false));
    scheduler
        .process_available(&runtime)
        .expect("initial scan should seed scanner SSE mixed-order fixture successfully");

    let original_series_id =
        load_active_series_id_for_book_url(&fixture.paths.main_db, &original_book_url).await;
    let original_book_id =
        load_active_book_id_by_url(&fixture.paths.main_db, &original_book_url).await;

    let previous_series_dir = fixture.library_root.join("Series-A");
    let renamed_series_dir = fixture.library_root.join("Series-B");
    fs::rename(&previous_series_dir, &renamed_series_dir)
        .expect("series directory rename should succeed for mixed scanner SSE ordering");
    let renamed_book_url = renamed_series_dir
        .join("Book-001.cbz")
        .to_string_lossy()
        .to_string();

    let cursor = komga_application::runtime_sse::current_runtime_sse_event_cursor();
    scheduler.enqueue(scan_library_task("library-1", 900, false));
    scheduler
        .process_available(&runtime)
        .expect("mixed rescan should publish runtime SSE events successfully");

    let renamed_series_id =
        load_active_series_id_for_book_url(&fixture.paths.main_db, &renamed_book_url).await;
    let renamed_book_id =
        load_active_book_id_by_url(&fixture.paths.main_db, &renamed_book_url).await;

    let (_, events) = komga_application::runtime_sse::pending_runtime_sse_events(
        cursor,
        "scanner-contract-admin",
        true,
    );
    let mixed_rescan_names = events
        .iter()
        .filter_map(|event| match event.name.as_str() {
            "BookChanged"
                if event.payload.get("bookId").and_then(|value| value.as_str())
                    == Some(original_book_id.as_str())
                    && event
                        .payload
                        .get("seriesId")
                        .and_then(|value| value.as_str())
                        == Some(original_series_id.as_str()) =>
            {
                Some(event.name.as_str())
            }
            "SeriesChanged"
                if event
                    .payload
                    .get("seriesId")
                    .and_then(|value| value.as_str())
                    == Some(original_series_id.as_str()) =>
            {
                Some(event.name.as_str())
            }
            "SeriesAdded"
                if event
                    .payload
                    .get("seriesId")
                    .and_then(|value| value.as_str())
                    == Some(renamed_series_id.as_str()) =>
            {
                Some(event.name.as_str())
            }
            "BookAdded"
                if event.payload.get("bookId").and_then(|value| value.as_str())
                    == Some(renamed_book_id.as_str())
                    && event
                        .payload
                        .get("seriesId")
                        .and_then(|value| value.as_str())
                        == Some(renamed_series_id.as_str()) =>
            {
                Some(event.name.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        mixed_rescan_names.starts_with(&[
            "BookChanged",
            "SeriesChanged",
            "SeriesAdded",
            "BookAdded",
        ]),
        "scanner mixed rescans must begin with Kotlin's delete-missing phase before add/update phase ordering: {mixed_rescan_names:?}",
    );

    fixture.cleanup();
}

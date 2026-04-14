use super::support::*;
use super::*;
use komga_rust::application::task_processing::{
    DefaultLibraryTaskEmitter, LibraryScanPipeline, ScanOneLibrary,
};
use komga_rust::infrastructure::task_queue::SqliteFilesystemLibraryScanPipeline;

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

    process_scan_library_task(fixture.config.clone(), "library-1", 900, false)
        .expect("scan write-shape contract should persist scanner rows");

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
        snapshot.series_metadata_rows >= 1,
        "scanner contract requires scan output to persist SERIES_METADATA rows compatible with Kotlin readers",
    );
    assert!(
        snapshot.book_metadata_aggregation_rows >= 1,
        "scanner contract requires scan output to persist BOOK_METADATA_AGGREGATION rows compatible with Kotlin readers",
    );
    assert!(
        snapshot.book_rows >= 1,
        "scanner contract requires scan output to persist BOOK rows compatible with Kotlin readers",
    );
    assert!(
        snapshot.book_metadata_rows >= 1,
        "scanner contract requires scan output to persist BOOK_METADATA rows compatible with Kotlin readers",
    );
    assert!(
        snapshot.media_file_rows >= 1,
        "scanner contract requires scanned archive file names to persist in MEDIA_FILE",
    );
    assert!(
        snapshot.sidecar_rows >= 2,
        "scanner contract requires series/book sidecars to persist in SIDECAR with Kotlin-compatible shape",
    );

    let pool = connect_pool(fixture.paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for sidecar shape verification");
    let persisted_sidecars =
        sqlx::query("SELECT URL, PARENT_URL FROM SIDECAR WHERE LIBRARY_ID = ? ORDER BY URL ASC")
            .bind("library-1")
            .fetch_all(&pool)
            .await
            .expect("persisted sidecar rows should be queryable after scan");
    pool.close().await;

    let expected_series_sidecar = fixture
        .library_root
        .join("Series-A")
        .join("ComicInfo.xml")
        .to_string_lossy()
        .to_string();
    let expected_book_sidecar = fixture
        .library_root
        .join("Series-A")
        .join("Book-001.xml")
        .to_string_lossy()
        .to_string();
    let expected_series_parent = fixture
        .library_root
        .join("Series-A")
        .to_string_lossy()
        .to_string();
    let expected_book_parent = fixture
        .library_root
        .join("Series-A")
        .join("Book-001.cbz")
        .to_string_lossy()
        .to_string();
    let sidecar_pairs = persisted_sidecars
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("URL"),
                row.get::<String, _>("PARENT_URL"),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        sidecar_pairs,
        vec![
            (expected_book_sidecar, expected_book_parent),
            (expected_series_sidecar, expected_series_parent),
        ],
        "scanner contract requires the seeded book and series ComicInfo sidecars to persist with Kotlin-compatible URL and parent linkage",
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

    process_scan_library_task(fixture.config.clone(), "library-1", 900, false)
        .expect("task-emission contract should execute scan/analyze runtime flow");

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
async fn pipeline_run_public_seam_keeps_runtime_follow_ups_ahead_of_sidecar_refresh_tasks() {
    let fixture = ScannerPersistenceFixture::new("scanner-pipeline-public-run-seam")
        .await
        .expect("scanner pipeline fixture should be created");

    let pipeline = SqliteFilesystemLibraryScanPipeline::new(
        fixture.paths.main_db.clone(),
        DefaultLibraryTaskEmitter::default(),
    );
    let result = pipeline
        .run(ScanOneLibrary::new("library-1", false))
        .expect("public pipeline run seam should execute scan orchestration");

    let follow_up_types = result
        .follow_up_tasks
        .iter()
        .map(|task| task.simple_type.as_str())
        .collect::<Vec<_>>();
    let analyze_index = follow_up_types
        .iter()
        .position(|simple_type| *simple_type == "ANALYZE_BOOK")
        .expect("scan run should enqueue analyze follow-up tasks before sidecar refresh work");
    let duplicate_pages_index = follow_up_types
        .iter()
        .position(|simple_type| *simple_type == "FIND_DUPLICATE_PAGES_TO_DELETE")
        .expect("scan run should enqueue duplicate-page cleanup follow-up work");
    let renumber_refresh_index = follow_up_types
        .iter()
        .position(|simple_type| *simple_type == "REFRESH_BOOK_METADATA")
        .expect("scan run should enqueue runtime metadata refresh follow-up work");
    let sidecar_refresh_index = follow_up_types
        .iter()
        .position(|simple_type| *simple_type == "REFRESH_SERIES_METADATA")
        .expect("scan run should enqueue sidecar-driven series metadata refresh work");

    assert!(
        analyze_index < duplicate_pages_index,
        "runtime analyze follow-ups must stay ahead of later maintenance and sidecar work",
    );
    assert!(
        duplicate_pages_index < renumber_refresh_index,
        "runtime maintenance follow-ups must stay ahead of renumber-triggered metadata refresh work",
    );
    assert!(
        renumber_refresh_index < sidecar_refresh_index,
        "sidecar refresh tasks must remain appended after the runtime follow-up phase of the public pipeline seam",
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

    process_scan_library_task(fixture.config.clone(), "library-1", 900, false)
        .expect("restart contract should persist scanner rows before rebuild");
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
async fn scanner_runtime_assigns_kotlin_like_natural_book_numbers_for_unmanaged_series_names() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-natural-book-numbering")
        .await
        .expect("scanner natural-numbering fixture should be created");

    let natural_series_dir = fixture.library_root.join("Series-Natural");
    fs::create_dir_all(&natural_series_dir)
        .expect("natural-numbering series directory should be created");
    for file_name in [
        "[飛田漱][女神異聞錄Q 迷宮闇影 Side P3(第10集)][東立電子版].cbz",
        "[飛田漱][女神異聞錄Q 迷宮闇影 Side P3(第2集)][東立電子版].cbz",
        "[飛田漱][女神異聞錄Q 迷宮闇影 Side P3(第1集)][東立電子版].cbz",
    ] {
        write_scannable_cbz_fixture(&natural_series_dir.join(file_name), file_name.as_bytes())
            .expect("natural-numbering cbz fixture should be written");
    }

    process_scan_library_task(fixture.config.clone(), "library-1", 900, false)
        .expect("natural-numbering contract should persist scanned rows");

    let pool = connect_pool(fixture.paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for natural-numbering verification");
    let rows = sqlx::query(
        "SELECT b.NAME AS BOOK_NAME, b.NUMBER AS BOOK_NUMBER, bm.NUMBER AS METADATA_NUMBER, \
         bm.NUMBER_SORT AS METADATA_NUMBER_SORT \
         FROM BOOK b \
         JOIN SERIES s ON s.ID = b.SERIES_ID \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         WHERE s.NAME = ? AND b.DELETED_DATE IS NULL \
         ORDER BY b.NUMBER ASC, b.ID ASC",
    )
    .bind("Series-Natural")
    .fetch_all(&pool)
    .await
    .expect("natural-numbering rows should be queryable");
    pool.close().await;

    let ordered = rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("BOOK_NAME"),
                row.get::<i64, _>("BOOK_NUMBER"),
                row.get::<String, _>("METADATA_NUMBER"),
                row.get::<f64, _>("METADATA_NUMBER_SORT"),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        ordered,
        vec![
            (
                "[飛田漱][女神異聞錄Q 迷宮闇影 Side P3(第1集)][東立電子版]".to_string(),
                1,
                "1".to_string(),
                1.0_f64,
            ),
            (
                "[飛田漱][女神異聞錄Q 迷宮闇影 Side P3(第2集)][東立電子版]".to_string(),
                2,
                "2".to_string(),
                2.0_f64,
            ),
            (
                "[飛田漱][女神異聞錄Q 迷宮闇影 Side P3(第10集)][東立電子版]".to_string(),
                3,
                "3".to_string(),
                3.0_f64,
            ),
        ],
        "scanner should assign Kotlin-like natural ordering numbers when no provider metadata supplies book numbering",
    );

    fixture.cleanup();
}

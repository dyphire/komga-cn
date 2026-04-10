use super::support::*;
use super::*;

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

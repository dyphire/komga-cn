use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use komga_compat_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::config::{RuntimeCli, RuntimeConfig};
use komga_rust::persistence::sqlite::connect_pool;
use komga_rust::scanner::{ScannerOptions, scan_root_folder};
use komga_rust::search::{SearchEntityType, SearchIndexLifecycle};
use komga_rust::task_queue::{TaskQueueRecord, TaskQueueScheduler};
use sqlx::Row;

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

    let _app = komga_rust::app::build_router_with_config(&fixture.config);

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

    let _app = komga_rust::app::build_router_with_config(&fixture.config);

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

    let _initial_runtime = komga_rust::app::build_router_with_config(&fixture.config);
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

    let _restarted_runtime = komga_rust::app::build_router_with_config(&fixture.config);
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

    let _initial_runtime = komga_rust::app::build_router_with_config(&fixture.config);

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
    paths: persistence_contract_fixture::LegacyDbPaths,
    library_root: PathBuf,
    config: RuntimeConfig,
}

impl ScannerPersistenceFixture {
    async fn new(case_id: &str) -> anyhow::Result<Self> {
        let paths = persistence_contract_fixture::new_legacy_db_paths(case_id)?;
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

fn runtime_config_for_paths(paths: &persistence_contract_fixture::LegacyDbPaths) -> RuntimeConfig {
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

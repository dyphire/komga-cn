use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use komga_compat_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::config::{RuntimeCli, RuntimeConfig};
use komga_rust::persistence::sqlite::connect_pool;
use komga_rust::scanner::{ScannerOptions, scan_root_folder};
use komga_rust::task_queue::TaskQueueScheduler;
use sqlx::Row;

#[path = "support/persistence_contract_fixture.rs"]
mod persistence_contract_fixture;

#[test]
fn scanner_persistence_contract_target_is_registered() {
    assert_required_target_declared("tasks/scanner", "scanner_persistence_contract");
}

#[tokio::test]
async fn scanner_scan_output_is_persisted_into_kotlin_compatible_library_series_book_and_sidecar_tables() {
    let fixture = ScannerPersistenceFixture::new("scanner-persistence-write-shape")
        .await
        .expect("scanner persistence fixture should be created");

    let scan_result = scan_root_folder(&fixture.library_root, &ScannerOptions::default())
        .expect("filesystem scan fixture should produce deterministic scanner output");
    assert_eq!(scan_result.series.len(), 1, "fixture sanity: one series expected");
    assert_eq!(scan_result.series[0].books.len(), 1, "fixture sanity: one book expected");
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
    assert_eq!(scan_result.series.len(), 1, "fixture sanity: one series expected");
    assert_eq!(scan_result.series[0].books.len(), 1, "fixture sanity: one book expected");

    let _app = komga_rust::app::build_router_with_config(&fixture.config);

    let content_snapshot = load_persistence_snapshot(&fixture.paths.main_db, "library-1").await;
    assert!(
        content_snapshot.series_rows >= 1 && content_snapshot.book_rows >= 1,
        "task-emission contract requires scanner content rows before asserting runtime task flow",
    );

    let task_snapshot = load_task_snapshot(&fixture.paths.tasks_db).await;
    assert!(
        task_snapshot.scan_library_rows >= 1,
        "scanner-triggered runtime contract requires SCAN_LIBRARY tasks to persist in TASK rows",
    );
    assert!(
        task_snapshot.analyze_book_rows >= 1,
        "scanner-triggered runtime contract requires ANALYZE_BOOK tasks to persist in TASK rows",
    );

    let scheduler = TaskQueueScheduler::for_runtime(fixture.config.clone(), "rust-main");
    let runtime_counts = scheduler.count_by_simple_type();
    assert_eq!(
        runtime_counts.get("SCAN_LIBRARY").copied(),
        Some(task_snapshot.scan_library_rows as usize),
        "runtime task flow must read persisted SCAN_LIBRARY rows from tasks.sqlite",
    );
    assert_eq!(
        runtime_counts.get("ANALYZE_BOOK").copied(),
        Some(task_snapshot.analyze_book_rows as usize),
        "runtime task flow must read persisted ANALYZE_BOOK rows from tasks.sqlite",
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
    assert_eq!(scan_result.series.len(), 1, "fixture sanity: one series expected");
    assert_eq!(scan_result.series[0].books.len(), 1, "fixture sanity: one book expected");

    let _initial_runtime = komga_rust::app::build_router_with_config(&fixture.config);
    let before_restart = load_persistence_snapshot(&fixture.paths.main_db, "library-1").await;
    let task_before_restart = load_task_snapshot(&fixture.paths.tasks_db).await;
    let runtime_before_restart = TaskQueueScheduler::for_runtime(fixture.config.clone(), "rust-main");

    assert!(
        before_restart.series_rows >= 1
            && before_restart.book_rows >= 1
            && before_restart.sidecar_rows >= 2,
        "restart contract requires scanner-derived rows to exist before runtime rebuild; memory-only scanner state is invalid",
    );
    assert!(
        task_before_restart.scan_library_rows >= 1 && task_before_restart.analyze_book_rows >= 1,
        "restart contract requires persisted scanner/analyze tasks before runtime rebuild; runtime-only queue state is invalid",
    );
    assert_eq!(
        runtime_before_restart
            .count_by_simple_type()
            .get("SCAN_LIBRARY")
            .copied(),
        Some(task_before_restart.scan_library_rows as usize),
        "runtime pre-restart task flow should expose persisted SCAN_LIBRARY rows",
    );
    assert_eq!(
        runtime_before_restart
            .count_by_simple_type()
            .get("ANALYZE_BOOK")
            .copied(),
        Some(task_before_restart.analyze_book_rows as usize),
        "runtime pre-restart task flow should expose persisted ANALYZE_BOOK rows",
    );

    let _restarted_runtime = komga_rust::app::build_router_with_config(&fixture.config);
    let after_restart = load_persistence_snapshot(&fixture.paths.main_db, "library-1").await;
    let task_after_restart = load_task_snapshot(&fixture.paths.tasks_db).await;
    let runtime_after_restart = TaskQueueScheduler::for_runtime(fixture.config.clone(), "rust-main");

    assert_eq!(
        after_restart, before_restart,
        "scanner persistence rows must survive runtime rebuild; losing rows indicates scan state stayed in memory",
    );
    assert_eq!(
        task_after_restart, task_before_restart,
        "scanner-triggered TASK rows must survive runtime rebuild; losing task rows indicates queue state stayed in memory",
    );
    assert_eq!(
        runtime_after_restart
            .count_by_simple_type()
            .get("SCAN_LIBRARY")
            .copied(),
        Some(task_after_restart.scan_library_rows as usize),
        "runtime post-restart task flow should keep persisted SCAN_LIBRARY visibility",
    );
    assert_eq!(
        runtime_after_restart
            .count_by_simple_type()
            .get("ANALYZE_BOOK")
            .copied(),
        Some(task_after_restart.analyze_book_rows as usize),
        "runtime post-restart task flow should keep persisted ANALYZE_BOOK visibility",
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
    scan_library_rows: i64,
    analyze_book_rows: i64,
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
    sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
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

    let library_rows = sqlx::query("SELECT COUNT(*) AS COUNT FROM LIBRARY WHERE ID = ?")
        .bind(library_id)
        .fetch_one(&pool)
        .await
        .expect("library row count should be queryable")
        .get::<i64, _>("COUNT");

    let series_rows = sqlx::query("SELECT COUNT(*) AS COUNT FROM SERIES WHERE LIBRARY_ID = ?")
        .bind(library_id)
        .fetch_one(&pool)
        .await
        .expect("series row count should be queryable")
        .get::<i64, _>("COUNT");

    let book_rows = sqlx::query("SELECT COUNT(*) AS COUNT FROM BOOK WHERE LIBRARY_ID = ?")
        .bind(library_id)
        .fetch_one(&pool)
        .await
        .expect("book row count should be queryable")
        .get::<i64, _>("COUNT");

    let media_file_rows = sqlx::query(
        "SELECT COUNT(*) AS COUNT FROM MEDIA_FILE WHERE BOOK_ID IN (SELECT ID FROM BOOK WHERE LIBRARY_ID = ?)",
    )
    .bind(library_id)
    .fetch_one(&pool)
    .await
    .expect("media_file row count should be queryable")
    .get::<i64, _>("COUNT");

    let sidecar_rows = sqlx::query("SELECT COUNT(*) AS COUNT FROM SIDECAR WHERE LIBRARY_ID = ?")
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

    let task_rows = sqlx::query("SELECT COUNT(*) AS COUNT FROM TASK")
        .fetch_one(&pool)
        .await
        .expect("task row count should be queryable")
        .get::<i64, _>("COUNT");

    let scan_library_rows = sqlx::query(
        "SELECT COUNT(*) AS COUNT FROM TASK WHERE SIMPLE_TYPE = 'SCAN_LIBRARY'",
    )
    .fetch_one(&pool)
    .await
    .expect("scan task row count should be queryable")
    .get::<i64, _>("COUNT");

    let analyze_book_rows = sqlx::query(
        "SELECT COUNT(*) AS COUNT FROM TASK WHERE SIMPLE_TYPE = 'ANALYZE_BOOK'",
    )
    .fetch_one(&pool)
    .await
    .expect("analyze task row count should be queryable")
    .get::<i64, _>("COUNT");

    pool.close().await;

    TaskSnapshot {
        task_rows,
        scan_library_rows,
        analyze_book_rows,
    }
}

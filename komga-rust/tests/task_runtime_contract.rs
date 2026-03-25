use std::collections::BTreeMap;
use std::path::Path;

use komga_compat_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::config::{RuntimeCli, RuntimeConfig};
use komga_rust::persistence::sqlite::connect_pool;
use komga_rust::task_queue::{TaskQueueRecord, TaskQueueScheduler};
use sqlx::Row;

#[path = "support/persistence_contract_fixture.rs"]
mod persistence_contract_fixture;

#[test]
fn task_runtime_contract_target_is_registered() {
    assert_required_target_declared("tasks/scanner", "task_runtime_contract");
}

#[tokio::test]
async fn persists_queued_running_and_completed_task_state_in_tasks_sqlite() {
    let paths = new_task_runtime_fixture("task-runtime-lifecycle").await;
    let config = runtime_config_for_paths(&paths);
    let mut scheduler = TaskQueueScheduler::for_runtime(config, "rust-main");

    scheduler.enqueue(TaskQueueRecord::new(
        "SCAN_LIBRARY:library-1",
        100,
        Some("library-1".to_string()),
    ));

    let queued_rows = load_task_rows(&paths.tasks_db).await;
    assert_eq!(
        queued_rows.len(),
        1,
        "enqueue must persist queued task rows in tasks.sqlite"
    );
    assert_eq!(queued_rows[0].id, "SCAN_LIBRARY:library-1");
    assert_eq!(queued_rows[0].priority, 100);
    assert_eq!(queued_rows[0].group_id.as_deref(), Some("library-1"));
    assert_eq!(queued_rows[0].simple_type, "SCAN_LIBRARY");
    assert!(
        !queued_rows[0].class_name.trim().is_empty(),
        "queued task rows must keep Kotlin-compatible CLASS metadata in tasks.sqlite",
    );
    assert!(
        !queued_rows[0].payload.trim().is_empty(),
        "queued task rows must keep Kotlin-compatible PAYLOAD metadata in tasks.sqlite",
    );
    assert_eq!(queued_rows[0].owner, None);

    let running = scheduler
        .take_next()
        .expect("queued task should be claimable for running state");
    assert_eq!(running.id, "SCAN_LIBRARY:library-1");

    let running_rows = load_task_rows(&paths.tasks_db).await;
    assert_eq!(running_rows.len(), 1);
    assert_eq!(running_rows[0].id, "SCAN_LIBRARY:library-1");
    assert_eq!(running_rows[0].owner.as_deref(), Some("rust-main"));

    assert!(
        scheduler.complete("SCAN_LIBRARY:library-1"),
        "runtime should report completed task deletion from persisted task store",
    );

    let completed_rows = load_task_rows(&paths.tasks_db).await;
    assert!(
        completed_rows.is_empty(),
        "completed tasks must be removed from persisted TASK rows so restarts do not resurrect them",
    );

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn restart_rehydrates_persisted_task_queue_state_from_tasks_sqlite() {
    let paths = new_task_runtime_fixture("task-runtime-restart").await;
    let config = runtime_config_for_paths(&paths);
    let mut initial = TaskQueueScheduler::for_runtime(config.clone(), "rust-main");

    initial.enqueue(TaskQueueRecord::new(
        "SCAN_LIBRARY:queued-after-restart",
        50,
        Some("library-1".to_string()),
    ));
    initial.enqueue(TaskQueueRecord::new(
        "ANALYZE_BOOK:running-after-restart",
        75,
        Some("book-1".to_string()),
    ));
    let claimed = initial
        .take_next()
        .expect("highest-priority task should be claimed before restart");
    assert_eq!(claimed.id, "ANALYZE_BOOK:running-after-restart");

    drop(initial);

    let restarted = TaskQueueScheduler::for_runtime(config, "rust-main");
    let grouped = restarted.admin().read_grouped_by_owner();

    assert_eq!(
        grouped.get(&Some("rust-main".to_string())).map(Vec::len),
        Some(1),
        "restart must reload running tasks owned before shutdown from tasks.sqlite",
    );
    assert_eq!(
        grouped.get(&None).map(Vec::len),
        Some(1),
        "restart must reload queued unowned tasks from tasks.sqlite",
    );
    assert_eq!(
        restarted
            .count_by_simple_type()
            .get("SCAN_LIBRARY")
            .copied(),
        Some(1),
        "restart should preserve queued task simple-type counts from persisted store",
    );
    assert_eq!(
        restarted
            .count_by_simple_type()
            .get("ANALYZE_BOOK")
            .copied(),
        Some(1),
        "restart should preserve running task simple-type counts from persisted store",
    );

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn rejects_memory_queue_by_requiring_restart_visible_tasks_from_sqlite() {
    let paths = new_task_runtime_fixture("task-runtime-reject-memory-only").await;
    let config = runtime_config_for_paths(&paths);
    let mut initial = TaskQueueScheduler::for_runtime(config.clone(), "rust-main");

    initial.enqueue(TaskQueueRecord::new(
        "SCAN_LIBRARY:restart-proof",
        40,
        Some("library-1".to_string()),
    ));

    let persisted_before_restart = load_task_rows(&paths.tasks_db).await;
    assert_eq!(
        persisted_before_restart
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        vec!["SCAN_LIBRARY:restart-proof"],
        "contract rejects memory-only queues by requiring task rows to exist in tasks.sqlite before restart",
    );

    drop(initial);

    let restarted = TaskQueueScheduler::for_runtime(config, "rust-main");
    assert_eq!(
        restarted
            .count_by_simple_type()
            .get("SCAN_LIBRARY")
            .copied(),
        Some(1),
        "rejects_memory_queue: tasks lost after restart indicate runtime kept task state only in memory instead of tasks.sqlite",
    );

    persistence_contract_fixture::cleanup(paths);
}

#[derive(Debug, Eq, PartialEq)]
struct PersistedTaskRow {
    id: String,
    priority: i64,
    group_id: Option<String>,
    class_name: String,
    simple_type: String,
    payload: String,
    owner: Option<String>,
}

async fn new_task_runtime_fixture(case_id: &str) -> persistence_contract_fixture::LegacyDbPaths {
    let paths = persistence_contract_fixture::new_legacy_db_paths(case_id)
        .expect("task runtime db paths should be created");
    persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db)
        .await
        .expect("main db flyway fixture should be created");
    persistence_contract_fixture::seed_tasks_db_from_flyway(&paths.tasks_db)
        .await
        .expect("tasks db flyway fixture should be created");
    paths
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
        .expect("runtime config should resolve persistence fixture paths")
}

async fn load_task_rows(path: &Path) -> Vec<PersistedTaskRow> {
    let pool = connect_pool(path, 1)
        .await
        .expect("sqlite pool should open for task inspection");
    let rows = sqlx::query(
        "SELECT ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER
         FROM TASK
         ORDER BY PRIORITY DESC, ID ASC",
    )
    .fetch_all(&pool)
    .await
    .expect("task rows should be queryable");

    let result = rows
        .into_iter()
        .map(|row| PersistedTaskRow {
            id: row.get::<String, _>("ID"),
            priority: row.get::<i64, _>("PRIORITY"),
            group_id: row.get::<Option<String>, _>("GROUP_ID"),
            class_name: row.get::<String, _>("CLASS"),
            simple_type: row.get::<String, _>("SIMPLE_TYPE"),
            payload: row.get::<String, _>("PAYLOAD"),
            owner: row.get::<Option<String>, _>("OWNER"),
        })
        .collect::<Vec<_>>();

    pool.close().await;
    result
}

use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_infrastructure::sqlite::write_models::server_settings::ServerSettingsStore;
use komga_infrastructure::sqlite::connect_persistence_context;
use komga_infrastructure::sqlite::connect_pool;

mod support;

use support::persistence_contract_fixture;

#[test]
fn settings_persistence_contract_target_is_registered() {
    assert_required_target_declared("settings", "settings_persistence_contract");
}

#[tokio::test]
async fn server_settings_rows_persist_in_flyway_seeded_main_db() {
    let paths = persistence_contract_fixture::new_runtime_db_paths("settings-persistence-core")
        .expect("settings persistence db paths should be created");
    persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db)
        .await
        .expect("main db flyway fixture should be created");
    persistence_contract_fixture::seed_tasks_db_from_flyway(&paths.tasks_db)
        .await
        .expect("tasks db flyway fixture should be created");

    let pool = connect_pool(&paths.main_db, 1)
        .await
        .expect("main sqlite pool should open");

    sqlx::query(
        "INSERT \
                 OR REPLACE INTO SERVER_SETTINGS (KEY, VALUE) \
                 VALUES (?, ?)",
    )
    .bind("TASK_POOL_SIZE")
    .bind("4")
    .execute(&pool)
    .await
    .expect("server settings row should upsert");

    let value: String = sqlx::query_scalar(
        "SELECT VALUE \
                                            FROM SERVER_SETTINGS \
                                            WHERE KEY = ?",
    )
    .bind("TASK_POOL_SIZE")
    .fetch_one(&pool)
    .await
    .expect("server settings row should be readable");
    assert_eq!(value, "4");

    pool.close().await;
    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn server_settings_store_round_trips_through_context_backed_path() {
    let paths = persistence_contract_fixture::new_runtime_db_paths("settings-persistence-context")
        .expect("settings persistence db paths should be created");
    persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db)
        .await
        .expect("main db flyway fixture should be created");
    persistence_contract_fixture::seed_tasks_db_from_flyway(&paths.tasks_db)
        .await
        .expect("tasks db flyway fixture should be created");

    let context = connect_persistence_context(&paths.main_db, 1)
        .await
        .expect("main sqlite persistence context should open");
    let store = ServerSettingsStore::from_context(context.clone());

    store
        .apply_changes(&[
            ("TASK_POOL_SIZE".to_string(), Some("4".to_string())),
            ("KOBO_PORT".to_string(), None),
        ])
        .await
        .expect("settings changes should persist via context-backed path");

    let persisted = store
        .load_map()
        .await
        .expect("settings map should load via context-backed path");
    assert_eq!(
        persisted.get("TASK_POOL_SIZE"),
        Some(&Some("4".to_string()))
    );
    assert_eq!(persisted.get("KOBO_PORT"), None);

    context.pool().close().await;
    persistence_contract_fixture::cleanup(paths);
}

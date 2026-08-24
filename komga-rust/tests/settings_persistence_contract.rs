use crate::support::sqlite::connect_test_pool;
use komga_application::operational::{ServerSettingChange, ServerSettingsPort};
use komga_infrastructure::operational::ServerSettingsStore;
use komga_infrastructure::persistence::connect_main_write_context;

mod support;

use support::fixture::TestDbFixture;

#[tokio::test]
async fn server_settings_rows_persist_in_flyway_seeded_main_db() {
    let ctx = TestDbFixture::new("settings-persistence-core").await;

    let pool = connect_test_pool(&ctx.paths().main_db, 1)
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
}

#[tokio::test]
async fn server_settings_store_round_trips_through_context_backed_path() {
    let ctx = TestDbFixture::new("settings-persistence-context").await;

    let context = connect_main_write_context(&ctx.paths().main_db)
        .await
        .expect("main sqlite write context should open");
    let store = ServerSettingsStore::from_context(context.clone());

    store
        .apply_changes(&[
            ServerSettingChange::set("TASK_POOL_SIZE", "4"),
            ServerSettingChange::delete("KOBO_PORT"),
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
}

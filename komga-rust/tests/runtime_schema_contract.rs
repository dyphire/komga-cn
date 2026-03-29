use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::infrastructure::SqlitePersistenceContext;
use komga_rust::infrastructure::sqlite::{
    connect_persistence_context, connect_pool, connect_tasks_pool, setup,
};
use sqlx::Row;

#[path = "support/persistence_contract_fixture.rs"]
mod persistence_contract_fixture;

#[test]
fn runtime_schema_contract_target_is_registered() {
    assert_required_target_declared("schema/bootstrap", "runtime_schema_contract");
}

#[tokio::test]
async fn bootstrap_fresh_install() {
    let paths = persistence_contract_fixture::new_runtime_db_paths("runtime-schema-fresh-install")
        .expect("fresh install db paths should be created");
    let oracle_paths =
        persistence_contract_fixture::new_runtime_db_paths("runtime-schema-fresh-install-oracle")
            .expect("oracle db paths should be created");

    let main_pool = connect_pool(&paths.main_db, 1)
        .await
        .expect("fresh main sqlite db should open");
    setup::bootstrap_pool(&main_pool)
        .await
        .expect("fresh install main db should be accepted");

    let tasks_pool = connect_pool(&paths.tasks_db, 1)
        .await
        .expect("fresh tasks sqlite db should open");
    setup::bootstrap_tasks_pool(&tasks_pool)
        .await
        .expect("fresh install tasks db should be bootstrapped");

    assert!(
        paths.main_db.exists(),
        "fresh install bootstrap should create Kotlin-compatible main sqlite file at {}",
        paths.main_db.display(),
    );
    assert!(
        paths.tasks_db.exists(),
        "fresh install bootstrap should create Kotlin-compatible tasks sqlite file at {}",
        paths.tasks_db.display(),
    );

    persistence_contract_fixture::seed_main_db_from_flyway(&oracle_paths.main_db)
        .await
        .expect("main db flyway oracle should be created");
    persistence_contract_fixture::seed_tasks_db_from_flyway(&oracle_paths.tasks_db)
        .await
        .expect("tasks db flyway oracle should be created");

    let fresh_main_inventory = schema_inventory(&paths.main_db)
        .await
        .expect("fresh main db schema inventory should load");
    let oracle_main_inventory = schema_inventory(&oracle_paths.main_db)
        .await
        .expect("oracle main db schema inventory should load");
    assert_eq!(
        fresh_main_inventory, oracle_main_inventory,
        "fresh install main db must match Kotlin/Flyway sqlite schema inventory exactly",
    );

    let fresh_tasks_inventory = schema_inventory(&paths.tasks_db)
        .await
        .expect("fresh tasks db schema inventory should load");
    let oracle_tasks_inventory = schema_inventory(&oracle_paths.tasks_db)
        .await
        .expect("oracle tasks db schema inventory should load");
    assert_eq!(
        fresh_tasks_inventory, oracle_tasks_inventory,
        "fresh install tasks db must match Kotlin/Flyway sqlite schema inventory exactly",
    );

    main_pool.close().await;
    tasks_pool.close().await;
    persistence_contract_fixture::cleanup(paths);
    persistence_contract_fixture::cleanup(oracle_paths);
}

#[tokio::test]
async fn open_current_schema_db() {
    let paths = persistence_contract_fixture::new_runtime_db_paths("runtime-schema-current")
        .expect("current schema db paths should be created");
    persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db)
        .await
        .expect("main db flyway fixture should be created");
    persistence_contract_fixture::seed_tasks_db_from_flyway(&paths.tasks_db)
        .await
        .expect("tasks db flyway fixture should be created");

    let main_before = schema_inventory(&paths.main_db)
        .await
        .expect("main db schema inventory should load before bootstrap");
    let tasks_before = schema_inventory(&paths.tasks_db)
        .await
        .expect("tasks db schema inventory should load before bootstrap");

    let main_pool = connect_pool(&paths.main_db, 1)
        .await
        .expect("current main sqlite db should open");
    setup::bootstrap_pool(&main_pool)
        .await
        .expect("current main sqlite db should pass schema gate without rewrite");

    let tasks_pool = connect_pool(&paths.tasks_db, 1)
        .await
        .expect("current tasks sqlite db should open");
    setup::bootstrap_tasks_pool(&tasks_pool)
        .await
        .expect("current tasks sqlite db should pass schema gate without rewrite");

    let main_after = schema_inventory(&paths.main_db)
        .await
        .expect("main db schema inventory should load after bootstrap");
    let tasks_after = schema_inventory(&paths.tasks_db)
        .await
        .expect("tasks db schema inventory should load after bootstrap");

    assert_eq!(
        main_after, main_before,
        "bootstrap must not mutate existing Kotlin-compatible main sqlite schema",
    );
    assert_eq!(
        tasks_after, tasks_before,
        "bootstrap must not mutate existing Kotlin-compatible tasks sqlite schema",
    );

    main_pool.close().await;
    tasks_pool.close().await;
    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn reject_outdated_schema() {
    let paths = persistence_contract_fixture::new_runtime_db_paths("runtime-schema-outdated")
        .expect("outdated db paths should be created");

    let pool = connect_pool(&paths.main_db, 1)
        .await
        .expect("sqlite pool should open");
    let persistence = SqlitePersistenceContext::new(pool.clone());

    persistence
        .pool_connection()
        .execute("CREATE TABLE IF NOT EXISTS libraries (id TEXT PRIMARY KEY)")
        .await
        .expect("schema fixture should be created");

    let error = setup::bootstrap_pool(&pool)
        .await
        .expect_err("outdated schema should be rejected");
    let message = error.to_string();

    assert!(
        message.contains("unsupported SQLite schema detected in table `announcements_read`"),
        "schema gate should identify missing table in deterministic text, got: {message}",
    );
    assert!(
        message.contains(
            "run Kotlin Komga once to upgrade the database schema before starting Rust runtime"
        ),
        "schema gate should provide explicit operator guidance, got: {message}",
    );

    pool.close().await;
    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn sqlite_connect_layer_bootstraps_main_and_tasks_databases() {
    let paths = persistence_contract_fixture::new_runtime_db_paths("runtime-schema-connect-layer")
        .expect("connect-layer db paths should be created");

    let main_context = connect_persistence_context(&paths.main_db, 1)
        .await
        .expect("main context connect should bootstrap main sqlite schema");
    let tasks_pool = connect_tasks_pool(&paths.tasks_db, 1)
        .await
        .expect("tasks connect should bootstrap tasks sqlite schema");

    let main_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) \
         FROM sqlite_master \
         WHERE type = 'table' \
         AND LOWER(name) = 'server_settings'",
    )
    .fetch_one(main_context.pool())
    .await
    .expect("main schema probe should succeed");
    assert_eq!(
        main_count, 1,
        "main connect-layer bootstrap must provision Kotlin-compatible SERVER_SETTINGS table",
    );

    let tasks_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) \
         FROM sqlite_master \
         WHERE type = 'table' \
         AND LOWER(name) = 'task'",
    )
    .fetch_one(&tasks_pool)
    .await
    .expect("tasks schema probe should succeed");
    assert_eq!(
        tasks_count, 1,
        "tasks connect-layer bootstrap must provision Kotlin-compatible TASK table",
    );

    main_context.pool().close().await;
    tasks_pool.close().await;
    persistence_contract_fixture::cleanup(paths);
}

async fn schema_inventory(
    path: &std::path::Path,
) -> anyhow::Result<Vec<(String, String, String, String)>> {
    let pool = connect_pool(path, 1).await?;
    let rows = sqlx::query(
        "SELECT type, name, tbl_name, COALESCE(sql, '') AS sql \
         FROM sqlite_master \
         WHERE type IN ('table', 'index', 'trigger', 'view') \
         AND name NOT LIKE 'sqlite_%' \
         ORDER BY type, name",
    )
    .fetch_all(&pool)
    .await?
    .into_iter()
    .map(|row| {
        (
            row.get::<String, _>("type"),
            row.get::<String, _>("name"),
            row.get::<String, _>("tbl_name"),
            normalize_schema_sql(&row.get::<String, _>("sql")),
        )
    })
    .collect();

    pool.close().await;
    Ok(rows)
}

fn normalize_schema_sql(sql: &str) -> String {
    sql.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(" ,", ",")
        .replace(" )", ")")
        .replace("( ", "(")
}

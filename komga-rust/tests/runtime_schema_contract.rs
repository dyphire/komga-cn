use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_infrastructure::context::SqlitePersistenceContext;
use komga_infrastructure::sqlite::{
    connect_persistence_context, connect_pool, connect_tasks_pool, setup,
};
use sqlx::Row;
use std::path::Path;

mod support;

use support::persistence_contract_fixture;

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

    let fresh_main_inventory = comparable_schema_inventory(&paths.main_db)
        .await
        .expect("fresh main db schema inventory should load");
    let oracle_main_inventory = comparable_schema_inventory(&oracle_paths.main_db)
        .await
        .expect("oracle main db schema inventory should load");
    assert_eq!(
        fresh_main_inventory, oracle_main_inventory,
        "fresh install main db must match Kotlin/Flyway sqlite schema inventory exactly",
    );

    let fresh_tasks_inventory = comparable_schema_inventory(&paths.tasks_db)
        .await
        .expect("fresh tasks db schema inventory should load");
    let oracle_tasks_inventory = comparable_schema_inventory(&oracle_paths.tasks_db)
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

    let main_before = comparable_schema_inventory(&paths.main_db)
        .await
        .expect("main db schema inventory should load before bootstrap");
    let tasks_before = comparable_schema_inventory(&paths.tasks_db)
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

    let main_after = comparable_schema_inventory(&paths.main_db)
        .await
        .expect("main db schema inventory should load after bootstrap");
    let tasks_after = comparable_schema_inventory(&paths.tasks_db)
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
async fn repair_historyless_tasks_schema_to_latest_inventory() {
    let paths = persistence_contract_fixture::new_runtime_db_paths(
        "runtime-schema-repair-historyless-tasks",
    )
    .expect("historyless tasks db paths should be created");
    let oracle_paths = persistence_contract_fixture::new_runtime_db_paths(
        "runtime-schema-repair-historyless-tasks-oracle",
    )
    .expect("oracle db paths should be created");

    let tasks_pool = connect_pool(&paths.tasks_db, 1)
        .await
        .expect("historyless tasks sqlite db should open");
    sqlx::query(
        "CREATE TABLE TASK (\
             ID varchar NOT NULL PRIMARY KEY, \
             PRIORITY int NOT NULL, \
             GROUP_ID varchar NULL, \
             CLASS varchar NOT NULL, \
             SIMPLE_TYPE varchar NOT NULL, \
             PAYLOAD varchar NOT NULL, \
             OWNER varchar NULL, \
             CREATED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP, \
             LAST_MODIFIED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP\
         )",
    )
    .execute(&tasks_pool)
    .await
    .expect("partial historyless tasks schema should be created");

    setup::bootstrap_tasks_pool(&tasks_pool)
        .await
        .expect("historyless tasks sqlite db should be repaired by rust runtime");

    persistence_contract_fixture::seed_tasks_db_from_flyway(&oracle_paths.tasks_db)
        .await
        .expect("tasks db flyway oracle should be created");

    let repaired_inventory = comparable_schema_inventory(&paths.tasks_db)
        .await
        .expect("repaired tasks db schema inventory should load");
    let oracle_inventory = comparable_schema_inventory(&oracle_paths.tasks_db)
        .await
        .expect("oracle tasks db schema inventory should load");

    assert_eq!(
        repaired_inventory, oracle_inventory,
        "historyless tasks sqlite db should be repaired to Kotlin/Flyway latest schema inventory",
    );

    tasks_pool.close().await;
    persistence_contract_fixture::cleanup(paths);
    persistence_contract_fixture::cleanup(oracle_paths);
}

#[tokio::test]
async fn repair_historyless_main_schema_with_missing_index_to_latest_inventory() {
    let paths = persistence_contract_fixture::new_runtime_db_paths(
        "runtime-schema-repair-historyless-main-missing-index",
    )
    .expect("historyless main db paths should be created");
    let oracle_paths = persistence_contract_fixture::new_runtime_db_paths(
        "runtime-schema-repair-historyless-main-missing-index-oracle",
    )
    .expect("oracle db paths should be created");

    persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db)
        .await
        .expect("historyless main sqlite db should be created from Rust-owned migrations");

    let main_pool = connect_pool(&paths.main_db, 1)
        .await
        .expect("historyless main sqlite db should open");
    sqlx::query("DROP INDEX IF EXISTS idx__series_metadata__title")
        .execute(&main_pool)
        .await
        .expect("current historyless main schema fixture should drop title index");

    setup::bootstrap_pool(&main_pool)
        .await
        .expect("historyless main sqlite db with missing index should be repaired by rust runtime");

    persistence_contract_fixture::seed_main_db_from_flyway(&oracle_paths.main_db)
        .await
        .expect("main db flyway oracle should be created");

    let repaired_inventory = comparable_schema_inventory(&paths.main_db)
        .await
        .expect("repaired main db schema inventory should load");
    let oracle_inventory = comparable_schema_inventory(&oracle_paths.main_db)
        .await
        .expect("oracle main db schema inventory should load");

    assert_eq!(
        repaired_inventory, oracle_inventory,
        "historyless main sqlite db with a missing index should be repaired to Kotlin/Flyway latest schema inventory",
    );

    main_pool.close().await;
    persistence_contract_fixture::cleanup(paths);
    persistence_contract_fixture::cleanup(oracle_paths);
}

#[tokio::test]
async fn repair_historyless_main_schema_with_missing_trigger_to_latest_inventory() {
    let paths = persistence_contract_fixture::new_runtime_db_paths(
        "runtime-schema-repair-historyless-main-missing-trigger",
    )
    .expect("historyless main db paths should be created");
    let oracle_paths = persistence_contract_fixture::new_runtime_db_paths(
        "runtime-schema-repair-historyless-main-missing-trigger-oracle",
    )
    .expect("oracle db paths should be created");

    persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db)
        .await
        .expect("historyless main sqlite db should be created from Rust-owned migrations");

    let main_pool = connect_pool(&paths.main_db, 1)
        .await
        .expect("historyless main sqlite db should open");
    sqlx::query("DROP TRIGGER IF EXISTS series_metadata__after_update")
        .execute(&main_pool)
        .await
        .expect("current historyless main schema fixture should drop trailing trigger");

    setup::bootstrap_pool(&main_pool).await.expect(
        "historyless main sqlite db with missing trigger should be repaired by rust runtime",
    );

    persistence_contract_fixture::seed_main_db_from_flyway(&oracle_paths.main_db)
        .await
        .expect("main db flyway oracle should be created");

    let repaired_inventory = comparable_schema_inventory(&paths.main_db)
        .await
        .expect("repaired main db schema inventory should load");
    let oracle_inventory = comparable_schema_inventory(&oracle_paths.main_db)
        .await
        .expect("oracle main db schema inventory should load");

    assert_eq!(
        repaired_inventory, oracle_inventory,
        "historyless main sqlite db with a missing trigger should be repaired to Kotlin/Flyway latest schema inventory",
    );

    main_pool.close().await;
    persistence_contract_fixture::cleanup(paths);
    persistence_contract_fixture::cleanup(oracle_paths);
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
        message.contains(
            "unsupported SQLite schema detected without Flyway migration history or current Kotlin-compatible schema"
        ),
        "schema gate should identify missing table in deterministic text, got: {message}",
    );
    assert!(
        message.contains("database schema is damaged, incomplete, or unrecognized and cannot be migrated automatically by the Rust runtime"),
        "schema gate should provide explicit operator guidance, got: {message}",
    );

    pool.close().await;
    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn migrate_legacy_main_schema_to_latest_inventory() {
    let paths = persistence_contract_fixture::new_runtime_db_paths("runtime-schema-migrate-legacy")
        .expect("legacy db paths should be created");
    let oracle_paths =
        persistence_contract_fixture::new_runtime_db_paths("runtime-schema-migrate-legacy-oracle")
            .expect("oracle db paths should be created");

    apply_sql_file(
        &paths.main_db,
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(
                "crates/infrastructure/sqlx-migrations/main/V20200706141854__initial_migration.sql",
            )
            .as_path(),
    )
    .await
    .expect("legacy main schema fixture should be created");
    seed_flyway_history(&paths.main_db, &["20200706141854"])
        .await
        .expect("legacy main db should carry flyway history");

    let main_pool = connect_pool(&paths.main_db, 1)
        .await
        .expect("legacy main sqlite db should open");
    setup::bootstrap_pool(&main_pool)
        .await
        .expect("legacy main sqlite db should be migrated by rust runtime");

    persistence_contract_fixture::seed_main_db_from_flyway(&oracle_paths.main_db)
        .await
        .expect("main db flyway oracle should be created");

    let migrated_inventory = comparable_schema_inventory(&paths.main_db)
        .await
        .expect("migrated main db schema inventory should load");
    let oracle_inventory = comparable_schema_inventory(&oracle_paths.main_db)
        .await
        .expect("oracle main db schema inventory should load");

    assert_eq!(
        migrated_inventory, oracle_inventory,
        "legacy main sqlite db should migrate to Kotlin/Flyway latest schema inventory",
    );

    main_pool.close().await;
    persistence_contract_fixture::cleanup(paths);
    persistence_contract_fixture::cleanup(oracle_paths);
}

#[tokio::test]
async fn migrate_legacy_main_schema_without_flyway_history_to_latest_inventory() {
    let paths = persistence_contract_fixture::new_runtime_db_paths(
        "runtime-schema-migrate-legacy-without-flyway-history",
    )
    .expect("legacy db paths should be created");
    let oracle_paths = persistence_contract_fixture::new_runtime_db_paths(
        "runtime-schema-migrate-legacy-without-flyway-history-oracle",
    )
    .expect("oracle db paths should be created");

    apply_sql_file(
        &paths.main_db,
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(
                "crates/infrastructure/sqlx-migrations/main/V20200706141854__initial_migration.sql",
            )
            .as_path(),
    )
    .await
    .expect("legacy main schema fixture without flyway history should be created");

    let main_pool = connect_pool(&paths.main_db, 1)
        .await
        .expect("legacy main sqlite db should open");
    setup::bootstrap_pool(&main_pool)
        .await
        .expect("legacy main sqlite db without flyway history should be migrated by rust runtime");

    persistence_contract_fixture::seed_main_db_from_flyway(&oracle_paths.main_db)
        .await
        .expect("main db flyway oracle should be created");

    let migrated_inventory = comparable_schema_inventory(&paths.main_db)
        .await
        .expect("migrated main db schema inventory should load");
    let oracle_inventory = comparable_schema_inventory(&oracle_paths.main_db)
        .await
        .expect("oracle main db schema inventory should load");

    assert_eq!(
        migrated_inventory, oracle_inventory,
        "legacy main sqlite db without flyway history should migrate to Kotlin/Flyway latest schema inventory",
    );

    main_pool.close().await;
    persistence_contract_fixture::cleanup(paths);
    persistence_contract_fixture::cleanup(oracle_paths);
}

#[tokio::test]
async fn migrate_historyless_kotlin_main_schema_checkpoints_to_latest_inventory() {
    const HISTORYLESS_SCHEMA_CHECKPOINTS: &[i64] = &[
        20200810165912,
        20200820154318,
        20240614170012,
        20250108115503,
        20250108172343,
    ];

    let oracle_paths = persistence_contract_fixture::new_runtime_db_paths(
        "runtime-schema-migrate-historyless-prefixes-oracle",
    )
    .expect("oracle db paths should be created");
    persistence_contract_fixture::seed_main_db_from_flyway(&oracle_paths.main_db)
        .await
        .expect("main db flyway oracle should be created");
    let oracle_inventory = comparable_schema_inventory(&oracle_paths.main_db)
        .await
        .expect("oracle main db schema inventory should load");

    for version in HISTORYLESS_SCHEMA_CHECKPOINTS {
        let paths = persistence_contract_fixture::new_runtime_db_paths(
            format!("runtime-schema-migrate-historyless-checkpoint-{version}").as_str(),
        )
        .expect("historyless prefix db paths should be created");

        persistence_contract_fixture::seed_main_db_from_flyway_through(&paths.main_db, *version)
            .await
            .unwrap_or_else(|error| {
                panic!("historyless Kotlin prefix fixture {version} should be created: {error}")
            });

        let main_pool = connect_pool(&paths.main_db, 1)
            .await
            .expect("historyless prefix main sqlite db should open");
        setup::bootstrap_pool(&main_pool)
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "historyless Kotlin checkpoint {version} should be migrated by rust runtime: {error}"
                )
            });

        let migrated_inventory = comparable_schema_inventory(&paths.main_db)
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "migrated historyless Kotlin checkpoint {version} inventory should load: {error}"
                )
            });
        assert_eq!(
            migrated_inventory, oracle_inventory,
            "historyless Kotlin main schema checkpoint {version} should migrate to Kotlin/Flyway latest schema inventory",
        );

        main_pool.close().await;
        persistence_contract_fixture::cleanup(paths);
    }

    persistence_contract_fixture::cleanup(oracle_paths);
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

    let main_journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode;")
        .fetch_one(main_context.pool())
        .await
        .expect("main journal mode probe should succeed");
    assert_eq!(
        main_journal_mode.to_ascii_lowercase(),
        "wal",
        "main connect-layer should align with Kotlin's WAL-backed SQLite design",
    );

    let tasks_journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode;")
        .fetch_one(&tasks_pool)
        .await
        .expect("tasks journal mode probe should succeed");
    assert_eq!(
        tasks_journal_mode.to_ascii_lowercase(),
        "wal",
        "tasks connect-layer should align with Kotlin's WAL-backed SQLite design",
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

async fn comparable_schema_inventory(
    path: &std::path::Path,
) -> anyhow::Result<Vec<(String, String, String, String)>> {
    let mut rows = schema_inventory(path).await?;
    rows.retain(|(object_type, name, _, _)| {
        !(object_type == "table"
            && (name.eq_ignore_ascii_case("_sqlx_migrations")
                || name.eq_ignore_ascii_case("flyway_schema_history")))
    });
    Ok(rows)
}

async fn apply_sql_file(db_path: &Path, sql_file: &Path) -> anyhow::Result<()> {
    let pool = connect_pool(db_path, 1).await?;
    let context = SqlitePersistenceContext::new(pool.clone());
    let content = std::fs::read_to_string(sql_file)?;

    for statement in split_statements(&content) {
        context.pool_connection().execute(&statement).await?;
    }

    pool.close().await;
    Ok(())
}

async fn seed_flyway_history(db_path: &Path, versions: &[&str]) -> anyhow::Result<()> {
    let pool = connect_pool(db_path, 1).await?;
    let context = SqlitePersistenceContext::new(pool.clone());

    context
        .pool_connection()
        .execute(
            "CREATE TABLE IF NOT EXISTS flyway_schema_history (version TEXT NULL, success BOOLEAN NOT NULL)",
        )
        .await?;

    for version in versions {
        sqlx::query("INSERT INTO flyway_schema_history(version, success) VALUES (?1, 1)")
            .bind(version)
            .execute(context.pool())
            .await?;
    }

    pool.close().await;
    Ok(())
}

fn normalize_schema_sql(sql: &str) -> String {
    sql.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(" ,", ",")
        .replace(" )", ")")
        .replace("( ", "(")
}

fn split_statements(content: &str) -> Vec<String> {
    let normalized = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n");

    let mut statements = Vec::new();
    let mut current = String::new();
    let chars = normalized.chars().collect::<Vec<_>>();
    let mut i = 0;
    let mut in_single_quote = false;

    while i < chars.len() {
        let ch = chars[i];

        if ch == '\'' {
            if in_single_quote && i + 1 < chars.len() && chars[i + 1] == '\'' {
                current.push(ch);
                current.push(chars[i + 1]);
                i += 2;
                continue;
            }
            in_single_quote = !in_single_quote;
            current.push(ch);
            i += 1;
            continue;
        }

        if ch == ';' && !in_single_quote {
            let statement = current.trim();
            if !statement.is_empty() {
                statements.push(statement.to_string());
            }
            current.clear();
            i += 1;
            continue;
        }

        current.push(ch);
        i += 1;
    }

    let trailing = current.trim();
    if !trailing.is_empty() {
        statements.push(trailing.to_string());
    }

    combine_trigger_blocks(statements)
}

fn combine_trigger_blocks(statements: Vec<String>) -> Vec<String> {
    let mut combined = Vec::new();
    let mut trigger_statement: Option<String> = None;

    for statement in statements {
        let normalized = statement.to_ascii_lowercase();

        if let Some(active) = &mut trigger_statement {
            active.push(';');
            active.push_str(&statement);

            if normalized.trim_end().ends_with("end") {
                combined.push(active.trim().to_string());
                trigger_statement = None;
            }
            continue;
        }

        if normalized.contains("create trigger") && !normalized.trim_end().ends_with("end") {
            trigger_statement = Some(statement);
            continue;
        }

        combined.push(statement);
    }

    if let Some(active) = trigger_statement {
        combined.push(active);
    }

    combined
}

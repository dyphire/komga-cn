use std::borrow::Cow;
use std::sync::OnceLock;

use sqlx::migrate::{Migration, MigrationType, Migrator};
use sqlx::{Row, SqlStr, SqliteConnection, SqlitePool};

mod embedded_migrations {
    include!(concat!(
        env!("OUT_DIR"),
        "/sqlx-migrations/embedded_migrations.rs"
    ));
}

use super::schema_definitions::{
    LEGACY_MAIN_SCHEMA_V20200706141854, LEGACY_MAIN_SCHEMA_V20200706141854_VERSION,
    MAIN_PREFIX_SCHEMA_INVENTORIES_JSON, PrefixSchemaInventory, REQUIRED_MAIN_SCHEMA,
    REQUIRED_TASKS_SCHEMA, SchemaInventoryObject, TASKS_PREFIX_SCHEMA_INVENTORIES_JSON,
};
use embedded_migrations::{EmbeddedMigration, MAIN_EMBEDDED_MIGRATIONS, TASKS_EMBEDDED_MIGRATIONS};

#[derive(Clone, Copy, Eq, PartialEq)]
enum SchemaTarget {
    Main,
    Tasks,
}

struct ComparableSchemaInventoryObject {
    object_type: String,
    name: String,
    table_name: String,
    sql: String,
}

impl ComparableSchemaInventoryObject {
    fn matches_expected(&self, expected: &SchemaInventoryObject) -> bool {
        self.object_type == expected.object_type
            && self.name == expected.name
            && self.table_name == expected.table_name
            && self.sql == expected.sql
    }
}

pub async fn bootstrap_pool(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut connection = pool.acquire().await?;
    bootstrap_or_migrate_schema(
        connection.as_mut(),
        main_migrator(),
        REQUIRED_MAIN_SCHEMA,
        SchemaTarget::Main,
    )
    .await
}

pub async fn bootstrap_tasks_pool(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut connection = pool.acquire().await?;
    bootstrap_or_migrate_schema(
        connection.as_mut(),
        tasks_migrator(),
        REQUIRED_TASKS_SCHEMA,
        SchemaTarget::Tasks,
    )
    .await
}

fn main_migrator() -> &'static Migrator {
    static MIGRATOR: OnceLock<Migrator> = OnceLock::new();
    MIGRATOR.get_or_init(|| build_migrator(MAIN_EMBEDDED_MIGRATIONS))
}

fn tasks_migrator() -> &'static Migrator {
    static MIGRATOR: OnceLock<Migrator> = OnceLock::new();
    MIGRATOR.get_or_init(|| build_migrator(TASKS_EMBEDDED_MIGRATIONS))
}

fn build_migrator(migrations: &[EmbeddedMigration]) -> Migrator {
    Migrator {
        migrations: Cow::Owned(
            migrations
                .iter()
                .map(|migration| {
                    Migration::new(
                        migration.version,
                        Cow::Borrowed(migration.description),
                        MigrationType::Simple,
                        SqlStr::from_static(migration.sql),
                        false,
                    )
                })
                .collect(),
        ),
        ..Migrator::DEFAULT
    }
}

async fn bootstrap_or_migrate_schema(
    connection: &mut SqliteConnection,
    migrator: &Migrator,
    required_schema: &[(&str, &[&str])],
    target: SchemaTarget,
) -> Result<(), sqlx::Error> {
    adopt_preexisting_schema(connection, migrator, required_schema, target).await?;
    migrator
        .run_direct(None, connection, false)
        .await
        .map_err(map_migrate_error)?;

    for (table, required_columns) in required_schema {
        ensure_required_table_columns(connection, table, required_columns).await?;
    }
    Ok(())
}

async fn is_fresh_install_database(connection: &mut SqliteConnection) -> Result<bool, sqlx::Error> {
    let tables = sqlx::query_as::<_, (String,)>(
        r#"SELECT name
FROM sqlite_master
WHERE type = 'table'
  AND name NOT LIKE 'sqlite_%'"#,
    )
    .fetch_all(&mut *connection)
    .await?;

    Ok(tables.is_empty())
}

async fn adopt_preexisting_schema(
    connection: &mut SqliteConnection,
    migrator: &Migrator,
    required_schema: &[(&str, &[&str])],
    target: SchemaTarget,
) -> Result<(), sqlx::Error> {
    if has_applied_sqlx_migrations(connection).await?
        || is_fresh_install_database(connection).await?
    {
        return Ok(());
    }

    let flyway_versions = load_applied_flyway_versions(connection).await?;
    if !flyway_versions.is_empty() {
        stamp_sqlx_migrations(connection, migrator, |version| {
            flyway_versions.contains(&version)
        })
        .await?;
        return Ok(());
    }

    if let Some(version) = repair_historyless_schema_prefix(connection, target).await? {
        stamp_sqlx_migrations(connection, migrator, |migration_version| {
            migration_version <= version
        })
        .await?;
        return Ok(());
    }

    if schema_matches_required_shape(connection, required_schema).await? {
        stamp_sqlx_migrations(connection, migrator, |_| true).await?;
        return Ok(());
    }

    if let Some(version) = detect_legacy_schema_baseline(connection, target).await? {
        stamp_sqlx_migrations(connection, migrator, |migration_version| {
            migration_version <= version
        })
        .await?;
        return Ok(());
    }

    Err(outdated_schema_error(
        "without Flyway migration history or current Kotlin-compatible schema".to_string(),
    ))
}

async fn has_applied_sqlx_migrations(
    connection: &mut SqliteConnection,
) -> Result<bool, sqlx::Error> {
    if !table_exists(connection, "_sqlx_migrations").await? {
        return Ok(false);
    }

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&mut *connection)
        .await?;
    Ok(count > 0)
}

async fn load_applied_flyway_versions(
    connection: &mut SqliteConnection,
) -> Result<Vec<i64>, sqlx::Error> {
    if !table_exists(connection, "flyway_schema_history").await? {
        return Ok(Vec::new());
    }

    let versions = sqlx::query_scalar::<_, String>(
        r#"SELECT version
FROM flyway_schema_history
WHERE success = 1
  AND version IS NOT NULL
ORDER BY version"#,
    )
    .fetch_all(&mut *connection)
    .await?;

    versions
        .into_iter()
        .map(|version| {
            version.parse::<i64>().map_err(|_| {
                sqlx::Error::Protocol(format!(
                    "unexpected Flyway migration version `{version}` in flyway_schema_history"
                ))
            })
        })
        .collect()
}

async fn stamp_sqlx_migrations<F>(
    connection: &mut SqliteConnection,
    migrator: &Migrator,
    should_stamp: F,
) -> Result<(), sqlx::Error>
where
    F: Fn(i64) -> bool,
{
    create_sqlx_migrations_table(connection).await?;

    for migration in migrator.iter() {
        if !migration.migration_type.is_up_migration() || !should_stamp(migration.version) {
            continue;
        }

        sqlx::query(
            r#"INSERT OR IGNORE INTO _sqlx_migrations (
    version, description, success, checksum, execution_time
)
VALUES (?1, ?2, 1, ?3, 0)"#,
        )
        .bind(migration.version)
        .bind(migration.description.as_ref())
        .bind(migration.checksum.as_ref().to_vec())
        .execute(&mut *connection)
        .await?;
    }

    Ok(())
}

async fn create_sqlx_migrations_table(
    connection: &mut SqliteConnection,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS _sqlx_migrations (
    version BIGINT PRIMARY KEY,
    description TEXT NOT NULL,
    installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    success BOOLEAN NOT NULL,
    checksum BLOB NOT NULL,
    execution_time BIGINT NOT NULL
)"#,
    )
    .execute(&mut *connection)
    .await?;
    Ok(())
}

async fn schema_matches_required_shape(
    connection: &mut SqliteConnection,
    required_schema: &[(&str, &[&str])],
) -> Result<bool, sqlx::Error> {
    for (table, required_columns) in required_schema {
        let existing_columns = table_columns(connection, table).await?;
        if existing_columns.is_empty() {
            return Ok(false);
        }

        if required_columns
            .iter()
            .any(|column| !existing_columns.iter().any(|existing| existing == column))
        {
            return Ok(false);
        }
    }

    Ok(true)
}

async fn detect_legacy_schema_baseline(
    connection: &mut SqliteConnection,
    target: SchemaTarget,
) -> Result<Option<i64>, sqlx::Error> {
    match target {
        SchemaTarget::Main => {
            if schema_matches_required_shape(connection, LEGACY_MAIN_SCHEMA_V20200706141854).await?
            {
                Ok(Some(LEGACY_MAIN_SCHEMA_V20200706141854_VERSION))
            } else if let Some(version) =
                detect_historyless_main_schema_prefix_version(connection).await?
            {
                Ok(Some(version))
            } else {
                Ok(None)
            }
        }
        SchemaTarget::Tasks => Ok(None),
    }
}

async fn table_exists(connection: &mut SqliteConnection, table: &str) -> Result<bool, sqlx::Error> {
    let count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*)
FROM sqlite_master
WHERE type = 'table'
  AND LOWER(name) = LOWER(?1)"#,
    )
    .bind(table)
    .fetch_one(&mut *connection)
    .await?;

    Ok(count > 0)
}

async fn detect_historyless_main_schema_prefix_version(
    connection: &mut SqliteConnection,
) -> Result<Option<i64>, sqlx::Error> {
    let live_inventory = comparable_schema_inventory(connection).await?;

    Ok(main_prefix_schema_inventories()
        .iter()
        .rev()
        .find(|entry| schema_inventory_matches(&live_inventory, &entry.objects))
        .map(|entry| entry.version))
}

async fn repair_historyless_schema_prefix(
    connection: &mut SqliteConnection,
    target: SchemaTarget,
) -> Result<Option<i64>, sqlx::Error> {
    let inventories = match target {
        SchemaTarget::Main => main_prefix_schema_inventories(),
        SchemaTarget::Tasks => tasks_prefix_schema_inventories(),
    };

    let Some(expected) = inventories.last() else {
        return Ok(None);
    };

    let live_inventory = comparable_schema_inventory(connection).await?;
    if schema_inventory_matches(&live_inventory, &expected.objects) {
        return Ok(Some(expected.version));
    }

    let Some(missing_objects) = missing_schema_objects(&live_inventory, &expected.objects) else {
        return Ok(None);
    };

    if missing_objects.is_empty() {
        return Ok(None);
    }

    if !can_repair_historyless_schema_objects(target, &missing_objects) {
        return Ok(None);
    }

    for object in missing_objects {
        if object.sql.is_empty() {
            return Ok(None);
        }
        sqlx::query(sqlx::AssertSqlSafe(object.sql.clone()))
            .execute(&mut *connection)
            .await?;
    }

    let repaired_inventory = comparable_schema_inventory(connection).await?;
    if schema_inventory_matches(&repaired_inventory, &expected.objects) {
        Ok(Some(expected.version))
    } else {
        Ok(None)
    }
}

fn main_prefix_schema_inventories() -> &'static [PrefixSchemaInventory] {
    static INVENTORIES: OnceLock<Vec<PrefixSchemaInventory>> = OnceLock::new();
    INVENTORIES
        .get_or_init(|| {
            serde_json::from_str(MAIN_PREFIX_SCHEMA_INVENTORIES_JSON)
                .expect("main prefix schema inventories JSON should parse")
        })
        .as_slice()
}

fn tasks_prefix_schema_inventories() -> &'static [PrefixSchemaInventory] {
    static INVENTORIES: OnceLock<Vec<PrefixSchemaInventory>> = OnceLock::new();
    INVENTORIES
        .get_or_init(|| {
            serde_json::from_str(TASKS_PREFIX_SCHEMA_INVENTORIES_JSON)
                .expect("tasks prefix schema inventories JSON should parse")
        })
        .as_slice()
}

fn can_repair_historyless_schema_objects(
    target: SchemaTarget,
    missing_objects: &[&SchemaInventoryObject],
) -> bool {
    match target {
        SchemaTarget::Main => missing_objects
            .iter()
            .all(|object| matches!(object.object_type.as_str(), "index" | "trigger" | "view")),
        SchemaTarget::Tasks => true,
    }
}

fn missing_schema_objects<'a>(
    live_inventory: &[ComparableSchemaInventoryObject],
    expected_inventory: &'a [SchemaInventoryObject],
) -> Option<Vec<&'a SchemaInventoryObject>> {
    let mut missing = Vec::new();
    let mut live_index = 0usize;

    for expected in expected_inventory {
        if let Some(live) = live_inventory.get(live_index)
            && live.matches_expected(expected)
        {
            live_index += 1;
            continue;
        }

        missing.push(expected);
    }

    if live_index == live_inventory.len() {
        Some(missing)
    } else {
        None
    }
}

fn schema_inventory_matches(
    live_inventory: &[ComparableSchemaInventoryObject],
    expected_inventory: &[SchemaInventoryObject],
) -> bool {
    live_inventory.len() == expected_inventory.len()
        && live_inventory
            .iter()
            .zip(expected_inventory.iter())
            .all(|(live, expected)| live.matches_expected(expected))
}

async fn comparable_schema_inventory(
    connection: &mut SqliteConnection,
) -> Result<Vec<ComparableSchemaInventoryObject>, sqlx::Error> {
    sqlx::query(
        r#"SELECT type, name, tbl_name, COALESCE(sql, '') AS sql
FROM sqlite_master
WHERE type IN ('table', 'index', 'trigger', 'view')
  AND name NOT LIKE 'sqlite_%'
  AND LOWER(name) NOT IN ('_sqlx_migrations', 'flyway_schema_history')
ORDER BY type, name"#,
    )
    .fetch_all(&mut *connection)
    .await
    .map(|rows| {
        rows.into_iter()
            .map(|row| {
                let sql = row.get::<String, _>("sql");
                ComparableSchemaInventoryObject {
                    object_type: row.get::<String, _>("type"),
                    name: row.get::<String, _>("name"),
                    table_name: row.get::<String, _>("tbl_name"),
                    sql: normalize_schema_sql(&sql),
                }
            })
            .collect()
    })
}

fn normalize_schema_sql(sql: &str) -> String {
    sql.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(" ,", ",")
        .replace(" )", ")")
        .replace("( ", "(")
}

fn map_migrate_error(error: sqlx::migrate::MigrateError) -> sqlx::Error {
    sqlx::Error::Protocol(error.to_string())
}

async fn ensure_required_table_columns(
    connection: &mut SqliteConnection,
    table: &str,
    required_columns: &[&str],
) -> Result<(), sqlx::Error> {
    let existing_columns = table_columns(connection, table).await?;

    if existing_columns.is_empty() {
        return Err(outdated_schema_error(format!("in table `{table}`")));
    }

    for column in required_columns {
        if !existing_columns.iter().any(|existing| existing == column) {
            return Err(outdated_schema_error(format!(
                "in table `{table}`: missing required column `{column}`",
            )));
        }
    }

    Ok(())
}

async fn table_columns(
    connection: &mut SqliteConnection,
    table: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let pragma = format!("PRAGMA table_info({table})");
    let columns = sqlx::query_as::<_, (i64, String, String, i64, Option<String>, i64)>(
        sqlx::AssertSqlSafe(pragma),
    )
    .fetch_all(&mut *connection)
    .await?
    .into_iter()
    .map(|(_, name, _, _, _, _)| name.to_ascii_lowercase())
    .collect::<Vec<_>>();
    Ok(columns)
}

fn outdated_schema_error(detail: String) -> sqlx::Error {
    sqlx::Error::Protocol(format!(
        "unsupported SQLite schema detected {detail}: database schema is damaged, incomplete, or unrecognized and cannot be migrated automatically by the Rust runtime",
    ))
}

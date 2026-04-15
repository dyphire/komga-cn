use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use komga_rust::infrastructure::{
    SqlitePersistenceContext,
    sqlite::{connect_pool, evict_shared_pools_for_paths},
};

pub struct RuntimeDbPaths {
    pub config_dir: PathBuf,
    pub main_db: PathBuf,
    pub tasks_db: PathBuf,
}

pub fn new_runtime_db_paths(case_id: &str) -> std::io::Result<RuntimeDbPaths> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("komga-persistence-contract-{case_id}-{nanos}"));
    fs::create_dir_all(&root)?;

    Ok(RuntimeDbPaths {
        main_db: root.join("database.sqlite"),
        tasks_db: root.join("tasks.sqlite"),
        config_dir: root,
    })
}

pub async fn seed_main_db_from_flyway(path: &Path) -> anyhow::Result<()> {
    let migration_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("crates/infrastructure/sqlx-migrations/main");
    execute_sql_files(path, &migration_dir, None).await
}

#[allow(dead_code)]
pub async fn seed_main_db_from_flyway_through(
    path: &Path,
    through_version: i64,
) -> anyhow::Result<()> {
    let migration_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("crates/infrastructure/sqlx-migrations/main");
    execute_sql_files(path, &migration_dir, Some(through_version)).await
}

pub async fn seed_tasks_db_from_flyway(path: &Path) -> anyhow::Result<()> {
    let migration_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../komga/src/flyway/resources/tasks/migration/sqlite");
    execute_sql_files(path, &migration_dir, None).await
}

pub fn cleanup(paths: RuntimeDbPaths) {
    close_fixture_shared_pools(&paths);
    cleanup_files(&paths);
}

pub async fn cleanup_async(paths: RuntimeDbPaths) {
    for pool in evict_shared_pools_for_paths(&[paths.main_db.clone(), paths.tasks_db.clone()]) {
        pool.close().await;
    }
    cleanup_files(&paths);
}

fn cleanup_files(paths: &RuntimeDbPaths) {
    for _ in 0..10 {
        for db_path in [&paths.main_db, &paths.tasks_db] {
            for path in sqlite_sidecar_paths(db_path) {
                let _ = std::fs::remove_file(path);
            }
        }
        let _ = std::fs::remove_dir_all(&paths.config_dir);
        if !paths.config_dir.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn close_fixture_shared_pools(paths: &RuntimeDbPaths) {
    let _ = evict_shared_pools_for_paths(&[paths.main_db.clone(), paths.tasks_db.clone()]);
}

fn sqlite_sidecar_paths(db_path: &Path) -> [PathBuf; 4] {
    let base_name = db_path.to_string_lossy().to_string();
    [
        db_path.to_path_buf(),
        PathBuf::from(format!("{base_name}-wal")),
        PathBuf::from(format!("{base_name}-shm")),
        PathBuf::from(format!("{base_name}-journal")),
    ]
}

async fn execute_sql_files(
    db_path: &Path,
    migration_dir: &Path,
    through_version: Option<i64>,
) -> anyhow::Result<()> {
    let pool = connect_pool(db_path, 1).await?;
    let context = SqlitePersistenceContext::new(pool.clone());

    for file in sorted_migration_files(migration_dir)? {
        if let Some(max_version) = through_version
            && parse_flyway_version(&file)? > max_version
        {
            break;
        }

        let content = std::fs::read_to_string(&file)?;
        let normalized = replace_flyway_placeholders(&content);

        for statement in split_statements(&normalized) {
            context
                .pool_connection()
                .execute(&statement)
                .await
                .with_context(|| {
                    format!(
                        "failed migration statement in {}: {}",
                        file.display(),
                        statement.chars().take(160).collect::<String>()
                    )
                })?;
        }
    }

    pool.close().await;
    Ok(())
}

fn sorted_migration_files(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("sql"))
        })
        .collect::<Vec<_>>();

    files.sort_by(|a, b| {
        a.file_name()
            .unwrap_or_default()
            .cmp(b.file_name().unwrap_or_default())
    });

    Ok(files)
}

fn parse_flyway_version(path: &Path) -> anyhow::Result<i64> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("migration filename should be valid utf-8")?;
    let base = file_name
        .strip_suffix(".sql")
        .context("migration file should have .sql suffix")?;
    let (version, _) = base
        .split_once("__")
        .context("migration file should contain Flyway version separator")?;
    let version = version
        .strip_prefix('V')
        .context("migration file should start with Flyway V prefix")?;
    version
        .parse::<i64>()
        .context("migration version should parse as integer")
}

fn replace_flyway_placeholders(content: &str) -> String {
    let substitutions = BTreeMap::from([
        ("${library-file-hashing}", "1"),
        ("${library-scan-startup}", "0"),
        ("${delete-empty-collections}", "1"),
        ("${delete-empty-read-lists}", "1"),
    ]);

    substitutions
        .into_iter()
        .fold(content.to_string(), |acc, (from, to)| acc.replace(from, to))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cleanup_closes_cached_shared_pools_for_fixture_databases() {
        let paths = new_runtime_db_paths("cleanup-closes-cached-shared-pools")
            .expect("fixture paths should be created");
        let main_db = paths.main_db.clone();
        let tasks_db = paths.tasks_db.clone();
        let config_dir = paths.config_dir.clone();

        seed_main_db_from_flyway(&main_db)
            .await
            .expect("fixture main db should be seeded before stale pool opens");
        let stale_pool = connect_pool(&main_db, 1)
            .await
            .expect("fixture main db should open");
        sqlx::query("INSERT INTO USER (ID, EMAIL, PASSWORD) VALUES (?, ?, ?)")
            .bind("stale-user")
            .bind("stale-user@example.org")
            .bind("test-password")
            .execute(&stale_pool)
            .await
            .expect("fixture user row should be inserted in stale database");
        drop(stale_pool);

        cleanup_async(paths).await;

        let main_exists_after_cleanup = main_db.exists();
        let wal_exists_after_cleanup =
            PathBuf::from(format!("{}-wal", main_db.to_string_lossy())).exists();
        let shm_exists_after_cleanup =
            PathBuf::from(format!("{}-shm", main_db.to_string_lossy())).exists();
        let config_exists_after_cleanup = config_dir.exists();

        fs::create_dir_all(&config_dir).expect("fixture root should be recreated after cleanup");
        let fresh_pool = connect_pool(&main_db, 1)
            .await
            .expect("fresh fixture main db should open after cleanup");
        let leaked_rows = sqlx::query_scalar::<_, String>("SELECT ID FROM USER")
            .fetch_all(&fresh_pool)
            .await;

        assert!(
            leaked_rows.is_err(),
            "fixture cleanup must not reuse a cached pool pointing at a deleted sqlite file; main_exists_after_cleanup={main_exists_after_cleanup}, wal_exists_after_cleanup={wal_exists_after_cleanup}, shm_exists_after_cleanup={shm_exists_after_cleanup}, config_exists_after_cleanup={config_exists_after_cleanup}",
        );

        fresh_pool.close().await;
        cleanup_async(RuntimeDbPaths {
            config_dir,
            main_db,
            tasks_db,
        })
        .await;
    }
}

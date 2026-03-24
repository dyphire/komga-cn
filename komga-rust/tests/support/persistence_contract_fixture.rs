use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use komga_rust::persistence::{SqlitePersistenceContext, sqlite::connect_pool};

pub struct LegacyDbPaths {
    pub config_dir: PathBuf,
    pub main_db: PathBuf,
    pub tasks_db: PathBuf,
}

pub fn new_legacy_db_paths(case_id: &str) -> std::io::Result<LegacyDbPaths> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("komga-persistence-contract-{case_id}-{nanos}"));
    fs::create_dir_all(&root)?;

    Ok(LegacyDbPaths {
        main_db: root.join("database.sqlite"),
        tasks_db: root.join("tasks.sqlite"),
        config_dir: root,
    })
}

pub async fn seed_main_db_from_flyway(path: &Path) -> anyhow::Result<()> {
    let migration_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../komga/src/flyway/resources/db/migration/sqlite");
    execute_sql_files(path, &migration_dir).await
}

pub async fn seed_tasks_db_from_flyway(path: &Path) -> anyhow::Result<()> {
    let migration_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../komga/src/flyway/resources/tasks/migration/sqlite");
    execute_sql_files(path, &migration_dir).await
}

pub fn cleanup(paths: LegacyDbPaths) {
    let _ = std::fs::remove_file(paths.main_db);
    let _ = std::fs::remove_file(paths.tasks_db);
    let _ = std::fs::remove_dir_all(paths.config_dir);
}

async fn execute_sql_files(db_path: &Path, migration_dir: &Path) -> anyhow::Result<()> {
    let pool = connect_pool(db_path, 1).await?;
    let context = SqlitePersistenceContext::new(pool.clone());

    for file in sorted_migration_files(migration_dir)? {
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

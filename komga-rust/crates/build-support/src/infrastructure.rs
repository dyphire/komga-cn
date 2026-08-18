use std::collections::BTreeMap;
use std::env;
use std::fmt::Write;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use reqwest::blocking::Client;
use rusqlite::Connection;
use serde_json::json;
use tar::Archive;

struct NormalizedMigration {
    version: i64,
    description: String,
    sql: String,
}

pub fn configure_infrastructure_build(manifest_dir: &Path, out_dir: &Path) {
    println!("cargo:rerun-if-changed=build.rs");

    let main_dir = manifest_dir.join("sqlx-migrations/main");
    let tasks_dir = manifest_dir.join("sqlx-migrations/tasks");
    let target_root = out_dir.join("sqlx-migrations");

    println!("cargo:rerun-if-changed={}", main_dir.display());
    println!("cargo:rerun-if-changed={}", tasks_dir.display());

    fs::create_dir_all(&target_root)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", target_root.display()));

    write_embedded_migrations_module(
        &main_dir,
        &tasks_dir,
        &target_root.join("embedded_migrations.rs"),
    );
    write_prefix_schema_inventories(
        &main_dir,
        &target_root.join("main-prefix-schema-inventories.json"),
    );
    write_prefix_schema_inventories(
        &tasks_dir,
        &target_root.join("tasks-prefix-schema-inventories.json"),
    );
    prepare_pdfium_vendor(manifest_dir);
}

fn prepare_pdfium_vendor(manifest_dir: &Path) {
    let workspace_root = manifest_dir
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|error| {
            panic!(
                "failed to resolve workspace root from {}: {error}",
                manifest_dir.display()
            )
        });
    let release_file = workspace_root.join("vendor/pdfium-release");
    println!("cargo:rerun-if-changed={}", release_file.display());
    let release_tag = fs::read_to_string(&release_file)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", release_file.display()))
        .trim()
        .to_owned();
    if release_tag.is_empty() {
        panic!(
            "Pdfium release tag in {} must not be empty",
            release_file.display()
        );
    }

    let vendor_root = workspace_root.join("vendor/pdfium-binaries");
    fs::create_dir_all(&vendor_root)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", vendor_root.display()));
    let gitkeep = vendor_root.join(".gitkeep");
    if !gitkeep.exists() {
        fs::write(&gitkeep, "")
            .unwrap_or_else(|error| panic!("failed to write {}: {error}", gitkeep.display()));
    }

    println!("cargo:rerun-if-env-changed=TARGET");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_OS");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_ARCH");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_ENV");

    let asset_name = pdfium_asset_name();
    let versioned_vendor_root = vendor_root.join(pdfium_vendor_directory_name(&release_tag));
    let platform_dir = versioned_vendor_root.join(pdfium_platform_key());
    let extract_dir = platform_dir.join(asset_name.trim_end_matches(".tgz"));
    let library_path = extract_dir.join(pdfium_library_relative_path());

    if !library_path.exists() {
        if extract_dir.exists() {
            fs::remove_dir_all(&extract_dir).unwrap_or_else(|error| {
                panic!("failed to clear {}: {error}", extract_dir.display())
            });
        }
        fs::create_dir_all(&extract_dir)
            .unwrap_or_else(|error| panic!("failed to create {}: {error}", extract_dir.display()));

        let archive_bytes = download_pdfium_archive(asset_name, &release_tag);
        extract_pdfium_archive(&archive_bytes, &extract_dir);
    }

    if !library_path.exists() {
        panic!(
            "expected Pdfium library at {} after extracting {}",
            library_path.display(),
            asset_name
        );
    }

    println!(
        "cargo:rustc-env=KOMGA_PDFIUM_LIB_PATH={}",
        library_path.display()
    );
}

fn pdfium_platform_key() -> String {
    let target_os = env::var("CARGO_CFG_TARGET_OS").expect("target os env should exist");
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").expect("target arch env should exist");
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    if target_env.is_empty() {
        format!("{target_os}-{target_arch}")
    } else {
        format!("{target_os}-{target_arch}-{target_env}")
    }
}

fn pdfium_asset_name() -> &'static str {
    let target_os = env::var("CARGO_CFG_TARGET_OS").expect("target os env should exist");
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").expect("target arch env should exist");
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    match (
        target_os.as_str(),
        target_arch.as_str(),
        target_env.as_str(),
    ) {
        ("linux", "aarch64", "musl") => "pdfium-linux-musl-arm64.tgz",
        ("linux", "aarch64", _) => "pdfium-linux-arm64.tgz",
        ("linux", "x86_64", "musl") => "pdfium-linux-musl-x64.tgz",
        ("linux", "x86_64", _) => "pdfium-linux-x64.tgz",
        ("macos", "aarch64", _) => "pdfium-mac-arm64.tgz",
        ("macos", "x86_64", _) => "pdfium-mac-x64.tgz",
        ("windows", "aarch64", _) => "pdfium-win-arm64.tgz",
        ("windows", "x86_64", _) => "pdfium-win-x64.tgz",
        ("windows", "x86", _) => "pdfium-win-x86.tgz",
        _ => panic!(
            "unsupported target for Pdfium binaries: os={target_os} arch={target_arch} env={target_env}"
        ),
    }
}

fn pdfium_library_relative_path() -> &'static str {
    match env::var("CARGO_CFG_TARGET_OS")
        .expect("target os env should exist")
        .as_str()
    {
        "linux" => "lib/libpdfium.so",
        "macos" => "lib/libpdfium.dylib",
        "windows" => "bin/pdfium.dll",
        other => panic!("unsupported target os for Pdfium library path: {other}"),
    }
}

fn download_pdfium_archive(asset_name: &str, release_tag: &str) -> Vec<u8> {
    let url = pdfium_release_download_url(release_tag, asset_name);
    let client = Client::builder()
        .build()
        .unwrap_or_else(|error| panic!("failed to build http client for Pdfium download: {error}"));
    let response = client
        .get(&url)
        .header(reqwest::header::USER_AGENT, "komga-rust-build")
        .send()
        .unwrap_or_else(|error| panic!("failed to download Pdfium archive from {url}: {error}"))
        .error_for_status()
        .unwrap_or_else(|error| panic!("unexpected Pdfium download status from {url}: {error}"));

    response
        .bytes()
        .unwrap_or_else(|error| panic!("failed to read Pdfium archive body from {url}: {error}"))
        .to_vec()
}

fn pdfium_release_download_url(release_tag: &str, asset_name: &str) -> String {
    let encoded_release_tag = release_tag.replace('/', "%2F");
    format!(
        "https://github.com/bblanchon/pdfium-binaries/releases/download/{encoded_release_tag}/{asset_name}"
    )
}

fn pdfium_vendor_directory_name(release_tag: &str) -> String {
    release_tag.replace('/', "-")
}

fn extract_pdfium_archive(archive_bytes: &[u8], extract_dir: &Path) {
    let decoder = GzDecoder::new(Cursor::new(archive_bytes));
    let mut archive = Archive::new(decoder);
    archive.unpack(extract_dir).unwrap_or_else(|error| {
        panic!(
            "failed to unpack Pdfium archive into {}: {error}",
            extract_dir.display()
        )
    });
}

fn write_embedded_migrations_module(main_dir: &Path, tasks_dir: &Path, target_file: &Path) {
    let main_migrations = normalized_migrations(main_dir);
    let tasks_migrations = normalized_migrations(tasks_dir);
    let mut contents = String::new();

    contents.push_str("pub(super) struct EmbeddedMigration {\n");
    contents.push_str("    pub(super) version: i64,\n");
    contents.push_str("    pub(super) description: &'static str,\n");
    contents.push_str("    pub(super) sql: &'static str,\n");
    contents.push_str("}\n\n");

    write_embedded_migration_array(&mut contents, "MAIN_EMBEDDED_MIGRATIONS", &main_migrations);
    contents.push('\n');
    write_embedded_migration_array(
        &mut contents,
        "TASKS_EMBEDDED_MIGRATIONS",
        &tasks_migrations,
    );

    fs::write(target_file, contents)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", target_file.display()));
}

fn normalized_migrations(source_dir: &Path) -> Vec<NormalizedMigration> {
    sorted_sql_files(source_dir)
        .into_iter()
        .map(|path| {
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_else(|| panic!("invalid migration filename: {}", path.display()));
            let (version, description) = parse_flyway_name(file_name);
            let sql = replace_flyway_placeholders(
                &fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display())),
            );
            NormalizedMigration {
                version,
                description,
                sql,
            }
        })
        .collect()
}

fn write_embedded_migration_array(
    contents: &mut String,
    const_name: &str,
    migrations: &[NormalizedMigration],
) {
    writeln!(
        contents,
        "pub(super) const {const_name}: &[EmbeddedMigration] = &["
    )
    .expect("embedded migration array header should write");

    for migration in migrations {
        writeln!(
            contents,
            "    EmbeddedMigration {{ version: {}, description: {:?}, sql: {:?} }},",
            migration.version, migration.description, migration.sql,
        )
        .expect("embedded migration entry should write");
    }

    contents.push_str("];\n");
}

fn write_prefix_schema_inventories(source_dir: &Path, target_file: &Path) {
    let temp_db = target_file
        .parent()
        .expect("schema inventory target parent should exist")
        .join(format!(
            "{}-prefix-schema.sqlite",
            target_file
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("schema")
        ));
    let _ = fs::remove_file(&temp_db);

    let connection = Connection::open(&temp_db)
        .unwrap_or_else(|error| panic!("failed to open {}: {error}", temp_db.display()));
    let mut inventories = Vec::new();

    for migration in normalized_migrations(source_dir) {
        for statement in split_statements(&migration.sql) {
            connection
                .execute_batch(&statement)
                .unwrap_or_else(|error| {
                    panic!(
                        "failed to apply schema migration v{} while building prefix schema inventories: {error}",
                        migration.version,
                    )
                });
        }

        inventories.push(json!({
            "version": migration.version,
            "objects": schema_inventory(&connection),
        }));
    }

    fs::write(
        target_file,
        serde_json::to_string(&inventories)
            .expect("schema inventory manifest should serialize to JSON"),
    )
    .unwrap_or_else(|error| panic!("failed to write {}: {error}", target_file.display()));

    drop(connection);
    let _ = fs::remove_file(temp_db);
}

fn sorted_sql_files(source_dir: &Path) -> Vec<PathBuf> {
    let mut files = fs::read_dir(source_dir)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", source_dir.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("sql"))
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn schema_inventory(connection: &Connection) -> Vec<serde_json::Value> {
    let mut statement = connection
        .prepare(
            r#"
        SELECT type, name, tbl_name, COALESCE(sql, '') AS sql
        FROM sqlite_master
        WHERE type IN ('table', 'index', 'trigger', 'view')
          AND name NOT LIKE 'sqlite_%'
        ORDER BY type, name
        "#,
        )
        .expect("schema inventory query should prepare");

    statement
        .query_map([], |row| {
            Ok(json!({
                "object_type": row.get::<_, String>(0)?,
                "name": row.get::<_, String>(1)?,
                "table_name": row.get::<_, String>(2)?,
                "sql": normalize_schema_sql(&row.get::<_, String>(3)?),
            }))
        })
        .expect("schema inventory query should run")
        .map(|row| row.expect("schema inventory row should decode"))
        .collect()
}

fn parse_flyway_name(file_name: &str) -> (i64, String) {
    let base = file_name
        .strip_suffix(".sql")
        .unwrap_or_else(|| panic!("unexpected migration extension: {file_name}"));
    let (version, description) = base
        .split_once("__")
        .unwrap_or_else(|| panic!("unexpected flyway migration name: {file_name}"));
    let version = version
        .strip_prefix('V')
        .unwrap_or_else(|| panic!("unexpected flyway migration version: {file_name}"))
        .parse::<i64>()
        .unwrap_or_else(|error| panic!("invalid flyway migration version in {file_name}: {error}"));

    (version, description.to_string())
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

#[cfg(test)]
mod tests {
    use super::{pdfium_release_download_url, pdfium_vendor_directory_name};

    #[test]
    fn pdfium_release_tag_is_encoded_and_mapped_to_vendor_directory() {
        let release_tag = "chromium/test";
        assert_eq!(
            pdfium_release_download_url(release_tag, "pdfium-win-x64.tgz"),
            "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2Ftest/pdfium-win-x64.tgz"
        );
        assert_eq!(pdfium_vendor_directory_name(release_tag), "chromium-test");
    }
}

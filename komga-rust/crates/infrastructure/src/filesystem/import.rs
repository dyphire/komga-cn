use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use komga_application::media_assets::{
    BooksImportEntry, ImportBookOutcome, ImportCopyMode, MediaImportPort,
};
use sqlx::Row;

use crate::sqlite::connect_pool;

#[derive(Clone, Debug)]
pub struct FilesystemImportPort {
    database_file: PathBuf,
}

impl FilesystemImportPort {
    pub fn new(database_file: impl Into<PathBuf>) -> Self {
        Self {
            database_file: database_file.into(),
        }
    }
}

impl MediaImportPort for FilesystemImportPort {
    async fn import_book(
        &self,
        copy_mode: ImportCopyMode,
        entry: BooksImportEntry,
    ) -> Result<Option<ImportBookOutcome>, String> {
        let library_roots = load_library_roots(self.database_file.as_path())
            .await
            .unwrap_or_default();
        if source_inside_library_roots(entry.source_file.as_path(), &library_roots) {
            return Ok(None);
        }

        let target =
            match load_import_series_target(self.database_file.as_path(), &entry.series_id).await {
                Ok(Some(target)) => target,
                Ok(None) => {
                    return Err(format!(
                        "series target for import was not found: {}",
                        entry.series_id
                    ));
                }
                Err(error) => return Err(error),
            };

        let mut upgrade_destination_name: Option<String> = None;
        if let Some(upgrade_book_id) = entry.upgrade_book_id.as_deref() {
            let upgrade_target = match load_import_upgrade_book_target(
                self.database_file.as_path(),
                upgrade_book_id,
            )
            .await
            {
                Ok(Some(target)) => target,
                Ok(None) => {
                    return Err(format!(
                        "upgrade target for import was not found: {upgrade_book_id}"
                    ));
                }
                Err(error) => return Err(error),
            };

            if upgrade_target.series_id != entry.series_id {
                return Err(format!(
                    "upgrade target series mismatch for import: expected {}, got {}",
                    entry.series_id, upgrade_target.series_id
                ));
            }

            upgrade_destination_name = Some(upgrade_target.file_name.clone());
            let upgrade_file = PathBuf::from(upgrade_target.library_root)
                .join(upgrade_target.series_url)
                .join(upgrade_target.file_name);
            let _ = fs::remove_file(upgrade_file);
        }

        let Some(destination_name) = resolve_import_destination_name(
            entry.source_file.as_path(),
            upgrade_destination_name
                .as_deref()
                .or(entry.destination_name.as_deref()),
        ) else {
            return Err(format!(
                "destination name for import is invalid: {}",
                entry.destination_name.as_deref().unwrap_or_default()
            ));
        };

        let destination_dir = PathBuf::from(&target.library_root).join(&target.series_url);
        fs::create_dir_all(&destination_dir)
            .map_err(|error| format!("create destination directory for import: {error}"))?;

        let destination_file = destination_dir.join(destination_name);
        apply_import_copy_mode(copy_mode, entry.source_file.as_path(), &destination_file)?;

        let sidecar_imported =
            import_book_sidecars(copy_mode, entry.source_file.as_path(), &destination_file)
                .unwrap_or(false);

        let imported_book_id = scanner_book_id_for_path(&destination_file);
        if let Some(upgrade_book_id) = entry.upgrade_book_id.as_deref() {
            let _ = migrate_upgraded_book_identity(
                self.database_file.as_path(),
                upgrade_book_id,
                imported_book_id.as_str(),
                &destination_file,
            )
            .await;
        }

        let _ = persist_book_imported_event(
            self.database_file.as_path(),
            imported_book_id.as_str(),
            target.series_id.as_str(),
            &destination_file,
            entry.source_file.as_path(),
            entry.upgrade_book_id.is_some(),
        )
        .await;

        Ok(Some(ImportBookOutcome {
            library_id: target.library_id,
            imported_book_id,
            sidecar_imported,
        }))
    }
}

struct ImportSeriesTarget {
    series_id: String,
    library_id: String,
    library_root: String,
    series_url: String,
}

struct ImportUpgradeBookTarget {
    series_id: String,
    library_root: String,
    series_url: String,
    file_name: String,
}

async fn load_import_series_target(
    database_file: &Path,
    series_id: &str,
) -> Result<Option<ImportSeriesTarget>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series target db: {error}"))?;

    let row = sqlx::query(
        "SELECT s.ID AS SERIES_ID, s.LIBRARY_ID AS LIBRARY_ID, s.URL AS SERIES_URL, \
                l.ROOT AS LIBRARY_ROOT \
         FROM SERIES s \
         JOIN LIBRARY l ON l.ID = s.LIBRARY_ID \
         WHERE s.ID = ? \
         LIMIT 1",
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query series target for import: {error}"))?;

    Ok(row.map(|row| ImportSeriesTarget {
        series_id: row.get::<String, _>("SERIES_ID"),
        library_id: row.get::<String, _>("LIBRARY_ID"),
        library_root: row.get::<String, _>("LIBRARY_ROOT"),
        series_url: row.get::<String, _>("SERIES_URL"),
    }))
}

async fn load_import_upgrade_book_target(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<ImportUpgradeBookTarget>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open upgrade book target db: {error}"))?;

    let row = sqlx::query(
        "SELECT b.SERIES_ID AS SERIES_ID, b.NAME AS FILE_NAME, s.URL AS SERIES_URL, \
                l.ROOT AS LIBRARY_ROOT \
         FROM BOOK b \
         JOIN SERIES s ON s.ID = b.SERIES_ID \
         JOIN LIBRARY l ON l.ID = b.LIBRARY_ID \
         WHERE b.ID = ? \
         LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query upgrade book target for import: {error}"))?;

    Ok(row.map(|row| ImportUpgradeBookTarget {
        series_id: row.get::<String, _>("SERIES_ID"),
        library_root: row.get::<String, _>("LIBRARY_ROOT"),
        series_url: row.get::<String, _>("SERIES_URL"),
        file_name: row.get::<String, _>("FILE_NAME"),
    }))
}

async fn load_library_roots(database_file: &Path) -> Result<Vec<PathBuf>, String> {
    if !database_file.exists() {
        return Ok(Vec::new());
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open library roots db: {error}"))?;

    let rows = sqlx::query("SELECT ROOT FROM LIBRARY")
        .fetch_all(&pool)
        .await
        .map_err(|error| format!("query library roots for import: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| PathBuf::from(row.get::<String, _>("ROOT")))
        .collect())
}

fn source_inside_library_roots(source_file: &Path, library_roots: &[PathBuf]) -> bool {
    let source = fs::canonicalize(source_file).unwrap_or_else(|_| source_file.to_path_buf());
    library_roots.iter().any(|root| {
        let root = fs::canonicalize(root).unwrap_or_else(|_| root.clone());
        source.starts_with(root)
    })
}

fn resolve_import_destination_name(
    source_file: &Path,
    destination_name: Option<&str>,
) -> Option<String> {
    if let Some(destination_name) = destination_name {
        if destination_name.contains('/') || destination_name.contains('\\') {
            return None;
        }
        return Some(destination_name.to_string());
    }

    source_file
        .file_name()
        .and_then(|value| value.to_str())
        .map(ToString::to_string)
}

fn apply_import_copy_mode(
    copy_mode: ImportCopyMode,
    source_file: &Path,
    destination_file: &Path,
) -> Result<(), String> {
    if !source_file.exists() {
        return Err("source file does not exist".to_string());
    }

    if destination_file.exists() {
        fs::remove_file(destination_file)
            .map_err(|error| format!("remove existing destination file: {error}"))?;
    }

    match copy_mode {
        ImportCopyMode::Copy => {
            fs::copy(source_file, destination_file)
                .map_err(|error| format!("copy source file for import: {error}"))?;
            Ok(())
        }
        ImportCopyMode::Move => {
            if let Err(error) = fs::rename(source_file, destination_file) {
                fs::copy(source_file, destination_file).map_err(|copy_error| {
                    format!(
                        "move source file for import failed ({error}); copy attempt failed: {copy_error}"
                    )
                })?;
                fs::remove_file(source_file).map_err(|remove_error| {
                    format!("remove source file after move-then-copy attempt: {remove_error}")
                })?;
            }
            Ok(())
        }
        ImportCopyMode::Hardlink => {
            if fs::hard_link(source_file, destination_file).is_err() {
                fs::copy(source_file, destination_file)
                    .map_err(|error| format!("hardlink/copy source file for import: {error}"))?;
            }
            Ok(())
        }
    }
}

fn import_book_sidecars(
    copy_mode: ImportCopyMode,
    source_file: &Path,
    destination_file: &Path,
) -> Result<bool, String> {
    let source_sidecar = source_file.with_extension("xml");
    if !source_sidecar.exists() {
        return Ok(false);
    }

    let destination_sidecar = destination_file.with_extension("xml");
    apply_import_copy_mode(
        copy_mode,
        source_sidecar.as_path(),
        destination_sidecar.as_path(),
    )?;
    Ok(true)
}

fn scanner_book_id_for_path(path: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    format!("book-{:016x}", hasher.finish())
}

async fn migrate_upgraded_book_identity(
    database_file: &Path,
    old_book_id: &str,
    new_book_id: &str,
    destination_file: &Path,
) -> Result<(), String> {
    if old_book_id == new_book_id || !database_file.exists() {
        return Ok(());
    }

    let destination_name = destination_file
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();
    let destination_url = destination_file.to_string_lossy().to_string();

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open import-upgrade migration db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin import-upgrade migration tx: {error}"))?;

    let source_exists = sqlx::query("SELECT 1 AS FOUND FROM BOOK WHERE ID = ? LIMIT 1")
        .bind(old_book_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| format!("query upgraded source book for migration: {error}"))?
        .is_some();
    if !source_exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback import-upgrade migration tx: {error}"))?;
        return Ok(());
    }

    sqlx::query(
        "INSERT INTO BOOK (ID, CREATED_DATE, LAST_MODIFIED_DATE, FILE_LAST_MODIFIED, NAME, URL, \
           SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, FILE_HASH, DELETED_DATE, oneshot, \
           FILE_HASH_KOREADER) SELECT ?, \
           CREATED_DATE, LAST_MODIFIED_DATE, FILE_LAST_MODIFIED, ?, ?, SERIES_ID, FILE_SIZE, \
           NUMBER, LIBRARY_ID, FILE_HASH, DELETED_DATE, oneshot, FILE_HASH_KOREADER \
         FROM BOOK \
         WHERE ID = ? \
         ON CONFLICT(ID) DO UPDATE \
         SET FILE_LAST_MODIFIED = excluded.FILE_LAST_MODIFIED, NAME = excluded.NAME, \
             URL = excluded.URL, SERIES_ID = excluded.SERIES_ID, FILE_SIZE = excluded.FILE_SIZE, \
             NUMBER = excluded.NUMBER, LIBRARY_ID = excluded.LIBRARY_ID, \
             FILE_HASH = excluded.FILE_HASH, DELETED_DATE = excluded.DELETED_DATE, \
             oneshot = excluded.oneshot, FILE_HASH_KOREADER = excluded.FILE_HASH_KOREADER, \
             LAST_MODIFIED_DATE = CURRENT_TIMESTAMP",
    )
    .bind(new_book_id)
    .bind(destination_name)
    .bind(destination_url)
    .bind(old_book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("upsert upgraded destination book identity: {error}"))?;

    sqlx::query("DELETE FROM BOOK_METADATA WHERE BOOK_ID = ?")
        .bind(new_book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            format!("delete destination metadata before upgrade migration: {error}")
        })?;
    sqlx::query(
        "UPDATE BOOK_METADATA SET BOOK_ID = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP WHERE BOOK_ID = ?",
    )
    .bind(new_book_id)
    .bind(old_book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("move book metadata during upgrade migration: {error}"))?;

    for table in [
        "BOOK_METADATA_AUTHOR",
        "BOOK_METADATA_TAG",
        "BOOK_METADATA_LINK",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE BOOK_ID = ?"))
            .bind(new_book_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                format!("delete destination {table} rows before upgrade migration: {error}")
            })?;

        sqlx::query(&format!("UPDATE {table} SET BOOK_ID = ? WHERE BOOK_ID = ?"))
            .bind(new_book_id)
            .bind(old_book_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("move {table} rows during upgrade migration: {error}"))?;
    }

    for table in ["MEDIA", "MEDIA_FILE", "MEDIA_PAGE", "READ_PROGRESS"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE BOOK_ID = ?"))
            .bind(new_book_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                format!("delete destination {table} rows before upgrade migration: {error}")
            })?;

        sqlx::query(&format!("UPDATE {table} SET BOOK_ID = ? WHERE BOOK_ID = ?"))
            .bind(new_book_id)
            .bind(old_book_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("move {table} rows during upgrade migration: {error}"))?;
    }

    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? AND TYPE = 'USER_UPLOADED'")
        .bind(new_book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            format!("delete destination user-uploaded thumbnails before upgrade migration: {error}")
        })?;
    sqlx::query(
        "UPDATE THUMBNAIL_BOOK SET BOOK_ID = ? WHERE BOOK_ID = ? AND TYPE = 'USER_UPLOADED'",
    )
    .bind(new_book_id)
    .bind(old_book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("move user-uploaded thumbnails during upgrade migration: {error}"))?;

    sqlx::query(
        "INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) SELECT READLIST_ID, ?, NUMBER \
         FROM READLIST_BOOK \
         WHERE BOOK_ID = ? \
         ON CONFLICT(READLIST_ID, BOOK_ID) DO NOTHING",
    )
    .bind(new_book_id)
    .bind(old_book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("copy readlist mapping rows during upgrade migration: {error}"))?;
    sqlx::query("DELETE FROM READLIST_BOOK WHERE BOOK_ID = ?")
        .bind(old_book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            format!("delete source readlist mappings during upgrade migration: {error}")
        })?;

    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind(old_book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("delete source thumbnails during upgrade migration: {error}"))?;

    sqlx::query("DELETE FROM BOOK WHERE ID = ?")
        .bind(old_book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("delete source book after upgrade migration: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit import-upgrade migration tx: {error}"))?;
    Ok(())
}

async fn persist_book_imported_event(
    database_file: &Path,
    book_id: &str,
    series_id: &str,
    destination_file: &Path,
    source_file: &Path,
    upgrade: bool,
) -> Result<(), String> {
    if !database_file.exists() {
        return Ok(());
    }

    let event_id = generated_historical_event_id();
    let destination_name = destination_file.to_string_lossy().to_string();
    let source_name = source_file.to_string_lossy().to_string();
    let upgrade_value = if upgrade { "Yes" } else { "No" };

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open historical-event db for import: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin historical-event tx for import: {error}"))?;

    sqlx::query("INSERT INTO HISTORICAL_EVENT (ID, TYPE, BOOK_ID, SERIES_ID) VALUES (?, ?, ?, ?)")
        .bind(&event_id)
        .bind("BookImported")
        .bind(book_id)
        .bind(series_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("insert historical BookImported event: {error}"))?;

    for (key, value) in [
        ("name", destination_name.as_str()),
        ("source", source_name.as_str()),
        ("upgrade", upgrade_value),
    ] {
        sqlx::query("INSERT INTO HISTORICAL_EVENT_PROPERTIES (ID, KEY, VALUE) VALUES (?, ?, ?)")
            .bind(&event_id)
            .bind(key)
            .bind(value)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                format!("insert historical event property '{key}' for BookImported: {error}")
            })?;
    }

    tx.commit()
        .await
        .map_err(|error| format!("commit historical-event tx for import: {error}"))?;

    Ok(())
}

fn generated_historical_event_id() -> String {
    random_prefixed_id("historical-event")
}

fn random_prefixed_id(prefix: &str) -> String {
    format!("{prefix}-{}", random_hex_token(12))
}

fn random_hex_token(byte_len: usize) -> String {
    let mut bytes = vec![0u8; byte_len];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        let _ = file.read_exact(&mut bytes);
    } else {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = ((seed >> ((index % 8) * 8)) as u8) ^ (index as u8).wrapping_mul(29);
        }
    }

    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_dir(case: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "komga-import-{case}-{nanos}-{}",
            std::process::id()
        ))
    }

    async fn create_test_db(case: &str) -> (PathBuf, sqlx::Pool<sqlx::Sqlite>, PathBuf) {
        let root = unique_temp_dir(case);
        fs::create_dir_all(&root).expect("temp root should be created");
        let db_path = root.join("import.sqlite");
        let pool = connect_pool(&db_path, 1)
            .await
            .expect("test db should open");

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS LIBRARY (ID varchar NOT NULL PRIMARY KEY, ROOT varchar NOT NULL)",
        )
        .execute(&pool)
        .await
        .expect("library table should be created");
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS SERIES (ID varchar NOT NULL PRIMARY KEY, LIBRARY_ID varchar NOT NULL, URL varchar NOT NULL)",
        )
        .execute(&pool)
        .await
        .expect("series table should be created");
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS BOOK (ID varchar NOT NULL PRIMARY KEY, SERIES_ID varchar NOT NULL, LIBRARY_ID varchar NOT NULL, NAME varchar NOT NULL)",
        )
        .execute(&pool)
        .await
        .expect("book table should be created");

        let library_root = root.join("library-root");
        fs::create_dir_all(&library_root).expect("library root should be created");
        sqlx::query("INSERT INTO LIBRARY (ID, ROOT) VALUES (?, ?)")
            .bind("library-1")
            .bind(library_root.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .expect("library row should be inserted");
        sqlx::query("INSERT INTO SERIES (ID, LIBRARY_ID, URL) VALUES (?, ?, ?)")
            .bind("series-1")
            .bind("library-1")
            .bind("series-one")
            .execute(&pool)
            .await
            .expect("series row should be inserted");
        sqlx::query("INSERT INTO SERIES (ID, LIBRARY_ID, URL) VALUES (?, ?, ?)")
            .bind("series-2")
            .bind("library-1")
            .bind("series-two")
            .execute(&pool)
            .await
            .expect("second series row should be inserted");

        (db_path, pool, root)
    }

    #[tokio::test]
    async fn import_book_returns_error_when_source_file_is_missing() {
        let (db_path, pool, root) = create_test_db("missing-source").await;
        let port = FilesystemImportPort::new(&db_path);

        let result = port
            .import_book(
                ImportCopyMode::Copy,
                BooksImportEntry {
                    source_file: root.join("missing.cbz"),
                    series_id: "series-1".to_string(),
                    destination_name: None,
                    upgrade_book_id: None,
                },
            )
            .await;

        let error = result.expect_err("missing source import should return an error");
        assert!(
            error.contains("source file does not exist"),
            "unexpected import error: {error}"
        );

        pool.close().await;
    }

    #[tokio::test]
    async fn import_book_returns_error_when_series_target_is_missing() {
        let (db_path, pool, root) = create_test_db("missing-series-target").await;
        let port = FilesystemImportPort::new(&db_path);
        let source_path = root.join("book.cbz");
        fs::write(&source_path, b"fixture").expect("source fixture should be written");

        let result = port
            .import_book(
                ImportCopyMode::Copy,
                BooksImportEntry {
                    source_file: source_path,
                    series_id: "missing-series".to_string(),
                    destination_name: None,
                    upgrade_book_id: None,
                },
            )
            .await;

        let error = result.expect_err("missing series target import should return an error");
        assert!(
            error.contains("series target") || error.contains("missing-series"),
            "unexpected import error: {error}"
        );

        pool.close().await;
    }

    #[tokio::test]
    async fn import_book_returns_error_when_destination_name_is_invalid() {
        let (db_path, pool, root) = create_test_db("invalid-destination-name").await;
        let port = FilesystemImportPort::new(&db_path);
        let source_path = root.join("book.cbz");
        fs::write(&source_path, b"fixture").expect("source fixture should be written");

        let result = port
            .import_book(
                ImportCopyMode::Copy,
                BooksImportEntry {
                    source_file: source_path,
                    series_id: "series-1".to_string(),
                    destination_name: Some("nested/book.cbz".to_string()),
                    upgrade_book_id: None,
                },
            )
            .await;

        let error = result.expect_err("invalid destination name import should return an error");
        assert!(
            error.contains("destination") || error.contains("nested/book.cbz"),
            "unexpected import error: {error}"
        );

        pool.close().await;
    }

    #[tokio::test]
    async fn import_book_returns_error_when_upgrade_target_series_mismatches() {
        let (db_path, pool, root) = create_test_db("upgrade-series-mismatch").await;
        let port = FilesystemImportPort::new(&db_path);
        let source_path = root.join("book.cbz");
        fs::write(&source_path, b"fixture").expect("source fixture should be written");

        sqlx::query("INSERT INTO BOOK (ID, SERIES_ID, LIBRARY_ID, NAME) VALUES (?, ?, ?, ?)")
            .bind("book-upgrade")
            .bind("series-2")
            .bind("library-1")
            .bind("existing.cbz")
            .execute(&pool)
            .await
            .expect("upgrade book row should be inserted");

        let result = port
            .import_book(
                ImportCopyMode::Copy,
                BooksImportEntry {
                    source_file: source_path,
                    series_id: "series-1".to_string(),
                    destination_name: None,
                    upgrade_book_id: Some("book-upgrade".to_string()),
                },
            )
            .await;

        let error = result.expect_err("upgrade series mismatch should return an error");
        assert!(
            error.contains("upgrade") || error.contains("series"),
            "unexpected import error: {error}"
        );

        pool.close().await;
    }

    #[tokio::test]
    async fn import_book_returns_error_when_upgrade_target_is_missing() {
        let (db_path, pool, root) = create_test_db("upgrade-target-missing").await;
        let port = FilesystemImportPort::new(&db_path);
        let source_path = root.join("book.cbz");
        fs::write(&source_path, b"fixture").expect("source fixture should be written");

        let result = port
            .import_book(
                ImportCopyMode::Copy,
                BooksImportEntry {
                    source_file: source_path,
                    series_id: "series-1".to_string(),
                    destination_name: None,
                    upgrade_book_id: Some("missing-upgrade-book".to_string()),
                },
            )
            .await;

        let error = result.expect_err("missing upgrade target should return an error");
        assert!(
            error.contains("upgrade") || error.contains("missing-upgrade-book"),
            "unexpected import error: {error}"
        );

        pool.close().await;
    }
}

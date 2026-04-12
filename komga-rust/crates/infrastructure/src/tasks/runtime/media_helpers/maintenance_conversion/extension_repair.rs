use super::*;
use crate::tasks::{
    PersistedExtensionRepairTarget, load_book_for_extension_repair,
    load_books_for_extension_repair, persist_book_extension_repair,
};
use crate::{resolve_library_item_path, resolve_stored_path};
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

static SKIPPED_EXTENSION_REPAIRS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn skipped_extension_repairs() -> &'static Mutex<HashSet<String>> {
    SKIPPED_EXTENSION_REPAIRS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn skipped_extension_repair_key(database_file: &Path, book_id: &str) -> String {
    format!("{}::{book_id}", database_file.display())
}

#[cfg(test)]
fn skipped_extension_repair_key_prefix(database_file: &Path) -> String {
    format!("{}::", database_file.display())
}

fn extension_repair_was_skipped(cache_key: &str) -> bool {
    skipped_extension_repairs()
        .lock()
        .expect("skipped extension repairs lock should not be poisoned")
        .contains(cache_key)
}

fn mark_extension_repair_skipped(cache_key: &str) {
    skipped_extension_repairs()
        .lock()
        .expect("skipped extension repairs lock should not be poisoned")
        .insert(cache_key.to_string());
}

pub(in crate::task_queue) fn find_books_for_extension_repair(
    runtime: &RuntimeConfig,
    library_id: &str,
) -> Result<Vec<PersistedExtensionRepairTarget>, TaskExecutionError> {
    let flags = load_library_maintenance_flags(runtime, library_id)?;
    if !flags.repair_extensions {
        return Ok(Vec::new());
    }

    let runtime = runtime.task_runtime_context();
    load_books_for_extension_repair(runtime.database_file.as_path(), library_id)
        .map_err(TaskExecutionError::runtime)
}

pub(in crate::task_queue) fn repair_extension(
    runtime: &RuntimeConfig,
    book_id: &str,
) -> Result<(), TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    let database_file = runtime.database_file.clone();
    let skip_cache_key = skipped_extension_repair_key(database_file.as_path(), book_id);

    let Some(row) = load_book_for_extension_repair(database_file.as_path(), book_id)
        .map_err(TaskExecutionError::runtime)?
    else {
        return Ok(());
    };

    let flags = load_library_maintenance_flags(&runtime, &row.library_id)?;
    if !flags.repair_extensions {
        return Ok(());
    }

    let book_id = row.book_id;
    let book_url = row.book_url;
    let library_root = row.library_root;
    let library_id = row.library_id;
    let media_type = row.media_type;

    if extension_repair_was_skipped(&skip_cache_key) {
        return Ok(());
    }

    let Some(correct_extension) = expected_extension_for_media_type(&media_type) else {
        return Ok(());
    };

    let resolved_library_root = resolve_stored_path(&library_root);
    let source_path = resolve_library_item_path(&library_root, &book_url);
    if !source_path.exists() {
        return Ok(());
    }

    let current_extension = source_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    if current_extension == correct_extension {
        return Ok(());
    }

    if media_type == "application/zip" && current_extension == "epub" {
        mark_extension_repair_skipped(&skip_cache_key);
        return Ok(());
    }

    let destination_path = source_path.with_extension(correct_extension);
    if destination_path.exists() {
        return Err(TaskExecutionError::runtime(format!(
            "failed to repair extension for '{book_id}': destination already exists '{}'",
            destination_path.display(),
        )));
    }

    fs::rename(&source_path, &destination_path).map_err(|error| {
        TaskExecutionError::runtime(format!(
            "failed to rename book file for extension repair '{}' -> '{}': {error}",
            source_path.display(),
            destination_path.display(),
        ))
    })?;

    let destination_url =
        normalize_library_relative_url(&resolved_library_root, &destination_path)?;
    let file_size = fs::metadata(&destination_path)
        .map(|metadata| metadata.len() as i64)
        .unwrap_or_default();
    let file_last_modified = fs::metadata(&destination_path)
        .map(|metadata| metadata_updated_unix_seconds(&metadata))
        .unwrap_or_default();

    let repair_result = persist_book_extension_repair(
        database_file.as_path(),
        &book_id,
        &library_id,
        &book_url,
        &destination_url,
        file_last_modified,
        file_size,
    )
    .map_err(TaskExecutionError::runtime);

    if let Err(error) = repair_result {
        let _ = fs::rename(&destination_path, &source_path);
        return Err(error);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::connect_pool;
    use komga_application::task_processing::TaskRuntimeContext;
    use sqlx::Row;

    fn unique_temp_path(prefix: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock should be after unix epoch")
                .as_nanos()
        ))
    }

    fn clear_skipped_extension_repairs_for_test_database(database_file: &Path) {
        skipped_extension_repairs()
            .lock()
            .expect("skipped extension repairs lock should not be poisoned")
            .retain(|entry| {
                !entry.starts_with(&skipped_extension_repair_key_prefix(database_file))
            });
    }

    #[tokio::test]
    async fn repair_extensions_remembers_previously_skipped_books_within_process() {
        let database_file = unique_temp_path("komga-repair-extensions-main");
        clear_skipped_extension_repairs_for_test_database(database_file.as_path());
        let config_dir = unique_temp_path("komga-repair-extensions-config");
        std::fs::create_dir_all(config_dir.join("books"))
            .expect("repair-extensions config dir should be created");
        let source_path = config_dir.join("books/repair-book.epub");
        std::fs::write(&source_path, b"repair-extension-skip-fixture")
            .expect("repair-extensions source file should be written");

        let pool = connect_pool(database_file.as_path(), 1)
            .await
            .expect("repair-extensions test db should open");
        for ddl in [
            "CREATE TABLE LIBRARY (ID varchar NOT NULL PRIMARY KEY, ROOT varchar NOT NULL, REPAIR_EXTENSIONS integer NOT NULL DEFAULT 0, CONVERT_TO_CBZ integer NOT NULL DEFAULT 0)",
            "CREATE TABLE BOOK (ID varchar NOT NULL PRIMARY KEY, URL varchar NOT NULL, LIBRARY_ID varchar NOT NULL, SERIES_ID varchar NOT NULL DEFAULT '', FILE_LAST_MODIFIED int NOT NULL DEFAULT 0, FILE_SIZE int NOT NULL DEFAULT 0, LAST_MODIFIED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP, DELETED_DATE timestamp NULL)",
            "CREATE TABLE MEDIA (BOOK_ID varchar NOT NULL PRIMARY KEY, MEDIA_TYPE varchar NOT NULL)",
            "CREATE TABLE SIDECAR (URL varchar NOT NULL PRIMARY KEY, PARENT_URL varchar NOT NULL, LIBRARY_ID varchar NOT NULL)",
        ] {
            sqlx::query(ddl)
                .execute(&pool)
                .await
                .expect("repair-extensions fixture schema should be created");
        }
        sqlx::query("INSERT INTO LIBRARY (ID, ROOT, REPAIR_EXTENSIONS) VALUES (?, ?, ?)")
            .bind("library-1")
            .bind(config_dir.to_string_lossy().to_string())
            .bind(true)
            .execute(&pool)
            .await
            .expect("repair-extensions library row should be inserted");
        sqlx::query(
            "INSERT INTO BOOK (ID, URL, LIBRARY_ID, SERIES_ID, FILE_LAST_MODIFIED, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("book-1")
        .bind("books/repair-book.epub")
        .bind("library-1")
        .bind("series-1")
        .bind(0_i64)
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("repair-extensions book row should be inserted");
        sqlx::query("INSERT INTO MEDIA (BOOK_ID, MEDIA_TYPE) VALUES (?, ?)")
            .bind("book-1")
            .bind("application/zip")
            .execute(&pool)
            .await
            .expect("repair-extensions media row should be inserted");
        pool.close().await;

        let runtime = TaskRuntimeContext {
            database_file: database_file.clone(),
            tasks_db_file: unique_temp_path("komga-repair-extensions-tasks"),
            lucene_data_directory: unique_temp_path("komga-repair-extensions-lucene"),
            consumes_queue: true,
            owns_main_database: true,
            owns_filesystem_scan_output: true,
            owns_sidecar_output: true,
            owns_search_index: true,
        };

        repair_extension(&runtime, "book-1")
            .expect("first repair-extension call should skip EPUB-detected-as-ZIP cleanly");

        let pool = connect_pool(database_file.as_path(), 1)
            .await
            .expect("repair-extensions db should reopen for media mutation");
        sqlx::query("UPDATE MEDIA SET MEDIA_TYPE = ? WHERE BOOK_ID = ?")
            .bind("application/pdf")
            .bind("book-1")
            .execute(&pool)
            .await
            .expect("repair-extensions media type should be changed after first skipped run");
        pool.close().await;

        repair_extension(&runtime, "book-1")
            .expect("second repair-extension call should short-circuit previously skipped books");

        let verify_pool = connect_pool(database_file.as_path(), 1)
            .await
            .expect("repair-extensions db should reopen for verification");
        let row = sqlx::query("SELECT URL FROM BOOK WHERE ID = ? LIMIT 1")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect("repair-extensions book row should be queryable");
        verify_pool.close().await;

        assert_eq!(row.get::<String, _>("URL"), "books/repair-book.epub");
        assert!(
            source_path.exists(),
            "skipped repair cache should prevent later runs from renaming the original file",
        );
        assert!(
            !config_dir.join("books/repair-book.pdf").exists(),
            "skipped repair cache should suppress later extension repair work for the same book id",
        );

        clear_skipped_extension_repairs_for_test_database(database_file.as_path());
        let _ = std::fs::remove_file(source_path);
        let _ = std::fs::remove_dir_all(config_dir);
        let _ = std::fs::remove_file(database_file);
    }

    #[tokio::test]
    async fn repair_extensions_does_not_cache_books_that_were_already_correct() {
        let database_file = unique_temp_path("komga-repair-extensions-candidate-main");
        clear_skipped_extension_repairs_for_test_database(database_file.as_path());
        let config_dir = unique_temp_path("komga-repair-extensions-candidate-config");
        std::fs::create_dir_all(config_dir.join("books"))
            .expect("repair-extensions candidate config dir should be created");
        let source_path = config_dir.join("books/repair-book.pdf");
        std::fs::write(&source_path, b"repair-extension-candidate-fixture")
            .expect("repair-extensions candidate source file should be written");

        let pool = connect_pool(database_file.as_path(), 1)
            .await
            .expect("repair-extensions candidate test db should open");
        for ddl in [
            "CREATE TABLE LIBRARY (ID varchar NOT NULL PRIMARY KEY, ROOT varchar NOT NULL, REPAIR_EXTENSIONS integer NOT NULL DEFAULT 0, CONVERT_TO_CBZ integer NOT NULL DEFAULT 0)",
            "CREATE TABLE BOOK (ID varchar NOT NULL PRIMARY KEY, URL varchar NOT NULL, LIBRARY_ID varchar NOT NULL, SERIES_ID varchar NOT NULL DEFAULT '', FILE_LAST_MODIFIED int NOT NULL DEFAULT 0, FILE_SIZE int NOT NULL DEFAULT 0, LAST_MODIFIED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP, DELETED_DATE timestamp NULL)",
            "CREATE TABLE MEDIA (BOOK_ID varchar NOT NULL PRIMARY KEY, MEDIA_TYPE varchar NOT NULL)",
            "CREATE TABLE SIDECAR (URL varchar NOT NULL PRIMARY KEY, PARENT_URL varchar NOT NULL, LIBRARY_ID varchar NOT NULL)",
        ] {
            sqlx::query(ddl)
                .execute(&pool)
                .await
                .expect("repair-extensions candidate fixture schema should be created");
        }
        sqlx::query("INSERT INTO LIBRARY (ID, ROOT, REPAIR_EXTENSIONS) VALUES (?, ?, ?)")
            .bind("library-1")
            .bind(config_dir.to_string_lossy().to_string())
            .bind(true)
            .execute(&pool)
            .await
            .expect("repair-extensions candidate library row should be inserted");
        sqlx::query(
            "INSERT INTO BOOK (ID, URL, LIBRARY_ID, SERIES_ID, FILE_LAST_MODIFIED, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("book-1")
        .bind("books/repair-book.pdf")
        .bind("library-1")
        .bind("series-1")
        .bind(0_i64)
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("repair-extensions candidate book row should be inserted");
        sqlx::query("INSERT INTO MEDIA (BOOK_ID, MEDIA_TYPE) VALUES (?, ?)")
            .bind("book-1")
            .bind("application/pdf")
            .execute(&pool)
            .await
            .expect("repair-extensions candidate media row should be inserted");
        pool.close().await;

        let runtime = TaskRuntimeContext {
            database_file: database_file.clone(),
            tasks_db_file: unique_temp_path("komga-repair-extensions-candidate-tasks"),
            lucene_data_directory: unique_temp_path("komga-repair-extensions-candidate-lucene"),
            consumes_queue: true,
            owns_main_database: true,
            owns_filesystem_scan_output: true,
            owns_sidecar_output: true,
            owns_search_index: true,
        };

        repair_extension(&runtime, "book-1")
            .expect("first repair-extension call should ignore already-correct books cleanly");

        let pool = connect_pool(database_file.as_path(), 1)
            .await
            .expect("repair-extensions candidate db should reopen for mismatch mutation");
        std::fs::rename(&source_path, config_dir.join("books/repair-book.bin"))
            .expect("repair-extensions candidate file should be renamed to mismatched extension");
        sqlx::query("UPDATE BOOK SET URL = ? WHERE ID = ?")
            .bind("books/repair-book.bin")
            .bind("book-1")
            .execute(&pool)
            .await
            .expect("repair-extensions candidate book url should be changed after first run");
        pool.close().await;

        repair_extension(&runtime, "book-1")
            .expect("second repair-extension call should repair newly mismatched books");

        let verify_pool = connect_pool(database_file.as_path(), 1)
            .await
            .expect("repair-extensions candidate db should reopen for verification");
        let row = sqlx::query("SELECT URL FROM BOOK WHERE ID = ? LIMIT 1")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect("repair-extensions candidate book row should be queryable");
        verify_pool.close().await;

        assert_eq!(row.get::<String, _>("URL"), "books/repair-book.pdf");
        assert!(
            source_path.exists(),
            "already-correct books must not be cached as skipped, so later mismatches still repair back to the correct extension",
        );
        assert!(
            !config_dir.join("books/repair-book.bin").exists(),
            "later mismatched files should still be repaired when the first run only observed a correct extension",
        );

        clear_skipped_extension_repairs_for_test_database(database_file.as_path());
        let _ = std::fs::remove_file(source_path);
        let _ = std::fs::remove_dir_all(config_dir);
        let _ = std::fs::remove_file(database_file);
    }

    #[tokio::test]
    async fn repair_extensions_skip_cache_isolated_by_runtime_database() {
        let skipped_db = unique_temp_path("komga-repair-extensions-isolated-skipped-main");
        clear_skipped_extension_repairs_for_test_database(skipped_db.as_path());
        let skipped_config_dir =
            unique_temp_path("komga-repair-extensions-isolated-skipped-config");
        std::fs::create_dir_all(skipped_config_dir.join("books"))
            .expect("isolated skipped config dir should be created");
        let skipped_source_path = skipped_config_dir.join("books/repair-book.epub");
        std::fs::write(&skipped_source_path, b"isolated-skipped-fixture")
            .expect("isolated skipped source file should be written");

        let skipped_pool = connect_pool(skipped_db.as_path(), 1)
            .await
            .expect("isolated skipped test db should open");
        for ddl in [
            "CREATE TABLE LIBRARY (ID varchar NOT NULL PRIMARY KEY, ROOT varchar NOT NULL, REPAIR_EXTENSIONS integer NOT NULL DEFAULT 0, CONVERT_TO_CBZ integer NOT NULL DEFAULT 0)",
            "CREATE TABLE BOOK (ID varchar NOT NULL PRIMARY KEY, URL varchar NOT NULL, LIBRARY_ID varchar NOT NULL, SERIES_ID varchar NOT NULL DEFAULT '', FILE_LAST_MODIFIED int NOT NULL DEFAULT 0, FILE_SIZE int NOT NULL DEFAULT 0, LAST_MODIFIED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP, DELETED_DATE timestamp NULL)",
            "CREATE TABLE MEDIA (BOOK_ID varchar NOT NULL PRIMARY KEY, MEDIA_TYPE varchar NOT NULL)",
            "CREATE TABLE SIDECAR (URL varchar NOT NULL PRIMARY KEY, PARENT_URL varchar NOT NULL, LIBRARY_ID varchar NOT NULL)",
        ] {
            sqlx::query(ddl)
                .execute(&skipped_pool)
                .await
                .expect("isolated skipped fixture schema should be created");
        }
        sqlx::query("INSERT INTO LIBRARY (ID, ROOT, REPAIR_EXTENSIONS) VALUES (?, ?, ?)")
            .bind("library-1")
            .bind(skipped_config_dir.to_string_lossy().to_string())
            .bind(true)
            .execute(&skipped_pool)
            .await
            .expect("isolated skipped library row should be inserted");
        sqlx::query(
            "INSERT INTO BOOK (ID, URL, LIBRARY_ID, SERIES_ID, FILE_LAST_MODIFIED, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("book-1")
        .bind("books/repair-book.epub")
        .bind("library-1")
        .bind("series-1")
        .bind(0_i64)
        .bind(0_i64)
        .execute(&skipped_pool)
        .await
        .expect("isolated skipped book row should be inserted");
        sqlx::query("INSERT INTO MEDIA (BOOK_ID, MEDIA_TYPE) VALUES (?, ?)")
            .bind("book-1")
            .bind("application/zip")
            .execute(&skipped_pool)
            .await
            .expect("isolated skipped media row should be inserted");
        skipped_pool.close().await;

        let skipped_runtime = TaskRuntimeContext {
            database_file: skipped_db.clone(),
            tasks_db_file: unique_temp_path("komga-repair-extensions-isolated-skipped-tasks"),
            lucene_data_directory: unique_temp_path(
                "komga-repair-extensions-isolated-skipped-lucene",
            ),
            consumes_queue: true,
            owns_main_database: true,
            owns_filesystem_scan_output: true,
            owns_sidecar_output: true,
            owns_search_index: true,
        };

        repair_extension(&skipped_runtime, "book-1")
            .expect("first runtime should mark its epub-detected-as-zip book as skipped");

        let candidate_db = unique_temp_path("komga-repair-extensions-isolated-candidate-main");
        clear_skipped_extension_repairs_for_test_database(candidate_db.as_path());
        let candidate_config_dir =
            unique_temp_path("komga-repair-extensions-isolated-candidate-config");
        std::fs::create_dir_all(candidate_config_dir.join("books"))
            .expect("isolated candidate config dir should be created");
        let candidate_bin_path = candidate_config_dir.join("books/repair-book.bin");
        std::fs::write(&candidate_bin_path, b"isolated-candidate-fixture")
            .expect("isolated candidate source file should be written");

        let candidate_pool = connect_pool(candidate_db.as_path(), 1)
            .await
            .expect("isolated candidate test db should open");
        for ddl in [
            "CREATE TABLE LIBRARY (ID varchar NOT NULL PRIMARY KEY, ROOT varchar NOT NULL, REPAIR_EXTENSIONS integer NOT NULL DEFAULT 0, CONVERT_TO_CBZ integer NOT NULL DEFAULT 0)",
            "CREATE TABLE BOOK (ID varchar NOT NULL PRIMARY KEY, URL varchar NOT NULL, LIBRARY_ID varchar NOT NULL, SERIES_ID varchar NOT NULL DEFAULT '', FILE_LAST_MODIFIED int NOT NULL DEFAULT 0, FILE_SIZE int NOT NULL DEFAULT 0, LAST_MODIFIED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP, DELETED_DATE timestamp NULL)",
            "CREATE TABLE MEDIA (BOOK_ID varchar NOT NULL PRIMARY KEY, MEDIA_TYPE varchar NOT NULL)",
            "CREATE TABLE SIDECAR (URL varchar NOT NULL PRIMARY KEY, PARENT_URL varchar NOT NULL, LIBRARY_ID varchar NOT NULL)",
        ] {
            sqlx::query(ddl)
                .execute(&candidate_pool)
                .await
                .expect("isolated candidate fixture schema should be created");
        }
        sqlx::query("INSERT INTO LIBRARY (ID, ROOT, REPAIR_EXTENSIONS) VALUES (?, ?, ?)")
            .bind("library-1")
            .bind(candidate_config_dir.to_string_lossy().to_string())
            .bind(true)
            .execute(&candidate_pool)
            .await
            .expect("isolated candidate library row should be inserted");
        sqlx::query(
            "INSERT INTO BOOK (ID, URL, LIBRARY_ID, SERIES_ID, FILE_LAST_MODIFIED, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("book-1")
        .bind("books/repair-book.bin")
        .bind("library-1")
        .bind("series-1")
        .bind(0_i64)
        .bind(0_i64)
        .execute(&candidate_pool)
        .await
        .expect("isolated candidate book row should be inserted");
        sqlx::query("INSERT INTO MEDIA (BOOK_ID, MEDIA_TYPE) VALUES (?, ?)")
            .bind("book-1")
            .bind("application/pdf")
            .execute(&candidate_pool)
            .await
            .expect("isolated candidate media row should be inserted");
        candidate_pool.close().await;

        let candidate_runtime = TaskRuntimeContext {
            database_file: candidate_db.clone(),
            tasks_db_file: unique_temp_path("komga-repair-extensions-isolated-candidate-tasks"),
            lucene_data_directory: unique_temp_path(
                "komga-repair-extensions-isolated-candidate-lucene",
            ),
            consumes_queue: true,
            owns_main_database: true,
            owns_filesystem_scan_output: true,
            owns_sidecar_output: true,
            owns_search_index: true,
        };

        repair_extension(&candidate_runtime, "book-1")
            .expect("separate runtime database should still repair its own mismatched book");

        let verify_pool = connect_pool(candidate_db.as_path(), 1)
            .await
            .expect("isolated candidate db should reopen for verification");
        let row = sqlx::query("SELECT URL FROM BOOK WHERE ID = ? LIMIT 1")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect("isolated candidate book row should be queryable");
        verify_pool.close().await;

        assert_eq!(row.get::<String, _>("URL"), "books/repair-book.pdf");
        assert!(
            candidate_config_dir.join("books/repair-book.pdf").exists(),
            "skip cache must not leak across distinct runtime databases that reuse the same book id",
        );
        assert!(
            !candidate_bin_path.exists(),
            "candidate runtime should finish the rename instead of short-circuiting on another database's cached skip",
        );

        clear_skipped_extension_repairs_for_test_database(skipped_db.as_path());
        clear_skipped_extension_repairs_for_test_database(candidate_db.as_path());
        let _ = std::fs::remove_file(skipped_source_path);
        let _ = std::fs::remove_dir_all(skipped_config_dir);
        let _ = std::fs::remove_file(skipped_db);
        let _ = std::fs::remove_dir_all(candidate_config_dir);
        let _ = std::fs::remove_file(candidate_db);
    }
}

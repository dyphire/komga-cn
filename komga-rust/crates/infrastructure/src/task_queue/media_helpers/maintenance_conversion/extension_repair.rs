use super::super::media_queries::load_book_for_extension_repair;
use super::super::media_updates::persist_book_extension_repair;
use super::*;
use crate::{resolve_library_item_path, resolve_stored_path};
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use tokio::fs;

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

pub(in crate::task_queue) async fn repair_extension(
    runtime: &RuntimeConfig,
    book_id: &str,
) -> Result<(), TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    let database_file = runtime.main_db.database_file().to_path_buf();
    let skip_cache_key = skipped_extension_repair_key(database_file.as_path(), book_id);

    let Some(row) = load_book_for_extension_repair(database_file.as_path(), book_id)
        .await
        .map_err(TaskExecutionError::runtime)?
    else {
        return Ok(());
    };

    let flags = load_library_maintenance_flags(&runtime, &row.library_id).await?;
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
    if fs::metadata(&source_path).await.is_err() {
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
    if fs::metadata(&destination_path).await.is_ok() {
        return Err(TaskExecutionError::runtime(format!(
            "failed to repair extension for '{book_id}': destination already exists '{}'",
            destination_path.display(),
        )));
    }

    fs::rename(&source_path, &destination_path)
        .await
        .map_err(|error| {
            TaskExecutionError::runtime(format!(
                "failed to rename book file for extension repair '{}' -> '{}': {error}",
                source_path.display(),
                destination_path.display(),
            ))
        })?;

    let destination_metadata = fs::metadata(&destination_path).await.map_err(|error| {
        TaskExecutionError::runtime(format!(
            "failed to load repaired file metadata '{}' for '{}': {error}",
            destination_path.display(),
            book_id,
        ))
    })?;
    let destination_url =
        normalize_library_relative_url(&resolved_library_root, &destination_path)?;
    let file_size = destination_metadata.len() as i64;
    let file_last_modified = metadata_updated_unix_seconds(&destination_metadata);

    let repair_result = persist_book_extension_repair(
        database_file.as_path(),
        &book_id,
        &library_id,
        &book_url,
        &destination_url,
        file_last_modified,
        file_size,
    )
    .await
    .map_err(TaskExecutionError::runtime);

    if let Err(error) = repair_result {
        let _ = fs::rename(&destination_path, &source_path).await;
        return Err(error);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::connect_test_pool;
    use crate::task_queue::test_support::RuntimeTestFixture;
    use sqlx::Row;

    fn clear_skipped_extension_repairs_for_test_database(database_file: &Path) {
        skipped_extension_repairs()
            .lock()
            .expect("skipped extension repairs lock should not be poisoned")
            .retain(|entry| {
                !entry.starts_with(&skipped_extension_repair_key_prefix(database_file))
            });
    }

    async fn seed_extension_repair_fixture(
        fixture: &RuntimeTestFixture,
        book_url: &str,
        media_type: &str,
    ) {
        let pool = fixture.main_pool().await;
        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT, REPAIR_EXTENSIONS) VALUES (?, ?, ?, ?)")
            .bind("library-1")
            .bind("Library 1")
            .bind(fixture.library_root.to_string_lossy().to_string())
            .bind(true)
            .execute(&pool)
            .await
            .expect("repair-extensions library row should be inserted");
        sqlx::query(
            "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?)",
        )
        .bind("series-1")
        .bind(0_i64)
        .bind("Series 1")
        .bind("series/series-1")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("repair-extensions series row should be inserted");
        sqlx::query(
            "INSERT INTO BOOK (ID, NAME, URL, LIBRARY_ID, SERIES_ID, FILE_LAST_MODIFIED, FILE_SIZE) VALUES (?, ?, ?, ?, ?, datetime(?, 'unixepoch'), ?)",
        )
        .bind("book-1")
        .bind("book-1")
        .bind(book_url)
        .bind("library-1")
        .bind("series-1")
        .bind(0_i64)
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("repair-extensions book row should be inserted");
        sqlx::query("INSERT INTO MEDIA (BOOK_ID, MEDIA_TYPE, STATUS) VALUES (?, ?, ?)")
            .bind("book-1")
            .bind(media_type)
            .bind("READY")
            .execute(&pool)
            .await
            .expect("repair-extensions media row should be inserted");
        pool.close().await;
    }

    #[tokio::test]
    async fn repair_extensions_remembers_previously_skipped_books_within_process() {
        let fixture = RuntimeTestFixture::new("repair-extensions-main");
        clear_skipped_extension_repairs_for_test_database(fixture.database_file.as_path());
        std::fs::create_dir_all(fixture.library_root.join("books"))
            .expect("repair-extensions config dir should be created");
        let source_path = fixture.library_root.join("books/repair-book.epub");
        std::fs::write(&source_path, b"repair-extension-skip-fixture")
            .expect("repair-extensions source file should be written");

        seed_extension_repair_fixture(&fixture, "books/repair-book.epub", "application/zip").await;

        let runtime = fixture.runtime_context(true, true).await;

        repair_extension(&runtime, "book-1")
            .await
            .expect("first repair-extension call should skip EPUB-detected-as-ZIP cleanly");

        let pool = connect_test_pool(fixture.database_file.as_path(), 1)
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
            .await
            .expect("second repair-extension call should short-circuit previously skipped books");

        let verify_pool = connect_test_pool(fixture.database_file.as_path(), 1)
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
            !fixture.library_root.join("books/repair-book.pdf").exists(),
            "skipped repair cache should suppress later extension repair work for the same book id",
        );

        clear_skipped_extension_repairs_for_test_database(fixture.database_file.as_path());
        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn repair_extensions_does_not_cache_books_that_were_already_correct() {
        let fixture = RuntimeTestFixture::new("repair-extensions-candidate");
        clear_skipped_extension_repairs_for_test_database(fixture.database_file.as_path());
        std::fs::create_dir_all(fixture.library_root.join("books"))
            .expect("repair-extensions candidate config dir should be created");
        let source_path = fixture.library_root.join("books/repair-book.pdf");
        std::fs::write(&source_path, b"repair-extension-candidate-fixture")
            .expect("repair-extensions candidate source file should be written");

        seed_extension_repair_fixture(&fixture, "books/repair-book.pdf", "application/pdf").await;

        let runtime = fixture.runtime_context(true, true).await;

        repair_extension(&runtime, "book-1")
            .await
            .expect("first repair-extension call should ignore already-correct books cleanly");

        let pool = connect_test_pool(fixture.database_file.as_path(), 1)
            .await
            .expect("repair-extensions candidate db should reopen for mismatch mutation");
        std::fs::rename(
            &source_path,
            fixture.library_root.join("books/repair-book.bin"),
        )
        .expect("repair-extensions candidate file should be renamed to mismatched extension");
        sqlx::query("UPDATE BOOK SET URL = ? WHERE ID = ?")
            .bind("books/repair-book.bin")
            .bind("book-1")
            .execute(&pool)
            .await
            .expect("repair-extensions candidate book url should be changed after first run");
        pool.close().await;

        repair_extension(&runtime, "book-1")
            .await
            .expect("second repair-extension call should repair newly mismatched books");

        let verify_pool = connect_test_pool(fixture.database_file.as_path(), 1)
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
            !fixture.library_root.join("books/repair-book.bin").exists(),
            "later mismatched files should still be repaired when the first run only observed a correct extension",
        );

        clear_skipped_extension_repairs_for_test_database(fixture.database_file.as_path());
        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn repair_extensions_skip_cache_isolated_by_runtime_database() {
        let skipped_fixture = RuntimeTestFixture::new("repair-extensions-isolated-skipped");
        clear_skipped_extension_repairs_for_test_database(skipped_fixture.database_file.as_path());
        std::fs::create_dir_all(skipped_fixture.library_root.join("books"))
            .expect("isolated skipped config dir should be created");
        let skipped_source_path = skipped_fixture.library_root.join("books/repair-book.epub");
        std::fs::write(&skipped_source_path, b"isolated-skipped-fixture")
            .expect("isolated skipped source file should be written");

        seed_extension_repair_fixture(
            &skipped_fixture,
            "books/repair-book.epub",
            "application/zip",
        )
        .await;

        let skipped_runtime = skipped_fixture.runtime_context(true, true).await;

        repair_extension(&skipped_runtime, "book-1")
            .await
            .expect("first runtime should mark its epub-detected-as-zip book as skipped");

        let candidate_fixture = RuntimeTestFixture::new("repair-extensions-isolated-candidate");
        clear_skipped_extension_repairs_for_test_database(
            candidate_fixture.database_file.as_path(),
        );
        std::fs::create_dir_all(candidate_fixture.library_root.join("books"))
            .expect("isolated candidate config dir should be created");
        let candidate_bin_path = candidate_fixture.library_root.join("books/repair-book.bin");
        std::fs::write(&candidate_bin_path, b"isolated-candidate-fixture")
            .expect("isolated candidate source file should be written");

        seed_extension_repair_fixture(
            &candidate_fixture,
            "books/repair-book.bin",
            "application/pdf",
        )
        .await;

        let candidate_runtime = candidate_fixture.runtime_context(true, true).await;

        repair_extension(&candidate_runtime, "book-1")
            .await
            .expect("separate runtime database should still repair its own mismatched book");

        let verify_pool = connect_test_pool(candidate_fixture.database_file.as_path(), 1)
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
            candidate_fixture
                .library_root
                .join("books/repair-book.pdf")
                .exists(),
            "skip cache must not leak across distinct runtime databases that reuse the same book id",
        );
        assert!(
            !candidate_bin_path.exists(),
            "candidate runtime should finish the rename instead of short-circuiting on another database's cached skip",
        );

        clear_skipped_extension_repairs_for_test_database(skipped_fixture.database_file.as_path());
        clear_skipped_extension_repairs_for_test_database(
            candidate_fixture.database_file.as_path(),
        );
        skipped_fixture.cleanup().await;
        candidate_fixture.cleanup().await;
    }
}

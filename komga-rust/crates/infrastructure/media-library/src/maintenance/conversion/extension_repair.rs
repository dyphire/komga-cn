use komga_application::task_processing::TaskProcessingError;
use std::io::ErrorKind;
use tokio::fs;

use super::super::archive::{metadata_updated_unix_seconds, normalize_library_relative_url};
use super::super::library_flags::load_library_maintenance_flags;
use super::super::persistence::load_book_for_extension_repair;
use super::super::updates::persist_book_extension_repair;
use crate::MediaLibraryJobContext;
use komga_infrastructure_base::{resolve_library_item_path, resolve_stored_path};
use komga_infrastructure_media_core::expected_extension_for_media_type;

pub async fn repair_extension(
    runtime: &MediaLibraryJobContext,
    book_id: &str,
) -> Result<(), TaskProcessingError> {
    let Some(row) = load_book_for_extension_repair(runtime.database().read_pool(), book_id)
        .await
        .map_err(TaskProcessingError::runtime)?
    else {
        return Ok(());
    };

    let flags = load_library_maintenance_flags(runtime, &row.library_id).await?;
    if !flags.repair_extensions {
        return Ok(());
    }

    let book_id = row.book_id;
    let book_url = row.book_url;
    let library_root = row.library_root;
    let library_id = row.library_id;
    let media_type = row.media_type;

    if runtime.extension_repair_was_skipped(&book_id) {
        return Ok(());
    }

    let Some(correct_extension) = expected_extension_for_media_type(&media_type) else {
        return Ok(());
    };

    let resolved_library_root = resolve_stored_path(&library_root);
    let source_path = resolve_library_item_path(&library_root, &book_url);
    match fs::metadata(&source_path).await {
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(TaskProcessingError::runtime(format!(
                "failed to read source file metadata for extension repair '{book_id}' ('{}'): {error}",
                source_path.display(),
            )));
        }
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
        runtime.mark_extension_repair_skipped(&book_id);
        return Ok(());
    }

    let destination_path = source_path.with_extension(correct_extension);
    match fs::metadata(&destination_path).await {
        Ok(_) => {
            return Err(TaskProcessingError::runtime(format!(
                "failed to repair extension for '{book_id}': destination already exists '{}'",
                destination_path.display(),
            )));
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            return Err(TaskProcessingError::runtime(format!(
                "failed to inspect extension repair destination for '{book_id}' ('{}'): {error}",
                destination_path.display(),
            )));
        }
    }

    fs::rename(&source_path, &destination_path)
        .await
        .map_err(|error| {
            TaskProcessingError::runtime(format!(
                "failed to rename book file for extension repair '{}' -> '{}': {error}",
                source_path.display(),
                destination_path.display(),
            ))
        })?;

    let destination_metadata = fs::metadata(&destination_path).await.map_err(|error| {
        TaskProcessingError::runtime(format!(
            "failed to load repaired file metadata '{}' for '{}': {error}",
            destination_path.display(),
            book_id,
        ))
    })?;
    let destination_url =
        normalize_library_relative_url(&resolved_library_root, &destination_path)?;
    let file_size = destination_metadata.len() as i64;
    let file_last_modified =
        metadata_updated_unix_seconds(&destination_metadata, &destination_path)
            .map_err(TaskProcessingError::runtime)?;

    let repair_result = persist_book_extension_repair(
        runtime.database().write_pool(),
        &book_id,
        &library_id,
        &book_url,
        &destination_url,
        file_last_modified,
        file_size,
    )
    .await
    .map_err(TaskProcessingError::runtime);

    if let Err(error) = repair_result {
        let _ = fs::rename(&destination_path, &source_path).await;
        return Err(error);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use crate::MediaLibraryJobContext;
    use komga_application::runtime_sse::RuntimeSseEventStore;
    use komga_infrastructure_base::DatabaseHandle;
    use komga_infrastructure_base::sqlite::{
        connect_main_write_context, connect_test_pool, evict_shared_pools_for_paths,
    };
    use sqlx::Row;

    use super::repair_extension;

    struct RuntimeTestFixture {
        database_file: PathBuf,
        library_root: PathBuf,
    }

    impl RuntimeTestFixture {
        fn new(case: &str) -> Self {
            let unique = format!(
                "komga-media-library-{case}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("clock should be after unix epoch")
                    .as_nanos()
            );
            Self {
                database_file: std::env::temp_dir().join(format!("{unique}.sqlite")),
                library_root: std::env::temp_dir().join(format!("{unique}-root")),
            }
        }

        async fn main_pool(&self) -> sqlx::SqlitePool {
            connect_main_write_context(&self.database_file)
                .await
                .expect("repair-extensions main db should bootstrap")
                .pool()
                .clone()
        }

        async fn runtime_context(&self) -> MediaLibraryJobContext {
            let main_db = DatabaseHandle::file_backed(self.database_file.clone())
                .await
                .expect("repair-extensions main db should open");
            MediaLibraryJobContext::new(
                main_db,
                true,
                true,
                Arc::new(RuntimeSseEventStore::default()),
                None,
            )
        }

        async fn cleanup(self) {
            for pool in evict_shared_pools_for_paths(std::slice::from_ref(&self.database_file)) {
                pool.close().await;
            }
            let base = self.database_file.to_string_lossy().to_string();
            for sidecar in [
                self.database_file.clone(),
                PathBuf::from(format!("{base}-wal")),
                PathBuf::from(format!("{base}-shm")),
                PathBuf::from(format!("{base}-journal")),
            ] {
                let _ = std::fs::remove_file(sidecar);
            }
            let _ = std::fs::remove_dir_all(self.library_root);
        }
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
    async fn repair_extensions_remembers_previously_skipped_books_within_runtime() {
        let fixture = RuntimeTestFixture::new("repair-extensions-main");
        std::fs::create_dir_all(fixture.library_root.join("books"))
            .expect("repair-extensions config dir should be created");
        let source_path = fixture.library_root.join("books/repair-book.epub");
        std::fs::write(&source_path, b"repair-extension-skip-fixture")
            .expect("repair-extensions source file should be written");

        seed_extension_repair_fixture(&fixture, "books/repair-book.epub", "application/zip").await;

        let runtime = fixture.runtime_context().await;

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

        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn repair_extensions_does_not_cache_books_that_were_already_correct() {
        let fixture = RuntimeTestFixture::new("repair-extensions-candidate");
        std::fs::create_dir_all(fixture.library_root.join("books"))
            .expect("repair-extensions candidate config dir should be created");
        let source_path = fixture.library_root.join("books/repair-book.pdf");
        std::fs::write(&source_path, b"repair-extension-candidate-fixture")
            .expect("repair-extensions candidate source file should be written");

        seed_extension_repair_fixture(&fixture, "books/repair-book.pdf", "application/pdf").await;

        let runtime = fixture.runtime_context().await;

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

        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn repair_extensions_propagates_invalid_source_path_metadata_error() {
        let fixture = RuntimeTestFixture::new("repair-extensions-invalid-source-path");
        seed_extension_repair_fixture(&fixture, "books/repair-book\0.bin", "application/pdf").await;
        let runtime = fixture.runtime_context().await;

        let error = repair_extension(&runtime, "book-1")
            .await
            .expect_err("invalid source path metadata error should fail extension repair");

        assert!(
            error.to_string().contains("source file metadata"),
            "{error}"
        );
        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn repair_extensions_skip_cache_isolated_by_task_runtime() {
        let fixture = RuntimeTestFixture::new("repair-extensions-isolated-runtime");
        std::fs::create_dir_all(fixture.library_root.join("books"))
            .expect("isolated skipped config dir should be created");
        let source_path = fixture.library_root.join("books/repair-book.epub");
        let destination_path = fixture.library_root.join("books/repair-book.pdf");
        std::fs::write(&source_path, b"isolated-skipped-fixture")
            .expect("isolated skipped source file should be written");

        seed_extension_repair_fixture(&fixture, "books/repair-book.epub", "application/zip").await;

        let skipped_runtime = fixture.runtime_context().await;

        repair_extension(&skipped_runtime, "book-1")
            .await
            .expect("first runtime should mark its epub-detected-as-zip book as skipped");

        let pool = connect_test_pool(fixture.database_file.as_path(), 1)
            .await
            .expect("isolated runtime db should reopen for media mutation");
        sqlx::query("UPDATE MEDIA SET MEDIA_TYPE = ? WHERE BOOK_ID = ?")
            .bind("application/pdf")
            .bind("book-1")
            .execute(&pool)
            .await
            .expect("isolated runtime media type should be changed after first skipped run");
        pool.close().await;

        let candidate_runtime = fixture.runtime_context().await;

        repair_extension(&candidate_runtime, "book-1")
            .await
            .expect("separate task runtime should not inherit the previous runtime skip cache");

        let verify_pool = connect_test_pool(fixture.database_file.as_path(), 1)
            .await
            .expect("isolated runtime db should reopen for verification");
        let row = sqlx::query("SELECT URL FROM BOOK WHERE ID = ? LIMIT 1")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect("isolated runtime book row should be queryable");
        verify_pool.close().await;

        assert_eq!(row.get::<String, _>("URL"), "books/repair-book.pdf");
        assert!(
            destination_path.exists(),
            "new task runtime should repair the same database/book after the media type changes",
        );
        assert!(
            !source_path.exists(),
            "new task runtime should finish the rename instead of short-circuiting on the previous runtime's cached skip",
        );

        fixture.cleanup().await;
    }
}

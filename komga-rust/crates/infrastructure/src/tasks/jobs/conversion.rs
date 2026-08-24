use komga_application::task_processing::{
    BookPayload, TaskExecutionOutcome, TaskKind, TaskProcessingError, TaskRequest,
};

use crate::media::maintenance::{convert_book, find_books_to_convert, repair_extension};
use crate::tasks::JobRuntime;

pub(in crate::tasks) async fn execute_repair_extension(
    runtime: &JobRuntime<'_>,
    book_id: &str,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    if !runtime.database().owns_main_database() {
        return Ok(TaskExecutionOutcome::completed());
    }

    repair_extension(runtime, book_id).await?;

    Ok(TaskExecutionOutcome::completed())
}

pub(in crate::tasks) async fn execute_find_books_to_convert(
    runtime: &JobRuntime<'_>,
    library_id: &str,
    priority: i32,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    if !runtime.database().owns_main_database() {
        return Ok(TaskExecutionOutcome::completed());
    }

    let books = find_books_to_convert(runtime, library_id).await?;

    let follow_up_tasks = books
        .into_iter()
        .map(|book| {
            TaskRequest::with_payload(TaskKind::ConvertBook, BookPayload::new(book.book_id))
                .priority(priority + 1)
                .group(book.series_id)
                .into_queue_record()
        })
        .collect();
    Ok(TaskExecutionOutcome::with_follow_up_tasks(follow_up_tasks))
}

pub(in crate::tasks) async fn execute_convert_book(
    runtime: &JobRuntime<'_>,
    book_id: &str,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    if !runtime.database().owns_main_database() {
        return Ok(TaskExecutionOutcome::completed());
    }

    convert_book(runtime, book_id).await?;
    Ok(TaskExecutionOutcome::completed())
}

#[cfg(test)]
mod tests {
    use crate::persistence::sqlite::connect_test_pool;
    use crate::tasks::queue::TaskQueueScheduler;
    use crate::tasks::test_support::{RuntimeTestFixture, execute_and_enqueue};
    use komga_application::task_processing::{BookPayload, TaskKind, TaskRequest};
    use sqlx::{Row, SqlitePool};

    fn archive_fixture_path(file_name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../komga/src/test/resources/archives")
            .join(file_name)
    }

    fn file_updated_unix_seconds_for_test(path: &std::path::Path) -> i64 {
        let metadata = std::fs::metadata(path).expect("test fixture metadata should be readable");
        [metadata.created().ok(), metadata.modified().ok()]
            .into_iter()
            .flatten()
            .map(|timestamp| {
                timestamp
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("test fixture timestamp should be after unix epoch")
                    .as_secs() as i64
            })
            .max()
            .expect("test fixture should expose created or modified timestamp")
    }

    fn page_media_type_for_test(file_name: &str) -> &'static str {
        match std::path::Path::new(file_name)
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .as_deref()
        {
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("png") => "image/png",
            Some("gif") => "image/gif",
            Some("webp") => "image/webp",
            Some("avif") => "image/avif",
            Some("bmp") => "image/bmp",
            _ => "application/octet-stream",
        }
    }

    async fn insert_series(pool: &SqlitePool, library_id: &str, series_id: &str) {
        sqlx::query(
            "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?)",
        )
        .bind(series_id)
        .bind(0_i64)
        .bind("Series 1")
        .bind(format!("series/{series_id}"))
        .bind(library_id)
        .execute(pool)
        .await
        .expect("series row should be inserted for conversion fixture");
    }

    #[tokio::test]
    async fn find_books_to_convert_enqueues_convert_book_grouped_by_series_id() {
        let fixture = RuntimeTestFixture::new("find-books-to-convert");
        let pool = fixture.main_pool().await;
        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT, CONVERT_TO_CBZ) VALUES (?, ?, ?, ?)")
            .bind("library-1")
            .bind("Library 1")
            .bind(fixture.library_root.to_string_lossy().to_string())
            .bind(true)
            .execute(&pool)
            .await
            .expect("library row should be inserted for find-books-to-convert fixture");
        insert_series(&pool, "library-1", "series-1").await;
        sqlx::query(
            r#"
            INSERT INTO BOOK (
                ID,
                NAME,
                URL,
                LIBRARY_ID,
                SERIES_ID,
                FILE_LAST_MODIFIED,
                FILE_SIZE,
                DELETED_DATE
            )
            VALUES (?, ?, ?, ?, ?, datetime(?, 'unixepoch'), ?, NULL)
            "#,
        )
        .bind("book-1")
        .bind("book-1")
        .bind("books/book-1.cbr")
        .bind("library-1")
        .bind("series-1")
        .bind(0_i64)
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("book row should be inserted for find-books-to-convert fixture");
        sqlx::query("INSERT INTO MEDIA (BOOK_ID, MEDIA_TYPE, STATUS) VALUES (?, ?, ?)")
            .bind("book-1")
            .bind("application/x-rar-compressed; version=5")
            .bind("READY")
            .execute(&pool)
            .await
            .expect("media row should be inserted for find-books-to-convert fixture");
        pool.close().await;

        let tasks_pool = fixture.tasks_pool().await;
        tasks_pool.close().await;

        let runtime = fixture.runtime_context(true, true).await;
        let scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
        let task = TaskRequest::new(TaskKind::FindBooksToConvert)
            .priority(1_000)
            .group("library-1")
            .into_queue_record_with_id("library-1");

        let result = execute_and_enqueue(&scheduler, &runtime, &task).await;
        assert!(matches!(result, Some(Ok(()))));
        assert_eq!(
            scheduler
                .count_by_simple_type()
                .await
                .expect("conversion fixture queue counts should load")
                .get("ConvertBook")
                .copied(),
            Some(1),
            "find-books-to-convert should enqueue one downstream convert task",
        );

        let tasks_pool = connect_test_pool(fixture.tasks_db_file.as_path(), 1)
            .await
            .expect("tasks db should open for convert-book grouping verification");
        let row = sqlx::query(
            "SELECT ID, GROUP_ID, PRIORITY FROM TASK WHERE SIMPLE_TYPE = 'ConvertBook' LIMIT 1",
        )
        .fetch_one(&tasks_pool)
        .await
        .expect("convert-book task row should be queryable");
        tasks_pool.close().await;

        assert_eq!(row.get::<String, _>("ID"), "ConvertBook_book-1");
        assert_eq!(
            row.get::<Option<String>, _>("GROUP_ID"),
            Some("series-1".to_string())
        );
        assert_eq!(row.get::<i64, _>("PRIORITY"), 1_001);

        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn find_books_to_convert_skips_when_library_convert_to_cbz_is_disabled() {
        let fixture = RuntimeTestFixture::new("find-books-disabled");
        let pool = fixture.main_pool().await;
        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT, CONVERT_TO_CBZ) VALUES (?, ?, ?, ?)")
            .bind("library-1")
            .bind("Library 1")
            .bind(fixture.library_root.to_string_lossy().to_string())
            .bind(false)
            .execute(&pool)
            .await
            .expect("disabled library row should be inserted for find-books-to-convert fixture");
        insert_series(&pool, "library-1", "series-1").await;
        sqlx::query(
            r#"
            INSERT INTO BOOK (
                ID,
                NAME,
                URL,
                LIBRARY_ID,
                SERIES_ID,
                FILE_LAST_MODIFIED,
                FILE_SIZE,
                DELETED_DATE
            )
            VALUES (?, ?, ?, ?, ?, datetime(?, 'unixepoch'), ?, NULL)
            "#,
        )
        .bind("book-1")
        .bind("book-1")
        .bind("books/book-1.cbr")
        .bind("library-1")
        .bind("series-1")
        .bind(0_i64)
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("disabled fixture book row should be inserted");
        sqlx::query("INSERT INTO MEDIA (BOOK_ID, MEDIA_TYPE, STATUS) VALUES (?, ?, ?)")
            .bind("book-1")
            .bind("application/vnd.comicbook-rar")
            .bind("READY")
            .execute(&pool)
            .await
            .expect("disabled fixture media row should be inserted");
        pool.close().await;

        let tasks_pool = fixture.tasks_pool().await;
        tasks_pool.close().await;

        let runtime = fixture.runtime_context(true, true).await;
        let scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
        let task = TaskRequest::new(TaskKind::FindBooksToConvert)
            .priority(1_000)
            .group("library-1")
            .into_queue_record_with_id("library-1");

        let result = execute_and_enqueue(&scheduler, &runtime, &task).await;
        assert!(matches!(result, Some(Ok(()))));
        assert!(
            scheduler
                .count_by_simple_type()
                .await
                .expect("conversion fixture queue counts should load")
                .is_empty(),
            "find-books-to-convert should not enqueue convert-book tasks when convert-to-cbz is disabled",
        );

        let tasks_pool = connect_test_pool(fixture.tasks_db_file.as_path(), 1)
            .await
            .expect("tasks db should open for disabled convert-book verification");
        let count = sqlx::query("SELECT COUNT(*) AS COUNT FROM TASK")
            .fetch_one(&tasks_pool)
            .await
            .expect("task row count should be queryable for disabled convert-book verification")
            .get::<i64, _>("COUNT");
        tasks_pool.close().await;

        assert_eq!(count, 0);

        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn convert_book_skips_when_source_file_last_modified_differs_from_database() {
        let book_id = "convert-last-modified-book-1";
        let fixture = RuntimeTestFixture::new("convert-book-last-modified");
        std::fs::create_dir_all(fixture.library_root.join("books"))
            .expect("convert-book last-modified directory should be created");
        let source_path = fixture.library_root.join("books/book-1.cbr");
        std::fs::write(&source_path, b"not-a-real-rar")
            .expect("convert-book last-modified source should be written");

        let actual_last_modified = file_updated_unix_seconds_for_test(&source_path);

        let pool = fixture.main_pool().await;
        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT, CONVERT_TO_CBZ) VALUES (?, ?, ?, ?)")
            .bind("library-1")
            .bind("Library 1")
            .bind(fixture.library_root.to_string_lossy().to_string())
            .bind(true)
            .execute(&pool)
            .await
            .expect("convert-book last-modified library row should be inserted");
        insert_series(&pool, "library-1", "series-1").await;
        sqlx::query(
            r#"
            INSERT INTO BOOK (
                ID,
                NAME,
                URL,
                LIBRARY_ID,
                SERIES_ID,
                FILE_LAST_MODIFIED,
                FILE_SIZE
            )
            VALUES (?, ?, ?, ?, ?, datetime(?, 'unixepoch'), ?)
            "#,
        )
        .bind(book_id)
        .bind("book-1")
        .bind("books/book-1.cbr")
        .bind("library-1")
        .bind("series-1")
        .bind(actual_last_modified.saturating_sub(10))
        .bind(12_i64)
        .execute(&pool)
        .await
        .expect("convert-book last-modified book row should be inserted");
        sqlx::query("INSERT INTO MEDIA (BOOK_ID, MEDIA_TYPE, STATUS) VALUES (?, ?, ?)")
            .bind(book_id)
            .bind("application/x-rar-compressed; version=5")
            .bind("READY")
            .execute(&pool)
            .await
            .expect("convert-book last-modified media row should be inserted");
        pool.close().await;

        let runtime = fixture.runtime_context(false, true).await;
        let scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
        let task = TaskRequest::with_payload(TaskKind::ConvertBook, BookPayload::new(book_id))
            .priority(900)
            .group("series-1")
            .into_queue_record();

        let result = execute_and_enqueue(&scheduler, &runtime, &task).await;
        assert!(matches!(result, Some(Ok(()))));
        assert!(source_path.exists());
        assert!(!fixture.library_root.join("books/book-1.cbz").exists());

        let verify_pool = connect_test_pool(fixture.database_file.as_path(), 1)
            .await
            .expect("convert-book last-modified verify db should open");
        let row = sqlx::query("SELECT URL FROM BOOK WHERE ID = ? LIMIT 1")
            .bind(book_id)
            .fetch_one(&verify_pool)
            .await
            .expect("convert-book last-modified row should be queryable");
        verify_pool.close().await;

        assert_eq!(row.get::<String, _>("URL"), "books/book-1.cbr");

        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn convert_book_propagates_invalid_source_path_metadata_error() {
        let book_id = "convert-invalid-source-path-book-1";
        let fixture = RuntimeTestFixture::new("convert-book-invalid-source-path");

        let pool = fixture.main_pool().await;
        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT, CONVERT_TO_CBZ) VALUES (?, ?, ?, ?)")
            .bind("library-1")
            .bind("Library 1")
            .bind(fixture.library_root.to_string_lossy().to_string())
            .bind(true)
            .execute(&pool)
            .await
            .expect("convert-book invalid path library row should be inserted");
        insert_series(&pool, "library-1", "series-1").await;
        sqlx::query(
            r#"
            INSERT INTO BOOK (
                ID,
                NAME,
                URL,
                LIBRARY_ID,
                SERIES_ID,
                FILE_LAST_MODIFIED,
                FILE_SIZE
            )
            VALUES (?, ?, ?, ?, ?, datetime(?, 'unixepoch'), ?)
            "#,
        )
        .bind(book_id)
        .bind("book-1")
        .bind("books/book-1\0.cbr")
        .bind("library-1")
        .bind("series-1")
        .bind(0_i64)
        .bind(12_i64)
        .execute(&pool)
        .await
        .expect("convert-book invalid path book row should be inserted");
        sqlx::query("INSERT INTO MEDIA (BOOK_ID, MEDIA_TYPE, STATUS) VALUES (?, ?, ?)")
            .bind(book_id)
            .bind("application/x-rar-compressed; version=5")
            .bind("READY")
            .execute(&pool)
            .await
            .expect("convert-book invalid path media row should be inserted");
        pool.close().await;

        let runtime = fixture.runtime_context(false, true).await;
        let scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
        let task = TaskRequest::with_payload(TaskKind::ConvertBook, BookPayload::new(book_id))
            .priority(900)
            .group("series-1")
            .into_queue_record();

        let result = execute_and_enqueue(&scheduler, &runtime, &task).await;
        let Some(Err(error)) = result else {
            panic!("invalid source path metadata error should fail convert-book");
        };
        assert!(
            error.to_string().contains("source file metadata"),
            "{error}"
        );

        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn convert_book_skips_after_a_previous_failed_conversion() {
        let book_id = "convert-failed-cache-book-1";
        let fixture = RuntimeTestFixture::new("convert-book-failed-cache");
        std::fs::create_dir_all(fixture.library_root.join("books"))
            .expect("convert-book failed-cache directory should be created");
        let source_path = fixture.library_root.join("books/book-1.cbr");
        std::fs::write(&source_path, b"not-a-real-rar")
            .expect("convert-book failed-cache source should be written");

        let actual_last_modified = file_updated_unix_seconds_for_test(&source_path);

        let pool = fixture.main_pool().await;
        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT, CONVERT_TO_CBZ) VALUES (?, ?, ?, ?)")
            .bind("library-1")
            .bind("Library 1")
            .bind(fixture.library_root.to_string_lossy().to_string())
            .bind(true)
            .execute(&pool)
            .await
            .expect("convert-book failed-cache library row should be inserted");
        insert_series(&pool, "library-1", "series-1").await;
        sqlx::query(
            r#"
            INSERT INTO BOOK (
                ID,
                NAME,
                URL,
                LIBRARY_ID,
                SERIES_ID,
                FILE_LAST_MODIFIED,
                FILE_SIZE
            )
            VALUES (?, ?, ?, ?, ?, datetime(?, 'unixepoch'), ?)
            "#,
        )
        .bind(book_id)
        .bind("book-1")
        .bind("books/book-1.cbr")
        .bind("library-1")
        .bind("series-1")
        .bind(actual_last_modified)
        .bind(12_i64)
        .execute(&pool)
        .await
        .expect("convert-book failed-cache book row should be inserted");
        sqlx::query("INSERT INTO MEDIA (BOOK_ID, MEDIA_TYPE, STATUS) VALUES (?, ?, ?)")
            .bind(book_id)
            .bind("application/x-rar-compressed; version=5")
            .bind("READY")
            .execute(&pool)
            .await
            .expect("convert-book failed-cache media row should be inserted");
        pool.close().await;

        let runtime = fixture.runtime_context(false, true).await;
        let scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
        let task = TaskRequest::with_payload(TaskKind::ConvertBook, BookPayload::new(book_id))
            .priority(900)
            .group("series-1")
            .into_queue_record();

        let first = execute_and_enqueue(&scheduler, &runtime, &task).await;
        assert!(matches!(first, Some(Err(_))));

        let second = execute_and_enqueue(&scheduler, &runtime, &task).await;
        assert!(matches!(second, Some(Ok(()))));
        assert!(source_path.exists());
        assert!(!fixture.library_root.join("books/book-1.cbz").exists());

        std::fs::copy(archive_fixture_path("rar4.rar"), &source_path)
            .expect("convert-book failed-cache source should be replaced with a valid RAR");
        let repaired_metadata = std::fs::metadata(&source_path)
            .expect("convert-book repaired source metadata should be readable");
        let repaired_last_modified = file_updated_unix_seconds_for_test(&source_path);
        let pool = connect_test_pool(fixture.database_file.as_path(), 1)
            .await
            .expect("convert-book failed-cache db should reopen for repaired source metadata");
        sqlx::query(
            "UPDATE BOOK SET FILE_LAST_MODIFIED = datetime(?, 'unixepoch'), FILE_SIZE = ? WHERE ID = ?",
        )
        .bind(repaired_last_modified)
        .bind(i64::try_from(repaired_metadata.len()).expect("fixture size should fit in i64"))
        .bind(book_id)
        .execute(&pool)
        .await
        .expect("convert-book repaired source metadata should be persisted");
        pool.close().await;

        let retry_runtime = fixture.runtime_context(false, true).await;
        let retry_scheduler =
            TaskQueueScheduler::for_runtime(retry_runtime.clone(), "rust-main").await;
        let retry = execute_and_enqueue(&retry_scheduler, &retry_runtime, &task).await;
        assert!(matches!(retry, Some(Ok(()))));
        assert!(!source_path.exists());
        assert!(fixture.library_root.join("books/book-1.cbz").exists());

        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn convert_book_persists_history_events_on_success() {
        let book_id = "convert-success-book-1";
        let fixture = RuntimeTestFixture::new("convert-book-success");
        std::fs::create_dir_all(fixture.library_root.join("books"))
            .expect("convert-book success directory should be created");

        let source_path = fixture.library_root.join("books/book-1.cbr");
        std::fs::copy(archive_fixture_path("rar4.rar"), &source_path)
            .expect("convert-book success source fixture should be copied");
        let preserved_page = crate::media::formats::rar::list_rar_entries(
            archive_fixture_path("rar4.rar").as_path(),
        )
        .expect("convert-book success rar fixture should be listable")
        .into_iter()
        .find(|entry| {
            matches!(
                page_media_type_for_test(&entry.file_name),
                "image/jpeg"
                    | "image/png"
                    | "image/gif"
                    | "image/webp"
                    | "image/avif"
                    | "image/bmp"
            )
        })
        .expect("convert-book success rar fixture should contain an image page");
        let preserved_page_hash = "existing-page-hash-1";

        let actual_last_modified = file_updated_unix_seconds_for_test(&source_path);

        let pool = fixture.main_pool().await;
        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT, CONVERT_TO_CBZ) VALUES (?, ?, ?, ?)")
            .bind("library-1")
            .bind("Library 1")
            .bind(fixture.library_root.to_string_lossy().to_string())
            .bind(true)
            .execute(&pool)
            .await
            .expect("convert-book success library row should be inserted");
        insert_series(&pool, "library-1", "series-1").await;
        sqlx::query(
            r#"
            INSERT INTO BOOK (
                ID,
                NAME,
                URL,
                LIBRARY_ID,
                SERIES_ID,
                FILE_LAST_MODIFIED,
                FILE_SIZE
            )
            VALUES (?, ?, ?, ?, ?, datetime(?, 'unixepoch'), ?)
            "#,
        )
        .bind(book_id)
        .bind("book-1")
        .bind("books/book-1.cbr")
        .bind("library-1")
        .bind("series-1")
        .bind(actual_last_modified)
        .bind(32_i64)
        .execute(&pool)
        .await
        .expect("convert-book success book row should be inserted");
        sqlx::query(
            "INSERT INTO MEDIA (BOOK_ID, MEDIA_TYPE, STATUS, PAGE_COUNT) VALUES (?, ?, ?, ?)",
        )
        .bind(book_id)
        .bind("application/x-rar-compressed; version=4")
        .bind("READY")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("convert-book success media row should be inserted");
        sqlx::query(
            r#"
            INSERT INTO MEDIA_PAGE (
                FILE_NAME,
                MEDIA_TYPE,
                NUMBER,
                BOOK_ID,
                width,
                height,
                FILE_HASH,
                FILE_SIZE
            )
            VALUES (?, ?, ?, ?, NULL, NULL, ?, ?)
            "#,
        )
        .bind(&preserved_page.file_name)
        .bind(page_media_type_for_test(&preserved_page.file_name))
        .bind(0_i64)
        .bind(book_id)
        .bind(preserved_page_hash)
        .bind(i64::try_from(preserved_page.unpacked_size).unwrap_or(i64::MAX))
        .execute(&pool)
        .await
        .expect("convert-book success source page hash should be inserted");
        pool.close().await;

        let runtime = fixture.runtime_context(false, false).await;
        let scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
        let task = TaskRequest::with_payload(TaskKind::ConvertBook, BookPayload::new(book_id))
            .priority(900)
            .group("series-1")
            .into_queue_record();

        let result = execute_and_enqueue(&scheduler, &runtime, &task).await;
        assert!(matches!(result, Some(Ok(()))));

        let destination_path = fixture.library_root.join("books/book-1.cbz");
        assert!(
            !source_path.exists(),
            "convert-book success should delete the original source file"
        );
        assert!(
            destination_path.exists(),
            "convert-book success should create the converted cbz file"
        );

        let verify_pool = connect_test_pool(fixture.database_file.as_path(), 1)
            .await
            .expect("convert-book success verify db should open");
        let book_row = sqlx::query("SELECT URL FROM BOOK WHERE ID = ? LIMIT 1")
            .bind(book_id)
            .fetch_one(&verify_pool)
            .await
            .expect("convert-book success book row should be queryable");
        assert_eq!(book_row.get::<String, _>("URL"), "books/book-1.cbz");

        let media_row = sqlx::query(
            "SELECT STATUS, MEDIA_TYPE, PAGE_COUNT FROM MEDIA WHERE BOOK_ID = ? LIMIT 1",
        )
        .bind(book_id)
        .fetch_one(&verify_pool)
        .await
        .expect("convert-book success media row should be queryable");
        assert_eq!(media_row.get::<String, _>("STATUS"), "READY");
        assert_eq!(media_row.get::<String, _>("MEDIA_TYPE"), "application/zip");
        assert!(
            media_row.get::<i64, _>("PAGE_COUNT") > 0,
            "convert-book success should analyze converted pages"
        );
        let preserved_page_row = sqlx::query(
            "SELECT FILE_HASH FROM MEDIA_PAGE WHERE BOOK_ID = ? AND FILE_NAME = ? AND MEDIA_TYPE = ? AND FILE_SIZE = ? LIMIT 1",
        )
        .bind(book_id)
        .bind(&preserved_page.file_name)
        .bind(page_media_type_for_test(&preserved_page.file_name))
        .bind(i64::try_from(preserved_page.unpacked_size).unwrap_or(i64::MAX))
        .fetch_one(&verify_pool)
        .await
        .expect("convert-book success preserved page row should be queryable");
        assert_eq!(
            preserved_page_row.get::<String, _>("FILE_HASH"),
            preserved_page_hash,
            "convert-book success should preserve matching page hashes across re-analysis"
        );

        let history_rows = sqlx::query(
            "SELECT ID, TYPE, BOOK_ID, SERIES_ID FROM HISTORICAL_EVENT ORDER BY ROWID ASC",
        )
        .fetch_all(&verify_pool)
        .await
        .expect("convert-book success historical events should be queryable");
        assert_eq!(history_rows.len(), 2);
        assert_eq!(history_rows[0].get::<String, _>("TYPE"), "BookFileDeleted");
        assert_eq!(history_rows[1].get::<String, _>("TYPE"), "BookConverted");
        assert_eq!(
            history_rows[0].get::<Option<String>, _>("BOOK_ID"),
            Some(book_id.to_string())
        );
        assert_eq!(
            history_rows[1].get::<Option<String>, _>("BOOK_ID"),
            Some(book_id.to_string())
        );
        assert_eq!(
            history_rows[0].get::<Option<String>, _>("SERIES_ID"),
            Some("series-1".to_string())
        );
        assert_eq!(
            history_rows[1].get::<Option<String>, _>("SERIES_ID"),
            Some("series-1".to_string())
        );

        let deleted_event_id = history_rows[0].get::<String, _>("ID");
        let converted_event_id = history_rows[1].get::<String, _>("ID");
        let deleted_props =
            sqlx::query("SELECT \"KEY\", VALUE FROM HISTORICAL_EVENT_PROPERTIES WHERE ID = ?")
                .bind(&deleted_event_id)
                .fetch_all(&verify_pool)
                .await
                .expect("convert-book success deleted-event properties should be queryable")
                .into_iter()
                .map(|row| (row.get::<String, _>("KEY"), row.get::<String, _>("VALUE")))
                .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            deleted_props.get("reason"),
            Some(&"File was deleted after conversion to CBZ".to_string())
        );
        assert_eq!(
            deleted_props.get("name"),
            Some(&source_path.to_string_lossy().to_string())
        );

        let converted_props =
            sqlx::query("SELECT \"KEY\", VALUE FROM HISTORICAL_EVENT_PROPERTIES WHERE ID = ?")
                .bind(&converted_event_id)
                .fetch_all(&verify_pool)
                .await
                .expect("convert-book success converted-event properties should be queryable")
                .into_iter()
                .map(|row| (row.get::<String, _>("KEY"), row.get::<String, _>("VALUE")))
                .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            converted_props.get("name"),
            Some(&destination_path.to_string_lossy().to_string())
        );
        assert_eq!(
            converted_props.get("former file"),
            Some(&source_path.to_string_lossy().to_string())
        );

        verify_pool.close().await;

        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn repair_extension_executes_per_book_task() {
        let book_id = "repair-task-book-1";
        let fixture = RuntimeTestFixture::new("repair-extension-per-book");
        std::fs::create_dir_all(fixture.library_root.join("books"))
            .expect("repair-extension per-book directory should be created");
        let source_path = fixture.library_root.join("books/repair-book.bin");
        std::fs::write(&source_path, b"repair-extension-per-book")
            .expect("repair-extension per-book source should be written");

        let pool = fixture.main_pool().await;
        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT, REPAIR_EXTENSIONS) VALUES (?, ?, ?, ?)")
            .bind("library-1")
            .bind("Library 1")
            .bind(fixture.library_root.to_string_lossy().to_string())
            .bind(true)
            .execute(&pool)
            .await
            .expect("repair-extension per-book library row should be inserted");
        insert_series(&pool, "library-1", "series-1").await;
        sqlx::query(
            r#"
            INSERT INTO BOOK (
                ID,
                NAME,
                URL,
                LIBRARY_ID,
                SERIES_ID,
                FILE_LAST_MODIFIED,
                FILE_SIZE
            )
            VALUES (?, ?, ?, ?, ?, datetime(?, 'unixepoch'), ?)
            "#,
        )
        .bind(book_id)
        .bind("repair-book")
        .bind("books/repair-book.bin")
        .bind("library-1")
        .bind("series-1")
        .bind(0_i64)
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("repair-extension per-book book row should be inserted");
        sqlx::query("INSERT INTO MEDIA (BOOK_ID, MEDIA_TYPE, STATUS) VALUES (?, ?, ?)")
            .bind(book_id)
            .bind("application/pdf")
            .bind("READY")
            .execute(&pool)
            .await
            .expect("repair-extension per-book media row should be inserted");
        pool.close().await;

        let tasks_pool = fixture.tasks_pool().await;
        tasks_pool.close().await;

        let runtime = fixture.runtime_context(true, true).await;
        let scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
        let task = TaskRequest::with_payload(TaskKind::RepairExtension, BookPayload::new(book_id))
            .priority(1_000)
            .group("series-1")
            .into_queue_record();

        let result = execute_and_enqueue(&scheduler, &runtime, &task).await;
        assert!(matches!(result, Some(Ok(()))));

        let verify_pool = connect_test_pool(fixture.database_file.as_path(), 1)
            .await
            .expect("repair-extension per-book verify db should open");
        let row = sqlx::query("SELECT URL FROM BOOK WHERE ID = ? LIMIT 1")
            .bind(book_id)
            .fetch_one(&verify_pool)
            .await
            .expect("repair-extension per-book book row should be queryable");
        verify_pool.close().await;

        assert_eq!(row.get::<String, _>("URL"), "books/repair-book.pdf");
        assert!(fixture.library_root.join("books/repair-book.pdf").exists());
        assert!(!source_path.exists());

        fixture.cleanup().await;
    }
}

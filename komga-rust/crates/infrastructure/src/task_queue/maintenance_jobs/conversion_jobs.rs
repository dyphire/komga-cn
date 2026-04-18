use super::*;

pub(super) fn try_execute(
    scheduler: &mut TaskQueueScheduler,
    runtime: &RuntimeConfig,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Option<Result<(), TaskExecutionError>> {
    let owns_main_database = runtime.task_runtime_context().owns_main_database;
    let result = match task.simple_type.as_str() {
        "REPAIR_EXTENSION" => {
            let Some(book_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "REPAIR_EXTENSION task must include a book id",
                )));
            };
            if !owns_main_database {
                return Some(Ok(()));
            }
            super::super::repair_extension(runtime, book_id)
        }
        "FIND_BOOKS_TO_CONVERT" => {
            let Some(library_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "FIND_BOOKS_TO_CONVERT task must include a library id",
                )));
            };
            if !owns_main_database {
                return Some(Ok(()));
            }
            let books = match super::super::find_books_to_convert(runtime, library_id) {
                Ok(books) => books,
                Err(error) => return Some(Err(error)),
            };
            for book in books {
                scheduler.enqueue(runtime_follow_up_task(RuntimeFollowUpTask::ConvertBook {
                    book_id: book.book_id,
                    series_id: book.series_id,
                    priority: task.priority + 1,
                }));
            }
            Ok(())
        }
        "CONVERT_BOOK" => {
            let Some(book_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "CONVERT_BOOK task must include a book id",
                )));
            };
            if !owns_main_database {
                return Some(Ok(()));
            }
            super::super::convert_book(runtime, book_id)
        }
        _ => return None,
    };

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::connect_test_pool;
    use crate::task_queue::test_support::RuntimeTestFixture;
    use sqlx::{Row, SqlitePool};

    fn archive_fixture_path(file_name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../komga/src/test/resources/archives")
            .join(file_name)
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

        let runtime = fixture.runtime_context(true, true);
        let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
        let task = TaskQueueRecord::new(
            "FIND_BOOKS_TO_CONVERT_library-1",
            1_000,
            Some("library-1".to_string()),
        )
        .with_simple_type("FIND_BOOKS_TO_CONVERT");

        let result = try_execute(&mut scheduler, &runtime, &task, Some("library-1"));
        assert!(matches!(result, Some(Ok(()))));
        assert_eq!(
            scheduler
                .count_by_simple_type()
                .get("CONVERT_BOOK")
                .copied(),
            Some(1),
            "find-books-to-convert should enqueue one downstream convert task",
        );

        let tasks_pool = connect_test_pool(fixture.tasks_db_file.as_path(), 1)
            .await
            .expect("tasks db should open for convert-book grouping verification");
        let row = sqlx::query(
            "SELECT ID, GROUP_ID, PRIORITY, PAYLOAD FROM TASK WHERE SIMPLE_TYPE = 'ConvertBook' LIMIT 1",
        )
                .fetch_one(&tasks_pool)
                .await
                .expect("convert-book task row should be queryable");
        tasks_pool.close().await;

        assert_eq!(row.get::<String, _>("ID"), "CONVERT_BOOK_book-1");
        assert_eq!(
            row.get::<Option<String>, _>("GROUP_ID"),
            Some("series-1".to_string())
        );
        assert_eq!(row.get::<i64, _>("PRIORITY"), 1_001);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("PAYLOAD"))
                .expect("convert-book payload should be valid JSON"),
            serde_json::json!({
                "bookId": "book-1",
                "priority": 1001,
                "groupId": "series-1",
                "uniqueId": "CONVERT_BOOK_book-1"
            }),
        );

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

        let runtime = fixture.runtime_context(true, true);
        let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
        let task = TaskQueueRecord::new(
            "FIND_BOOKS_TO_CONVERT_library-1",
            1_000,
            Some("library-1".to_string()),
        )
        .with_simple_type("FIND_BOOKS_TO_CONVERT");

        let result = try_execute(&mut scheduler, &runtime, &task, Some("library-1"));
        assert!(matches!(result, Some(Ok(()))));
        assert!(
            scheduler.count_by_simple_type().is_empty(),
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

        let actual_last_modified = std::fs::metadata(&source_path)
            .expect("convert-book last-modified source metadata should be readable")
            .modified()
            .expect("convert-book last-modified source modified time should be readable")
            .duration_since(std::time::UNIX_EPOCH)
            .expect("convert-book last-modified source time should be after unix epoch")
            .as_secs() as i64;

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

        let runtime = fixture.runtime_context(false, true);
        let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
        let task = TaskQueueRecord::new(
            format!("CONVERT_BOOK_{book_id}"),
            900,
            Some("series-1".to_string()),
        )
        .with_simple_type("CONVERT_BOOK");

        let result = try_execute(&mut scheduler, &runtime, &task, Some(book_id));
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
    async fn convert_book_skips_after_a_previous_failed_conversion() {
        let book_id = "convert-failed-cache-book-1";
        let fixture = RuntimeTestFixture::new("convert-book-failed-cache");
        std::fs::create_dir_all(fixture.library_root.join("books"))
            .expect("convert-book failed-cache directory should be created");
        let source_path = fixture.library_root.join("books/book-1.cbr");
        std::fs::write(&source_path, b"not-a-real-rar")
            .expect("convert-book failed-cache source should be written");

        let actual_last_modified = std::fs::metadata(&source_path)
            .expect("convert-book failed-cache source metadata should be readable")
            .modified()
            .expect("convert-book failed-cache source modified time should be readable")
            .duration_since(std::time::UNIX_EPOCH)
            .expect("convert-book failed-cache source time should be after unix epoch")
            .as_secs() as i64;

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

        let runtime = fixture.runtime_context(false, true);
        let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
        let task = TaskQueueRecord::new(
            format!("CONVERT_BOOK_{book_id}"),
            900,
            Some("series-1".to_string()),
        )
        .with_simple_type("CONVERT_BOOK");

        let first = try_execute(&mut scheduler, &runtime, &task, Some(book_id));
        assert!(matches!(first, Some(Err(_))));

        let second = try_execute(&mut scheduler, &runtime, &task, Some(book_id));
        assert!(matches!(second, Some(Ok(()))));
        assert!(source_path.exists());
        assert!(!fixture.library_root.join("books/book-1.cbz").exists());

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
        let preserved_page =
            crate::rar_support::list_rar_entries(archive_fixture_path("rar4.rar").as_path())
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

        let source_metadata = std::fs::metadata(&source_path)
            .expect("convert-book success source metadata should be readable");
        let actual_last_modified = source_metadata
            .created()
            .ok()
            .into_iter()
            .chain(source_metadata.modified().ok())
            .max()
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|value| value.as_secs() as i64)
            .unwrap_or_default();

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

        let runtime = fixture.runtime_context(false, false);
        let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
        let task = TaskQueueRecord::new(
            format!("CONVERT_BOOK_{book_id}"),
            900,
            Some("series-1".to_string()),
        )
        .with_simple_type("CONVERT_BOOK");

        let result = try_execute(&mut scheduler, &runtime, &task, Some(book_id));
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

        let runtime = fixture.runtime_context(true, true);
        let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main");
        let task = TaskQueueRecord::new(
            format!("REPAIR_EXTENSION_{book_id}"),
            1_000,
            Some("series-1".to_string()),
        )
        .with_simple_type("REPAIR_EXTENSION")
        .with_payload(
            serde_json::json!({
                "bookId": book_id,
                "priority": 1000,
                "groupId": "series-1",
                "uniqueId": format!("REPAIR_EXTENSION_{book_id}"),
            })
            .to_string(),
        );

        let result = try_execute(&mut scheduler, &runtime, &task, Some(book_id));
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

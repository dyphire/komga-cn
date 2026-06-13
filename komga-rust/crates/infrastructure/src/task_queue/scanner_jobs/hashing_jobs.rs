use komga_application::task_processing::{
    BookPayload, RemoveHashedPagesPayload, TaskExecutionOutcome, TaskKind, TaskProcessingError,
    TaskRequest,
};

use super::super::media_helpers::{
    HashedPageToDelete, find_books_with_missing_page_hash, find_duplicate_pages_to_delete,
    hash_book, hash_book_pages, load_library_hashing_flags, remove_hashed_pages,
};
use super::super::runtime_context::JobRuntime;

pub(in crate::task_queue) async fn execute_hash_book_pages(
    runtime: &JobRuntime<'_>,
    book_id: &str,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    if !runtime.database().owns_main_database() {
        return Ok(TaskExecutionOutcome::completed());
    }

    hash_book_pages(runtime, book_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
}

pub(in crate::task_queue) async fn execute_hash_book(
    runtime: &JobRuntime<'_>,
    book_id: &str,
    koreader: bool,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    hash_book(runtime, book_id, koreader).await?;

    Ok(TaskExecutionOutcome::completed())
}

pub(in crate::task_queue) async fn execute_find_books_with_missing_page_hash(
    runtime: &JobRuntime<'_>,
    library_id: &str,
    priority: i32,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    if !runtime.database().owns_main_database() {
        return Ok(TaskExecutionOutcome::completed());
    }

    let hashing_flags = load_library_hashing_flags(runtime, library_id).await?;
    if !hashing_flags.hash_pages {
        return Ok(TaskExecutionOutcome::completed());
    }

    let book_ids = find_books_with_missing_page_hash(runtime, Some(library_id)).await?;
    let follow_up_tasks = book_ids
        .into_iter()
        .map(|book_id| {
            let priority = priority.saturating_add(1);
            TaskRequest::with_payload(TaskKind::HashBookPages, BookPayload::new(book_id))
                .priority(priority)
                .into_queue_record()
        })
        .collect();
    Ok(TaskExecutionOutcome::with_follow_up_tasks(follow_up_tasks))
}

pub(in crate::task_queue) async fn execute_find_duplicate_pages_to_delete(
    runtime: &JobRuntime<'_>,
    library_id: &str,
    priority: i32,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    if !runtime.database().owns_main_database() {
        return Ok(TaskExecutionOutcome::completed());
    }

    let targets = find_duplicate_pages_to_delete(runtime, library_id).await?;
    let mut follow_up_tasks = Vec::new();
    for (book_id, pages) in targets {
        let priority = priority.saturating_add(1);
        follow_up_tasks.push(
            TaskRequest::with_payload(
                TaskKind::RemoveHashedPages,
                RemoveHashedPagesPayload::new(book_id, pages),
            )
            .priority(priority)
            .into_queue_record(),
        );
    }
    Ok(TaskExecutionOutcome::with_follow_up_tasks(follow_up_tasks))
}

pub(in crate::task_queue) async fn execute_remove_hashed_pages(
    runtime: &JobRuntime<'_>,
    book_id: &str,
    pages: &[HashedPageToDelete],
    priority: i32,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    if !runtime.database().owns_main_database() {
        return Ok(TaskExecutionOutcome::completed());
    }

    let book_id = book_id.to_string();
    let regenerate_thumbnail = remove_hashed_pages(runtime, &book_id, pages).await?;
    let follow_up_tasks = if regenerate_thumbnail {
        vec![
            TaskRequest::new(TaskKind::GenerateBookThumbnail)
                .priority(priority.saturating_add(1))
                .into_queue_record_with_id(&book_id),
        ]
    } else {
        Vec::new()
    };
    Ok(TaskExecutionOutcome::with_follow_up_tasks(follow_up_tasks))
}

#[cfg(test)]
mod tests {
    use crate::sqlite::connect_test_pool;
    use crate::task_queue::TaskRuntimeContext;
    use crate::task_queue::queue_scheduler::TaskQueueScheduler;
    use crate::task_queue::test_support::{RuntimeTestFixture, execute_and_enqueue};
    use image::{ImageBuffer, Rgba};
    use komga_application::task_processing::{
        RemoveHashedPagesPayload, TaskKind, TaskQueueRecord, TaskRequest,
    };
    use sqlx::{Row, SqlitePool};
    use std::io::Write;

    use super::super::super::media_helpers::HashedPageToDelete;

    struct ZipFixturePageEntry {
        file_name: String,
        file_size: i64,
    }

    struct RemoveHashedPagesFailureFixture {
        runtime_fixture: RuntimeTestFixture,
        runtime: TaskRuntimeContext,
        task: TaskQueueRecord,
    }

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let image =
            ImageBuffer::<Rgba<u8>, Vec<u8>>::from_pixel(width, height, Rgba([12, 34, 56, 255]));
        let mut output = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut output, image::ImageFormat::Png)
            .expect("png fixture should encode");
        output.into_inner()
    }

    fn write_zip_fixture(path: &std::path::Path) -> Vec<ZipFixturePageEntry> {
        let file =
            std::fs::File::create(path).expect("remove-hashed-pages zip fixture should be created");
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        let entries = [("0001.png", png_bytes(3, 5)), ("0002.png", png_bytes(4, 6))];

        let mut fixture_pages = Vec::new();
        for (name, bytes) in entries {
            zip.start_file(name, options)
                .expect("remove-hashed-pages zip entry should start");
            zip.write_all(&bytes)
                .expect("remove-hashed-pages zip entry bytes should be written");
            fixture_pages.push(ZipFixturePageEntry {
                file_name: name.to_string(),
                file_size: bytes.len() as i64,
            });
        }
        zip.finish()
            .expect("remove-hashed-pages zip fixture should finish cleanly");

        fixture_pages
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
        .expect("series row should be inserted for hashing fixture");
    }

    async fn create_remove_hashed_pages_failure_fixture(
        case: &str,
        book_url: &str,
        media_type: &str,
        media_status: &str,
        create_source_file: bool,
    ) -> RemoveHashedPagesFailureFixture {
        let fixture = RuntimeTestFixture::new(&format!("remove-hashed-pages-{case}"));
        std::fs::create_dir_all(fixture.library_root.join("books"))
            .expect("remove-hashed-pages failure fixture root should be created");

        let source_path = fixture.library_root.join(book_url);
        let file_last_modified = if create_source_file {
            std::fs::write(&source_path, b"remove-hashed-pages-failure")
                .expect("remove-hashed-pages failure fixture source should be written");
            std::fs::metadata(&source_path)
                .expect("remove-hashed-pages failure fixture metadata should be readable")
                .modified()
                .expect("remove-hashed-pages failure fixture modified time should be readable")
                .duration_since(std::time::UNIX_EPOCH)
                .expect(
                    "remove-hashed-pages failure fixture modified time should be after unix epoch",
                )
                .as_secs() as i64
        } else {
            0_i64
        };

        let pool = fixture.main_pool().await;
        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
            .bind("library-1")
            .bind("Library 1")
            .bind(fixture.library_root.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .expect("remove-hashed-pages failure fixture library row should be inserted");
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
        .bind("book-1")
        .bind("book-1")
        .bind(book_url)
        .bind("library-1")
        .bind("series-1")
        .bind(file_last_modified)
        .bind(32_i64)
        .execute(&pool)
        .await
        .expect("remove-hashed-pages failure fixture book row should be inserted");
        sqlx::query("INSERT INTO MEDIA (BOOK_ID, MEDIA_TYPE, STATUS) VALUES (?, ?, ?)")
            .bind("book-1")
            .bind(media_type)
            .bind(media_status)
            .execute(&pool)
            .await
            .expect("remove-hashed-pages failure fixture media row should be inserted");
        pool.close().await;

        let runtime = fixture.runtime_context(false, false).await;
        let task = TaskRequest::with_payload(
            TaskKind::RemoveHashedPages,
            RemoveHashedPagesPayload::new(
                "book-1",
                vec![HashedPageToDelete {
                    file_hash: "hash-one".to_string(),
                    file_size: 111,
                    file_name: "0001.png".to_string(),
                    media_type: "image/png".to_string(),
                    page_number: 1,
                }],
            ),
        )
        .priority(12)
        .into_queue_record();

        RemoveHashedPagesFailureFixture {
            runtime_fixture: fixture,
            runtime,
            task,
        }
    }

    #[tokio::test]
    async fn missing_page_hash_finder_enqueues_kotlin_style_hash_book_pages_ids() {
        let fixture = RuntimeTestFixture::new("missing-page-hash-finder");
        let pool = fixture.main_pool().await;
        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT, HASH_PAGES) VALUES (?, ?, ?, ?)")
            .bind("library-1")
            .bind("Library 1")
            .bind(fixture.library_root.to_string_lossy().to_string())
            .bind(true)
            .execute(&pool)
            .await
            .expect("library row should be inserted for missing page hash finder fixture");
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
        .bind("books/book-1.cbz")
        .bind("library-1")
        .bind("series-1")
        .bind(0_i64)
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("book row should be inserted for missing page hash finder fixture");
        sqlx::query(
            "INSERT INTO MEDIA_PAGE (FILE_NAME, MEDIA_TYPE, NUMBER, BOOK_ID, FILE_HASH) VALUES (?, ?, ?, ?, ?)",
        )
            .bind("0001.png")
            .bind("image/png")
            .bind(0_i64)
            .bind("book-1")
            .bind("")
            .execute(&pool)
            .await
            .expect("media page row should be inserted for missing page hash finder fixture");
        pool.close().await;

        let runtime = fixture.runtime_context(false, true).await;
        let scheduler =
            TaskQueueScheduler::for_runtime(runtime.clone(), "missing-page-hash-finder-test").await;
        let finder_task = TaskRequest::new(TaskKind::FindBooksWithMissingPageHash)
            .into_queue_record_with_id("library-1");

        let result = execute_and_enqueue(&scheduler, &runtime, &finder_task).await;
        assert!(matches!(result, Some(Ok(()))));

        let generated = scheduler
            .admin_for_test()
            .await
            .admin
            .take_available("missing-page-hash-finder-assert")
            .expect("finder should enqueue one hash book pages task");

        assert_eq!(generated.id, "HashBookPages_book-1");
        assert_eq!(generated.simple_type, "HashBookPages");
        assert_eq!(generated.group, None);
        assert_eq!(generated.priority, 1);

        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn missing_page_hash_finder_skips_when_library_hash_pages_is_disabled() {
        let fixture = RuntimeTestFixture::new("missing-page-hash-disabled");
        let pool = fixture.main_pool().await;
        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT, HASH_PAGES) VALUES (?, ?, ?, ?)")
            .bind("library-1")
            .bind("Library 1")
            .bind(fixture.library_root.to_string_lossy().to_string())
            .bind(false)
            .execute(&pool)
            .await
            .expect("library row should be inserted for disabled page-hash fixture");
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
        .bind("books/book-1.cbz")
        .bind("library-1")
        .bind("series-1")
        .bind(0_i64)
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("book row should be inserted for disabled page-hash fixture");
        sqlx::query(
            "INSERT INTO MEDIA_PAGE (FILE_NAME, MEDIA_TYPE, NUMBER, BOOK_ID, FILE_HASH) VALUES (?, ?, ?, ?, ?)",
        )
            .bind("0001.png")
            .bind("image/png")
            .bind(0_i64)
            .bind("book-1")
            .bind("")
            .execute(&pool)
            .await
            .expect("media page row should be inserted for disabled page-hash fixture");
        pool.close().await;

        let runtime = fixture.runtime_context(false, true).await;
        let scheduler =
            TaskQueueScheduler::for_runtime(runtime.clone(), "missing-page-hash-disabled-test")
                .await;
        let finder_task = TaskRequest::new(TaskKind::FindBooksWithMissingPageHash)
            .priority(3)
            .into_queue_record_with_id("library-1");

        let result = execute_and_enqueue(&scheduler, &runtime, &finder_task).await;
        assert!(matches!(result, Some(Ok(()))));
        assert!(
            scheduler
                .admin_for_test()
                .await
                .admin
                .take_available("missing-page-hash-disabled-assert")
                .is_none(),
            "finder must not enqueue HashBookPages tasks when library.hashPages is disabled at execution time",
        );

        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn remove_hashed_pages_persists_duplicate_page_deleted_history_and_thumbnail_task() {
        let book_id = "book-1";
        let fixture = RuntimeTestFixture::new("remove-hashed-pages");
        std::fs::create_dir_all(fixture.library_root.join("books"))
            .expect("remove-hashed-pages library root should be created");

        let book_path = fixture.library_root.join("books/book-1.cbz");
        let page_entries = write_zip_fixture(book_path.as_path());
        let first_page = &page_entries[0];
        let second_page = &page_entries[1];
        let book_metadata = std::fs::metadata(&book_path)
            .expect("remove-hashed-pages book metadata should be readable");
        let file_last_modified = book_metadata
            .modified()
            .expect("remove-hashed-pages modified time should be readable")
            .duration_since(std::time::UNIX_EPOCH)
            .expect("remove-hashed-pages modified time should be after unix epoch")
            .as_secs() as i64;
        let file_size = i64::try_from(book_metadata.len()).unwrap_or(i64::MAX);

        let pool = fixture.main_pool().await;
        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
            .bind("library-1")
            .bind("Library 1")
            .bind(fixture.library_root.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .expect("remove-hashed-pages library row should be inserted");
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
        .bind("books/book-1.cbz")
        .bind("library-1")
        .bind("series-1")
        .bind(file_last_modified)
        .bind(file_size)
        .execute(&pool)
        .await
        .expect("remove-hashed-pages book row should be inserted");
        sqlx::query(
            "INSERT INTO MEDIA (BOOK_ID, MEDIA_TYPE, STATUS, PAGE_COUNT) VALUES (?, ?, ?, ?)",
        )
        .bind(book_id)
        .bind("application/zip")
        .bind("READY")
        .bind(2_i64)
        .execute(&pool)
        .await
        .expect("remove-hashed-pages media row should be inserted");
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
        .bind(&first_page.file_name)
        .bind("image/png")
        .bind(0_i64)
        .bind(book_id)
        .bind("hash-one")
        .bind(first_page.file_size)
        .execute(&pool)
        .await
        .expect("remove-hashed-pages first page row should be inserted");
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
        .bind(&second_page.file_name)
        .bind("image/png")
        .bind(1_i64)
        .bind(book_id)
        .bind("hash-two")
        .bind(second_page.file_size)
        .execute(&pool)
        .await
        .expect("remove-hashed-pages second page row should be inserted");
        sqlx::query("INSERT INTO PAGE_HASH (HASH, ACTION, DELETE_COUNT) VALUES (?, ?, ?)")
            .bind("hash-one")
            .bind("DELETE_AUTO")
            .bind(0_i64)
            .execute(&pool)
            .await
            .expect("remove-hashed-pages first page hash row should be inserted");
        sqlx::query("INSERT INTO PAGE_HASH (HASH, ACTION, DELETE_COUNT) VALUES (?, ?, ?)")
            .bind("hash-two")
            .bind("DELETE_AUTO")
            .bind(0_i64)
            .execute(&pool)
            .await
            .expect("remove-hashed-pages second page hash row should be inserted");
        pool.close().await;

        let runtime = fixture.runtime_context(false, false).await;
        let scheduler =
            TaskQueueScheduler::for_runtime(runtime.clone(), "remove-hashed-pages-test").await;
        let task = TaskRequest::with_payload(
            TaskKind::RemoveHashedPages,
            RemoveHashedPagesPayload::new(
                book_id,
                vec![HashedPageToDelete {
                    file_hash: "hash-one".to_string(),
                    file_size: first_page.file_size,
                    file_name: first_page.file_name.clone(),
                    media_type: "image/png".to_string(),
                    page_number: 1,
                }],
            ),
        )
        .priority(12)
        .into_queue_record();

        let result = execute_and_enqueue(&scheduler, &runtime, &task).await;
        assert!(matches!(result, Some(Ok(()))));

        let generated = scheduler
            .admin_for_test()
            .await
            .admin
            .take_available("remove-hashed-pages-thumbnail-assert")
            .expect(
                "remove-hashed-pages should enqueue generate thumbnail when first page is removed",
            );
        assert_eq!(generated.id, "GenerateBookThumbnail_book-1");
        assert_eq!(generated.simple_type, "GenerateBookThumbnail");

        let verify_pool = connect_test_pool(fixture.database_file.as_path(), 1)
            .await
            .expect("remove-hashed-pages verify db should open");
        let remaining_pages = sqlx::query(
            "SELECT FILE_NAME, NUMBER, FILE_HASH FROM MEDIA_PAGE WHERE BOOK_ID = ? ORDER BY NUMBER ASC",
        )
        .bind(book_id)
        .fetch_all(&verify_pool)
        .await
        .expect("remove-hashed-pages remaining media pages should be queryable");
        assert_eq!(remaining_pages.len(), 1);
        assert_eq!(remaining_pages[0].get::<String, _>("FILE_NAME"), "0002.png");
        assert_eq!(remaining_pages[0].get::<i64, _>("NUMBER"), 0_i64);

        let delete_count = sqlx::query("SELECT DELETE_COUNT FROM PAGE_HASH WHERE HASH = ?")
            .bind("hash-one")
            .fetch_one(&verify_pool)
            .await
            .expect("remove-hashed-pages delete count should be queryable")
            .get::<i64, _>("DELETE_COUNT");
        assert_eq!(delete_count, 1);

        let history_rows = sqlx::query(
            "SELECT ID, TYPE, BOOK_ID, SERIES_ID FROM HISTORICAL_EVENT ORDER BY ROWID ASC",
        )
        .fetch_all(&verify_pool)
        .await
        .expect("remove-hashed-pages historical events should be queryable");
        assert_eq!(history_rows.len(), 1);
        assert_eq!(
            history_rows[0].get::<String, _>("TYPE"),
            "DuplicatePageDeleted"
        );
        assert_eq!(
            history_rows[0].get::<Option<String>, _>("BOOK_ID"),
            Some(book_id.to_string())
        );
        assert_eq!(
            history_rows[0].get::<Option<String>, _>("SERIES_ID"),
            Some("series-1".to_string())
        );

        let props =
            sqlx::query("SELECT \"KEY\", VALUE FROM HISTORICAL_EVENT_PROPERTIES WHERE ID = ?")
                .bind(history_rows[0].get::<String, _>("ID"))
                .fetch_all(&verify_pool)
                .await
                .expect("remove-hashed-pages historical event properties should be queryable")
                .into_iter()
                .map(|row| (row.get::<String, _>("KEY"), row.get::<String, _>("VALUE")))
                .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            props.get("name"),
            Some(&book_path.to_string_lossy().to_string())
        );
        assert_eq!(props.get("page number"), Some(&"1".to_string()));
        assert_eq!(props.get("page file name"), Some(&"0001.png".to_string()));
        assert_eq!(props.get("page file hash"), Some(&"hash-one".to_string()));
        assert_eq!(
            props.get("page file size"),
            Some(&first_page.file_size.to_string())
        );
        assert_eq!(props.get("page media type"), Some(&"image/png".to_string()));
        verify_pool.close().await;

        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn remove_hashed_pages_fails_when_source_file_is_missing() {
        let fixture = create_remove_hashed_pages_failure_fixture(
            "missing-file",
            "books/book-1.cbz",
            "application/zip",
            "READY",
            false,
        )
        .await;
        let scheduler = TaskQueueScheduler::for_runtime(
            fixture.runtime.clone(),
            "remove-hashed-pages-missing-file-test",
        )
        .await;

        let result = execute_and_enqueue(&scheduler, &fixture.runtime, &fixture.task).await;
        let Some(Err(error)) = result else {
            panic!("remove-hashed-pages missing-file should fail");
        };
        assert!(
            error
                .message
                .contains("file not found for hashed-page removal")
        );

        fixture.runtime_fixture.cleanup().await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn remove_hashed_pages_propagates_source_metadata_errors() {
        let fixture = create_remove_hashed_pages_failure_fixture(
            "source-metadata-error",
            "blocked/book-1.cbz",
            "application/zip",
            "READY",
            false,
        )
        .await;
        std::fs::write(
            fixture.runtime_fixture.library_root.join("blocked"),
            b"not a directory",
        )
        .expect("blocking source component should be written");
        let scheduler = TaskQueueScheduler::for_runtime(
            fixture.runtime.clone(),
            "remove-hashed-pages-source-metadata-error-test",
        )
        .await;

        let result = execute_and_enqueue(&scheduler, &fixture.runtime, &fixture.task).await;
        let Some(Err(error)) = result else {
            panic!("remove-hashed-pages source metadata error should fail");
        };
        assert!(
            error
                .message
                .contains("failed to inspect source path for hashed-page removal")
        );

        fixture.runtime_fixture.cleanup().await;
    }

    #[tokio::test]
    async fn remove_hashed_pages_fails_when_media_type_is_unsupported() {
        let fixture = create_remove_hashed_pages_failure_fixture(
            "unsupported-media",
            "books/book-1.pdf",
            "application/pdf",
            "READY",
            true,
        )
        .await;
        let scheduler = TaskQueueScheduler::for_runtime(
            fixture.runtime.clone(),
            "remove-hashed-pages-unsupported-media-test",
        )
        .await;

        let result = execute_and_enqueue(&scheduler, &fixture.runtime, &fixture.task).await;
        let Some(Err(error)) = result else {
            panic!("remove-hashed-pages unsupported-media should fail");
        };
        assert!(
            error
                .message
                .contains("unsupported media type for hashed-page removal")
        );

        fixture.runtime_fixture.cleanup().await;
    }

    #[tokio::test]
    async fn remove_hashed_pages_fails_when_media_is_not_ready() {
        let fixture = create_remove_hashed_pages_failure_fixture(
            "media-not-ready",
            "books/book-1.cbz",
            "application/zip",
            "OUTDATED",
            true,
        )
        .await;
        let scheduler = TaskQueueScheduler::for_runtime(
            fixture.runtime.clone(),
            "remove-hashed-pages-not-ready-test",
        )
        .await;

        let result = execute_and_enqueue(&scheduler, &fixture.runtime, &fixture.task).await;
        let Some(Err(error)) = result else {
            panic!("remove-hashed-pages not-ready should fail");
        };
        assert!(
            error
                .message
                .contains("media not ready for hashed-page removal")
        );

        fixture.runtime_fixture.cleanup().await;
    }
}

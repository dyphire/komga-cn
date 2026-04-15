use super::*;

pub(super) fn try_execute(
    scheduler: &mut TaskQueueScheduler,
    runtime: &RuntimeConfig,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Option<Result<(), TaskExecutionError>> {
    let owns_main_database = runtime.task_runtime_context().owns_main_database;
    let result = match task.simple_type.as_str() {
        "HASH_BOOK_PAGES" => {
            if !owns_main_database {
                return Some(Ok(()));
            }
            let Some(book_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "HASH_BOOK_PAGES task must include a book id",
                )));
            };
            super::super::hash_book_pages(runtime, book_id)
        }
        "HASH_BOOK" => {
            let Some(book_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "HASH_BOOK task must include a book id",
                )));
            };
            super::super::hash_book(runtime, book_id, false)
        }
        "HASH_BOOK_KOREADER" => {
            let Some(book_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "HASH_BOOK_KOREADER task must include a book id",
                )));
            };
            super::super::hash_book(runtime, book_id, true)
        }
        "FIND_BOOKS_WITH_MISSING_PAGE_HASH" => {
            if !owns_main_database {
                return Some(Ok(()));
            }
            let Some(library_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "FIND_BOOKS_WITH_MISSING_PAGE_HASH task must include a library id",
                )));
            };
            let hashing_flags = match load_library_hashing_flags(runtime, library_id) {
                Ok(flags) => flags,
                Err(error) => return Some(Err(error)),
            };
            if !hashing_flags.hash_pages {
                return Some(Ok(()));
            }
            let book_ids = match find_books_with_missing_page_hash(runtime, task_target) {
                Ok(ids) => ids,
                Err(error) => return Some(Err(error)),
            };
            for book_id in book_ids {
                let priority = task.priority.saturating_add(1);
                scheduler.enqueue(runtime_follow_up_task(RuntimeFollowUpTask::HashBookPages {
                    book_id,
                    priority,
                }));
            }
            Ok(())
        }
        "FIND_DUPLICATE_PAGES_TO_DELETE" => {
            if !owns_main_database {
                return Some(Ok(()));
            }
            let Some(library_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "FIND_DUPLICATE_PAGES_TO_DELETE task must include a library id",
                )));
            };
            let targets = match find_duplicate_pages_to_delete(runtime, library_id) {
                Ok(targets) => targets,
                Err(error) => return Some(Err(error)),
            };
            for (book_id, pages) in targets {
                let priority = task.priority.saturating_add(1);
                let payload = match serde_json::to_string(
                    &super::super::RemoveHashedPagesPayload::new(book_id.clone(), pages, priority),
                ) {
                    Ok(payload) => payload,
                    Err(error) => {
                        return Some(Err(TaskExecutionError::runtime(format!(
                            "failed to serialize REMOVE_HASHED_PAGES payload: {error}",
                        ))));
                    }
                };
                scheduler.enqueue(runtime_follow_up_task(
                    RuntimeFollowUpTask::RemoveHashedPages {
                        book_id,
                        priority,
                        payload,
                    },
                ));
            }
            Ok(())
        }
        "REMOVE_HASHED_PAGES" => {
            if !owns_main_database {
                return Some(Ok(()));
            }
            let Some(book_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "REMOVE_HASHED_PAGES task must include a book id",
                )));
            };
            let Some(payload) = task.payload.as_deref() else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "REMOVE_HASHED_PAGES task requires serialized payload",
                )));
            };
            let parsed =
                match serde_json::from_str::<super::super::RemoveHashedPagesPayload>(payload) {
                    Ok(parsed) => parsed,
                    Err(error) => {
                        return Some(Err(TaskExecutionError::runtime(format!(
                            "failed to parse REMOVE_HASHED_PAGES payload: {error}",
                        ))));
                    }
                };
            if parsed.book_id != book_id {
                return Some(Err(TaskExecutionError::invalid_task(
                    "REMOVE_HASHED_PAGES payload book id must match task id",
                )));
            }
            if parsed.unique_id != task.id {
                return Some(Err(TaskExecutionError::invalid_task(
                    "REMOVE_HASHED_PAGES payload unique id must match task id",
                )));
            }

            let regenerate_thumbnail = match remove_hashed_pages(runtime, book_id, &parsed.pages) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            if regenerate_thumbnail {
                scheduler.enqueue(runtime_follow_up_task(
                    RuntimeFollowUpTask::GenerateBookThumbnail {
                        book_id: book_id.to_string(),
                        priority: task.priority.saturating_add(1),
                    },
                ));
            }
            Ok(())
        }
        _ => return None,
    };

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::connect_pool;
    use komga_application::task_processing::TaskQueueAdminPort;
    use komga_application::task_processing::TaskRuntimeContext;
    use serde_json::json;
    use sqlx::Row;
    use std::io::Write;

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

    fn write_zip_fixture(path: &std::path::Path) -> Vec<(String, i64)> {
        let file =
            std::fs::File::create(path).expect("remove-hashed-pages zip fixture should be created");
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        let entries = [
            ("0001.png", b"page-one".as_slice()),
            ("0002.png", b"page-two".as_slice()),
        ];

        for (name, bytes) in entries {
            zip.start_file(name, options)
                .expect("remove-hashed-pages zip entry should start");
            zip.write_all(bytes)
                .expect("remove-hashed-pages zip entry bytes should be written");
        }
        zip.finish()
            .expect("remove-hashed-pages zip fixture should finish cleanly");

        vec![
            ("0001.png".to_string(), 8_i64),
            ("0002.png".to_string(), 8_i64),
        ]
    }

    async fn create_remove_hashed_pages_failure_fixture(
        case: &str,
        book_url: &str,
        media_type: &str,
        media_status: &str,
        create_source_file: bool,
    ) -> (
        TaskRuntimeContext,
        TaskQueueRecord,
        std::path::PathBuf,
        std::path::PathBuf,
    ) {
        let database_file = unique_temp_path(&format!("komga-remove-hashed-pages-{case}-main"));
        let tasks_db_file = unique_temp_path(&format!("komga-remove-hashed-pages-{case}-tasks"));
        let lucene_dir = unique_temp_path(&format!("komga-remove-hashed-pages-{case}-lucene"));
        let library_root = unique_temp_path(&format!("komga-remove-hashed-pages-{case}-root"));
        std::fs::create_dir_all(library_root.join("books"))
            .expect("remove-hashed-pages failure fixture root should be created");

        let source_path = library_root.join(book_url);
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

        let pool = connect_pool(database_file.as_path(), 1)
            .await
            .expect("remove-hashed-pages failure fixture db should open");
        for ddl in [
            "CREATE TABLE LIBRARY (ID varchar NOT NULL PRIMARY KEY, ROOT varchar NOT NULL, ANALYZE_DIMENSIONS integer NOT NULL DEFAULT 1)",
            "CREATE TABLE BOOK (ID varchar NOT NULL PRIMARY KEY, URL varchar NOT NULL, LIBRARY_ID varchar NOT NULL, SERIES_ID varchar NOT NULL, FILE_LAST_MODIFIED datetime NOT NULL DEFAULT CURRENT_TIMESTAMP, FILE_SIZE int NOT NULL DEFAULT 0)",
            "CREATE TABLE MEDIA (BOOK_ID varchar NOT NULL PRIMARY KEY, MEDIA_TYPE varchar NOT NULL, STATUS varchar NOT NULL)",
            "CREATE TABLE MEDIA_PAGE (FILE_NAME varchar NOT NULL, MEDIA_TYPE varchar NOT NULL, NUMBER int NOT NULL, BOOK_ID varchar NOT NULL, FILE_HASH varchar NOT NULL DEFAULT '', FILE_SIZE int NOT NULL DEFAULT 0)",
        ] {
            sqlx::query(ddl)
                .execute(&pool)
                .await
                .expect("remove-hashed-pages failure fixture schema should be created");
        }
        sqlx::query("INSERT INTO LIBRARY (ID, ROOT) VALUES (?, ?)")
            .bind("library-1")
            .bind(library_root.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .expect("remove-hashed-pages failure fixture library row should be inserted");
        sqlx::query(
            "INSERT INTO BOOK (ID, URL, LIBRARY_ID, SERIES_ID, FILE_LAST_MODIFIED, FILE_SIZE) VALUES (?, ?, ?, ?, datetime(?, 'unixepoch'), ?)",
        )
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

        let runtime = TaskRuntimeContext {
            database_file: database_file.clone(),
            tasks_db_file,
            lucene_data_directory: lucene_dir,
            consumes_queue: false,
            owns_main_database: true,
            owns_filesystem_scan_output: true,
            owns_sidecar_output: true,
            owns_search_index: false,
        };
        let payload = serde_json::to_string(&super::super::RemoveHashedPagesPayload::new(
            "book-1".to_string(),
            vec![super::super::HashedPageToDelete {
                file_hash: "hash-one".to_string(),
                file_size: 111,
                file_name: "0001.png".to_string(),
                media_type: "image/png".to_string(),
                page_number: 1,
            }],
            12,
        ))
        .expect("remove-hashed-pages failure fixture payload should serialize");
        let task = TaskQueueRecord::new("REMOVE_HASHED_PAGES_book-1", 12, None)
            .with_simple_type("REMOVE_HASHED_PAGES")
            .with_payload(payload);

        (runtime, task, database_file, library_root)
    }

    #[tokio::test]
    async fn missing_page_hash_finder_enqueues_kotlin_style_hash_book_pages_ids() {
        let database_file = unique_temp_path("komga-missing-page-hash-finder-main");
        let tasks_db_file = unique_temp_path("komga-missing-page-hash-finder-tasks");
        let lucene_dir = unique_temp_path("komga-missing-page-hash-finder-lucene");

        let pool = connect_pool(database_file.as_path(), 1)
            .await
            .expect("missing page hash finder test db should open");
        for ddl in [
            "CREATE TABLE LIBRARY (ID varchar NOT NULL PRIMARY KEY, HASH_FILES integer NOT NULL DEFAULT 0, HASH_PAGES integer NOT NULL DEFAULT 0, HASH_KOREADER integer NOT NULL DEFAULT 0)",
            "CREATE TABLE BOOK (ID varchar NOT NULL PRIMARY KEY, LIBRARY_ID varchar NOT NULL, DELETED_DATE timestamp NULL)",
            "CREATE TABLE MEDIA_PAGE (BOOK_ID varchar NOT NULL, FILE_HASH varchar NULL)",
        ] {
            sqlx::query(ddl)
                .execute(&pool)
                .await
                .expect("missing page hash finder fixture schema should be created");
        }
        sqlx::query("INSERT INTO LIBRARY (ID, HASH_PAGES) VALUES (?, ?)")
            .bind("library-1")
            .bind(true)
            .execute(&pool)
            .await
            .expect("library row should be inserted for missing page hash finder fixture");
        sqlx::query("INSERT INTO BOOK (ID, LIBRARY_ID, DELETED_DATE) VALUES (?, ?, NULL)")
            .bind("book-1")
            .bind("library-1")
            .execute(&pool)
            .await
            .expect("book row should be inserted for missing page hash finder fixture");
        sqlx::query("INSERT INTO MEDIA_PAGE (BOOK_ID, FILE_HASH) VALUES (?, ?)")
            .bind("book-1")
            .bind("")
            .execute(&pool)
            .await
            .expect("media page row should be inserted for missing page hash finder fixture");
        pool.close().await;

        let runtime = TaskRuntimeContext {
            database_file: database_file.clone(),
            tasks_db_file,
            lucene_data_directory: lucene_dir,
            consumes_queue: false,
            owns_main_database: true,
            owns_filesystem_scan_output: true,
            owns_sidecar_output: true,
            owns_search_index: true,
        };
        let mut scheduler =
            TaskQueueScheduler::for_runtime(runtime.clone(), "missing-page-hash-finder-test");
        let finder_task =
            TaskQueueRecord::new("FIND_BOOKS_WITH_MISSING_PAGE_HASH_library-1", 0, None)
                .with_simple_type("FIND_BOOKS_WITH_MISSING_PAGE_HASH");

        let result = try_execute(&mut scheduler, &runtime, &finder_task, Some("library-1"));
        assert!(matches!(result, Some(Ok(()))));

        let generated = scheduler
            .admin_mut()
            .take_available("missing-page-hash-finder-assert")
            .expect("finder should enqueue one hash book pages task");

        assert_eq!(generated.id, "HASH_BOOK_PAGES_book-1");
        assert_eq!(generated.simple_type, "HASH_BOOK_PAGES");
        assert_eq!(generated.group, None);
        assert_eq!(
            generated
                .payload
                .as_deref()
                .and_then(|payload| serde_json::from_str::<serde_json::Value>(payload).ok()),
            Some(json!({
                "bookId": "book-1",
                "priority": 1,
                "groupId": serde_json::Value::Null,
                "uniqueId": "HASH_BOOK_PAGES_book-1"
            })),
        );

        let _ = std::fs::remove_file(database_file);
    }

    #[tokio::test]
    async fn missing_page_hash_finder_skips_when_library_hash_pages_is_disabled() {
        let database_file = unique_temp_path("komga-missing-page-hash-disabled-main");
        let tasks_db_file = unique_temp_path("komga-missing-page-hash-disabled-tasks");
        let lucene_dir = unique_temp_path("komga-missing-page-hash-disabled-lucene");

        let pool = connect_pool(database_file.as_path(), 1)
            .await
            .expect("missing page hash disabled test db should open");
        for ddl in [
            "CREATE TABLE LIBRARY (ID varchar NOT NULL PRIMARY KEY, HASH_FILES integer NOT NULL DEFAULT 0, HASH_PAGES integer NOT NULL DEFAULT 0, HASH_KOREADER integer NOT NULL DEFAULT 0)",
            "CREATE TABLE BOOK (ID varchar NOT NULL PRIMARY KEY, LIBRARY_ID varchar NOT NULL, DELETED_DATE timestamp NULL)",
            "CREATE TABLE MEDIA_PAGE (BOOK_ID varchar NOT NULL, FILE_HASH varchar NULL)",
        ] {
            sqlx::query(ddl)
                .execute(&pool)
                .await
                .expect("missing page hash disabled fixture schema should be created");
        }
        sqlx::query("INSERT INTO LIBRARY (ID, HASH_PAGES) VALUES (?, ?)")
            .bind("library-1")
            .bind(false)
            .execute(&pool)
            .await
            .expect("library row should be inserted for disabled page-hash fixture");
        sqlx::query("INSERT INTO BOOK (ID, LIBRARY_ID, DELETED_DATE) VALUES (?, ?, NULL)")
            .bind("book-1")
            .bind("library-1")
            .execute(&pool)
            .await
            .expect("book row should be inserted for disabled page-hash fixture");
        sqlx::query("INSERT INTO MEDIA_PAGE (BOOK_ID, FILE_HASH) VALUES (?, ?)")
            .bind("book-1")
            .bind("")
            .execute(&pool)
            .await
            .expect("media page row should be inserted for disabled page-hash fixture");
        pool.close().await;

        let runtime = TaskRuntimeContext {
            database_file: database_file.clone(),
            tasks_db_file,
            lucene_data_directory: lucene_dir,
            consumes_queue: false,
            owns_main_database: true,
            owns_filesystem_scan_output: true,
            owns_sidecar_output: true,
            owns_search_index: true,
        };
        let mut scheduler =
            TaskQueueScheduler::for_runtime(runtime.clone(), "missing-page-hash-disabled-test");
        let finder_task =
            TaskQueueRecord::new("FIND_BOOKS_WITH_MISSING_PAGE_HASH_library-1", 3, None)
                .with_simple_type("FIND_BOOKS_WITH_MISSING_PAGE_HASH");

        let result = try_execute(&mut scheduler, &runtime, &finder_task, Some("library-1"));
        assert!(matches!(result, Some(Ok(()))));
        assert!(
            scheduler
                .admin_mut()
                .take_available("missing-page-hash-disabled-assert")
                .is_none(),
            "finder must not enqueue HASH_BOOK_PAGES tasks when library.hashPages is disabled at execution time",
        );

        let _ = std::fs::remove_file(database_file);
    }

    #[tokio::test]
    async fn remove_hashed_pages_persists_duplicate_page_deleted_history_and_thumbnail_task() {
        let book_id = "book-1";
        let database_file = unique_temp_path("komga-remove-hashed-pages-main");
        let tasks_db_file = unique_temp_path("komga-remove-hashed-pages-tasks");
        let lucene_dir = unique_temp_path("komga-remove-hashed-pages-lucene");
        let library_root = unique_temp_path("komga-remove-hashed-pages-root");
        std::fs::create_dir_all(library_root.join("books"))
            .expect("remove-hashed-pages library root should be created");

        let book_path = library_root.join("books/book-1.cbz");
        let page_entries = write_zip_fixture(book_path.as_path());
        let book_metadata = std::fs::metadata(&book_path)
            .expect("remove-hashed-pages book metadata should be readable");
        let file_last_modified = book_metadata
            .modified()
            .expect("remove-hashed-pages modified time should be readable")
            .duration_since(std::time::UNIX_EPOCH)
            .expect("remove-hashed-pages modified time should be after unix epoch")
            .as_secs() as i64;
        let file_size = i64::try_from(book_metadata.len()).unwrap_or(i64::MAX);

        let pool = connect_pool(database_file.as_path(), 1)
            .await
            .expect("remove-hashed-pages db should open");
        for ddl in [
            "CREATE TABLE LIBRARY (ID varchar NOT NULL PRIMARY KEY, ROOT varchar NOT NULL, ANALYZE_DIMENSIONS integer NOT NULL DEFAULT 1)",
            "CREATE TABLE BOOK (ID varchar NOT NULL PRIMARY KEY, NAME varchar NOT NULL, URL varchar NOT NULL, LIBRARY_ID varchar NOT NULL, SERIES_ID varchar NOT NULL, FILE_LAST_MODIFIED datetime NOT NULL DEFAULT CURRENT_TIMESTAMP, FILE_SIZE int NOT NULL DEFAULT 0, FILE_HASH varchar NOT NULL DEFAULT '', FILE_HASH_KOREADER varchar NOT NULL DEFAULT '', LAST_MODIFIED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP)",
            "CREATE TABLE BOOK_METADATA (BOOK_ID varchar NOT NULL PRIMARY KEY, TITLE varchar NULL)",
            "CREATE TABLE MEDIA (BOOK_ID varchar NOT NULL PRIMARY KEY, MEDIA_TYPE varchar NOT NULL, STATUS varchar NOT NULL, PAGE_COUNT int NOT NULL DEFAULT 0, LAST_MODIFIED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP)",
            "CREATE TABLE MEDIA_PAGE (FILE_NAME varchar NOT NULL, MEDIA_TYPE varchar NOT NULL, NUMBER int NOT NULL, BOOK_ID varchar NOT NULL, width int NULL, height int NULL, FILE_HASH varchar NOT NULL DEFAULT '', FILE_SIZE int NOT NULL DEFAULT 0)",
            "CREATE TABLE PAGE_HASH (HASH varchar NOT NULL PRIMARY KEY, DELETE_COUNT int NOT NULL DEFAULT 0, LAST_MODIFIED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP)",
            "CREATE TABLE HISTORICAL_EVENT (ID varchar NOT NULL PRIMARY KEY, TYPE varchar NOT NULL, BOOK_ID varchar NULL, SERIES_ID varchar NULL, TIMESTAMP datetime NOT NULL DEFAULT CURRENT_TIMESTAMP)",
            "CREATE TABLE HISTORICAL_EVENT_PROPERTIES (ID varchar NOT NULL, \"KEY\" varchar NOT NULL, VALUE varchar NOT NULL)",
        ] {
            sqlx::query(ddl)
                .execute(&pool)
                .await
                .expect("remove-hashed-pages fixture schema should be created");
        }
        sqlx::query("INSERT INTO LIBRARY (ID, ROOT) VALUES (?, ?)")
            .bind("library-1")
            .bind(library_root.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .expect("remove-hashed-pages library row should be inserted");
        sqlx::query(
            "INSERT INTO BOOK (ID, NAME, URL, LIBRARY_ID, SERIES_ID, FILE_LAST_MODIFIED, FILE_SIZE) VALUES (?, ?, ?, ?, ?, datetime(?, 'unixepoch'), ?)",
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
            "INSERT INTO MEDIA_PAGE (FILE_NAME, MEDIA_TYPE, NUMBER, BOOK_ID, width, height, FILE_HASH, FILE_SIZE) VALUES (?, ?, ?, ?, NULL, NULL, ?, ?)",
        )
        .bind(&page_entries[0].0)
        .bind("image/png")
        .bind(0_i64)
        .bind(book_id)
        .bind("hash-one")
        .bind(page_entries[0].1)
        .execute(&pool)
        .await
        .expect("remove-hashed-pages first page row should be inserted");
        sqlx::query(
            "INSERT INTO MEDIA_PAGE (FILE_NAME, MEDIA_TYPE, NUMBER, BOOK_ID, width, height, FILE_HASH, FILE_SIZE) VALUES (?, ?, ?, ?, NULL, NULL, ?, ?)",
        )
        .bind(&page_entries[1].0)
        .bind("image/png")
        .bind(1_i64)
        .bind(book_id)
        .bind("hash-two")
        .bind(page_entries[1].1)
        .execute(&pool)
        .await
        .expect("remove-hashed-pages second page row should be inserted");
        sqlx::query("INSERT INTO PAGE_HASH (HASH, DELETE_COUNT) VALUES (?, ?)")
            .bind("hash-one")
            .bind(0_i64)
            .execute(&pool)
            .await
            .expect("remove-hashed-pages first page hash row should be inserted");
        sqlx::query("INSERT INTO PAGE_HASH (HASH, DELETE_COUNT) VALUES (?, ?)")
            .bind("hash-two")
            .bind(0_i64)
            .execute(&pool)
            .await
            .expect("remove-hashed-pages second page hash row should be inserted");
        pool.close().await;

        let runtime = TaskRuntimeContext {
            database_file: database_file.clone(),
            tasks_db_file,
            lucene_data_directory: lucene_dir,
            consumes_queue: false,
            owns_main_database: true,
            owns_filesystem_scan_output: true,
            owns_sidecar_output: true,
            owns_search_index: false,
        };
        let mut scheduler =
            TaskQueueScheduler::for_runtime(runtime.clone(), "remove-hashed-pages-test");
        let payload = serde_json::to_string(&super::super::RemoveHashedPagesPayload::new(
            book_id.to_string(),
            vec![super::super::HashedPageToDelete {
                file_hash: "hash-one".to_string(),
                file_size: page_entries[0].1,
                file_name: page_entries[0].0.clone(),
                media_type: "image/png".to_string(),
                page_number: 1,
            }],
            12,
        ))
        .expect("remove-hashed-pages payload should serialize");
        let task = TaskQueueRecord::new("REMOVE_HASHED_PAGES_book-1", 12, None)
            .with_simple_type("REMOVE_HASHED_PAGES")
            .with_payload(payload);

        let result = try_execute(&mut scheduler, &runtime, &task, Some(book_id));
        assert!(matches!(result, Some(Ok(()))));

        let generated = scheduler
            .admin_mut()
            .take_available("remove-hashed-pages-thumbnail-assert")
            .expect(
                "remove-hashed-pages should enqueue generate thumbnail when first page is removed",
            );
        assert_eq!(generated.id, "GENERATE_BOOK_THUMBNAIL_book-1");
        assert_eq!(generated.simple_type, "GENERATE_BOOK_THUMBNAIL");

        let verify_pool = connect_pool(database_file.as_path(), 1)
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
            Some(&page_entries[0].1.to_string())
        );
        assert_eq!(props.get("page media type"), Some(&"image/png".to_string()));
        verify_pool.close().await;

        let _ = std::fs::remove_file(database_file);
        let _ = std::fs::remove_file(book_path);
        let _ = std::fs::remove_dir_all(library_root);
    }

    #[tokio::test]
    async fn remove_hashed_pages_fails_when_source_file_is_missing() {
        let (runtime, task, database_file, library_root) =
            create_remove_hashed_pages_failure_fixture(
                "missing-file",
                "books/book-1.cbz",
                "application/zip",
                "READY",
                false,
            )
            .await;
        let mut scheduler = TaskQueueScheduler::for_runtime(
            runtime.clone(),
            "remove-hashed-pages-missing-file-test",
        );

        let result = try_execute(&mut scheduler, &runtime, &task, Some("book-1"));
        let Some(Err(error)) = result else {
            panic!("remove-hashed-pages missing-file should fail");
        };
        assert!(
            error
                .message
                .contains("file not found for hashed-page removal")
        );

        let _ = std::fs::remove_file(database_file);
        let _ = std::fs::remove_dir_all(library_root);
    }

    #[tokio::test]
    async fn remove_hashed_pages_fails_when_media_type_is_unsupported() {
        let (runtime, task, database_file, library_root) =
            create_remove_hashed_pages_failure_fixture(
                "unsupported-media",
                "books/book-1.pdf",
                "application/pdf",
                "READY",
                true,
            )
            .await;
        let mut scheduler = TaskQueueScheduler::for_runtime(
            runtime.clone(),
            "remove-hashed-pages-unsupported-media-test",
        );

        let result = try_execute(&mut scheduler, &runtime, &task, Some("book-1"));
        let Some(Err(error)) = result else {
            panic!("remove-hashed-pages unsupported-media should fail");
        };
        assert!(
            error
                .message
                .contains("unsupported media type for hashed-page removal")
        );

        let _ = std::fs::remove_file(database_file);
        let _ = std::fs::remove_dir_all(library_root);
    }

    #[tokio::test]
    async fn remove_hashed_pages_fails_when_media_is_not_ready() {
        let (runtime, task, database_file, library_root) =
            create_remove_hashed_pages_failure_fixture(
                "media-not-ready",
                "books/book-1.cbz",
                "application/zip",
                "OUTDATED",
                true,
            )
            .await;
        let mut scheduler =
            TaskQueueScheduler::for_runtime(runtime.clone(), "remove-hashed-pages-not-ready-test");

        let result = try_execute(&mut scheduler, &runtime, &task, Some("book-1"));
        let Some(Err(error)) = result else {
            panic!("remove-hashed-pages not-ready should fail");
        };
        assert!(
            error
                .message
                .contains("media not ready for hashed-page removal")
        );

        let _ = std::fs::remove_file(database_file);
        let _ = std::fs::remove_dir_all(library_root);
    }
}

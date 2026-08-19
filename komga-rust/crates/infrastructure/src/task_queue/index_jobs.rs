use crate::search::index_lifecycle::SearchEntityType;
use crate::task_queue::JobRuntime;
use crate::task_queue::media_helpers::{
    find_books_for_thumbnail_regeneration, find_books_with_undersized_generated_thumbnails,
};
use komga_application::task_processing::{RefreshBookMetadataPayload, TaskKind, TaskRequest};
use komga_application::task_processing::{TaskExecutionOutcome, TaskProcessingError};
use komga_domain::discovery::MediaStatus;

pub(in crate::task_queue) async fn execute_analyze_book(
    runtime: &JobRuntime<'_>,
    book_id: &str,
    priority: i32,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    let book_id = book_id.to_string();
    let outcome = super::index_tasks::analyze_book(runtime, &book_id).await?;

    if outcome.media_status == Some(MediaStatus::Ready) && !outcome.series_id.is_empty() {
        let follow_up_priority = priority.saturating_add(1);
        return Ok(TaskExecutionOutcome::with_follow_up_tasks(vec![
            TaskRequest::new(TaskKind::GenerateBookThumbnail)
                .priority(follow_up_priority)
                .into_queue_record_with_id(&book_id),
            TaskRequest::with_payload(
                TaskKind::RefreshBookMetadata,
                RefreshBookMetadataPayload::new(book_id.clone()),
            )
            .priority(follow_up_priority)
            .group(outcome.series_id)
            .into_queue_record(),
        ]));
    }

    Ok(TaskExecutionOutcome::completed())
}

pub(in crate::task_queue) async fn execute_rebuild_index(
    runtime: &JobRuntime<'_>,
    entity_types: Option<&[SearchEntityType]>,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    super::index_tasks::rebuild_index(runtime, entity_types).await?;

    Ok(TaskExecutionOutcome::completed())
}

pub(in crate::task_queue) async fn execute_find_book_thumbnails_to_regenerate(
    runtime: &JobRuntime<'_>,
    for_bigger_result_only: bool,
    priority: i32,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    let book_ids = if for_bigger_result_only {
        let max_edge = i64::from(
            runtime
                .thumbnail_regeneration_policy()
                .generated_thumbnail_max_edge,
        );
        find_books_with_undersized_generated_thumbnails(runtime, max_edge).await?
    } else {
        find_books_for_thumbnail_regeneration(runtime).await?
    };
    let follow_up_tasks = book_ids
        .into_iter()
        .map(|book_id| {
            TaskRequest::new(TaskKind::GenerateBookThumbnail)
                .priority(priority)
                .into_queue_record_with_id(&book_id)
        })
        .collect();
    Ok(TaskExecutionOutcome::with_follow_up_tasks(follow_up_tasks))
}

#[cfg(test)]
mod tests {
    use crate::database_handle::DatabaseHandle;
    use crate::sqlite::{
        connect_main_write_context, connect_task_pool, connect_task_write_pool, connect_test_pool,
        default_read_max_connections,
    };
    use crate::task_queue::queue_scheduler::TaskQueueScheduler;
    use crate::task_queue::test_support::{RuntimeTestFixture, execute_and_enqueue};
    use crate::task_queue::{TaskRuntimeContext, TaskRuntimeOwnershipOverrides};
    use image::{ImageBuffer, Rgba};
    use komga_application::task_processing::{
        BookPayload, FindBookThumbnailsToRegeneratePayload, TaskKind, TaskRequest,
        ThumbnailRegenerationPolicy,
    };
    use komga_domain::media_assets::ThumbnailType;
    use sqlx::{Row, SqlitePool};
    use std::fs::File;
    use std::io::Write;
    use zip::CompressionMethod;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    #[derive(Debug, PartialEq, Eq)]
    struct FixturePageSize {
        width: u32,
        height: u32,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct PersistedPageDimensions {
        number: i64,
        width: Option<i64>,
        height: Option<i64>,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct PersistedMediaSummary {
        status: String,
        page_count: i64,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct QueuedThumbnailTask {
        id: String,
        simple_type: String,
        priority: i32,
        group: Option<String>,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct QueuedFollowUpTask {
        id: String,
        simple_type: String,
        priority: i32,
        group: Option<String>,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct PersistedReadProgress {
        user_id: String,
        page: i64,
        completed: i64,
        last_modified_date: Option<String>,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct PersistedSeriesProgress {
        user_id: String,
        read_count: i64,
        in_progress_count: i64,
        last_modified_date: String,
    }

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

    fn archive_fixture_path(file_name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../komga/src/test/resources/archives")
            .join(file_name)
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

    fn write_cbz_fixture(path: &std::path::Path, page_sizes: &[FixturePageSize]) {
        let file = File::create(path).expect("cbz fixture should be created");
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o644);
        for (index, page_size) in page_sizes.iter().enumerate() {
            zip.start_file(format!("{:08}.png", index + 1), options)
                .expect("cbz page entry should be created");
            zip.write_all(&png_bytes(page_size.width, page_size.height))
                .expect("cbz page bytes should be written");
        }
        zip.finish().expect("cbz fixture should finish");
    }

    fn write_invalid_cbz_fixture(path: &std::path::Path) {
        let file = File::create(path).expect("invalid cbz fixture should be created");
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o644);
        zip.start_file("00000001.png", options)
            .expect("invalid cbz page entry should be created");
        zip.write_all(b"not-an-image")
            .expect("invalid cbz page bytes should be written");
        zip.finish().expect("invalid cbz fixture should finish");
    }

    fn write_kepub_fixture(path: &std::path::Path) {
        let file = File::create(path).expect("KEPUB fixture should be created");
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o644);
        for (name, bytes) in [
            ("mimetype", b"application/epub+zip".as_slice()),
            (
                "META-INF/container.xml",
                br#"<container><rootfiles><rootfile full-path="content.opf"/></rootfiles></container>"#,
            ),
            (
                "content.opf",
                br#"<package><manifest><item id="page" href="page.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="page"/></spine></package>"#,
            ),
            (
                "page.xhtml",
                br#"<html><body><span class="koboSpan" id="kobo.1.1">Text</span></body></html>"#,
            ),
        ] {
            zip.start_file(name, options)
                .expect("KEPUB fixture entry should be created");
            zip.write_all(bytes)
                .expect("KEPUB fixture entry should be written");
        }
        zip.finish().expect("KEPUB fixture should finish");
    }

    fn write_missing_resource_epub_fixture(path: &std::path::Path) {
        let file = File::create(path).expect("partial EPUB fixture should be created");
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o644);
        for (name, bytes) in [
            ("mimetype", b"application/epub+zip".as_slice()),
            (
                "META-INF/container.xml",
                br#"<container><rootfiles><rootfile full-path="content.opf"/></rootfiles></container>"#,
            ),
            (
                "content.opf",
                br#"<package><manifest><item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/><item id="missing" href="missing.css" media-type="text/css"/></manifest><spine><itemref idref="chapter"/></spine></package>"#,
            ),
            (
                "chapter.xhtml",
                br#"<html xmlns="http://www.w3.org/1999/xhtml"><body>Text</body></html>"#,
            ),
        ] {
            zip.start_file(name, options)
                .expect("partial EPUB entry should be created");
            zip.write_all(bytes)
                .expect("partial EPUB entry should be written");
        }
        zip.finish().expect("partial EPUB fixture should finish");
    }

    fn write_cbz_with_non_page_fixture(path: &std::path::Path) {
        let file = File::create(path).expect("CBZ metadata fixture should be created");
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o644);
        for (name, bytes) in [
            ("00000001.png", png_bytes(48, 96)),
            (
                "ComicInfo.xml",
                br#"<ComicInfo><Title>Fixture</Title></ComicInfo>"#.to_vec(),
            ),
            ("notes.txt", b"fixture notes".to_vec()),
        ] {
            zip.start_file(name, options)
                .expect("CBZ metadata entry should be created");
            zip.write_all(&bytes)
                .expect("CBZ metadata entry should be written");
        }
        zip.finish().expect("CBZ metadata fixture should finish");
    }

    async fn open_bootstrapped_main_pool(database_file: &std::path::Path) -> SqlitePool {
        let context = connect_main_write_context(database_file)
            .await
            .expect("index-jobs fixture db should bootstrap main schema");
        context.pool().clone()
    }

    async fn insert_library(
        pool: &SqlitePool,
        library_root: &std::path::Path,
        analyze_dimensions: bool,
    ) {
        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT, ANALYZE_DIMENSIONS) VALUES (?, ?, ?, ?)")
            .bind("library-1")
            .bind("Library 1")
            .bind(library_root.to_string_lossy().to_string())
            .bind(analyze_dimensions)
            .execute(pool)
            .await
            .expect("index-jobs fixture library row should be inserted");
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
        .expect("index-jobs fixture series row should be inserted");
    }

    async fn insert_book(
        pool: &SqlitePool,
        book_id: &str,
        name: &str,
        url: &str,
        library_id: &str,
        series_id: &str,
        deleted_date: Option<&str>,
    ) {
        sqlx::query(
            r#"
            INSERT INTO BOOK (
                ID,
                FILE_LAST_MODIFIED,
                NAME,
                URL,
                SERIES_ID,
                FILE_SIZE,
                NUMBER,
                LIBRARY_ID,
                DELETED_DATE
            )
            VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(book_id)
        .bind(0_i64)
        .bind(name)
        .bind(url)
        .bind(series_id)
        .bind(0_i64)
        .bind(1_i64)
        .bind(library_id)
        .bind(deleted_date)
        .execute(pool)
        .await
        .expect("index-jobs fixture book row should be inserted");
    }

    async fn insert_user(pool: &SqlitePool, user_id: &str) {
        sqlx::query("INSERT INTO USER (ID, EMAIL, PASSWORD) VALUES (?, ?, ?)")
            .bind(user_id)
            .bind(format!("{user_id}@example.org"))
            .bind("test-password")
            .execute(pool)
            .await
            .expect("index-jobs fixture user row should be inserted");
    }

    async fn seed_analyze_book_dimension_fixture(
        case: &str,
        analyze_dimensions: bool,
    ) -> RuntimeTestFixture {
        let fixture = RuntimeTestFixture::new(&format!("analyze-book-dimensions-{case}"));
        std::fs::create_dir_all(fixture.library_root.join("books"))
            .expect("analyze-book dimensions library root should be created");
        let archive_path = fixture.library_root.join("books/book-1.cbz");
        write_cbz_fixture(
            &archive_path,
            &[
                FixturePageSize {
                    width: 48,
                    height: 96,
                },
                FixturePageSize {
                    width: 120,
                    height: 80,
                },
            ],
        );

        let pool = open_bootstrapped_main_pool(fixture.database_file.as_path()).await;
        insert_library(&pool, &fixture.library_root, analyze_dimensions).await;
        insert_series(&pool, "library-1", "series-1").await;
        insert_book(
            &pool,
            "book-1",
            "book-1",
            "books/book-1.cbz",
            "library-1",
            "series-1",
            None,
        )
        .await;
        pool.close().await;

        fixture
    }

    async fn load_persisted_page_dimensions(
        database_file: &std::path::Path,
        book_id: &str,
    ) -> Vec<PersistedPageDimensions> {
        let pool = connect_test_pool(database_file, 1)
            .await
            .expect("page dimension verify db should open");
        let rows = sqlx::query(
            r#"
            SELECT NUMBER, width, height
            FROM MEDIA_PAGE
            WHERE BOOK_ID = ?
            ORDER BY NUMBER ASC
            "#,
        )
        .bind(book_id)
        .fetch_all(&pool)
        .await
        .expect("page dimensions should be queryable");
        pool.close().await;

        rows.into_iter()
            .map(|row| PersistedPageDimensions {
                number: row.get("NUMBER"),
                width: row.get("width"),
                height: row.get("height"),
            })
            .collect()
    }

    async fn load_persisted_media_summary(
        database_file: &std::path::Path,
        book_id: &str,
    ) -> PersistedMediaSummary {
        let pool = connect_test_pool(database_file, 1)
            .await
            .expect("media summary verify db should open");
        let row = sqlx::query("SELECT STATUS, PAGE_COUNT FROM MEDIA WHERE BOOK_ID = ? LIMIT 1")
            .bind(book_id)
            .fetch_one(&pool)
            .await
            .expect("media summary should be queryable");
        pool.close().await;

        PersistedMediaSummary {
            status: row.get("STATUS"),
            page_count: row.get("PAGE_COUNT"),
        }
    }

    async fn load_persisted_epub_capabilities(pool: &SqlitePool, book_id: &str) -> (bool, bool) {
        let row = sqlx::query(
            "SELECT EPUB_DIVINA_COMPATIBLE, EPUB_IS_KEPUB FROM MEDIA WHERE BOOK_ID = ? LIMIT 1",
        )
        .bind(book_id)
        .fetch_one(pool)
        .await
        .expect("EPUB capabilities should be queryable");
        (row.get("EPUB_DIVINA_COMPATIBLE"), row.get("EPUB_IS_KEPUB"))
    }

    async fn load_persisted_media_comment(
        database_file: &std::path::Path,
        book_id: &str,
    ) -> Option<String> {
        let pool = connect_test_pool(database_file, 1)
            .await
            .expect("media comment verify db should open");
        let row = sqlx::query("SELECT COMMENT FROM MEDIA WHERE BOOK_ID = ? LIMIT 1")
            .bind(book_id)
            .fetch_one(&pool)
            .await
            .expect("media comment should be queryable");
        let comment = row.get("COMMENT");
        pool.close().await;
        comment
    }

    async fn load_persisted_media_files(
        database_file: &std::path::Path,
        book_id: &str,
    ) -> Vec<(String, Option<String>, Option<String>, Option<i64>)> {
        let pool = connect_test_pool(database_file, 1)
            .await
            .expect("media files verify db should open");
        let rows = sqlx::query(
            "SELECT FILE_NAME, MEDIA_TYPE, SUB_TYPE, FILE_SIZE FROM MEDIA_FILE WHERE BOOK_ID = ? ORDER BY rowid ASC",
        )
        .bind(book_id)
        .fetch_all(&pool)
        .await
        .expect("media files should be queryable");
        pool.close().await;
        rows.into_iter()
            .map(|row| {
                (
                    row.get("FILE_NAME"),
                    row.get("MEDIA_TYPE"),
                    row.get("SUB_TYPE"),
                    row.get("FILE_SIZE"),
                )
            })
            .collect()
    }

    fn analyzed_fixture_page_count(file_name: &str, _book_url: &str) -> i64 {
        super::super::media_helpers::analyze_book_media_file(
            &archive_fixture_path(file_name),
            false,
        )
        .expect("analyze-book fixture should be analyzable")
        .pages
        .len() as i64
    }

    #[tokio::test]
    async fn thumbnail_finder_full_regeneration_targets_all_non_deleted_books() {
        let database_file = unique_temp_path("komga-thumbnail-finder-all-books-main");
        let tasks_db_file = unique_temp_path("komga-thumbnail-finder-all-books-tasks");
        let lucene_dir = unique_temp_path("komga-thumbnail-finder-all-books-lucene");
        let library_root = unique_temp_path("komga-thumbnail-finder-all-books-root");

        let pool = open_bootstrapped_main_pool(database_file.as_path()).await;
        insert_library(&pool, &library_root, true).await;
        insert_series(&pool, "library-1", "series-1").await;
        for (book_id, deleted_date) in [
            ("book-1", Option::<String>::None),
            ("book-2", Option::<String>::None),
            ("book-3", Some("2025-01-01 00:00:00".to_string())),
        ] {
            insert_book(
                &pool,
                book_id,
                book_id,
                &format!("books/{book_id}.cbz"),
                "library-1",
                "series-1",
                deleted_date.as_deref(),
            )
            .await;
        }
        sqlx::query("INSERT INTO THUMBNAIL_BOOK (ID, BOOK_ID, TYPE, SELECTED) VALUES (?, ?, ?, ?)")
            .bind("thumb-book-1")
            .bind("book-1")
            .bind(ThumbnailType::UserUploaded.persisted_name())
            .bind(true)
            .execute(&pool)
            .await
            .expect("selected thumbnail row should be inserted for book-1");
        pool.close().await;

        let task_write_pool = connect_task_write_pool(&database_file)
            .await
            .expect("test private write pool should open");
        let task_read_pool = connect_task_pool(&database_file, default_read_max_connections())
            .await
            .expect("test private read pool should open");
        let runtime = TaskRuntimeContext::new(
            DatabaseHandle::file_backed(database_file.clone())
                .await
                .expect("test db should open"),
            tasks_db_file,
            lucene_dir,
            false,
            1,
            task_write_pool,
            task_read_pool,
        );
        let scheduler =
            TaskQueueScheduler::for_runtime(runtime.clone(), "thumbnail-finder-all-books-test")
                .await;
        let finder_task = TaskRequest::with_payload(
            TaskKind::FindBookThumbnailsToRegenerate,
            FindBookThumbnailsToRegeneratePayload::new(false),
        )
        .priority(6)
        .into_queue_record();

        let result = execute_and_enqueue(&scheduler, &runtime, &finder_task).await;
        assert!(matches!(result, Some(Ok(()))));

        let mut generated = Vec::new();
        while let Some(task) = scheduler
            .admin_for_test()
            .await
            .admin
            .take_available("thumbnail-finder-all-books-assert")
        {
            generated.push(QueuedThumbnailTask {
                id: task.id,
                simple_type: task.simple_type,
                priority: task.priority,
                group: task.group,
            });
        }
        generated.sort_by(|left, right| left.id.cmp(&right.id));

        assert_eq!(
            generated,
            vec![
                QueuedThumbnailTask {
                    id: "GenerateBookThumbnail_book-1".to_string(),
                    simple_type: "GenerateBookThumbnail".to_string(),
                    priority: 6,
                    group: None,
                },
                QueuedThumbnailTask {
                    id: "GenerateBookThumbnail_book-2".to_string(),
                    simple_type: "GenerateBookThumbnail".to_string(),
                    priority: 6,
                    group: None,
                },
            ],
            "full thumbnail regeneration should target every non-deleted book and keep the finder task priority for Kotlin parity",
        );

        let _ = std::fs::remove_file(database_file);
        let _ = std::fs::remove_dir_all(library_root);
    }

    #[tokio::test]
    async fn thumbnail_finder_bigger_only_uses_runtime_thumbnail_policy() {
        let database_file = unique_temp_path("komga-thumbnail-finder-bigger-policy-main");
        let tasks_db_file = unique_temp_path("komga-thumbnail-finder-bigger-policy-tasks");
        let lucene_dir = unique_temp_path("komga-thumbnail-finder-bigger-policy-lucene");
        let library_root = unique_temp_path("komga-thumbnail-finder-bigger-policy-root");

        let pool = open_bootstrapped_main_pool(database_file.as_path()).await;
        insert_library(&pool, &library_root, true).await;
        insert_series(&pool, "library-1", "series-1").await;
        for book_id in ["book-small", "book-large"] {
            insert_book(
                &pool,
                book_id,
                book_id,
                &format!("books/{book_id}.cbz"),
                "library-1",
                "series-1",
                None,
            )
            .await;
        }
        for (thumbnail_id, book_id, width, height) in [
            ("thumb-small", "book-small", 350_i64, 350_i64),
            ("thumb-large", "book-large", 650_i64, 650_i64),
        ] {
            sqlx::query(
                "INSERT INTO THUMBNAIL_BOOK (ID, BOOK_ID, TYPE, SELECTED, WIDTH, HEIGHT) VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(thumbnail_id)
            .bind(book_id)
            .bind(ThumbnailType::Generated.persisted_name())
            .bind(true)
            .bind(width)
            .bind(height)
            .execute(&pool)
            .await
            .expect("generated thumbnail row should be inserted");
        }
        pool.close().await;

        let task_write_pool = connect_task_write_pool(&database_file)
            .await
            .expect("test private write pool should open");
        let task_read_pool = connect_task_pool(&database_file, default_read_max_connections())
            .await
            .expect("test private read pool should open");
        let runtime = TaskRuntimeContext::new(
            DatabaseHandle::file_backed(database_file.clone())
                .await
                .expect("test db should open"),
            tasks_db_file,
            lucene_dir,
            false,
            1,
            task_write_pool,
            task_read_pool,
        )
        .with_thumbnail_regeneration_policy(ThumbnailRegenerationPolicy {
            generated_thumbnail_max_edge: 600,
        });
        let scheduler =
            TaskQueueScheduler::for_runtime(runtime.clone(), "thumbnail-finder-bigger-policy-test")
                .await;
        let finder_task = TaskRequest::with_payload(
            TaskKind::FindBookThumbnailsToRegenerate,
            FindBookThumbnailsToRegeneratePayload::new(true),
        )
        .priority(6)
        .into_queue_record();

        let result = execute_and_enqueue(&scheduler, &runtime, &finder_task).await;
        assert!(matches!(result, Some(Ok(()))));

        let generated = scheduler
            .admin_for_test()
            .await
            .admin
            .take_available("thumbnail-finder-bigger-policy-assert")
            .expect("runtime policy should enqueue undersized generated thumbnail");
        assert_eq!(generated.id, "GenerateBookThumbnail_book-small");
        assert!(
            scheduler
                .admin_for_test()
                .await
                .admin
                .take_available("thumbnail-finder-bigger-policy-assert")
                .is_none(),
            "runtime policy should not enqueue thumbnails at or above the configured edge",
        );

        let _ = std::fs::remove_file(database_file);
        let _ = std::fs::remove_dir_all(library_root);
    }

    #[tokio::test]
    async fn analyze_book_enqueues_follow_ups_without_touching_book_last_modified() {
        let fixture = seed_analyze_book_dimension_fixture("analyze-book-follow-up", true).await;
        let runtime = fixture.runtime_context(false, false).await;
        let scheduler =
            TaskQueueScheduler::for_runtime(runtime.clone(), "analyze-book-follow-up-test").await;
        let task = TaskRequest::with_payload(TaskKind::AnalyzeBook, BookPayload::new("book-1"))
            .priority(90)
            .group("series-1")
            .into_queue_record();

        let expected_last_modified = "2000-01-01 00:00:00";
        let pool_before = connect_test_pool(fixture.database_file.as_path(), 1)
            .await
            .expect("analyze-book follow-up precondition db should open");
        sqlx::query("UPDATE BOOK SET LAST_MODIFIED_DATE = ? WHERE ID = ?")
            .bind(expected_last_modified)
            .bind("book-1")
            .execute(&pool_before)
            .await
            .expect("analyze-book follow-up book timestamp should be pinned");
        pool_before.close().await;

        let result = execute_and_enqueue(&scheduler, &runtime, &task).await;
        assert!(matches!(result, Some(Ok(()))));

        let verify_pool = connect_test_pool(fixture.database_file.as_path(), 1)
            .await
            .expect("analyze-book follow-up verify db should open");
        let media_row =
            sqlx::query("SELECT STATUS, PAGE_COUNT FROM MEDIA WHERE BOOK_ID = ? LIMIT 1")
                .bind("book-1")
                .fetch_one(&verify_pool)
                .await
                .expect("analyze-book follow-up media row should be queryable");
        let book_row = sqlx::query("SELECT LAST_MODIFIED_DATE FROM BOOK WHERE ID = ? LIMIT 1")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect("analyze-book follow-up book row should be queryable");
        verify_pool.close().await;
        assert_eq!(media_row.get::<String, _>("STATUS"), "READY");
        assert!(media_row.get::<i64, _>("PAGE_COUNT") > 0);
        assert_eq!(
            load_persisted_page_dimensions(fixture.database_file.as_path(), "book-1").await,
            vec![
                PersistedPageDimensions {
                    number: 0,
                    width: Some(48),
                    height: Some(96),
                },
                PersistedPageDimensions {
                    number: 1,
                    width: Some(120),
                    height: Some(80),
                },
            ],
            "analyze-book should persist page dimensions when library ANALYZE_DIMENSIONS is enabled",
        );
        assert_eq!(
            book_row.get::<String, _>("LAST_MODIFIED_DATE"),
            expected_last_modified,
            "ready analyze-book should not refresh BOOK last-modified",
        );

        let mut queued = Vec::new();
        while let Some(task) = scheduler
            .admin_for_test()
            .await
            .admin
            .take_available("analyze-book-follow-up-assert")
        {
            queued.push(QueuedFollowUpTask {
                id: task.id,
                simple_type: task.simple_type,
                priority: task.priority,
                group: task.group,
            });
        }
        queued.sort_by(|left, right| left.id.cmp(&right.id));

        assert_eq!(
            queued,
            vec![
                QueuedFollowUpTask {
                    id: "GenerateBookThumbnail_book-1".to_string(),
                    simple_type: "GenerateBookThumbnail".to_string(),
                    priority: 91,
                    group: None,
                },
                QueuedFollowUpTask {
                    id: "RefreshBookMetadata_book-1".to_string(),
                    simple_type: "RefreshBookMetadata".to_string(),
                    priority: 91,
                    group: Some("series-1".to_string()),
                },
            ],
            "ready analyze-book must enqueue Kotlin-style thumbnail and metadata follow-up tasks",
        );

        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn analyze_book_persists_and_clears_epub_capabilities() {
        let fixture = RuntimeTestFixture::new("analyze-book-epub-capabilities");
        std::fs::create_dir_all(fixture.library_root.join("books"))
            .expect("EPUB analysis library root should be created");
        let book_path = fixture.library_root.join("books/book-1.epub");
        std::fs::copy(archive_fixture_path("epub3.epub"), &book_path)
            .expect("fixed-layout EPUB fixture should be copied");

        let pool = open_bootstrapped_main_pool(fixture.database_file.as_path()).await;
        insert_library(&pool, &fixture.library_root, false).await;
        insert_series(&pool, "library-1", "series-1").await;
        insert_book(
            &pool,
            "book-1",
            "book-1",
            "books/book-1.epub",
            "library-1",
            "series-1",
            None,
        )
        .await;
        pool.close().await;

        let runtime = fixture.runtime_context(false, false).await;
        super::execute_analyze_book(&runtime.job(), "book-1", 90)
            .await
            .expect("fixed-layout EPUB analysis should succeed");

        let verify_pool = connect_test_pool(fixture.database_file.as_path(), 1)
            .await
            .expect("EPUB capabilities verify db should open");
        assert_eq!(
            load_persisted_epub_capabilities(&verify_pool, "book-1").await,
            (true, false),
        );

        write_kepub_fixture(&book_path);
        super::execute_analyze_book(&runtime.job(), "book-1", 90)
            .await
            .expect("KEPUB analysis should succeed");
        assert_eq!(
            load_persisted_epub_capabilities(&verify_pool, "book-1").await,
            (false, true),
        );

        let reflowable_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../komga/src/test/resources/epub/The Incomplete Theft - Ralph Burke.epub");
        std::fs::copy(reflowable_path, &book_path)
            .expect("reflowable EPUB fixture should replace KEPUB fixture");
        super::execute_analyze_book(&runtime.job(), "book-1", 90)
            .await
            .expect("reflowable EPUB reanalysis should succeed");
        assert_eq!(
            load_persisted_epub_capabilities(&verify_pool, "book-1").await,
            (false, false),
        );
        verify_pool.close().await;

        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn analyze_book_persists_and_clears_media_comment() {
        let fixture = RuntimeTestFixture::new("analyze-book-media-comment");
        std::fs::create_dir_all(fixture.library_root.join("books"))
            .expect("media comment library root should be created");
        let book_path = fixture.library_root.join("books/book-1.epub");
        write_missing_resource_epub_fixture(&book_path);

        let pool = open_bootstrapped_main_pool(fixture.database_file.as_path()).await;
        insert_library(&pool, &fixture.library_root, false).await;
        insert_series(&pool, "library-1", "series-1").await;
        insert_book(
            &pool,
            "book-1",
            "book-1",
            "books/book-1.epub",
            "library-1",
            "series-1",
            None,
        )
        .await;
        pool.close().await;

        let runtime = fixture.runtime_context(false, false).await;
        super::execute_analyze_book(&runtime.job(), "book-1", 90)
            .await
            .expect("partial EPUB analysis should succeed");
        assert_eq!(
            load_persisted_media_comment(fixture.database_file.as_path(), "book-1").await,
            Some("ERR_1033 [missing.css]".to_string()),
        );

        let reflowable_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../komga/src/test/resources/epub/The Incomplete Theft - Ralph Burke.epub");
        std::fs::copy(reflowable_path, &book_path)
            .expect("clean reflowable EPUB fixture should replace the partial fixture");
        super::execute_analyze_book(&runtime.job(), "book-1", 90)
            .await
            .expect("reanalysis should succeed");
        assert_eq!(
            load_persisted_media_comment(fixture.database_file.as_path(), "book-1").await,
            None,
        );

        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn analyze_book_replaces_scanner_file_with_archive_metadata() {
        let fixture = RuntimeTestFixture::new("analyze-book-archive-media-files");
        std::fs::create_dir_all(fixture.library_root.join("books"))
            .expect("archive media files library root should be created");
        let book_path = fixture.library_root.join("books/book-1.cbz");
        write_cbz_with_non_page_fixture(&book_path);

        let pool = open_bootstrapped_main_pool(fixture.database_file.as_path()).await;
        insert_library(&pool, &fixture.library_root, false).await;
        insert_series(&pool, "library-1", "series-1").await;
        insert_book(
            &pool,
            "book-1",
            "book-1",
            "books/book-1.cbz",
            "library-1",
            "series-1",
            None,
        )
        .await;
        sqlx::query("INSERT INTO MEDIA_FILE (FILE_NAME, BOOK_ID, FILE_SIZE) VALUES (?, ?, ?)")
            .bind("book-1.cbz")
            .bind("book-1")
            .bind(123_i64)
            .execute(&pool)
            .await
            .expect("scanner media file row should be inserted");
        pool.close().await;

        let runtime = fixture.runtime_context(false, false).await;
        super::execute_analyze_book(&runtime.job(), "book-1", 90)
            .await
            .expect("archive analysis should succeed");

        let files = load_persisted_media_files(fixture.database_file.as_path(), "book-1").await;
        assert!(
            !files
                .iter()
                .any(|(file_name, _, _, _)| file_name == "book-1.cbz"),
            "scanner-owned physical book file must not remain in MEDIA_FILE",
        );
        assert!(
            files
                .iter()
                .any(|(file_name, media_type, sub_type, file_size)| {
                    file_name == "ComicInfo.xml"
                        && media_type.is_some()
                        && sub_type.is_none()
                        && file_size.is_some()
                })
        );
        assert!(
            files
                .iter()
                .any(|(file_name, media_type, sub_type, file_size)| {
                    file_name == "notes.txt"
                        && media_type.is_some()
                        && sub_type.is_none()
                        && file_size.is_some()
                })
        );

        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn analyze_book_records_decode_error_and_continues_with_next_book() {
        let fixture = RuntimeTestFixture::new("analyze-book-decode-error-continues");
        std::fs::create_dir_all(fixture.library_root.join("books"))
            .expect("analyze-book decode-error library root should be created");
        write_invalid_cbz_fixture(&fixture.library_root.join("books/book-bad.cbz"));
        write_cbz_fixture(
            &fixture.library_root.join("books/book-good.cbz"),
            &[FixturePageSize {
                width: 48,
                height: 96,
            }],
        );

        let pool = open_bootstrapped_main_pool(fixture.database_file.as_path()).await;
        insert_library(&pool, &fixture.library_root, true).await;
        insert_series(&pool, "library-1", "series-1").await;
        for (book_id, url) in [
            ("book-bad", "books/book-bad.cbz"),
            ("book-good", "books/book-good.cbz"),
        ] {
            insert_book(&pool, book_id, book_id, url, "library-1", "series-1", None).await;
        }
        pool.close().await;

        let runtime = fixture.runtime_context(true, false).await;
        let scheduler =
            TaskQueueScheduler::for_runtime(runtime.clone(), "analyze-book-decode-error-test")
                .await;
        for book_id in ["book-bad", "book-good"] {
            scheduler
                .enqueue(
                    TaskRequest::with_payload(TaskKind::AnalyzeBook, BookPayload::new(book_id))
                        .group("series-1")
                        .into_queue_record(),
                )
                .await
                .expect("analyze-book task should enqueue");
        }

        let bad_task = scheduler
            .take_next()
            .await
            .expect("bad analyze-book task should be claimable")
            .expect("bad analyze-book task should exist");
        assert_eq!(bad_task.id, "AnalyzeBook_book-bad");
        assert!(matches!(
            execute_and_enqueue(&scheduler, &runtime, &bad_task).await,
            Some(Ok(()))
        ));

        let good_task = scheduler
            .take_next()
            .await
            .expect("good analyze-book task should still be claimable")
            .expect("good analyze-book task should exist after bad book analysis");
        assert_eq!(good_task.id, "AnalyzeBook_book-good");
        assert!(matches!(
            execute_and_enqueue(&scheduler, &runtime, &good_task).await,
            Some(Ok(()))
        ));

        assert_eq!(
            load_persisted_media_summary(fixture.database_file.as_path(), "book-bad").await,
            PersistedMediaSummary {
                status: "ERROR".to_string(),
                page_count: 0,
            },
            "decode errors should be recorded on the current book instead of failing the queue",
        );
        let good_media =
            load_persisted_media_summary(fixture.database_file.as_path(), "book-good").await;
        assert_eq!(good_media.status, "READY");
        assert!(
            good_media.page_count > 0,
            "the next book should be analyzed after a previous decode error",
        );

        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn analyze_book_keeps_page_dimensions_null_when_library_analysis_is_disabled() {
        let fixture =
            seed_analyze_book_dimension_fixture("analyze-book-dimensions-disabled", false).await;
        let runtime = fixture.runtime_context(false, false).await;
        let scheduler = TaskQueueScheduler::for_runtime(
            runtime.clone(),
            "analyze-book-disabled-dimensions-test",
        )
        .await;
        let task = TaskRequest::with_payload(TaskKind::AnalyzeBook, BookPayload::new("book-1"))
            .priority(90)
            .group("series-1")
            .into_queue_record();

        let result = execute_and_enqueue(&scheduler, &runtime, &task).await;
        assert!(matches!(result, Some(Ok(()))));

        assert_eq!(
            load_persisted_page_dimensions(fixture.database_file.as_path(), "book-1").await,
            vec![
                PersistedPageDimensions {
                    number: 0,
                    width: None,
                    height: None,
                },
                PersistedPageDimensions {
                    number: 1,
                    width: None,
                    height: None,
                },
            ],
            "analyze-book should leave page dimensions null when library ANALYZE_DIMENSIONS is disabled",
        );

        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn analyze_book_adjusts_existing_read_progress_when_outdated_page_count_changes() {
        let database_file = unique_temp_path("komga-analyze-book-read-progress-adjust-main");
        let tasks_db_file = unique_temp_path("komga-analyze-book-read-progress-adjust-tasks");
        let lucene_dir = unique_temp_path("komga-analyze-book-read-progress-adjust-lucene");
        let library_root = unique_temp_path("komga-analyze-book-read-progress-adjust-root");
        std::fs::create_dir_all(library_root.join("books"))
            .expect("analyze-book read-progress adjust root should be created");
        std::fs::copy(
            archive_fixture_path("rar4.rar"),
            library_root.join("books/book-1.cbr"),
        )
        .expect("analyze-book read-progress adjust source fixture should be copied");

        let pool = open_bootstrapped_main_pool(database_file.as_path()).await;
        insert_library(&pool, &library_root, true).await;
        insert_series(&pool, "library-1", "series-1").await;
        insert_book(
            &pool,
            "book-1",
            "book-1",
            "books/book-1.cbr",
            "library-1",
            "series-1",
            None,
        )
        .await;
        insert_user(&pool, "user-completed").await;
        insert_user(&pool, "user-incomplete").await;
        sqlx::query(
            "INSERT INTO MEDIA (BOOK_ID, STATUS, MEDIA_TYPE, PAGE_COUNT) VALUES (?, ?, ?, ?)",
        )
        .bind("book-1")
        .bind("OUTDATED")
        .bind("application/x-rar-compressed; version=4")
        .bind(10_i64)
        .execute(&pool)
        .await
        .expect("analyze-book read-progress adjust media row should be inserted");
        sqlx::query(
            "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED, READ_DATE, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("book-1")
        .bind("user-completed")
        .bind(10_i64)
        .bind(true)
        .bind("2001-01-01 00:00:00")
        .bind("2001-01-01 00:00:00")
        .execute(&pool)
        .await
        .expect("completed read progress row should be inserted");
        sqlx::query(
            "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED, READ_DATE, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("book-1")
        .bind("user-incomplete")
        .bind(4_i64)
        .bind(false)
        .bind("2001-01-02 00:00:00")
        .bind("2001-01-02 00:00:00")
        .execute(&pool)
        .await
        .expect("incomplete read progress row should be inserted");
        for (user_id, read_count, in_progress_count, most_recent_read_date) in [
            ("user-completed", 1_i64, 0_i64, Some("2001-01-01 00:00:00")),
            ("user-incomplete", 0_i64, 1_i64, Some("2001-01-02 00:00:00")),
        ] {
            sqlx::query(
                r#"
                INSERT INTO READ_PROGRESS_SERIES (
                    SERIES_ID,
                    USER_ID,
                    READ_COUNT,
                    IN_PROGRESS_COUNT,
                    MOST_RECENT_READ_DATE,
                    LAST_MODIFIED_DATE
                )
                VALUES (?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind("series-1")
            .bind(user_id)
            .bind(read_count)
            .bind(in_progress_count)
            .bind(most_recent_read_date)
            .bind("2000-01-01 00:00:00")
            .execute(&pool)
            .await
            .expect("series read progress row should be inserted");
        }
        pool.close().await;

        let task_write_pool = connect_task_write_pool(&database_file)
            .await
            .expect("test private write pool should open");
        let task_read_pool = connect_task_pool(&database_file, default_read_max_connections())
            .await
            .expect("test private read pool should open");
        let runtime = TaskRuntimeContext::new(
            DatabaseHandle::file_backed(database_file.clone())
                .await
                .expect("test db should open"),
            tasks_db_file,
            lucene_dir,
            false,
            1,
            task_write_pool,
            task_read_pool,
        )
        .with_ownership_overrides(TaskRuntimeOwnershipOverrides {
            owns_search_index: Some(false),
            ..TaskRuntimeOwnershipOverrides::default()
        });
        let scheduler = TaskQueueScheduler::for_runtime(
            runtime.clone(),
            "analyze-book-read-progress-adjust-test",
        )
        .await;
        let task = TaskRequest::with_payload(TaskKind::AnalyzeBook, BookPayload::new("book-1"))
            .priority(90)
            .group("series-1")
            .into_queue_record();

        let result = execute_and_enqueue(&scheduler, &runtime, &task).await;
        assert!(matches!(result, Some(Ok(()))));

        let verify_pool = connect_test_pool(database_file.as_path(), 1)
            .await
            .expect("analyze-book read-progress adjust verify db should open");
        let page_count = sqlx::query("SELECT PAGE_COUNT FROM MEDIA WHERE BOOK_ID = ? LIMIT 1")
            .bind("book-1")
            .fetch_one(&verify_pool)
            .await
            .expect("adjusted media row should be queryable")
            .get::<i64, _>("PAGE_COUNT");
        let progress_rows = sqlx::query(
            "SELECT USER_ID, PAGE, COMPLETED FROM READ_PROGRESS WHERE BOOK_ID = ? ORDER BY USER_ID ASC",
        )
        .bind("book-1")
        .fetch_all(&verify_pool)
        .await
        .expect("adjusted read progress rows should be queryable");
        let series_rows = sqlx::query(
            r#"
            SELECT USER_ID, READ_COUNT, IN_PROGRESS_COUNT, LAST_MODIFIED_DATE
            FROM READ_PROGRESS_SERIES
            WHERE SERIES_ID = ?
            ORDER BY USER_ID ASC
            "#,
        )
        .bind("series-1")
        .fetch_all(&verify_pool)
        .await
        .expect("adjusted series read progress rows should be queryable");
        verify_pool.close().await;

        let persisted_progress = progress_rows
            .into_iter()
            .map(|row| PersistedReadProgress {
                user_id: row.get("USER_ID"),
                page: row.get("PAGE"),
                completed: row.get("COMPLETED"),
                last_modified_date: None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            persisted_progress,
            vec![
                PersistedReadProgress {
                    user_id: "user-completed".to_string(),
                    page: page_count,
                    completed: 1_i64,
                    last_modified_date: None,
                },
                PersistedReadProgress {
                    user_id: "user-incomplete".to_string(),
                    page: 1_i64,
                    completed: 0_i64,
                    last_modified_date: None,
                },
            ],
            "outdated analyze-book should realign completed progress to the new page count and reset incomplete progress to page 1",
        );

        let persisted_series = series_rows
            .into_iter()
            .map(|row| PersistedSeriesProgress {
                user_id: row.get("USER_ID"),
                read_count: row.get("READ_COUNT"),
                in_progress_count: row.get("IN_PROGRESS_COUNT"),
                last_modified_date: row.get("LAST_MODIFIED_DATE"),
            })
            .collect::<Vec<_>>();
        assert_eq!(persisted_series[0].user_id, "user-completed".to_string());
        assert_eq!(persisted_series[0].read_count, 1_i64);
        assert_eq!(persisted_series[0].in_progress_count, 0_i64);
        assert_ne!(
            persisted_series[0].last_modified_date,
            "2000-01-01 00:00:00"
        );
        assert_eq!(persisted_series[1].user_id, "user-incomplete".to_string());
        assert_eq!(persisted_series[1].read_count, 0_i64);
        assert_eq!(persisted_series[1].in_progress_count, 1_i64);
        assert_ne!(
            persisted_series[1].last_modified_date,
            "2000-01-01 00:00:00"
        );

        let _ = std::fs::remove_file(database_file);
        let _ = std::fs::remove_dir_all(library_root);
    }

    #[tokio::test]
    async fn analyze_book_keeps_existing_read_progress_when_outdated_page_count_is_unchanged() {
        let database_file = unique_temp_path("komga-analyze-book-read-progress-keep-main");
        let tasks_db_file = unique_temp_path("komga-analyze-book-read-progress-keep-tasks");
        let lucene_dir = unique_temp_path("komga-analyze-book-read-progress-keep-lucene");
        let library_root = unique_temp_path("komga-analyze-book-read-progress-keep-root");
        std::fs::create_dir_all(library_root.join("books"))
            .expect("analyze-book read-progress keep root should be created");
        std::fs::copy(
            archive_fixture_path("rar4.rar"),
            library_root.join("books/book-1.cbr"),
        )
        .expect("analyze-book read-progress keep source fixture should be copied");
        let actual_page_count = analyzed_fixture_page_count("rar4.rar", "books/book-1.cbr");

        let pool = open_bootstrapped_main_pool(database_file.as_path()).await;
        insert_library(&pool, &library_root, true).await;
        insert_series(&pool, "library-1", "series-1").await;
        insert_book(
            &pool,
            "book-1",
            "book-1",
            "books/book-1.cbr",
            "library-1",
            "series-1",
            None,
        )
        .await;
        insert_user(&pool, "user-completed").await;
        insert_user(&pool, "user-incomplete").await;
        sqlx::query(
            "INSERT INTO MEDIA (BOOK_ID, STATUS, MEDIA_TYPE, PAGE_COUNT) VALUES (?, ?, ?, ?)",
        )
        .bind("book-1")
        .bind("OUTDATED")
        .bind("application/x-rar-compressed; version=4")
        .bind(actual_page_count)
        .execute(&pool)
        .await
        .expect("analyze-book read-progress keep media row should be inserted");
        sqlx::query(
            "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED, READ_DATE, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("book-1")
        .bind("user-completed")
        .bind(actual_page_count)
        .bind(true)
        .bind("2001-01-01 00:00:00")
        .bind("2001-01-01 00:00:00")
        .execute(&pool)
        .await
        .expect("same-count completed read progress row should be inserted");
        sqlx::query(
            "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED, READ_DATE, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("book-1")
        .bind("user-incomplete")
        .bind(0_i64)
        .bind(false)
        .bind("2001-01-02 00:00:00")
        .bind("2001-01-02 00:00:00")
        .execute(&pool)
        .await
        .expect("same-count incomplete read progress row should be inserted");
        sqlx::query(
            r#"
            INSERT INTO READ_PROGRESS_SERIES (
                SERIES_ID,
                USER_ID,
                READ_COUNT,
                IN_PROGRESS_COUNT,
                MOST_RECENT_READ_DATE,
                LAST_MODIFIED_DATE
            )
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("series-1")
        .bind("user-completed")
        .bind(1_i64)
        .bind(0_i64)
        .bind("2001-01-01 00:00:00")
        .bind("2000-01-01 00:00:00")
        .execute(&pool)
        .await
        .expect("same-count completed series row should be inserted");
        sqlx::query(
            r#"
            INSERT INTO READ_PROGRESS_SERIES (
                SERIES_ID,
                USER_ID,
                READ_COUNT,
                IN_PROGRESS_COUNT,
                MOST_RECENT_READ_DATE,
                LAST_MODIFIED_DATE
            )
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("series-1")
        .bind("user-incomplete")
        .bind(0_i64)
        .bind(1_i64)
        .bind("2001-01-02 00:00:00")
        .bind("2000-01-01 00:00:00")
        .execute(&pool)
        .await
        .expect("same-count incomplete series row should be inserted");
        pool.close().await;

        let task_write_pool = connect_task_write_pool(&database_file)
            .await
            .expect("test private write pool should open");
        let task_read_pool = connect_task_pool(&database_file, default_read_max_connections())
            .await
            .expect("test private read pool should open");
        let runtime = TaskRuntimeContext::new(
            DatabaseHandle::file_backed(database_file.clone())
                .await
                .expect("test db should open"),
            tasks_db_file,
            lucene_dir,
            false,
            1,
            task_write_pool,
            task_read_pool,
        )
        .with_ownership_overrides(TaskRuntimeOwnershipOverrides {
            owns_search_index: Some(false),
            ..TaskRuntimeOwnershipOverrides::default()
        });
        let scheduler = TaskQueueScheduler::for_runtime(
            runtime.clone(),
            "analyze-book-read-progress-keep-test",
        )
        .await;
        let task = TaskRequest::with_payload(TaskKind::AnalyzeBook, BookPayload::new("book-1"))
            .priority(90)
            .group("series-1")
            .into_queue_record();

        let result = execute_and_enqueue(&scheduler, &runtime, &task).await;
        assert!(matches!(result, Some(Ok(()))));

        let verify_pool = connect_test_pool(database_file.as_path(), 1)
            .await
            .expect("analyze-book read-progress keep verify db should open");
        let progress_rows = sqlx::query(
            "SELECT USER_ID, PAGE, COMPLETED, LAST_MODIFIED_DATE FROM READ_PROGRESS WHERE BOOK_ID = ? ORDER BY USER_ID ASC",
        )
        .bind("book-1")
        .fetch_all(&verify_pool)
        .await
        .expect("same-count read progress rows should be queryable");
        let series_rows = sqlx::query(
            r#"
            SELECT USER_ID, READ_COUNT, IN_PROGRESS_COUNT, LAST_MODIFIED_DATE
            FROM READ_PROGRESS_SERIES
            WHERE SERIES_ID = ?
            ORDER BY USER_ID ASC
            "#,
        )
        .bind("series-1")
        .fetch_all(&verify_pool)
        .await
        .expect("same-count series read progress rows should be queryable");
        verify_pool.close().await;

        let persisted_progress = progress_rows
            .into_iter()
            .map(|row| PersistedReadProgress {
                user_id: row.get("USER_ID"),
                page: row.get("PAGE"),
                completed: row.get("COMPLETED"),
                last_modified_date: Some(row.get("LAST_MODIFIED_DATE")),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            persisted_progress,
            vec![
                PersistedReadProgress {
                    user_id: "user-completed".to_string(),
                    page: actual_page_count,
                    completed: 1_i64,
                    last_modified_date: Some("2001-01-01 00:00:00".to_string()),
                },
                PersistedReadProgress {
                    user_id: "user-incomplete".to_string(),
                    page: 0_i64,
                    completed: 0_i64,
                    last_modified_date: Some("2001-01-02 00:00:00".to_string()),
                },
            ],
            "outdated analyze-book must keep read progress untouched when the page count is unchanged",
        );

        let persisted_series = series_rows
            .into_iter()
            .map(|row| PersistedSeriesProgress {
                user_id: row.get("USER_ID"),
                read_count: row.get("READ_COUNT"),
                in_progress_count: row.get("IN_PROGRESS_COUNT"),
                last_modified_date: row.get("LAST_MODIFIED_DATE"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            persisted_series,
            vec![
                PersistedSeriesProgress {
                    user_id: "user-completed".to_string(),
                    read_count: 1_i64,
                    in_progress_count: 0_i64,
                    last_modified_date: "2000-01-01 00:00:00".to_string(),
                },
                PersistedSeriesProgress {
                    user_id: "user-incomplete".to_string(),
                    read_count: 0_i64,
                    in_progress_count: 1_i64,
                    last_modified_date: "2000-01-01 00:00:00".to_string(),
                },
            ],
            "unchanged page counts must not refresh series read-progress aggregates",
        );

        let _ = std::fs::remove_file(database_file);
        let _ = std::fs::remove_dir_all(library_root);
    }
}

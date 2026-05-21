use super::{TaskExecutionError, TaskExecutionOutcome, TaskQueueRecord};
use crate::operational_settings_access::load_server_settings;
use crate::search::index_lifecycle::SearchEntityType;
use crate::sqlite::write_models::server_settings::ServerSettingsStore;
use crate::task_queue::JobRuntime;
use komga_application::task_processing::{RefreshBookMetadataPayload, TaskKind, TaskRequest};
use serde_json::Value;

fn thumbnail_max_edge(thumbnail_size: &str) -> i64 {
    match thumbnail_size {
        "MEDIUM" => 600,
        "LARGE" => 900,
        "XLARGE" => 1200,
        _ => 300,
    }
}

pub(in crate::task_queue) async fn execute_analyze_book(
    runtime: &JobRuntime<'_>,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Result<TaskExecutionOutcome, TaskExecutionError> {
    let Some(book_id) = task_target else {
        return Err(TaskExecutionError::invalid_task(
            "AnalyzeBook task must include a book id",
        ));
    };

    let book_id = book_id.to_string();
    let outcome = super::index_tasks::analyze_book(runtime, &book_id).await?;

    if outcome.media_status.eq_ignore_ascii_case("READY") && !outcome.series_id.is_empty() {
        let follow_up_priority = task.priority.saturating_add(1);
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
    task: &TaskQueueRecord,
) -> Result<TaskExecutionOutcome, TaskExecutionError> {
    let entity_types = parse_rebuild_index_entities(task.payload.as_deref())?;
    super::index_tasks::rebuild_index(runtime, entity_types.as_deref()).await?;

    Ok(TaskExecutionOutcome::completed())
}

pub(in crate::task_queue) async fn execute_find_book_thumbnails_to_regenerate(
    runtime: &JobRuntime<'_>,
    task: &TaskQueueRecord,
) -> Result<TaskExecutionOutcome, TaskExecutionError> {
    let for_bigger_result_only = parse_for_bigger_result_only(task.payload.as_deref());
    let book_ids = if for_bigger_result_only {
        let settings_store =
            ServerSettingsStore::new(runtime.database().main_db().database_file().to_path_buf());
        let settings = load_server_settings(&settings_store)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "load server settings for thumbnail finder failed: {error}"
                ))
            })?;
        let max_edge = thumbnail_max_edge(settings.thumbnail_size);
        super::find_books_with_undersized_generated_thumbnails(runtime, max_edge).await?
    } else {
        super::find_books_for_thumbnail_regeneration(runtime).await?
    };
    let follow_up_tasks = book_ids
        .into_iter()
        .map(|book_id| {
            TaskRequest::new(TaskKind::GenerateBookThumbnail)
                .priority(task.priority)
                .into_queue_record_with_id(&book_id)
        })
        .collect();
    Ok(TaskExecutionOutcome::with_follow_up_tasks(follow_up_tasks))
}

fn parse_rebuild_index_entities(
    payload: Option<&str>,
) -> Result<Option<Vec<SearchEntityType>>, TaskExecutionError> {
    let Some(payload) = payload else {
        return Ok(None);
    };
    let payload = serde_json::from_str::<Value>(payload).map_err(|error| {
        TaskExecutionError::runtime(format!("RebuildIndex payload must be valid JSON: {error}"))
    })?;
    let Some(entities) = payload.get("entities") else {
        return Ok(None);
    };
    if entities.is_null() {
        return Ok(None);
    }
    let entity_values = entities.as_array().ok_or_else(|| {
        TaskExecutionError::invalid_task("RebuildIndex payload field 'entities' must be an array")
    })?;

    let mut parsed = Vec::new();
    for entity in entity_values {
        let entity_type = parse_rebuild_index_entity(entity).ok_or_else(|| {
            TaskExecutionError::runtime(format!(
                "RebuildIndex payload contains unsupported entity selector: {entity}"
            ))
        })?;
        if !parsed.contains(&entity_type) {
            parsed.push(entity_type);
        }
    }

    Ok(Some(parsed))
}

fn parse_rebuild_index_entity(value: &Value) -> Option<SearchEntityType> {
    let raw = match value {
        Value::String(value) => Some(value.as_str()),
        Value::Object(value) => value.get("type").and_then(Value::as_str),
        _ => None,
    }?;

    match raw.trim().to_ascii_lowercase().as_str() {
        "book" => Some(SearchEntityType::Book),
        "series" => Some(SearchEntityType::Series),
        "collection" => Some(SearchEntityType::Collection),
        "readlist" => Some(SearchEntityType::ReadList),
        _ => None,
    }
}

fn parse_for_bigger_result_only(payload: Option<&str>) -> bool {
    payload
        .and_then(|payload| serde_json::from_str::<Value>(payload).ok())
        .and_then(|payload| {
            payload
                .get("for_bigger_result_only")
                .or_else(|| payload.get("forBiggerResultOnly"))
                .and_then(Value::as_bool)
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database_handle::DatabaseHandle;
    use crate::sqlite::{
        connect_main_write_context, connect_task_pool, connect_task_write_pool, connect_test_pool,
        default_read_max_connections,
    };
    use crate::task_queue::queue_scheduler::TaskQueueScheduler;
    use crate::task_queue::test_support::RuntimeTestFixture;
    use crate::task_queue::{TaskRuntimeContext, TaskRuntimeOwnershipOverrides};
    use image::{ImageBuffer, Rgba};
    use sqlx::{Row, SqlitePool};
    use std::fs::File;
    use std::io::Write;
    use zip::CompressionMethod;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

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

    fn write_cbz_fixture(path: &std::path::Path, page_sizes: &[(u32, u32)]) {
        let file = File::create(path).expect("cbz fixture should be created");
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o644);
        for (index, (width, height)) in page_sizes.iter().copied().enumerate() {
            zip.start_file(format!("{:08}.png", index + 1), options)
                .expect("cbz page entry should be created");
            zip.write_all(&png_bytes(width, height))
                .expect("cbz page bytes should be written");
        }
        zip.finish().expect("cbz fixture should finish");
    }

    async fn open_bootstrapped_main_pool(database_file: &std::path::Path) -> SqlitePool {
        let context = connect_main_write_context(database_file)
            .await
            .expect("index-jobs fixture db should bootstrap main schema");
        context.pool().clone()
    }

    async fn execute_and_enqueue(
        scheduler: &TaskQueueScheduler,
        runtime: &TaskRuntimeContext,
        task: &TaskQueueRecord,
        _task_target: Option<&str>,
    ) -> Option<Result<(), TaskExecutionError>> {
        match crate::task_queue::task_executor::execute_task(&runtime.job(), task).await {
            Ok(outcome) => {
                outcome.enqueue_into(scheduler).await;
                Some(Ok(()))
            }
            Err(error) => Some(Err(error)),
        }
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
        write_cbz_fixture(&archive_path, &[(48, 96), (120, 80)]);

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
    ) -> Vec<(i64, Option<i64>, Option<i64>)> {
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
            .map(|row| {
                (
                    row.get::<i64, _>("NUMBER"),
                    row.get::<Option<i64>, _>("width"),
                    row.get::<Option<i64>, _>("height"),
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
    async fn thumbnail_finder_enqueues_kotlin_style_generate_thumbnail_ids() {
        let database_file = unique_temp_path("komga-thumbnail-finder-main");
        let tasks_db_file = unique_temp_path("komga-thumbnail-finder-tasks");
        let lucene_dir = unique_temp_path("komga-thumbnail-finder-lucene");
        let library_root = unique_temp_path("komga-thumbnail-finder-root");

        let pool = open_bootstrapped_main_pool(database_file.as_path()).await;
        insert_library(&pool, &library_root, true).await;
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
            TaskQueueScheduler::for_runtime(runtime.clone(), "thumbnail-finder-test").await;
        let finder_task = TaskQueueRecord::new("FindBookThumbnailsToRegenerate", 6, None)
            .with_payload(serde_json::json!({ "for_bigger_result_only": false }).to_string());

        let result = execute_and_enqueue(&scheduler, &runtime, &finder_task, None).await;
        assert!(matches!(result, Some(Ok(()))));

        let generated = scheduler
            .admin_for_test()
            .await
            .admin
            .take_available("thumbnail-finder-assert")
            .expect("finder should enqueue one generate thumbnail task");

        assert_eq!(generated.id, "GenerateBookThumbnail_book-1");
        assert_eq!(generated.simple_type, "GenerateBookThumbnail");
        assert_eq!(generated.priority, 6);
        assert_eq!(generated.group, None);

        let _ = std::fs::remove_file(database_file);
        let _ = std::fs::remove_dir_all(library_root);
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
            .bind("USER_UPLOADED")
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
        let finder_task = TaskQueueRecord::new("FindBookThumbnailsToRegenerate", 6, None)
            .with_payload(serde_json::json!({ "for_bigger_result_only": false }).to_string());

        let result = execute_and_enqueue(&scheduler, &runtime, &finder_task, None).await;
        assert!(matches!(result, Some(Ok(()))));

        let mut generated = Vec::new();
        while let Some(task) = scheduler
            .admin_for_test()
            .await
            .admin
            .take_available("thumbnail-finder-all-books-assert")
        {
            generated.push((task.id, task.priority));
        }
        generated.sort_by(|left, right| left.0.cmp(&right.0));

        assert_eq!(
            generated,
            vec![
                ("GenerateBookThumbnail_book-1".to_string(), 6),
                ("GenerateBookThumbnail_book-2".to_string(), 6),
            ],
            "full thumbnail regeneration should target every non-deleted book and keep the finder task priority for Kotlin parity",
        );

        let _ = std::fs::remove_file(database_file);
        let _ = std::fs::remove_dir_all(library_root);
    }

    #[tokio::test]
    async fn analyze_book_enqueues_thumbnail_and_metadata_follow_ups_when_ready() {
        let fixture = seed_analyze_book_dimension_fixture("analyze-book-follow-up", true).await;
        let runtime = fixture.runtime_context(false, false).await;
        let scheduler =
            TaskQueueScheduler::for_runtime(runtime.clone(), "analyze-book-follow-up-test").await;
        let task = TaskQueueRecord::new("AnalyzeBook_book-1", 90, Some("series-1".to_string()))
            .with_simple_type("AnalyzeBook");

        let result = execute_and_enqueue(&scheduler, &runtime, &task, Some("book-1")).await;
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
            vec![(0, Some(48), Some(96)), (1, Some(120), Some(80))],
            "analyze-book should persist page dimensions when library ANALYZE_DIMENSIONS is enabled",
        );
        assert!(
            book_row.get::<String, _>("LAST_MODIFIED_DATE") != "2000-01-01 00:00:00",
            "ready analyze-book should refresh BOOK last-modified for downstream SSE visibility",
        );

        let mut queued = Vec::new();
        while let Some(task) = scheduler
            .admin_for_test()
            .await
            .admin
            .take_available("analyze-book-follow-up-assert")
        {
            queued.push((task.id, task.simple_type, task.priority, task.group));
        }
        queued.sort_by(|left, right| left.0.cmp(&right.0));

        assert_eq!(
            queued,
            vec![
                (
                    "GenerateBookThumbnail_book-1".to_string(),
                    "GenerateBookThumbnail".to_string(),
                    91,
                    None,
                ),
                (
                    "RefreshBookMetadata_book-1".to_string(),
                    "RefreshBookMetadata".to_string(),
                    91,
                    Some("series-1".to_string()),
                ),
            ],
            "ready analyze-book must enqueue Kotlin-style thumbnail and metadata follow-up tasks",
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
        let task = TaskQueueRecord::new("AnalyzeBook_book-1", 90, Some("series-1".to_string()))
            .with_simple_type("AnalyzeBook");

        let result = execute_and_enqueue(&scheduler, &runtime, &task, Some("book-1")).await;
        assert!(matches!(result, Some(Ok(()))));

        assert_eq!(
            load_persisted_page_dimensions(fixture.database_file.as_path(), "book-1").await,
            vec![(0, None, None), (1, None, None)],
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
        let task = TaskQueueRecord::new("AnalyzeBook_book-1", 90, Some("series-1".to_string()))
            .with_simple_type("AnalyzeBook");

        let result = execute_and_enqueue(&scheduler, &runtime, &task, Some("book-1")).await;
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
            .map(|row| {
                (
                    row.get::<String, _>("USER_ID"),
                    row.get::<i64, _>("PAGE"),
                    row.get::<i64, _>("COMPLETED"),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            persisted_progress,
            vec![
                ("user-completed".to_string(), page_count, 1_i64),
                ("user-incomplete".to_string(), 1_i64, 0_i64),
            ],
            "outdated analyze-book should realign completed progress to the new page count and reset incomplete progress to page 1",
        );

        let persisted_series = series_rows
            .into_iter()
            .map(|row| {
                (
                    row.get::<String, _>("USER_ID"),
                    row.get::<i64, _>("READ_COUNT"),
                    row.get::<i64, _>("IN_PROGRESS_COUNT"),
                    row.get::<String, _>("LAST_MODIFIED_DATE"),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(persisted_series[0].0, "user-completed".to_string());
        assert_eq!(persisted_series[0].1, 1_i64);
        assert_eq!(persisted_series[0].2, 0_i64);
        assert_ne!(persisted_series[0].3, "2000-01-01 00:00:00");
        assert_eq!(persisted_series[1].0, "user-incomplete".to_string());
        assert_eq!(persisted_series[1].1, 0_i64);
        assert_eq!(persisted_series[1].2, 1_i64);
        assert_ne!(persisted_series[1].3, "2000-01-01 00:00:00");

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
        let task = TaskQueueRecord::new("AnalyzeBook_book-1", 90, Some("series-1".to_string()))
            .with_simple_type("AnalyzeBook");

        let result = execute_and_enqueue(&scheduler, &runtime, &task, Some("book-1")).await;
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
            .map(|row| {
                (
                    row.get::<String, _>("USER_ID"),
                    row.get::<i64, _>("PAGE"),
                    row.get::<i64, _>("COMPLETED"),
                    row.get::<String, _>("LAST_MODIFIED_DATE"),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            persisted_progress,
            vec![
                (
                    "user-completed".to_string(),
                    actual_page_count,
                    1_i64,
                    "2001-01-01 00:00:00".to_string(),
                ),
                (
                    "user-incomplete".to_string(),
                    0_i64,
                    0_i64,
                    "2001-01-02 00:00:00".to_string(),
                ),
            ],
            "outdated analyze-book must keep read progress untouched when the page count is unchanged",
        );

        let persisted_series = series_rows
            .into_iter()
            .map(|row| {
                (
                    row.get::<String, _>("USER_ID"),
                    row.get::<i64, _>("READ_COUNT"),
                    row.get::<i64, _>("IN_PROGRESS_COUNT"),
                    row.get::<String, _>("LAST_MODIFIED_DATE"),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            persisted_series,
            vec![
                (
                    "user-completed".to_string(),
                    1_i64,
                    0_i64,
                    "2000-01-01 00:00:00".to_string(),
                ),
                (
                    "user-incomplete".to_string(),
                    0_i64,
                    1_i64,
                    "2000-01-01 00:00:00".to_string(),
                ),
            ],
            "unchanged page counts must not refresh series read-progress aggregates",
        );

        let _ = std::fs::remove_file(database_file);
        let _ = std::fs::remove_dir_all(library_root);
    }

    #[test]
    fn thumbnail_finder_payload_accepts_kotlin_camel_case_flag() {
        assert!(parse_for_bigger_result_only(Some(
            r#"{"forBiggerResultOnly":true}"#
        )));
    }

    #[test]
    fn rebuild_index_payload_accepts_kotlin_entity_names() {
        assert_eq!(
            parse_rebuild_index_entities(Some(r#"{"entities":["Collection","Series"]}"#))
                .expect("rebuild index payload should parse"),
            Some(vec![SearchEntityType::Collection, SearchEntityType::Series])
        );
    }
}

use anyhow::Context;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::tasks::JobRuntime;
use komga_application::runtime_sse::{RuntimeSseEventSink, RuntimeSseEventStore};
use komga_application::task_processing::{
    CleanupEmptySetsPolicy, LibraryScanInterval, LibraryScanPipeline, LibraryScanProfile,
    LibraryScanScheduleState, ScanOneLibrary, ScanOneLibraryResult, ScanSchedulingTrigger,
    ScheduledLibraryScanBatch, ScheduledLibraryScanTask, TaskKind, TaskProcessingError,
    TaskRequest, TaskSchedule,
};
use sqlx::{Row, SqlitePool};
use tokio::time::Instant;

use super::LibraryScanner;
use super::follow_up::ScanFollowUpPlanner;
use crate::discovery::deletion::{cleanup_empty_sets_rows, empty_trash_rows};

async fn load_library_scan_profiles(pool: &SqlitePool) -> anyhow::Result<Vec<LibraryScanProfile>> {
    let rows = sqlx::query(
        r#"SELECT
            ID,
            SCAN_STARTUP,
            SCAN_INTERVAL
        FROM LIBRARY
        ORDER BY ID ASC"#,
    )
    .fetch_all(pool)
    .await
    .context("query scan profiles")?;

    rows.into_iter()
        .map(|row| {
            let scan_interval =
                library_scan_interval_from_db(&row.get::<String, _>("SCAN_INTERVAL"))?;
            Ok(LibraryScanProfile {
                library_id: row.get::<String, _>("ID"),
                scan_startup: row.get::<bool, _>("SCAN_STARTUP"),
                scan_interval,
            })
        })
        .collect::<Result<Vec<_>, _>>()
}

fn library_scan_interval_from_db(value: &str) -> anyhow::Result<LibraryScanInterval> {
    let normalized = value.trim().to_ascii_uppercase();
    LibraryScanInterval::from_persisted_name(normalized.as_str())
        .ok_or_else(|| anyhow::anyhow!(format!("unsupported library scan interval: {value}")))
}

#[derive(Clone)]
pub struct SqliteFilesystemLibraryScanPipeline {
    owns_main_database: bool,
    owns_filesystem_scan_output: bool,
    task_read_pool: SqlitePool,
    task_write_pool: SqlitePool,
    cleanup_empty_sets_policy: CleanupEmptySetsPolicy,
    runtime_events: Arc<dyn RuntimeSseEventSink>,
}

impl SqliteFilesystemLibraryScanPipeline {
    pub async fn for_runtime(runtime: &JobRuntime<'_>) -> anyhow::Result<Self> {
        Ok(Self {
            owns_main_database: runtime.database().owns_main_database(),
            owns_filesystem_scan_output: runtime.filesystem().owns_filesystem_scan_output(),
            task_read_pool: runtime.database().read_pool().clone(),
            task_write_pool: runtime.database().write_pool().clone(),
            cleanup_empty_sets_policy: runtime
                .cleanup_empty_sets_policy()
                .await
                .map_err(TaskProcessingError::runtime)?,
            runtime_events: runtime.runtime_events_arc(),
        })
    }

    pub(crate) async fn execute_scan(
        &self,
        request: ScanOneLibrary,
    ) -> Result<ScanOneLibraryResult, TaskProcessingError> {
        if !self.owns_filesystem_scan_output {
            return Ok(ScanOneLibraryResult::skipped_external_owned(
                request.library_id,
            ));
        }

        self.run(request).await
    }

    async fn load_profiles(
        &self,
    ) -> Result<Vec<komga_application::task_processing::LibraryScanProfile>, TaskProcessingError>
    {
        load_library_scan_profiles(&self.task_read_pool)
            .await
            .map_err(|error| {
                TaskProcessingError::runtime(format!("load library scan profiles: {error}"))
            })
    }

    fn emit_scan_tasks<I>(
        &self,
        configured_library_count: usize,
        tasks: I,
    ) -> ScheduledLibraryScanBatch
    where
        I: IntoIterator<Item = (String, TaskSchedule)>,
    {
        let tasks = tasks
            .into_iter()
            .map(|(library_id, schedule)| {
                let deep_scan = false;
                let priority = schedule.scan_priority();
                let task = TaskRequest::with_payload(
                    TaskKind::ScanLibrary,
                    komga_application::task_processing::ScanLibraryPayload::new(
                        &library_id,
                        deep_scan,
                    ),
                )
                .priority(priority)
                .into_queue_record_with_id(&format!("{library_id}_DEEP_{deep_scan}"));
                ScheduledLibraryScanTask::new(library_id, task)
            })
            .collect();
        ScheduledLibraryScanBatch::new(configured_library_count, tasks)
    }

    async fn schedule_startup(&self) -> Result<ScheduledLibraryScanBatch, TaskProcessingError> {
        let profiles = self.load_profiles().await?;
        let configured_library_count = profiles.len();
        let startup_libraries = profiles
            .into_iter()
            .filter(|profile| profile.scan_startup)
            .map(|profile| (profile.library_id, TaskSchedule::Startup));
        Ok(self.emit_scan_tasks(configured_library_count, startup_libraries))
    }

    async fn schedule_tick(
        &self,
        state: &LibraryScanScheduleState,
    ) -> Result<ScheduledLibraryScanBatch, TaskProcessingError> {
        let profiles = self.load_profiles().await?;
        let configured_library_count = profiles.len();
        let due_libraries = profiles.into_iter().filter_map(|profile| {
            if profile.scan_interval == LibraryScanInterval::Disabled {
                return None;
            }

            let seconds = profile.scan_interval.duration_seconds()?;
            let elapsed = state
                .elapsed_since_last_run_by_library
                .get(profile.library_id.as_str())?;
            (*elapsed >= std::time::Duration::from_secs(seconds)).then_some((
                profile.library_id,
                TaskSchedule::Interval(profile.scan_interval),
            ))
        });
        Ok(self.emit_scan_tasks(configured_library_count, due_libraries))
    }

    pub(crate) async fn sync_periodic_library_scan_state(
        &self,
        last_run_by_library: &mut HashMap<String, Instant>,
    ) -> Result<(), TaskProcessingError> {
        let profiles = self.load_profiles().await?;
        let active_library_ids = profiles
            .into_iter()
            .filter(|profile| profile.scan_interval.duration_seconds().is_some())
            .map(|profile| profile.library_id)
            .collect::<HashSet<_>>();

        for library_id in &active_library_ids {
            last_run_by_library
                .entry(library_id.clone())
                .or_insert_with(Instant::now);
        }
        last_run_by_library
            .retain(|library_id, _| active_library_ids.contains(library_id.as_str()));

        Ok(())
    }

    async fn cleanup_empty_sets(&self) -> Result<(), TaskProcessingError> {
        if !self.owns_main_database {
            return Ok(());
        }

        cleanup_empty_sets_rows(&self.task_write_pool, self.cleanup_empty_sets_policy)
            .await
            .map_err(|error| TaskProcessingError::runtime(format!("cleanup empty sets: {error}")))
    }

    async fn empty_trash(&self, library_id: &str) -> Result<(), TaskProcessingError> {
        if !self.owns_main_database {
            return Ok(());
        }

        empty_trash_rows(&self.task_write_pool, library_id)
            .await
            .map_err(|error| TaskProcessingError::runtime(format!("empty trash: {error}")))
    }
}

impl Default for SqliteFilesystemLibraryScanPipeline {
    fn default() -> Self {
        let pool = sqlx::SqlitePool::connect_lazy(":memory:")
            .expect("lazy in-memory pool should not fail");
        Self {
            owns_main_database: true,
            owns_filesystem_scan_output: true,
            task_read_pool: pool.clone(),
            task_write_pool: pool,
            cleanup_empty_sets_policy: CleanupEmptySetsPolicy::default(),
            runtime_events: Arc::new(RuntimeSseEventStore::default()),
        }
    }
}

#[async_trait::async_trait]
impl LibraryScanPipeline for SqliteFilesystemLibraryScanPipeline {
    async fn schedule(
        &self,
        trigger: ScanSchedulingTrigger,
        state: &LibraryScanScheduleState,
    ) -> Result<ScheduledLibraryScanBatch, TaskProcessingError> {
        match trigger {
            ScanSchedulingTrigger::Startup => self.schedule_startup().await,
            ScanSchedulingTrigger::Tick => self.schedule_tick(state).await,
        }
    }

    async fn run(
        &self,
        request: ScanOneLibrary,
    ) -> Result<ScanOneLibraryResult, TaskProcessingError> {
        let library_id = request.library_id;
        let scanner =
            LibraryScanner::new(self.task_write_pool.clone(), self.runtime_events.clone());
        let scan_result = scanner.execute(&library_id, request.deep_scan).await?;

        if scan_result.should_empty_trash {
            self.empty_trash(&library_id).await?;
        }
        self.cleanup_empty_sets().await?;

        let follow_up_tasks = ScanFollowUpPlanner::new(self.task_read_pool.clone())
            .plan(&library_id, &scan_result)
            .await?;

        Ok(ScanOneLibraryResult::executed(library_id, follow_up_tasks))
    }
}

impl SqliteFilesystemLibraryScanPipeline {
    pub fn from_pools(write_pool: SqlitePool) -> Self {
        Self {
            owns_main_database: true,
            owns_filesystem_scan_output: true,
            task_read_pool: write_pool.clone(),
            task_write_pool: write_pool,
            cleanup_empty_sets_policy: CleanupEmptySetsPolicy::default(),
            runtime_events: Arc::new(RuntimeSseEventStore::default()),
        }
    }

    #[cfg(test)]
    fn with_filesystem_scan_output_ownership(mut self, owns_filesystem_scan_output: bool) -> Self {
        self.owns_filesystem_scan_output = owns_filesystem_scan_output;
        self
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use super::*;
    use crate::persistence::sqlite::{connect_test_pool, schema};
    use komga_application::task_processing::RefreshBookMetadataPayload;
    use sha2::{Digest, Sha256};

    fn temp_db_path(case_id: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("komga-rust-{case_id}-{nanos}.sqlite"))
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hasher
            .finalize()
            .iter()
            .map(|value| format!("{value:02x}"))
            .collect::<String>()
    }

    async fn seed_library_profiles(db_path: &Path, rows: &[(&str, bool, &str)]) {
        let pool = connect_test_pool(db_path, 1)
            .await
            .expect("temporary sqlite db should open");
        schema::bootstrap_pool(&pool)
            .await
            .expect("temporary sqlite db should bootstrap main schema");
        for (library_id, scan_startup, scan_interval) in rows {
            sqlx::query(
                "INSERT INTO LIBRARY (ID, NAME, ROOT, SCAN_STARTUP, SCAN_INTERVAL) VALUES (?, ?, ?, ?, ?)",
            )
                .bind(library_id.to_string())
                .bind(format!("Library {library_id}"))
                .bind(std::env::temp_dir().to_string_lossy().to_string())
                .bind(*scan_startup)
                .bind(scan_interval.to_string())
                .execute(&pool)
                .await
                .expect("library row should be inserted");
        }
        pool.close().await;
    }

    #[tokio::test]
    async fn execute_scan_skips_external_owned_output() {
        let db_path = temp_db_path("library-scan-pipeline-task-resolution");
        let root = std::env::temp_dir().join(format!(
            "komga-rust-task-resolution-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("Series-A")).expect("scan root should be created");
        std::fs::write(root.join("Series-A").join("Book-001.cbz"), b"book")
            .expect("book fixture should be created");

        let pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open");
        schema::bootstrap_pool(&pool)
            .await
            .expect("temporary sqlite db should bootstrap main schema");
        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
            .bind("library-1")
            .bind("Library 1")
            .bind(root.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .expect("library row should be inserted");
        pool.close().await;

        let read_pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open for pipeline");
        let pipeline = SqliteFilesystemLibraryScanPipeline::from_pools(read_pool.clone())
            .with_filesystem_scan_output_ownership(false);
        let result = pipeline
            .execute_scan(ScanOneLibrary::new("library-1", false))
            .await
            .expect("external-owned scan should be handled by pipeline");

        assert_eq!(result.library_id, "library-1");
        assert_eq!(
            result.outcome,
            komga_application::task_processing::ScanOneLibraryOutcome::SkippedExternalOwned,
        );
        assert!(result.follow_up_tasks.is_empty());

        let persisted_books = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM BOOK")
            .fetch_one(&read_pool)
            .await
            .expect("book count should be queryable");
        assert_eq!(
            persisted_books, 0,
            "external-owned scan output must not persist scan-derived book rows",
        );

        read_pool.close().await;
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn schedule_startup_only_emits_startup_enabled_canonical_scan_tasks() {
        let db_path = temp_db_path("library-scan-pipeline-startup");
        seed_library_profiles(
            db_path.as_path(),
            &[
                ("library-2", false, "DAILY"),
                ("library-1", true, "DISABLED"),
            ],
        )
        .await;

        let read_pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open for pipeline");
        let pipeline = SqliteFilesystemLibraryScanPipeline::from_pools(read_pool.clone());
        let scheduled = pipeline
            .schedule(
                ScanSchedulingTrigger::Startup,
                &LibraryScanScheduleState::default(),
            )
            .await
            .expect("startup scheduling should succeed");

        assert_eq!(scheduled.configured_library_count, 2);
        assert_eq!(scheduled.tasks.len(), 1);
        assert_eq!(scheduled.tasks[0].library_id, "library-1");
        let scheduled = scheduled.into_queue_records();

        assert_eq!(scheduled.len(), 1);
        assert_eq!(scheduled[0].id, "ScanLibrary_library-1_DEEP_false");
        assert_eq!(scheduled[0].simple_type, "ScanLibrary");
        assert_eq!(scheduled[0].priority, 4);

        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn schedule_tick_only_emits_due_interval_tasks_from_in_memory_state() {
        let db_path = temp_db_path("library-scan-pipeline-tick");
        seed_library_profiles(
            db_path.as_path(),
            &[
                ("library-disabled", true, "DISABLED"),
                ("library-due", true, "HOURLY"),
                ("library-not-due", true, "DAILY"),
                ("library-never-ran", true, "HOURLY"),
            ],
        )
        .await;

        let read_pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open for pipeline");
        let pipeline = SqliteFilesystemLibraryScanPipeline::from_pools(read_pool.clone());
        let mut state = LibraryScanScheduleState::default();
        state.mark_elapsed("library-due", Duration::from_secs((60 * 60) + 5));
        state.mark_elapsed("library-not-due", Duration::from_secs(5));

        let scheduled = pipeline
            .schedule(ScanSchedulingTrigger::Tick, &state)
            .await
            .expect("periodic scheduling should succeed");

        assert_eq!(scheduled.configured_library_count, 4);
        assert_eq!(scheduled.tasks.len(), 1);
        assert_eq!(scheduled.tasks[0].library_id, "library-due");
        let scheduled = scheduled.into_queue_records();

        assert_eq!(scheduled.len(), 1);
        assert_eq!(scheduled[0].id, "ScanLibrary_library-due_DEEP_false");

        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn run_rejects_missing_library_row() {
        let db_path = temp_db_path("library-scan-pipeline-missing-library");
        let pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open");
        schema::bootstrap_pool(&pool)
            .await
            .expect("temporary sqlite db should bootstrap main schema");
        pool.close().await;

        let read_pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open for pipeline");
        let pipeline = SqliteFilesystemLibraryScanPipeline::from_pools(read_pool.clone());
        let error = pipeline
            .run(ScanOneLibrary::new("missing-library".to_string(), false))
            .await
            .expect_err("scan must fail when the task target has no library row");

        assert!(error.message.contains("missing-library"));

        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn run_enqueues_refresh_series_metadata_for_existing_oneshot_with_new_book() {
        let db_path = temp_db_path("library-scan-pipeline-oneshot-refresh-series");
        let root = std::env::temp_dir().join(format!(
            "komga-rust-oneshot-refresh-series-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("oneshots")).expect("oneshot root should be created");
        std::fs::write(root.join("oneshots").join("existing.cbz"), b"existing")
            .expect("existing oneshot should be created");
        std::fs::write(root.join("oneshots").join("new.cbz"), b"new")
            .expect("new oneshot should be created");

        let pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open");
        schema::bootstrap_pool(&pool)
            .await
            .expect("temporary sqlite db should bootstrap main schema");
        sqlx::query(
            "INSERT INTO LIBRARY (ID, NAME, ROOT, ONESHOTS_DIRECTORY, SCAN_CBX) VALUES (?, ?, ?, ?, 1)",
        )
        .bind("library-1")
        .bind("Library 1")
        .bind(root.to_string_lossy().to_string())
        .bind("oneshots")
        .execute(&pool)
        .await
        .expect("library row should be inserted");
        sqlx::query(
            r#"INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, oneshot)
VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?, 1)"#,
        )
        .bind("series-existing")
        .bind(0_i64)
        .bind("existing")
        .bind("oneshots/existing.cbz")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("existing oneshot series should be inserted");
        sqlx::query(
            r#"INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, SERIES_ID)
VALUES (?, ?, ?, ?)"#,
        )
        .bind("ONGOING")
        .bind("existing")
        .bind("existing")
        .bind("series-existing")
        .execute(&pool)
        .await
        .expect("existing oneshot series metadata should be inserted");
        sqlx::query(
            r#"INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, oneshot)
VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?, ?, ?, ?, 1)"#,
        )
        .bind("book-existing")
        .bind(0_i64)
        .bind("existing")
        .bind("oneshots/existing.cbz")
        .bind("series-existing")
        .bind(8_i64)
        .bind(1_i64)
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("existing oneshot book should be inserted");
        sqlx::query(
            r#"INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, BOOK_ID)
VALUES (?, ?, ?, ?)"#,
        )
        .bind("1")
        .bind(1.0_f64)
        .bind("existing")
        .bind("book-existing")
        .execute(&pool)
        .await
        .expect("existing oneshot book metadata should be inserted");
        pool.close().await;

        let read_pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open for pipeline");
        let pipeline = SqliteFilesystemLibraryScanPipeline::from_pools(read_pool.clone());
        let result = pipeline
            .run(ScanOneLibrary::new("library-1".to_string(), false))
            .await
            .expect("scan pipeline should succeed");
        let refresh_series_tasks = result
            .follow_up_tasks
            .into_iter()
            .filter(|task| task.simple_type == "RefreshSeriesMetadata")
            .collect::<Vec<_>>();

        assert_eq!(refresh_series_tasks.len(), 2);
        assert!(
            refresh_series_tasks
                .iter()
                .any(|task| task.group.as_deref() == Some("series-existing"))
        );

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn run_enqueues_refresh_book_metadata_for_restored_book_without_locked_title() {
        let db_path = temp_db_path("library-scan-pipeline-restore-book-title-refresh");
        let root = std::env::temp_dir().join(format!(
            "komga-rust-restore-book-title-refresh-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("books"))
            .expect("restore-book refresh root should be created");
        let restored_bytes = b"restored-book-content";
        std::fs::write(root.join("books/restored.cbz"), restored_bytes)
            .expect("restored book file should be created");

        let pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open");
        schema::bootstrap_pool(&pool)
            .await
            .expect("temporary sqlite db should bootstrap main schema");
        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
            .bind("library-1")
            .bind("Library 1")
            .bind(root.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .expect("library row should be inserted");
        sqlx::query(
            r#"INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID)
VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?)"#,
        )
        .bind("series-1")
        .bind(0_i64)
        .bind("Series One")
        .bind("series/series-one")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("series row should be inserted");
        sqlx::query(
            r#"INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, SERIES_ID)
VALUES (?, ?, ?, ?)"#,
        )
        .bind("ONGOING")
        .bind("Series One")
        .bind("Series One")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series metadata row should be inserted");
        sqlx::query(
            r#"INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, FILE_HASH, DELETED_DATE)
VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)"#,
        )
        .bind("book-deleted")
        .bind(0_i64)
        .bind("legacy-title")
        .bind("books/legacy.cbz")
        .bind("series-1")
        .bind(restored_bytes.len() as i64)
        .bind(1_i64)
        .bind("library-1")
        .bind(sha256_hex(restored_bytes))
        .execute(&pool)
        .await
        .expect("deleted book row should be inserted");
        sqlx::query(
            r#"INSERT INTO BOOK_METADATA (TITLE, TITLE_LOCK, SUMMARY, SUMMARY_LOCK, NUMBER, NUMBER_LOCK, NUMBER_SORT, NUMBER_SORT_LOCK, ISBN, ISBN_LOCK, AUTHORS_LOCK, TAGS_LOCK, LINKS_LOCK, BOOK_ID)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind("Imported Legacy Title")
        .bind(false)
        .bind("legacy summary")
        .bind(true)
        .bind("7")
        .bind(true)
        .bind(7.0_f64)
        .bind(true)
        .bind("isbn-legacy")
        .bind(true)
        .bind(false)
        .bind(false)
        .bind(false)
        .bind("book-deleted")
        .execute(&pool)
        .await
        .expect("deleted book metadata row should be inserted");
        pool.close().await;

        let read_pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open for pipeline");
        let pipeline = SqliteFilesystemLibraryScanPipeline::from_pools(read_pool.clone());
        let result = pipeline
            .run(ScanOneLibrary::new("library-1".to_string(), false))
            .await
            .expect("scan pipeline should succeed");
        let refresh_book_tasks = result
            .follow_up_tasks
            .iter()
            .filter(|task| task.simple_type == "RefreshBookMetadata")
            .collect::<Vec<_>>();

        assert_eq!(refresh_book_tasks.len(), 1);

        let refresh_book_id = refresh_book_tasks[0]
            .id
            .strip_prefix("RefreshBookMetadata_")
            .expect("refresh book metadata task id should include book id");
        let refresh_book_payload =
            RefreshBookMetadataPayload::from_task_record(refresh_book_tasks[0], refresh_book_id)
                .expect("refresh book metadata task should use application payload contract");
        assert_eq!(
            refresh_book_payload.capabilities.as_deref(),
            Some(&["TITLE".to_string()][..]),
        );
        assert_eq!(refresh_book_payload.book_id.as_str(), refresh_book_id,);
        let refresh_book_group = refresh_book_tasks[0]
            .group
            .as_deref()
            .expect("refresh book metadata task should be grouped by restored series");

        let refresh_series_tasks = result
            .follow_up_tasks
            .iter()
            .filter(|task| task.simple_type == "RefreshSeriesMetadata")
            .collect::<Vec<_>>();
        assert_eq!(refresh_series_tasks.len(), 1);
        assert_eq!(
            refresh_series_tasks[0].group.as_deref(),
            Some(refresh_book_group),
        );

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn schedule_startup_propagates_invalid_interval_from_non_startup_profile() {
        let db_path = temp_db_path("library-scan-pipeline-invalid-interval");
        seed_library_profiles(
            db_path.as_path(),
            &[
                ("library-1", true, "DAILY"),
                ("library-2", false, "FUTURE_VALUE"),
            ],
        )
        .await;

        let read_pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open for pipeline");
        let pipeline = SqliteFilesystemLibraryScanPipeline::from_pools(read_pool.clone());
        let error = pipeline
            .schedule(
                ScanSchedulingTrigger::Startup,
                &LibraryScanScheduleState::default(),
            )
            .await
            .expect_err("invalid intervals should fail startup scheduling");

        assert!(
            error
                .message
                .contains("unsupported library scan interval: FUTURE_VALUE")
        );

        let _ = std::fs::remove_file(db_path);
    }
}

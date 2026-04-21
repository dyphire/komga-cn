use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

use komga_application::task_processing::{
    DefaultLibraryTaskEmitter, DefaultTaskProtocolCatalog, LibraryScanInterval,
    LibraryScanPipeline, LibraryScanScheduleState, LibraryTaskBatch, LibraryTaskCommand,
    LibraryTaskEmitter, ScanOneLibrary, ScanOneLibraryResult, ScanSchedulingTrigger,
    TaskProcessingError, TaskQueueRecord, TaskRuntimeContext, TaskSchedule,
    normalize_library_scan_profiles,
};
use tokio::time::Instant;

use crate::tasks::cleanup_workflow::{cleanup_empty_sets_rows, empty_trash_rows};
use crate::tasks::library_scan_profiles::load_library_scan_profiles;
use crate::tasks::media_queries::{
    load_books_for_extension_repair, load_books_requiring_analysis,
    load_books_with_missing_file_hash, load_library_hashing_flags, load_library_maintenance_flags,
};

use super::{
    ExecutedLibraryScan, RuntimeFollowUpTask, enqueue_sidecar_refresh_tasks,
    execute_scan_orchestration, runtime_follow_up_task,
};

/// This seam is introduced before the runtime migration so later tasks can move startup,
/// periodic scheduling, and scan execution behind one boundary without changing task
/// protocol ownership in the same patch.
#[derive(Clone, Debug)]
pub struct SqliteFilesystemLibraryScanPipeline<
    E = DefaultLibraryTaskEmitter<DefaultTaskProtocolCatalog>,
> {
    database_file: PathBuf,
    owns_main_database: bool,
    library_task_emitter: E,
}

impl<E> SqliteFilesystemLibraryScanPipeline<E> {
    pub fn new(database_file: impl Into<PathBuf>, library_task_emitter: E) -> Self {
        Self {
            database_file: database_file.into(),
            owns_main_database: true,
            library_task_emitter,
        }
    }

    pub fn for_runtime(runtime: &TaskRuntimeContext, library_task_emitter: E) -> Self {
        Self {
            database_file: runtime.database_file.clone(),
            owns_main_database: runtime.owns_main_database,
            library_task_emitter,
        }
    }

    fn load_profiles(
        &self,
    ) -> Result<Vec<komga_application::task_processing::LibraryScanProfile>, TaskProcessingError>
    {
        load_library_scan_profiles(self.database_file.as_path()).map_err(|error| {
            TaskProcessingError::runtime(format!("load library scan profiles: {error}"))
        })
    }
}

impl<E> SqliteFilesystemLibraryScanPipeline<E>
where
    E: LibraryTaskEmitter,
{
    fn emit_scan_tasks<I>(&self, tasks: I) -> LibraryTaskBatch
    where
        I: IntoIterator<Item = (String, TaskSchedule)>,
    {
        let mut planned_tasks = Vec::new();
        for (library_id, schedule) in tasks {
            planned_tasks.extend(
                self.library_task_emitter
                    .emit(LibraryTaskCommand::ScanLibrary {
                        library_id,
                        deep_scan: false,
                        schedule,
                    })
                    .tasks,
            );
        }

        LibraryTaskBatch::new(planned_tasks)
    }

    fn schedule_startup(&self) -> Result<LibraryTaskBatch, TaskProcessingError> {
        let profiles =
            normalize_library_scan_profiles(&self.load_profiles()?).map_err(|error| {
                TaskProcessingError::runtime(format!(
                    "normalize startup library scan profiles: {error}"
                ))
            })?;
        let startup_libraries = profiles
            .into_iter()
            .filter(|profile| profile.scan_startup)
            .map(|profile| (profile.library_id, TaskSchedule::Startup));
        Ok(self.emit_scan_tasks(startup_libraries))
    }

    fn schedule_tick(
        &self,
        state: &LibraryScanScheduleState,
    ) -> Result<LibraryTaskBatch, TaskProcessingError> {
        let profiles =
            normalize_library_scan_profiles(&self.load_profiles()?).map_err(|error| {
                TaskProcessingError::runtime(format!(
                    "normalize periodic library scan profiles: {error}"
                ))
            })?;
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
        Ok(self.emit_scan_tasks(due_libraries))
    }

    pub(crate) fn sync_periodic_library_scan_state(
        &self,
        last_run_by_library: &mut HashMap<String, Instant>,
    ) -> Result<(), TaskProcessingError> {
        let profiles =
            normalize_library_scan_profiles(&self.load_profiles()?).map_err(|error| {
                TaskProcessingError::runtime(format!(
                    "normalize periodic library scan profiles: {error}"
                ))
            })?;
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

    fn cleanup_empty_sets(&self) -> Result<(), TaskProcessingError> {
        if !self.owns_main_database {
            return Ok(());
        }

        cleanup_empty_sets_rows(self.database_file.as_path())
            .map_err(|error| TaskProcessingError::runtime(format!("cleanup empty sets: {error}")))
    }

    fn empty_trash(&self, library_id: &str) -> Result<(), TaskProcessingError> {
        if !self.owns_main_database {
            return Ok(());
        }

        empty_trash_rows(self.database_file.as_path(), library_id)
            .map_err(|error| TaskProcessingError::runtime(format!("empty trash: {error}")))
    }

    fn collect_runtime_follow_up_tasks(
        &self,
        library_id: &str,
        executed_scan: &ExecutedLibraryScan,
    ) -> Result<Vec<TaskQueueRecord>, TaskProcessingError> {
        const DEFAULT_PRIORITY: i32 = 4;
        const LOW_PRIORITY: i32 = 2;
        const LOWEST_PRIORITY: i32 = 0;

        let mut follow_up_tasks = Vec::<TaskQueueRecord>::new();

        let hashing_flags = load_library_hashing_flags(self.database_file.as_path(), library_id)
            .map_err(|error| {
                TaskProcessingError::runtime(format!("load library hashing flags: {error}"))
            })?;
        let analyzable_book_ids = load_books_requiring_analysis(
            self.database_file.as_path(),
            &executed_scan.scan.book_ids,
        )
        .map_err(|error| {
            TaskProcessingError::runtime(format!("load books requiring analysis: {error}"))
        })?
        .into_iter()
        .collect::<HashSet<_>>();
        for series in &executed_scan.scan.series_rows {
            for book in &series.books {
                if analyzable_book_ids.contains(&book.book_id) {
                    follow_up_tasks.push(runtime_follow_up_task(
                        RuntimeFollowUpTask::AnalyzeBook {
                            book_id: book.book_id.clone(),
                            series_id: series.series_id.clone(),
                            priority: DEFAULT_PRIORITY,
                        },
                    ));
                }
            }
        }

        if hashing_flags.hash_files {
            let book_ids =
                load_books_with_missing_file_hash(self.database_file.as_path(), library_id, false)
                    .map_err(|error| {
                        TaskProcessingError::runtime(format!(
                            "load books with missing file hash: {error}"
                        ))
                    })?;
            for book_id in book_ids {
                follow_up_tasks.push(runtime_follow_up_task(RuntimeFollowUpTask::HashBook {
                    book_id,
                    priority: LOWEST_PRIORITY,
                }));
            }
        }

        if hashing_flags.hash_koreader {
            let book_ids =
                load_books_with_missing_file_hash(self.database_file.as_path(), library_id, true)
                    .map_err(|error| {
                    TaskProcessingError::runtime(format!(
                        "load books with missing koreader hash: {error}"
                    ))
                })?;
            for book_id in book_ids {
                follow_up_tasks.push(runtime_follow_up_task(
                    RuntimeFollowUpTask::HashBookKoreader {
                        book_id,
                        priority: LOWEST_PRIORITY,
                    },
                ));
            }
        }

        if hashing_flags.hash_pages {
            follow_up_tasks.push(runtime_follow_up_task(
                RuntimeFollowUpTask::FindBooksWithMissingPageHash {
                    library_id: library_id.to_string(),
                },
            ));
        }
        follow_up_tasks.push(runtime_follow_up_task(
            RuntimeFollowUpTask::FindDuplicatePagesToDelete {
                library_id: library_id.to_string(),
                priority: LOWEST_PRIORITY,
            },
        ));

        let maintenance_flags =
            load_library_maintenance_flags(self.database_file.as_path(), library_id).map_err(
                |error| {
                    TaskProcessingError::runtime(format!("load library maintenance flags: {error}"))
                },
            )?;
        if maintenance_flags.repair_extensions {
            let books = load_books_for_extension_repair(self.database_file.as_path(), library_id)
                .map_err(|error| {
                TaskProcessingError::runtime(format!("load books for extension repair: {error}"))
            })?;
            for book in books {
                follow_up_tasks.push(runtime_follow_up_task(
                    RuntimeFollowUpTask::RepairExtension {
                        book_id: book.book_id,
                        series_id: book.series_id,
                        priority: LOW_PRIORITY,
                    },
                ));
            }
        }
        if maintenance_flags.convert_to_cbz {
            follow_up_tasks.push(runtime_follow_up_task(
                RuntimeFollowUpTask::FindBooksToConvert {
                    library_id: library_id.to_string(),
                    priority: LOWEST_PRIORITY,
                },
            ));
        }

        let mut changed_series_ids = executed_scan.changed_series_ids.to_vec();
        changed_series_ids.sort();
        changed_series_ids.dedup();
        for series_id in changed_series_ids {
            follow_up_tasks.push(runtime_follow_up_task(
                RuntimeFollowUpTask::RefreshSeriesMetadata {
                    series_id,
                    priority: DEFAULT_PRIORITY,
                },
            ));
        }

        let book_series_ids = executed_scan
            .scan
            .series_rows
            .iter()
            .flat_map(|series| {
                series
                    .books
                    .iter()
                    .map(|book| (book.book_id.clone(), series.series_id.clone()))
            })
            .collect::<HashMap<_, _>>();
        let mut book_metadata_capabilities = BTreeMap::<String, BTreeSet<String>>::new();
        for refresh in &executed_scan.book_metadata_refreshes {
            book_metadata_capabilities
                .entry(refresh.book_id.clone())
                .or_default()
                .extend(refresh.capabilities.iter().cloned());
        }
        for book_id in &executed_scan.renumbered_book_ids {
            book_metadata_capabilities
                .entry(book_id.clone())
                .or_default();
        }
        for (book_id, capabilities) in book_metadata_capabilities {
            let capabilities =
                (!capabilities.is_empty()).then(|| capabilities.into_iter().collect::<Vec<_>>());
            follow_up_tasks.push(runtime_follow_up_task(
                RuntimeFollowUpTask::RefreshBookMetadata {
                    book_id: book_id.clone(),
                    series_id: book_series_ids.get(&book_id).cloned(),
                    priority: DEFAULT_PRIORITY,
                    capabilities,
                },
            ));
        }

        enqueue_sidecar_refresh_tasks(
            &mut follow_up_tasks,
            &executed_scan.scan,
            &executed_scan.changed_sidecar_urls,
            DEFAULT_PRIORITY,
        );

        Ok(follow_up_tasks)
    }
}

impl Default
    for SqliteFilesystemLibraryScanPipeline<DefaultLibraryTaskEmitter<DefaultTaskProtocolCatalog>>
{
    fn default() -> Self {
        Self::new(PathBuf::new(), DefaultLibraryTaskEmitter::default())
    }
}

impl<E> LibraryScanPipeline for SqliteFilesystemLibraryScanPipeline<E>
where
    E: LibraryTaskEmitter,
{
    fn schedule(
        &self,
        trigger: ScanSchedulingTrigger,
        state: &LibraryScanScheduleState,
    ) -> Result<LibraryTaskBatch, TaskProcessingError> {
        match trigger {
            ScanSchedulingTrigger::Startup => self.schedule_startup(),
            ScanSchedulingTrigger::Tick => self.schedule_tick(state),
        }
    }

    fn run(&self, request: ScanOneLibrary) -> Result<ScanOneLibraryResult, TaskProcessingError> {
        let library_id = request.library_id;
        let executed_scan = execute_scan_orchestration(
            self.database_file.as_path(),
            &library_id,
            request.deep_scan,
        )?;

        if executed_scan.should_empty_trash {
            self.empty_trash(&library_id)?;
        }
        self.cleanup_empty_sets()?;

        let follow_up_tasks = self.collect_runtime_follow_up_tasks(&library_id, &executed_scan)?;

        Ok(ScanOneLibraryResult::executed(library_id, follow_up_tasks))
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use super::*;
    use crate::sqlite::{connect_test_pool, setup};
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
        setup::bootstrap_pool(&pool)
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

        let pipeline = SqliteFilesystemLibraryScanPipeline::new(
            db_path.clone(),
            DefaultLibraryTaskEmitter::default(),
        );
        let scheduled = pipeline
            .schedule(
                ScanSchedulingTrigger::Startup,
                &LibraryScanScheduleState::default(),
            )
            .expect("startup scheduling should succeed")
            .into_queue_records();

        assert_eq!(scheduled.len(), 1);
        assert_eq!(scheduled[0].id, "SCAN_LIBRARY_library-1_DEEP_false");
        assert_eq!(scheduled[0].simple_type, "SCAN_LIBRARY");
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

        let pipeline = SqliteFilesystemLibraryScanPipeline::new(
            db_path.clone(),
            DefaultLibraryTaskEmitter::default(),
        );
        let mut state = LibraryScanScheduleState::default();
        state.mark_elapsed("library-due", Duration::from_secs((60 * 60) + 5));
        state.mark_elapsed("library-not-due", Duration::from_secs(5));

        let scheduled = pipeline
            .schedule(ScanSchedulingTrigger::Tick, &state)
            .expect("periodic scheduling should succeed")
            .into_queue_records();

        assert_eq!(scheduled.len(), 1);
        assert_eq!(scheduled[0].id, "SCAN_LIBRARY_library-due_DEEP_false");

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
        setup::bootstrap_pool(&pool)
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

        let pipeline = SqliteFilesystemLibraryScanPipeline::new(
            db_path.clone(),
            DefaultLibraryTaskEmitter::default(),
        );
        let result = pipeline
            .run(ScanOneLibrary::new("library-1".to_string(), false))
            .expect("scan pipeline should succeed");
        let refresh_series_tasks = result
            .follow_up_tasks
            .into_iter()
            .filter(|task| task.simple_type == "REFRESH_SERIES_METADATA")
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
        setup::bootstrap_pool(&pool)
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

        let pipeline = SqliteFilesystemLibraryScanPipeline::new(
            db_path.clone(),
            DefaultLibraryTaskEmitter::default(),
        );
        let result = pipeline
            .run(ScanOneLibrary::new("library-1".to_string(), false))
            .expect("scan pipeline should succeed");
        let refresh_book_tasks = result
            .follow_up_tasks
            .iter()
            .filter(|task| task.simple_type == "REFRESH_BOOK_METADATA")
            .collect::<Vec<_>>();

        assert_eq!(refresh_book_tasks.len(), 1);

        let refresh_book_payload = refresh_book_tasks[0]
            .payload
            .as_deref()
            .and_then(|payload| serde_json::from_str::<serde_json::Value>(payload).ok())
            .expect("refresh book metadata payload should be valid JSON");
        assert_eq!(
            refresh_book_payload
                .get("capabilities")
                .and_then(|value| value.as_array())
                .cloned(),
            Some(vec![serde_json::Value::String("TITLE".to_string())]),
        );
        assert_eq!(
            refresh_book_payload
                .get("groupId")
                .and_then(|value| value.as_str()),
            refresh_book_tasks[0].group.as_deref(),
        );
        assert_eq!(
            refresh_book_payload
                .get("bookId")
                .and_then(|value| value.as_str()),
            refresh_book_tasks[0]
                .id
                .strip_prefix("REFRESH_BOOK_METADATA_"),
        );

        let refresh_series_tasks = result
            .follow_up_tasks
            .iter()
            .filter(|task| task.simple_type == "REFRESH_SERIES_METADATA")
            .collect::<Vec<_>>();
        assert_eq!(refresh_series_tasks.len(), 1);
        assert_eq!(
            refresh_series_tasks[0].group.as_deref(),
            refresh_book_tasks[0].group.as_deref(),
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

        let pipeline = SqliteFilesystemLibraryScanPipeline::new(
            db_path.clone(),
            DefaultLibraryTaskEmitter::default(),
        );
        let error = pipeline
            .schedule(
                ScanSchedulingTrigger::Startup,
                &LibraryScanScheduleState::default(),
            )
            .expect_err("invalid intervals should fail startup scheduling");

        assert!(
            error
                .message
                .contains("unsupported library scan interval: FUTURE_VALUE")
        );

        let _ = std::fs::remove_file(db_path);
    }
}

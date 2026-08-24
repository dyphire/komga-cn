use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use komga_application::task_processing::{
    BookPayload, LibraryPayload, RefreshBookMetadataPayload, SeriesPayload, TaskKind,
    TaskProcessingError, TaskQueueRecord, TaskRequest,
};
use sqlx::SqlitePool;

use crate::media::library_scan::{LibraryScanResult, enqueue_sidecar_refresh_tasks};
use crate::media::maintenance::persistence::{
    load_books_for_extension_repair, load_books_requiring_analysis,
    load_books_with_missing_file_hash, load_library_hashing_flags, load_library_maintenance_flags,
};

pub(super) struct ScanFollowUpPlanner {
    pool: SqlitePool,
}

impl ScanFollowUpPlanner {
    pub(super) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub(super) async fn plan(
        &self,
        library_id: &str,
        scan_result: &LibraryScanResult,
    ) -> Result<Vec<TaskQueueRecord>, TaskProcessingError> {
        const DEFAULT_PRIORITY: i32 = 4;
        const LOW_PRIORITY: i32 = 2;
        const LOWEST_PRIORITY: i32 = 0;

        let mut follow_up_tasks = Vec::<TaskQueueRecord>::new();

        let hashing_flags = load_library_hashing_flags(&self.pool, library_id)
            .await
            .map_err(|error| {
                TaskProcessingError::runtime(format!("load library hashing flags: {error}"))
            })?;
        let analyzable_book_ids = load_books_requiring_analysis(&self.pool, &scan_result.book_ids)
            .await
            .map_err(|error| {
                TaskProcessingError::runtime(format!("load books requiring analysis: {error}"))
            })?
            .into_iter()
            .collect::<HashSet<_>>();
        for series in &scan_result.series_rows {
            for book in &series.books {
                if analyzable_book_ids.contains(&book.book_id) {
                    follow_up_tasks.push(
                        TaskRequest::new(TaskKind::AnalyzeBook)
                            .priority(DEFAULT_PRIORITY)
                            .group(series.series_id.clone())
                            .into_queue_record_with_id(&book.book_id),
                    );
                }
            }
        }

        if hashing_flags.hash_files {
            let book_ids = load_books_with_missing_file_hash(&self.pool, library_id, false)
                .await
                .map_err(|error| {
                    TaskProcessingError::runtime(format!(
                        "load books with missing file hash: {error}"
                    ))
                })?;
            for book_id in book_ids {
                follow_up_tasks.push(
                    TaskRequest::with_payload(TaskKind::HashBook, BookPayload::new(book_id))
                        .priority(LOWEST_PRIORITY)
                        .into_queue_record(),
                );
            }
        }

        if hashing_flags.hash_koreader {
            let book_ids = load_books_with_missing_file_hash(&self.pool, library_id, true)
                .await
                .map_err(|error| {
                    TaskProcessingError::runtime(format!(
                        "load books with missing koreader hash: {error}"
                    ))
                })?;
            for book_id in book_ids {
                follow_up_tasks.push(
                    TaskRequest::with_payload(
                        TaskKind::HashBookKoreader,
                        BookPayload::new(book_id),
                    )
                    .priority(LOWEST_PRIORITY)
                    .into_queue_record(),
                );
            }
        }

        if hashing_flags.hash_pages {
            follow_up_tasks.push(
                TaskRequest::with_payload(
                    TaskKind::FindBooksWithMissingPageHash,
                    LibraryPayload::new(library_id),
                )
                .priority(LOWEST_PRIORITY)
                .into_queue_record(),
            );
        }
        follow_up_tasks.push(
            TaskRequest::new(TaskKind::FindDuplicatePagesToDelete)
                .priority(LOWEST_PRIORITY)
                .into_queue_record_with_id(library_id),
        );

        let maintenance_flags = load_library_maintenance_flags(&self.pool, library_id)
            .await
            .map_err(|error| {
                TaskProcessingError::runtime(format!("load library maintenance flags: {error}"))
            })?;
        if maintenance_flags.repair_extensions {
            let books = load_books_for_extension_repair(&self.pool, library_id)
                .await
                .map_err(|error| {
                    TaskProcessingError::runtime(format!(
                        "load books for extension repair: {error}"
                    ))
                })?;
            for book in books {
                follow_up_tasks.push(
                    TaskRequest::with_payload(
                        TaskKind::RepairExtension,
                        BookPayload::new(book.book_id.clone()),
                    )
                    .priority(LOW_PRIORITY)
                    .group(book.series_id.clone())
                    .into_queue_record(),
                );
            }
        }
        if maintenance_flags.convert_to_cbz {
            follow_up_tasks.push(
                TaskRequest::new(TaskKind::FindBooksToConvert)
                    .priority(LOWEST_PRIORITY)
                    .into_queue_record_with_id(library_id),
            );
        }

        let mut changed_series_ids = scan_result.changed_series_ids.to_vec();
        changed_series_ids.sort();
        changed_series_ids.dedup();
        for series_id in changed_series_ids {
            follow_up_tasks.push(
                TaskRequest::with_payload(
                    TaskKind::RefreshSeriesMetadata,
                    SeriesPayload::new(&series_id),
                )
                .priority(DEFAULT_PRIORITY)
                .group(&series_id)
                .into_queue_record(),
            );
        }

        let book_series_ids = scan_result
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
        for refresh in &scan_result.book_metadata_refreshes {
            book_metadata_capabilities
                .entry(refresh.book_id.clone())
                .or_default()
                .extend(refresh.capabilities.iter().cloned());
        }
        for book_id in &scan_result.renumbered_book_ids {
            book_metadata_capabilities
                .entry(book_id.clone())
                .or_default();
        }
        for (book_id, capabilities) in book_metadata_capabilities {
            let capabilities =
                (!capabilities.is_empty()).then(|| capabilities.into_iter().collect::<Vec<_>>());
            let mut payload = RefreshBookMetadataPayload::new(&book_id);
            if let Some(caps) = capabilities {
                payload = payload.with_capabilities(caps);
            }
            let mut req = TaskRequest::with_payload(TaskKind::RefreshBookMetadata, payload)
                .priority(DEFAULT_PRIORITY);
            if let Some(gid) = book_series_ids.get(&book_id) {
                req = req.group(gid.clone());
            }
            follow_up_tasks.push(req.into_queue_record());
        }

        enqueue_sidecar_refresh_tasks(
            &mut follow_up_tasks,
            &scan_result.series_rows,
            &scan_result.sidecars,
            &scan_result.changed_sidecar_urls,
            DEFAULT_PRIORITY,
        );

        Ok(follow_up_tasks)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::persistence::sqlite::{connect_test_pool, schema};
    use komga_application::task_processing::RefreshBookMetadataPayload;

    use super::ScanFollowUpPlanner;
    use crate::media::library_scan::{
        BookMetadataRefreshRequest, LibraryScanResult, ScannedBookRow, ScannedSeriesRow,
        ScannedSidecarRow, ScannedSidecarSource, ScannedSidecarType,
    };

    fn temp_db_path(case_id: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("komga-rust-{case_id}-{nanos}.sqlite"))
    }

    #[tokio::test]
    async fn planner_orders_runtime_follow_ups_before_sidecar_refreshes() {
        let db_path = temp_db_path("scan-follow-up-order");
        let pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open");
        schema::bootstrap_pool(&pool)
            .await
            .expect("temporary sqlite db should bootstrap main schema");
        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
            .bind("library-1")
            .bind("Library 1")
            .bind(std::env::temp_dir().to_string_lossy().to_string())
            .execute(&pool)
            .await
            .expect("library row should be inserted");

        let scan_result = LibraryScanResult {
            book_ids: vec!["book-1".to_string()],
            series_rows: vec![ScannedSeriesRow {
                series_id: "series-1".to_string(),
                series_name: "Series One".to_string(),
                series_url: "Series One".to_string(),
                series_last_modified_unix_seconds: 1,
                oneshot: false,
                books: vec![ScannedBookRow {
                    book_id: "book-1".to_string(),
                    book_name: "Book One".to_string(),
                    book_url: "Series One/book.cbz".to_string(),
                    file_size: 10,
                    file_last_modified_unix_seconds: 1,
                    oneshot: false,
                }],
            }],
            sidecars: vec![ScannedSidecarRow {
                url: "Series One/series.json".to_string(),
                parent_url: "Series One".to_string(),
                last_modified_unix_seconds: 2,
                source: ScannedSidecarSource::Series,
                sidecar_type: ScannedSidecarType::Metadata,
            }],
            changed_sidecar_urls: vec!["Series One/series.json".to_string()],
            renumbered_book_ids: vec!["book-1".to_string()],
            changed_series_ids: vec!["series-1".to_string()],
            book_metadata_refreshes: vec![BookMetadataRefreshRequest {
                book_id: "book-1".to_string(),
                series_id: "series-1".to_string(),
                capabilities: vec!["TITLE".to_string()],
            }],
            should_empty_trash: false,
        };

        let tasks = ScanFollowUpPlanner::new(pool.clone())
            .plan("library-1", &scan_result)
            .await
            .expect("scan follow-up planning should succeed");
        let task_types = tasks
            .iter()
            .map(|task| task.simple_type.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            task_types,
            vec![
                "AnalyzeBook",
                "FindDuplicatePagesToDelete",
                "RefreshSeriesMetadata",
                "RefreshBookMetadata",
                "RefreshSeriesMetadata",
            ],
        );

        let refresh_book_tasks = tasks
            .iter()
            .filter(|task| task.simple_type == "RefreshBookMetadata")
            .collect::<Vec<_>>();
        assert_eq!(refresh_book_tasks.len(), 1);
        assert_eq!(refresh_book_tasks[0].group.as_deref(), Some("series-1"));

        let payload = RefreshBookMetadataPayload::from_task_record(refresh_book_tasks[0], "book-1")
            .expect("refresh book metadata task should use application payload contract");
        assert_eq!(
            payload.capabilities.as_deref(),
            Some(&["TITLE".to_string()][..])
        );

        pool.close().await;
        let _ = std::fs::remove_file(db_path);
    }
}

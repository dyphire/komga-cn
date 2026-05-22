mod scan_diff;
mod scan_discovery;
mod scan_models;
mod scan_persist;
mod scan_restore;
mod scan_sse;
mod sidecars;

pub(crate) use scan_models::LibraryScanResult;
pub(super) use sidecars::enqueue_sidecar_refresh_tasks;

use sqlx::SqlitePool;

use komga_application::task_processing::TaskProcessingError;

use scan_diff::scan_library;
use scan_persist::ScannedLibraryPersistence;

/// Owns the "scan a library" capability.
/// Single entry point hides FS walking, DB diffing, persistence, SSE emission,
/// and post-scan trash checks behind one `execute()` call.
pub(crate) struct LibraryScanner {
    pool: SqlitePool,
}

impl LibraryScanner {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Scan filesystem → diff against DB → persist changes → emit SSE → check trash.
    pub async fn execute(
        &self,
        library_id: &str,
        deep_scan: bool,
    ) -> Result<LibraryScanResult, TaskProcessingError> {
        let scan = scan_library(&self.pool, library_id, deep_scan)
            .await
            .map_err(|error| TaskProcessingError::runtime(format!("scan library: {error}")))?;

        let persistence = ScannedLibraryPersistence::new(&self.pool, library_id, &scan)
            .execute()
            .await
            .map_err(|error| {
                TaskProcessingError::runtime(format!("persist scanned library: {error}"))
            })?;

        Ok(LibraryScanResult {
            book_ids: scan.book_ids,
            series_rows: scan.series_rows,
            sidecars: scan.sidecars,
            changed_sidecar_urls: persistence.changed_sidecar_urls,
            renumbered_book_ids: persistence.renumbered_book_ids,
            changed_series_ids: persistence.changed_series_ids,
            book_metadata_refreshes: persistence.book_metadata_refreshes,
            should_empty_trash: persistence.should_empty_trash,
        })
    }
}

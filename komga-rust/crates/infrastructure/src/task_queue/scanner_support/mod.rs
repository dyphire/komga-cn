mod scan_core;
mod scan_models;
mod scan_sse;
mod sidecars;

pub(crate) use scan_models::LibraryScanResult;
pub(super) use sidecars::enqueue_sidecar_refresh_tasks;

use sqlx::SqlitePool;

use komga_application::task_processing::TaskProcessingError;

use scan_core::{
    library_empty_trash_after_scan, load_changed_sidecars, persist_scanned_library, scan_library,
};

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

        let changed_sidecar_urls = load_changed_sidecars(&self.pool, library_id, &scan.sidecars)
            .await
            .map_err(|error| {
                TaskProcessingError::runtime(format!("load changed sidecars: {error}"))
            })?;

        let outcome = persist_scanned_library(&self.pool, library_id, &scan)
            .await
            .map_err(|error| {
                TaskProcessingError::runtime(format!("persist scanned library: {error}"))
            })?;

        let should_empty_trash = library_empty_trash_after_scan(&self.pool, library_id)
            .await
            .map_err(|error| {
                TaskProcessingError::runtime(format!("load post-scan trash state: {error}"))
            })?;

        Ok(LibraryScanResult {
            book_ids: scan.book_ids,
            series_rows: scan.series_rows,
            sidecars: scan.sidecars,
            changed_sidecar_urls,
            renumbered_book_ids: outcome.renumbered_book_ids,
            changed_series_ids: outcome.changed_series_ids,
            book_metadata_refreshes: outcome.book_metadata_refreshes,
            should_empty_trash,
        })
    }
}

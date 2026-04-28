use std::path::Path;

use super::*;
use crate::tasks::scanner::{
    BookMetadataRefreshRequest, library_empty_trash_after_scan, load_changed_sidecars,
    persist_scanned_library, scan_library,
};

pub(crate) struct ExecutedLibraryScan {
    pub(crate) scan: ScannedLibrary,
    pub(crate) changed_sidecar_urls: Vec<String>,
    pub(crate) renumbered_book_ids: Vec<String>,
    pub(crate) changed_series_ids: Vec<String>,
    pub(crate) book_metadata_refreshes: Vec<BookMetadataRefreshRequest>,
    pub(crate) should_empty_trash: bool,
}

pub(crate) async fn execute_scan_orchestration(
    database_file: &Path,
    library_id: &str,
    deep_scan: bool,
) -> Result<ExecutedLibraryScan, TaskProcessingError> {
    let scan = scan_library(database_file, library_id, deep_scan)
        .await
        .map_err(|error| TaskProcessingError::runtime(format!("scan library: {error}")))?;
    let changed_sidecar_urls = load_changed_sidecars(database_file, library_id, &scan.sidecars)
        .await
        .map_err(|error| TaskProcessingError::runtime(format!("load changed sidecars: {error}")))?;
    let outcome = persist_scanned_library(database_file, library_id, &scan)
        .await
        .map_err(|error| {
            TaskProcessingError::runtime(format!("persist scanned library: {error}"))
        })?;
    let should_empty_trash = library_empty_trash_after_scan(database_file, library_id)
        .await
        .map_err(|error| {
            TaskProcessingError::runtime(format!("load post-scan trash state: {error}"))
        })?;

    Ok(ExecutedLibraryScan {
        scan,
        changed_sidecar_urls,
        renumbered_book_ids: outcome.renumbered_book_ids,
        changed_series_ids: outcome.changed_series_ids,
        book_metadata_refreshes: outcome.book_metadata_refreshes,
        should_empty_trash,
    })
}

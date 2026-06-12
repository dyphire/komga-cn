use std::collections::HashSet;

use super::scan_sse::RuntimeSseRecord;

/// Unified output of a complete library scan cycle.
/// Contains everything the pipeline needs to decide follow-up tasks,
/// without coupling to task-kind knowledge.
#[derive(Clone, Debug)]
pub(in crate::task_queue) struct LibraryScanResult {
    pub(in crate::task_queue) book_ids: Vec<String>,
    pub(in crate::task_queue) series_rows: Vec<ScannedSeriesRow>,
    pub(in crate::task_queue) sidecars: Vec<ScannedSidecarRow>,
    pub(in crate::task_queue) changed_sidecar_urls: Vec<String>,
    pub(in crate::task_queue) renumbered_book_ids: Vec<String>,
    pub(in crate::task_queue) changed_series_ids: Vec<String>,
    pub(in crate::task_queue) book_metadata_refreshes: Vec<BookMetadataRefreshRequest>,
    pub(in crate::task_queue) should_empty_trash: bool,
}

#[derive(Clone, Debug)]
pub(in crate::task_queue) struct LibraryScanConfig {
    pub(in crate::task_queue) root: String,
    pub(in crate::task_queue) scan_cbx: bool,
    pub(in crate::task_queue) scan_pdf: bool,
    pub(in crate::task_queue) scan_epub: bool,
    pub(in crate::task_queue) scan_force_modified_time: bool,
    pub(in crate::task_queue) oneshots_directory: Option<String>,
    pub(in crate::task_queue) scan_directory_exclusions: Vec<String>,
}

#[derive(Clone, Debug)]
pub(in crate::task_queue) struct ScannedLibrary {
    pub(in crate::task_queue) root_available: bool,
    pub(in crate::task_queue) series_rows: Vec<ScannedSeriesRow>,
    pub(in crate::task_queue) sidecars: Vec<ScannedSidecarRow>,
    pub(in crate::task_queue) book_ids: Vec<String>,
    pub(in crate::task_queue) changed_existing_book_ids: HashSet<String>,
    pub(in crate::task_queue) series_ids_requiring_book_sync: HashSet<String>,
    pub(in crate::task_queue) discovered_series_ids: HashSet<String>,
    pub(in crate::task_queue) discovered_book_ids: HashSet<String>,
}

#[derive(Clone, Debug)]
pub(in crate::task_queue) struct ExistingScannedBookRow {
    pub(in crate::task_queue) book_id: String,
    pub(in crate::task_queue) series_id: String,
    pub(in crate::task_queue) file_last_modified_unix_seconds: i64,
}

#[derive(Clone, Debug)]
pub(in crate::task_queue) struct ExistingScannedSeriesRow {
    pub(in crate::task_queue) file_last_modified_unix_seconds: i64,
}

#[derive(Clone, Debug)]
pub(super) struct PersistedScannedSeriesBookRow {
    pub(super) book_id: String,
    pub(super) book_name: String,
    pub(super) book_number: i64,
    pub(super) metadata_number: String,
    pub(super) metadata_number_sort: f64,
    pub(super) metadata_number_lock: bool,
    pub(super) metadata_number_sort_lock: bool,
}

#[derive(Clone, Debug)]
pub(in crate::task_queue) struct ScannedSeriesRow {
    pub(in crate::task_queue) series_id: String,
    pub(in crate::task_queue) series_name: String,
    pub(in crate::task_queue) series_url: String,
    pub(in crate::task_queue) series_last_modified_unix_seconds: i64,
    pub(in crate::task_queue) oneshot: bool,
    pub(in crate::task_queue) books: Vec<ScannedBookRow>,
}

#[derive(Clone, Debug)]
pub(in crate::task_queue) struct ScannedBookRow {
    pub(in crate::task_queue) book_id: String,
    pub(in crate::task_queue) book_name: String,
    pub(in crate::task_queue) book_url: String,
    pub(in crate::task_queue) file_name: String,
    pub(in crate::task_queue) file_size: i64,
    pub(in crate::task_queue) file_last_modified_unix_seconds: i64,
    pub(in crate::task_queue) oneshot: bool,
}

#[derive(Clone, Debug)]
pub(in crate::task_queue) struct ScannedSidecarRow {
    pub(in crate::task_queue) url: String,
    pub(in crate::task_queue) parent_url: String,
    pub(in crate::task_queue) last_modified_unix_seconds: i64,
    pub(in crate::task_queue) source: ScannedSidecarSource,
    pub(in crate::task_queue) sidecar_type: ScannedSidecarType,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::task_queue) enum ScannedSidecarSource {
    Series,
    Book,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::task_queue) enum ScannedSidecarType {
    Metadata,
    Artwork,
}

#[derive(Clone, Debug)]
pub(super) struct InsertedBookCandidate {
    pub(super) book_id: String,
    pub(super) book_url: String,
    pub(super) file_size: i64,
    pub(super) series_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::task_queue) struct BookMetadataRefreshRequest {
    pub(in crate::task_queue) book_id: String,
    pub(in crate::task_queue) series_id: String,
    pub(in crate::task_queue) capabilities: Vec<String>,
}

#[derive(Clone, Debug)]
pub(super) struct InsertedSeriesCandidate {
    pub(super) series_id: String,
    pub(super) series_title: String,
    pub(super) books: Vec<InsertedBookCandidate>,
}

pub(super) struct PersistScannedLibraryOutcome {
    pub(super) renumbered_book_ids: Vec<String>,
    pub(super) library_changed: bool,
    pub(super) changed_series_ids: Vec<String>,
    pub(super) book_metadata_refreshes: Vec<BookMetadataRefreshRequest>,
    pub(super) runtime_events: Vec<RuntimeSseRecord>,
}

#[derive(Clone, Debug)]
pub(super) struct RestoredBookMatches {
    pub(super) series_ids: Vec<String>,
    pub(super) book_metadata_refreshes: Vec<BookMetadataRefreshRequest>,
}

#[derive(Clone, Debug)]
pub(super) struct RestoredSeriesMatch {
    pub(super) inserted_series_id: String,
    pub(super) deleted_series_id: String,
}

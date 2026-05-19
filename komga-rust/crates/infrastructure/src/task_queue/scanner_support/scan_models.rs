use std::collections::HashSet;

use super::scan_sse::RuntimeSseRecord;

/// Unified output of a complete library scan cycle.
/// Contains everything the pipeline needs to decide follow-up tasks,
/// without coupling to task-kind knowledge.
#[derive(Clone, Debug)]
pub(crate) struct LibraryScanResult {
    pub book_ids: Vec<String>,
    pub series_rows: Vec<ScannedSeriesRow>,
    pub sidecars: Vec<ScannedSidecarRow>,
    pub changed_sidecar_urls: Vec<String>,
    pub renumbered_book_ids: Vec<String>,
    pub changed_series_ids: Vec<String>,
    pub book_metadata_refreshes: Vec<BookMetadataRefreshRequest>,
    pub should_empty_trash: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct LibraryScanConfig {
    pub root: String,
    pub scan_cbx: bool,
    pub scan_pdf: bool,
    pub scan_epub: bool,
    pub scan_force_modified_time: bool,
    pub oneshots_directory: Option<String>,
    pub scan_directory_exclusions: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct ScannedLibrary {
    pub root_available: bool,
    pub series_rows: Vec<ScannedSeriesRow>,
    pub sidecars: Vec<ScannedSidecarRow>,
    pub book_ids: Vec<String>,
    pub changed_existing_book_ids: HashSet<String>,
    pub series_ids_requiring_book_sync: HashSet<String>,
    pub discovered_series_ids: HashSet<String>,
    pub discovered_book_ids: HashSet<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct ExistingScannedBookRow {
    pub(crate) book_id: String,
    pub(crate) series_id: String,
    pub(crate) file_last_modified_unix_seconds: i64,
}

#[derive(Clone, Debug)]
pub(crate) struct ExistingScannedSeriesRow {
    pub(crate) file_last_modified_unix_seconds: i64,
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
pub(crate) struct ScannedSeriesRow {
    pub series_id: String,
    pub series_name: String,
    pub series_url: String,
    pub series_last_modified_unix_seconds: i64,
    pub oneshot: bool,
    pub books: Vec<ScannedBookRow>,
}

#[derive(Clone, Debug)]
pub(crate) struct ScannedBookRow {
    pub book_id: String,
    pub book_name: String,
    pub book_url: String,
    pub file_name: String,
    pub file_size: i64,
    pub file_last_modified_unix_seconds: i64,
    pub oneshot: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ScannedSidecarRow {
    pub url: String,
    pub parent_url: String,
    pub last_modified_unix_seconds: i64,
    pub source: ScannedSidecarSource,
    pub sidecar_type: ScannedSidecarType,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ScannedSidecarSource {
    Series,
    Book,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ScannedSidecarType {
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
pub(crate) struct BookMetadataRefreshRequest {
    pub(crate) book_id: String,
    pub(crate) series_id: String,
    pub(crate) capabilities: Vec<String>,
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
pub(super) struct RestoredSeriesMatch {
    pub(super) inserted_series_id: String,
    pub(super) deleted_series_id: String,
}

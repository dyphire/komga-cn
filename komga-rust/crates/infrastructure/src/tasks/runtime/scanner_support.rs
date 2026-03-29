use super::*;
pub(super) use crate::tasks::{
    ScannedLibrary, ScannedSidecarSource, ScannedSidecarType, library_empty_trash_after_scan,
    load_changed_sidecars, persist_scanned_library,
};

#[path = "scanner_support/sidecars.rs"]
mod sidecars;

pub(super) use sidecars::enqueue_sidecar_refresh_tasks;

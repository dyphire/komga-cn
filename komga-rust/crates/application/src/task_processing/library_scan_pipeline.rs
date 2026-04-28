use std::collections::HashMap;
use std::time::Duration;

use super::contracts::{TaskProcessingError, TaskQueueRecord};
use super::protocol::LibraryTaskBatch;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScanSchedulingTrigger {
    Startup,
    Tick,
}

#[derive(Clone, Debug, Default)]
pub struct LibraryScanScheduleState {
    pub elapsed_since_last_run_by_library: HashMap<String, Duration>,
}

impl LibraryScanScheduleState {
    pub fn mark_elapsed(&mut self, library_id: impl Into<String>, elapsed: Duration) {
        self.elapsed_since_last_run_by_library
            .insert(library_id.into(), elapsed);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanOneLibrary {
    pub library_id: String,
    pub deep_scan: bool,
}

impl ScanOneLibrary {
    pub fn new(library_id: impl Into<String>, deep_scan: bool) -> Self {
        Self {
            library_id: library_id.into(),
            deep_scan,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScanOneLibraryOutcome {
    Executed,
    SkippedExternalOwned,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanOneLibraryResult {
    pub library_id: String,
    pub outcome: ScanOneLibraryOutcome,
    pub follow_up_tasks: Vec<TaskQueueRecord>,
}

impl ScanOneLibraryResult {
    pub fn executed(library_id: impl Into<String>, follow_up_tasks: Vec<TaskQueueRecord>) -> Self {
        Self {
            library_id: library_id.into(),
            outcome: ScanOneLibraryOutcome::Executed,
            follow_up_tasks,
        }
    }

    pub fn skipped_external_owned(library_id: impl Into<String>) -> Self {
        Self {
            library_id: library_id.into(),
            outcome: ScanOneLibraryOutcome::SkippedExternalOwned,
            follow_up_tasks: Vec::new(),
        }
    }
}

#[allow(async_fn_in_trait)]
pub trait LibraryScanPipeline {
    async fn schedule(
        &self,
        trigger: ScanSchedulingTrigger,
        state: &LibraryScanScheduleState,
    ) -> Result<LibraryTaskBatch, TaskProcessingError>;

    async fn run(
        &self,
        request: ScanOneLibrary,
    ) -> Result<ScanOneLibraryResult, TaskProcessingError>;
}

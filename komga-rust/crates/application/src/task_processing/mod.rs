mod contracts;
mod library_scan_pipeline;
mod persisted_task_row;
mod protocol;
mod runtime_policies;
mod runtime_task_request;
mod task_engine;
mod task_records;
#[cfg(test)]
mod task_records_tests;
mod task_registry;

pub use crate::library_catalog::LibraryScanInterval;
pub use contracts::{
    TaskExecutionFinalizationPort, TaskExecutionOutcome, TaskExecutionResult, TaskProcessingError,
    TaskQueueOrchestrator, TaskQueueRecord, finalize_task_execution,
};
pub use library_scan_pipeline::{
    LibraryScanPipeline, LibraryScanProfile, LibraryScanScheduleState, ScanOneLibrary,
    ScanOneLibraryOutcome, ScanOneLibraryResult, ScanSchedulingTrigger, ScheduledLibraryScanBatch,
    ScheduledLibraryScanTask,
};
pub use persisted_task_row::PersistedTaskRowShape;
pub use protocol::{
    BookSeriesRef, LibraryTaskBatch, LibraryTaskCommand, TaskSchedule, emit_library_task_batch,
};
pub use runtime_policies::{CleanupEmptySetsPolicy, ThumbnailRegenerationPolicy};
pub use runtime_task_request::RuntimeTaskRequest;
pub use task_engine::{QueueStatus, SubmitUrgency, TaskQueue, TaskQueueAdmin};
pub use task_records::{
    book_analyze_task_record, book_metadata_refresh_task_records, series_analyze_task_records,
    series_metadata_refresh_task_records,
};
pub use task_registry::{
    BookPayload, FindBookThumbnailsToRegeneratePayload, HashedPageToDeletePayload,
    ImportBookCopyMode, ImportBookPayload, LibraryPayload, RebuildIndexEntity, RebuildIndexPayload,
    RefreshBookMetadataPayload, RemoveHashedPagesPayload, ScanLibraryPayload, SeriesPayload,
    TaskKind, TaskParseError, TaskPayload, TaskRequest, TaskTypeMetadata,
};

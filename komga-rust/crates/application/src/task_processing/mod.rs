mod contracts;
mod library_scan_pipeline;
mod protocol;
mod scanner_maintenance;
mod task_engine;
mod task_registry;

pub use contracts::{
    LibraryScanInterval, TaskExecutionFinalizationPort, TaskExecutionOutcome, TaskExecutionResult,
    TaskProcessingError, TaskQueueOrchestrator, TaskQueueRecord, finalize_task_execution,
};
pub use library_scan_pipeline::{
    LibraryScanPipeline, LibraryScanScheduleState, ScanOneLibrary, ScanOneLibraryOutcome,
    ScanOneLibraryResult, ScanSchedulingTrigger, ScheduledLibraryScanBatch,
    ScheduledLibraryScanTask,
};
pub use protocol::{
    BookSeriesRef, LibraryTaskBatch, LibraryTaskCommand, OpaqueTask, PersistedTaskRowShape,
    TaskSchedule, emit_library_task_batch,
};
pub use scanner_maintenance::{
    LibraryScanProfile, NormalizedLibraryScanProfile, library_scan_interval_from_db,
    normalize_library_scan_profiles,
};
pub use task_engine::{QueueStatus, SubmitUrgency, TaskQueue, TaskQueueAdmin};
pub use task_registry::{
    BookPayload, HashedPageToDeletePayload, LibraryPayload, RefreshBookMetadataPayload,
    RemoveHashedPagesPayload, ScanLibraryPayload, SeriesPayload, TaskKind, TaskParseError,
    TaskPayload, TaskRequest, TaskTypeMetadata,
};

mod contracts;
mod library_scan_pipeline;
mod protocol;
mod scanner_maintenance;
mod task_engine;
mod task_registry;

pub use contracts::{
    LibraryScanInterval, TaskProcessingError, TaskQueueOrchestrator, TaskQueueRecord,
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
    BookPayload, LibraryPayload, RefreshBookMetadataPayload, ScanLibraryPayload, SeriesPayload,
    TaskKind, TaskParseError, TaskPayload, TaskRequest, TaskTypeMetadata,
};

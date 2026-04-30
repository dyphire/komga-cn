mod contracts;
mod library_scan_pipeline;
mod protocol;
mod scanner_maintenance;
mod task_kind;

pub use contracts::{
    LibraryScanInterval, TaskProcessingError, TaskQueueAdminPort, TaskQueueExecutionPort,
    TaskQueueOrchestrator, TaskQueueRecord, TaskQueueRepository,
};
pub use library_scan_pipeline::{
    LibraryScanPipeline, LibraryScanScheduleState, ScanOneLibrary, ScanOneLibraryOutcome,
    ScanOneLibraryResult, ScanSchedulingTrigger,
};
pub use protocol::{
    BookSeriesRef, LibraryTaskBatch, LibraryTaskCommand, OpaqueTask, PersistedTaskRowShape,
    TaskSchedule, emit_library_task_batch,
};
pub use scanner_maintenance::{
    LibraryScanProfile, NormalizedLibraryScanProfile, library_scan_interval_from_db,
    normalize_library_scan_profiles,
};
pub use task_kind::{
    BookPayload, LibraryPayload, RefreshBookMetadataPayload, ScanLibraryPayload, SeriesPayload,
    TaskDefinition, TaskKind, TaskNameFormat, TaskParseError, TaskPayload, TaskRequest,
};

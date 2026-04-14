mod contracts;
mod library_scan_pipeline;
mod protocol;
mod scanner_maintenance;

pub use contracts::{
    LibraryScanInterval, TaskProcessingError, TaskQueueAdminPort, TaskQueueExecutionPort,
    TaskQueueOrchestrator, TaskQueueRecord, TaskQueueRepository, TaskRuntimeConfig,
    TaskRuntimeContext,
};
pub use library_scan_pipeline::{
    LibraryScanPipeline, LibraryScanScheduleState, ScanOneLibrary, ScanOneLibraryOutcome,
    ScanOneLibraryResult, ScanSchedulingTrigger,
};
pub use protocol::{
    BookSeriesRef, DefaultLibraryTaskEmitter, DefaultTaskProtocolCatalog, LibraryTaskBatch,
    LibraryTaskCommand, LibraryTaskEmitter, OpaqueTask, PersistedTaskRowShape, PlannedTask,
    PlannedTaskKind, TaskDescriptor, TaskProtocolCatalog, TaskSchedule,
};
pub use scanner_maintenance::{
    LibraryScanProfile, NormalizedLibraryScanProfile, library_scan_interval_from_db,
    normalize_library_scan_profiles,
};

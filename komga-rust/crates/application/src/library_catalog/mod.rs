mod catalog_port;
mod commands;
#[cfg(test)]
mod commands_tests;
mod models;
mod queries;
mod repository;
mod task_records;

pub use catalog_port::LibraryCatalogPort;
pub use commands::{CreateLibraryResult, LibraryCatalogCommandService, LibraryTaskResult};
pub use models::{LibraryChangeSet, LibraryRecord, LibraryScanInterval, LibrarySeriesCover};
pub use queries::{LibraryCatalogQueryService, LibraryDetailAccess};
pub use repository::{
    LibraryBookSeriesRecord, LibraryCatalogMutationError, LibraryCatalogMutationPort,
    LibraryCatalogReadPort, LibrarySeriesAndBookIds,
};

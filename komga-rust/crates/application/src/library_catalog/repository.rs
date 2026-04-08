use std::future::Future;

use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext};

use crate::task_processing::TaskQueueRecord;

use super::LibraryRecord;

type LibrarySeriesAndBookIds = Option<(Vec<String>, Vec<(String, String)>)>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LibraryCatalogMutationError {
    NotFound,
    Validation(String),
    Persistence(String),
}

impl LibraryCatalogMutationError {
    pub fn persistence(error: impl Into<String>) -> Self {
        Self::Persistence(error.into())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateLibraryResult {
    pub library: LibraryRecord,
    pub task_records: Vec<TaskQueueRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryTaskResult {
    pub task_records: Vec<TaskQueueRecord>,
}

pub trait LibraryCatalogReadPort {
    fn list_libraries(
        &self,
        context: &DiscoveryQueryContext,
    ) -> impl Future<Output = Result<Vec<LibraryRecord>, DiscoveryError>>;

    fn get_library(
        &self,
        context: &DiscoveryQueryContext,
        library_id: &str,
    ) -> impl Future<Output = Result<Option<LibraryRecord>, DiscoveryError>>;
}

pub trait LibraryCatalogMutationPort {
    fn load_library(
        &self,
        library_id: &str,
    ) -> impl Future<Output = Result<Option<LibraryRecord>, String>>;

    fn validate_library(&self, library: &LibraryRecord)
    -> impl Future<Output = Result<(), String>>;

    fn create_library(&self, library: &LibraryRecord) -> impl Future<Output = Result<(), String>>;

    fn update_library(&self, library: &LibraryRecord)
    -> impl Future<Output = Result<bool, String>>;

    fn delete_library(&self, library_id: &str) -> impl Future<Output = Result<bool, String>>;

    fn library_book_ids_with_empty_hash(
        &self,
        library_id: &str,
        koreader: bool,
    ) -> impl Future<Output = Result<Vec<String>, String>>;

    fn library_books_with_mismatched_extensions(
        &self,
        library_id: &str,
    ) -> impl Future<Output = Result<Vec<(String, String)>, String>>;

    fn library_book_ids(
        &self,
        library_id: &str,
    ) -> impl Future<Output = Result<Option<Vec<String>>, String>>;

    fn library_series_and_book_ids(
        &self,
        library_id: &str,
    ) -> impl Future<Output = Result<LibrarySeriesAndBookIds, String>>;
}

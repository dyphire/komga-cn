use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext};

use super::LibraryRecord;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryBookSeriesRecord {
    pub book_id: String,
    pub series_id: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LibrarySeriesAndBookIds {
    pub series_ids: Vec<String>,
    pub books: Vec<LibraryBookSeriesRecord>,
}

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

#[async_trait::async_trait]
pub trait LibraryCatalogReadPort: Send + Sync {
    async fn list_libraries(
        &self,
        context: &DiscoveryQueryContext,
    ) -> Result<Vec<LibraryRecord>, DiscoveryError>;

    async fn get_library(
        &self,
        context: &DiscoveryQueryContext,
        library_id: &str,
    ) -> Result<Option<LibraryRecord>, DiscoveryError>;
}

#[async_trait::async_trait]
pub trait LibraryCatalogMutationPort: Send + Sync {
    async fn load_library(&self, library_id: &str) -> Result<Option<LibraryRecord>, String>;

    async fn validate_library(&self, library: &LibraryRecord) -> Result<(), String>;

    async fn create_library(&self, library: &LibraryRecord) -> Result<(), String>;

    async fn update_library(&self, library: &LibraryRecord) -> Result<bool, String>;

    async fn delete_library(&self, library_id: &str) -> Result<bool, String>;

    async fn library_book_ids_with_empty_hash(
        &self,
        library_id: &str,
        koreader: bool,
    ) -> Result<Vec<String>, String>;

    async fn library_books_with_mismatched_extensions(
        &self,
        library_id: &str,
    ) -> Result<Vec<LibraryBookSeriesRecord>, String>;

    async fn library_book_ids(&self, library_id: &str) -> Result<Option<Vec<String>>, String>;

    async fn library_series_and_book_ids(
        &self,
        library_id: &str,
    ) -> Result<Option<LibrarySeriesAndBookIds>, String>;
}

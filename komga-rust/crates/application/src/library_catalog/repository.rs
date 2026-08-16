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

#[derive(Debug)]
pub enum LibraryCatalogMutationError {
    NotFound,
    Validation(String),
    Persistence(anyhow::Error),
}

impl LibraryCatalogMutationError {
    pub fn persistence(error: anyhow::Error) -> Self {
        Self::Persistence(error)
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
    async fn load_library(&self, library_id: &str) -> anyhow::Result<Option<LibraryRecord>>;

    async fn validate_library(&self, library: &LibraryRecord) -> anyhow::Result<()>;

    async fn create_library(&self, library: &LibraryRecord) -> anyhow::Result<()>;

    async fn update_library(&self, library: &LibraryRecord) -> anyhow::Result<bool>;

    async fn delete_library(&self, library_id: &str) -> anyhow::Result<bool>;

    async fn library_book_ids_with_empty_hash(
        &self,
        library_id: &str,
        koreader: bool,
    ) -> anyhow::Result<Vec<String>>;

    async fn library_books_with_mismatched_extensions(
        &self,
        library_id: &str,
    ) -> anyhow::Result<Vec<LibraryBookSeriesRecord>>;

    async fn library_book_ids(&self, library_id: &str) -> anyhow::Result<Option<Vec<String>>>;

    async fn library_series_and_book_ids(
        &self,
        library_id: &str,
    ) -> anyhow::Result<Option<LibrarySeriesAndBookIds>>;
}

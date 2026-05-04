use super::read_models::{BookReadModel, SeriesReadModel};
use async_trait::async_trait;
use komga_domain::discovery::{
    BookFilter, BookSort, DiscoveryError, DiscoveryQueryContext, PageEnvelope, SeriesFilter,
    SeriesSort,
};

#[derive(Clone, Debug)]
pub struct SeriesBrowseQuery {
    pub filter: SeriesFilter,
    pub sort: Vec<SeriesSort>,
    pub search: Option<String>,
    pub page: usize,
    pub size: usize,
    pub unpaged: bool,
}

impl Default for SeriesBrowseQuery {
    fn default() -> Self {
        Self {
            filter: SeriesFilter { condition: None },
            sort: vec![],
            search: None,
            page: 0,
            size: 20,
            unpaged: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BooksBrowseQuery {
    pub filter: BookFilter,
    pub sort: Vec<BookSort>,
    pub search: Option<String>,
    pub page: usize,
    pub size: usize,
    pub unpaged: bool,
}

impl Default for BooksBrowseQuery {
    fn default() -> Self {
        Self {
            filter: BookFilter {
                condition: None,
                direct_browse_book_id: None,
            },
            sort: vec![],
            search: None,
            page: 0,
            size: 20,
            unpaged: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BooksFeedQuery {
    pub library_ids: Option<Vec<String>>,
    pub page: usize,
    pub size: usize,
    pub unpaged: bool,
}

#[derive(Clone, Debug)]
pub enum BookTagScope {
    All,
    Series(String),
    Libraries(Vec<String>),
    ReadList(String),
}

#[async_trait]
pub trait DiscoveryListService: Send + Sync {
    async fn list_series(
        &self,
        context: &DiscoveryQueryContext,
        query: SeriesBrowseQuery,
    ) -> Result<PageEnvelope<SeriesReadModel>, DiscoveryError>;

    async fn list_books(
        &self,
        context: &DiscoveryQueryContext,
        query: BooksBrowseQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError>;

    async fn list_books_latest(
        &self,
        context: &DiscoveryQueryContext,
        query: BooksFeedQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError>;

    async fn list_series_alphabetical_groups(
        &self,
        context: &DiscoveryQueryContext,
        filter: SeriesFilter,
        search: Option<String>,
    ) -> Result<Vec<serde_json::Value>, DiscoveryError>;

    async fn list_genres(
        &self,
        context: &DiscoveryQueryContext,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError>;

    async fn list_tags(
        &self,
        context: &DiscoveryQueryContext,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError>;

    async fn list_languages(
        &self,
        context: &DiscoveryQueryContext,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError>;

    async fn list_publishers(
        &self,
        context: &DiscoveryQueryContext,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError>;

    async fn list_age_ratings(
        &self,
        context: &DiscoveryQueryContext,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError>;

    async fn list_sharing_labels(
        &self,
        context: &DiscoveryQueryContext,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError>;

    async fn list_series_tags(
        &self,
        context: &DiscoveryQueryContext,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError>;

    async fn list_series_release_dates(
        &self,
        context: &DiscoveryQueryContext,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError>;

    async fn list_book_tags(
        &self,
        context: &DiscoveryQueryContext,
        scope: Option<BookTagScope>,
        library_ids: Option<Vec<String>>,
    ) -> Result<Vec<String>, DiscoveryError>;
}

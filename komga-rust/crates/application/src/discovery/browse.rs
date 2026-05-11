use super::read_models::{BookReadModel, SeriesReadModel};
use async_trait::async_trait;
use komga_domain::discovery::{
    BookFilter, BookSort, DiscoveryError, DiscoveryQueryContext, PageEnvelope, SeriesFilter,
    SeriesSort,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageRequest {
    pub page: usize,
    pub size: usize,
    pub unpaged: bool,
}

impl Default for PageRequest {
    fn default() -> Self {
        Self {
            page: 0,
            size: 20,
            unpaged: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SeriesBrowseRequest {
    pub filter: SeriesFilter,
    pub sort: Vec<SeriesSort>,
    pub search: Option<String>,
    pub page: PageRequest,
}

impl Default for SeriesBrowseRequest {
    fn default() -> Self {
        Self {
            filter: SeriesFilter { condition: None },
            sort: vec![],
            search: None,
            page: PageRequest::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct BooksBrowseRequest {
    pub filter: BookFilter,
    pub sort: Vec<BookSort>,
    pub search: Option<String>,
    pub page: PageRequest,
}

impl Default for BooksBrowseRequest {
    fn default() -> Self {
        Self {
            filter: BookFilter {
                condition: None,
                direct_browse_book_id: None,
            },
            sort: vec![],
            search: None,
            page: PageRequest::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct LatestBooksRequest {
    pub library_ids: Option<Vec<String>>,
    pub page: PageRequest,
}

#[derive(Clone, Debug)]
pub struct SeriesAlphabeticalGroupsRequest {
    pub filter: SeriesFilter,
    pub search: Option<String>,
}

#[derive(Clone, Debug)]
pub enum BookTagScope {
    All,
    Series(String),
    Libraries(Vec<String>),
    ReadList(String),
}

#[async_trait]
pub trait DiscoveryBrowseService: Send + Sync {
    async fn list_series(
        &self,
        context: &DiscoveryQueryContext,
        request: SeriesBrowseRequest,
    ) -> Result<PageEnvelope<SeriesReadModel>, DiscoveryError>;

    async fn list_books(
        &self,
        context: &DiscoveryQueryContext,
        request: BooksBrowseRequest,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError>;

    async fn list_latest_books(
        &self,
        context: &DiscoveryQueryContext,
        request: LatestBooksRequest,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError>;

    async fn list_series_alphabetical_groups(
        &self,
        context: &DiscoveryQueryContext,
        request: SeriesAlphabeticalGroupsRequest,
    ) -> Result<Vec<serde_json::Value>, DiscoveryError>;
}

#[async_trait]
pub trait DiscoveryFacetService: Send + Sync {
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

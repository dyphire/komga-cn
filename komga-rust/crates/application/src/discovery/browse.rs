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

impl From<SeriesBrowseQuery> for SeriesBrowseRequest {
    fn from(query: SeriesBrowseQuery) -> Self {
        Self {
            filter: query.filter,
            sort: query.sort,
            search: query.search,
            page: PageRequest {
                page: query.page,
                size: query.size,
                unpaged: query.unpaged,
            },
        }
    }
}

impl From<SeriesBrowseRequest> for SeriesBrowseQuery {
    fn from(request: SeriesBrowseRequest) -> Self {
        Self {
            filter: request.filter,
            sort: request.sort,
            search: request.search,
            page: request.page.page,
            size: request.page.size,
            unpaged: request.page.unpaged,
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

impl From<BooksBrowseQuery> for BooksBrowseRequest {
    fn from(query: BooksBrowseQuery) -> Self {
        Self {
            filter: query.filter,
            sort: query.sort,
            search: query.search,
            page: PageRequest {
                page: query.page,
                size: query.size,
                unpaged: query.unpaged,
            },
        }
    }
}

impl From<BooksBrowseRequest> for BooksBrowseQuery {
    fn from(request: BooksBrowseRequest) -> Self {
        Self {
            filter: request.filter,
            sort: request.sort,
            search: request.search,
            page: request.page.page,
            size: request.page.size,
            unpaged: request.page.unpaged,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LatestBooksRequest {
    pub library_ids: Option<Vec<String>>,
    pub page: PageRequest,
}

#[derive(Clone, Debug)]
pub struct BooksFeedQuery {
    pub library_ids: Option<Vec<String>>,
    pub page: usize,
    pub size: usize,
    pub unpaged: bool,
}

impl From<BooksFeedQuery> for LatestBooksRequest {
    fn from(query: BooksFeedQuery) -> Self {
        Self {
            library_ids: query.library_ids,
            page: PageRequest {
                page: query.page,
                size: query.size,
                unpaged: query.unpaged,
            },
        }
    }
}

impl From<LatestBooksRequest> for BooksFeedQuery {
    fn from(request: LatestBooksRequest) -> Self {
        Self {
            library_ids: request.library_ids,
            page: request.page.page,
            size: request.page.size,
            unpaged: request.page.unpaged,
        }
    }
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

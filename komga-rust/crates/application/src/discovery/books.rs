use komga_domain::discovery::{
    DirectBrowseBooksListFamily, DiscoveryError, DiscoveryQueryContext, PageEnvelope,
};

use super::query_service::{DiscoveryQueries, DiscoveryQueryRepository};
use super::read_models::{
    BookDetailReadModel, BookReadModel, BookResourceReadModel, ReadListReadModel,
};
use super::request_shape::classify_book_sorts;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BooksListQuery {
    pub page: usize,
    pub size: usize,
    pub unpaged: bool,
    pub direct_browse_family: Option<DirectBrowseBooksListFamily>,
    pub library_ids: Option<Vec<String>>,
    pub series_ids: Option<Vec<String>>,
    pub deleted: Option<bool>,
    pub oneshot: Option<bool>,
    pub tags: Option<Vec<String>>,
    pub read_statuses: Option<Vec<String>>,
    pub media_profiles: Option<Vec<String>>,
    pub media_statuses: Option<Vec<String>>,
    pub authors: Option<Vec<String>>,
    pub release_dates: Option<Vec<String>>,
    pub sort: Vec<String>,
    pub search: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BooksLatestQuery {
    pub page: usize,
    pub size: usize,
    pub unpaged: bool,
    pub library_ids: Option<Vec<String>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookDetailQuery {
    pub book_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookSiblingQuery {
    pub book_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookReadlistsQuery {
    pub book_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeBooksListQuery {
    pub page: usize,
    pub size: usize,
    pub unpaged: bool,
    pub library_ids: Option<Vec<String>>,
    pub series_ids: Option<Vec<String>>,
    pub deleted: Option<bool>,
    pub oneshot: Option<bool>,
    pub tags: Option<Vec<String>>,
    pub read_statuses: Option<Vec<String>>,
    pub media_profiles: Option<Vec<String>>,
    pub media_statuses: Option<Vec<String>>,
    pub authors: Option<Vec<String>>,
    pub release_dates: Option<Vec<String>>,
    pub sort: Vec<String>,
    pub search: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeBooksLatestQuery {
    pub page: usize,
    pub size: usize,
    pub unpaged: bool,
    pub library_ids: Option<Vec<String>>,
}

impl<R> DiscoveryQueries<R>
where
    R: DiscoveryQueryRepository,
{
    pub async fn list_books(
        &self,
        context: &DiscoveryQueryContext,
        query: BooksListQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        classify_book_sorts(&query.sort)?;
        self.repository
            .list_books(context, runtime_books_list_query(query))
            .await
    }

    pub async fn list_books_latest(
        &self,
        context: &DiscoveryQueryContext,
        query: BooksLatestQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        self.repository
            .list_books_latest(
                context,
                RuntimeBooksLatestQuery {
                    page: query.page,
                    size: query.size,
                    unpaged: query.unpaged,
                    library_ids: query.library_ids,
                },
            )
            .await
    }

    pub async fn resolve_book_resource(
        &self,
        book_id: &str,
    ) -> Result<Option<BookResourceReadModel>, DiscoveryError> {
        self.repository.resolve_book_resource(book_id).await
    }

    pub async fn get_book_detail(
        &self,
        context: &DiscoveryQueryContext,
        query: BookDetailQuery,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
        self.repository.get_book_detail(context, query).await
    }

    pub async fn get_book_sibling_previous(
        &self,
        context: &DiscoveryQueryContext,
        query: BookSiblingQuery,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
        self.repository
            .get_book_sibling_previous(context, query)
            .await
    }

    pub async fn get_book_sibling_next(
        &self,
        context: &DiscoveryQueryContext,
        query: BookSiblingQuery,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
        self.repository.get_book_sibling_next(context, query).await
    }

    pub async fn list_book_readlists(
        &self,
        context: &DiscoveryQueryContext,
        query: BookReadlistsQuery,
    ) -> Result<Vec<ReadListReadModel>, DiscoveryError> {
        self.repository.list_book_readlists(context, query).await
    }
}

pub(crate) fn runtime_books_list_query(query: BooksListQuery) -> RuntimeBooksListQuery {
    RuntimeBooksListQuery {
        page: query.page,
        size: query.size,
        unpaged: query.unpaged,
        library_ids: query.library_ids,
        series_ids: query.series_ids,
        deleted: query.deleted,
        oneshot: query.oneshot,
        tags: query.tags,
        read_statuses: query.read_statuses,
        media_profiles: query.media_profiles,
        media_statuses: query.media_statuses,
        authors: query.authors,
        release_dates: query.release_dates,
        sort: query.sort,
        search: query.search,
    }
}

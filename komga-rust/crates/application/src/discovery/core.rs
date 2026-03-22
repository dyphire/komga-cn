use std::future::Future;

use komga_domain::discovery::{
    BookDetailReadModel, BookReadModel, BookResourceReadModel, CollectionReadModel, DiscoveryError,
    DiscoveryQueryContext, LibraryReadModel, PageEnvelope, ReadListReadModel,
    SeriesDetailReadModel, SeriesReadModel, SeriesResourceReadModel,
};

use super::{
    BookDetailQuery, BookReadlistsQuery, BookSiblingQuery, NativeBooksLatestQuery,
    NativeBooksListQuery, NativeReadListBooksQuery, NativeSeriesListQuery, SeriesCollectionsQuery,
    SeriesDetailQuery,
};

pub trait DiscoveryQueryRepository {
    fn list_libraries(
        &self,
        context: &DiscoveryQueryContext,
    ) -> impl Future<Output = Result<Vec<LibraryReadModel>, DiscoveryError>>;

    fn list_series(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeSeriesListQuery,
    ) -> impl Future<Output = Result<PageEnvelope<SeriesReadModel>, DiscoveryError>>;

    fn list_books(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeBooksListQuery,
    ) -> impl Future<Output = Result<PageEnvelope<BookReadModel>, DiscoveryError>>;

    fn list_books_latest(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeBooksLatestQuery,
    ) -> impl Future<Output = Result<PageEnvelope<BookReadModel>, DiscoveryError>>;

    fn list_readlist_books(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeReadListBooksQuery,
    ) -> impl Future<Output = Result<PageEnvelope<BookReadModel>, DiscoveryError>>;

    fn resolve_series_resource(
        &self,
        series_id: &str,
    ) -> impl Future<Output = Result<Option<SeriesResourceReadModel>, DiscoveryError>>;

    fn get_series_detail(
        &self,
        context: &DiscoveryQueryContext,
        query: SeriesDetailQuery,
    ) -> impl Future<Output = Result<Option<SeriesDetailReadModel>, DiscoveryError>>;

    fn resolve_book_resource(
        &self,
        book_id: &str,
    ) -> impl Future<Output = Result<Option<BookResourceReadModel>, DiscoveryError>>;

    fn get_book_detail(
        &self,
        context: &DiscoveryQueryContext,
        query: BookDetailQuery,
    ) -> impl Future<Output = Result<Option<BookDetailReadModel>, DiscoveryError>>;

    fn get_book_sibling_previous(
        &self,
        context: &DiscoveryQueryContext,
        query: BookSiblingQuery,
    ) -> impl Future<Output = Result<Option<BookDetailReadModel>, DiscoveryError>>;

    fn get_book_sibling_next(
        &self,
        context: &DiscoveryQueryContext,
        query: BookSiblingQuery,
    ) -> impl Future<Output = Result<Option<BookDetailReadModel>, DiscoveryError>>;

    fn list_book_readlists(
        &self,
        context: &DiscoveryQueryContext,
        query: BookReadlistsQuery,
    ) -> impl Future<Output = Result<Vec<ReadListReadModel>, DiscoveryError>>;

    fn list_series_collections(
        &self,
        context: &DiscoveryQueryContext,
        query: SeriesCollectionsQuery,
    ) -> impl Future<Output = Result<Vec<CollectionReadModel>, DiscoveryError>>;
}

pub struct DiscoveryQueries<R> {
    pub(in crate::discovery) repository: R,
}

impl<R> DiscoveryQueries<R>
where
    R: DiscoveryQueryRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }
}

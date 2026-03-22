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
    ) -> Result<Vec<LibraryReadModel>, DiscoveryError>;

    fn list_series(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeSeriesListQuery,
    ) -> Result<PageEnvelope<SeriesReadModel>, DiscoveryError>;

    fn list_books(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeBooksListQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError>;

    fn list_books_latest(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeBooksLatestQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError>;

    fn list_readlist_books(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeReadListBooksQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError>;

    fn resolve_series_resource(
        &self,
        series_id: &str,
    ) -> Result<Option<SeriesResourceReadModel>, DiscoveryError>;

    fn get_series_detail(
        &self,
        context: &DiscoveryQueryContext,
        query: SeriesDetailQuery,
    ) -> Result<Option<SeriesDetailReadModel>, DiscoveryError>;

    fn resolve_book_resource(
        &self,
        book_id: &str,
    ) -> Result<Option<BookResourceReadModel>, DiscoveryError>;

    fn get_book_detail(
        &self,
        context: &DiscoveryQueryContext,
        query: BookDetailQuery,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError>;

    fn get_book_sibling_previous(
        &self,
        context: &DiscoveryQueryContext,
        query: BookSiblingQuery,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError>;

    fn get_book_sibling_next(
        &self,
        context: &DiscoveryQueryContext,
        query: BookSiblingQuery,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError>;

    fn list_book_readlists(
        &self,
        context: &DiscoveryQueryContext,
        query: BookReadlistsQuery,
    ) -> Result<Vec<ReadListReadModel>, DiscoveryError>;

    fn list_series_collections(
        &self,
        context: &DiscoveryQueryContext,
        query: SeriesCollectionsQuery,
    ) -> Result<Vec<CollectionReadModel>, DiscoveryError>;
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

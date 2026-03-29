use std::future::Future;

use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext, PageEnvelope};

use super::read_models::{
    BookDetailReadModel, BookReadModel, BookResourceReadModel, CollectionReadModel,
    LibraryReadModel, ReadListReadModel, SeriesDetailReadModel, SeriesReadModel,
    SeriesResourceReadModel,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoveryRequestValidation {
    Lenient,
    Strict,
}

impl DiscoveryRequestValidation {
    pub fn is_strict(self) -> bool {
        matches!(self, Self::Strict)
    }
}

pub fn query_validation_mode(strict_runtime_shape: bool) -> DiscoveryRequestValidation {
    if strict_runtime_shape {
        DiscoveryRequestValidation::Strict
    } else {
        DiscoveryRequestValidation::Lenient
    }
}

pub fn reject_bootstrap_shape_mismatch(
    strict_runtime_shape: bool,
    has_bootstrap_series_id: bool,
    has_query_parameters: bool,
) -> bool {
    !strict_runtime_shape && has_bootstrap_series_id && has_query_parameters
}

pub fn bootstrap_series_id_for_runtime_shape(
    strict_runtime_shape: bool,
    series_id: Option<String>,
) -> Option<String> {
    if strict_runtime_shape {
        None
    } else {
        series_id
    }
}

pub fn requested_library_ids_for_runtime_shape(
    strict_runtime_shape: bool,
    library_ids: Option<Vec<String>>,
) -> Option<Vec<String>> {
    if strict_runtime_shape {
        library_ids
    } else {
        None
    }
}
use super::{
    BookDetailQuery, BookReadlistsQuery, BookSiblingQuery, ReadListDetailQuery,
    RuntimeBooksLatestQuery, RuntimeBooksListQuery, RuntimeReadListBooksQuery,
    RuntimeReadListsQuery, RuntimeSeriesListQuery, SeriesCollectionsQuery, SeriesDetailQuery,
};

pub trait DiscoveryQueryRepository {
    fn list_libraries(
        &self,
        context: &DiscoveryQueryContext,
    ) -> impl Future<Output = Result<Vec<LibraryReadModel>, DiscoveryError>>;

    fn list_series(
        &self,
        context: &DiscoveryQueryContext,
        query: RuntimeSeriesListQuery,
    ) -> impl Future<Output = Result<PageEnvelope<SeriesReadModel>, DiscoveryError>>;

    fn list_books(
        &self,
        context: &DiscoveryQueryContext,
        query: RuntimeBooksListQuery,
    ) -> impl Future<Output = Result<PageEnvelope<BookReadModel>, DiscoveryError>>;

    fn list_books_latest(
        &self,
        context: &DiscoveryQueryContext,
        query: RuntimeBooksLatestQuery,
    ) -> impl Future<Output = Result<PageEnvelope<BookReadModel>, DiscoveryError>>;

    fn list_readlist_books(
        &self,
        context: &DiscoveryQueryContext,
        query: RuntimeReadListBooksQuery,
    ) -> impl Future<Output = Result<PageEnvelope<BookReadModel>, DiscoveryError>>;

    fn list_readlists(
        &self,
        context: &DiscoveryQueryContext,
        query: RuntimeReadListsQuery,
    ) -> impl Future<Output = Result<PageEnvelope<ReadListReadModel>, DiscoveryError>>;

    fn get_readlist_detail(
        &self,
        _context: &DiscoveryQueryContext,
        _query: ReadListDetailQuery,
    ) -> impl Future<Output = Result<Option<ReadListReadModel>, DiscoveryError>> {
        std::future::ready(Ok(None))
    }

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
    pub(crate) repository: R,
}

impl<R> DiscoveryQueries<R>
where
    R: DiscoveryQueryRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }
}

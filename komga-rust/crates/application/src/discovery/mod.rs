mod books;
mod index_maintenance;
mod libraries;
mod query_service;
mod read_models;
mod readlists;
mod request_shape;
mod series;

pub use books::{
    BookDetailQuery, BookReadlistsQuery, BookSiblingQuery, BooksLatestQuery, BooksListQuery,
    RuntimeBooksLatestQuery, RuntimeBooksListQuery,
};
pub use index_maintenance::{
    DiscoveryIndexDocument, DiscoveryIndexEntityType, DiscoveryIndexError, DiscoveryIndexEvent,
    DiscoveryIndexLifecyclePort, DiscoveryIndexMaintenance, DiscoveryIndexStartupState,
};
pub use libraries::LibraryListQuery;
pub use query_service::{
    DiscoveryQueries, DiscoveryQueryRepository, DiscoveryRequestValidation,
    bootstrap_series_id_for_runtime_shape, query_validation_mode, reject_bootstrap_shape_mismatch,
    requested_library_ids_for_runtime_shape,
};
pub use read_models::{
    BookDetailReadModel, BookReadModel, BookResourceReadModel, CollectionReadModel,
    LibraryReadModel, ReadListReadModel, SeriesDetailReadModel, SeriesReadModel,
    SeriesResourceReadModel,
};
pub use readlists::{
    ReadListBooksOwnership, ReadListBooksQuery, ReadListDetailQuery, ReadListsQuery,
    RuntimeReadListBooksQuery, RuntimeReadListsQuery, classify_readlist_books_query,
    normalize_readlists_search,
};
pub use series::{
    RuntimeSeriesListQuery, SeriesCollectionsQuery, SeriesDetailQuery, SeriesListQuery,
};

#[path = "discovery/books.rs"]
mod books;
#[path = "discovery/core.rs"]
mod core;
#[path = "discovery/helpers.rs"]
mod helpers;
#[path = "discovery/libraries.rs"]
mod libraries;
#[path = "discovery/readlists.rs"]
mod readlists;
#[path = "discovery/series.rs"]
mod series;

pub use books::{
    BookDetailQuery, BookReadlistsQuery, BookSiblingQuery, BooksLatestQuery, BooksListQuery,
    NativeBooksLatestQuery, NativeBooksListQuery,
};
pub use core::{DiscoveryQueries, DiscoveryQueryRepository};
pub use libraries::LibraryListQuery;
pub use readlists::{
    NativeReadListBooksQuery, NativeReadListsQuery, ReadListBooksOwnership, ReadListBooksQuery,
    ReadListDetailQuery, ReadListsQuery, classify_readlist_books_query,
    classify_readlists_browse_query, normalize_readlists_search,
};
pub use series::{
    NativeSeriesListQuery, SeriesCollectionsQuery, SeriesDetailQuery, SeriesListQuery,
};

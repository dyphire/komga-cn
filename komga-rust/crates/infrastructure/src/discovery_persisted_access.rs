use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path as FsPath;

use crate::sqlite::connect_pool;
use sqlx::{QueryBuilder, Row, Sqlite};

#[path = "discovery_persisted_access/authors.rs"]
mod authors;
#[path = "discovery_persisted_access/books.rs"]
mod books;
#[path = "discovery_persisted_access/common.rs"]
mod common;
#[path = "discovery_persisted_access/facets.rs"]
mod facets;
#[path = "discovery_persisted_access/library_mappings.rs"]
mod library_mappings;
#[path = "discovery_persisted_access/models.rs"]
mod models;
#[path = "discovery_persisted_access/runtime_queries.rs"]
mod runtime_queries;
#[path = "discovery_persisted_access/series.rs"]
mod series;

pub use authors::{
    load_persisted_author_names, load_persisted_author_roles, load_persisted_authors_by_scope,
};
pub use books::{
    load_book_poster_summaries, load_persisted_book_count, load_persisted_book_summaries,
    load_persisted_book_summaries_by_ids,
};
pub use facets::{
    load_persisted_age_ratings, load_persisted_genres, load_persisted_languages,
    load_persisted_publishers, load_persisted_series_release_dates, load_persisted_series_tags,
    load_persisted_sharing_labels, load_persisted_tags,
};
pub use library_mappings::{
    load_collection_memberships, load_persisted_library_ids, load_readlist_memberships,
};
pub use models::{
    AuthorEntry, AuthorsScope, BookBrowseEntry, BookPosterSummary, BookSummary, BookTagsScope,
    SeriesSummary,
};
pub use runtime_queries::{
    load_persisted_book_tags, load_persisted_duplicate_books, load_persisted_ondeck_books,
    load_series_read_progress_counts, load_series_total_book_counts, persisted_utc_date_minus_days,
};
pub use series::{
    load_persisted_series_count, load_persisted_series_summaries,
    load_persisted_series_summaries_by_ids, persisted_series_exist,
};

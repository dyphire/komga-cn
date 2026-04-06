use super::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::future::Future;
use std::path::Path as FsPath;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use axum::http::Uri;
use komga_application::discovery::BookReadModel;
use komga_domain::discovery::{DiscoveryError, PageEnvelope};
use serde_json::{Value, json};

use crate::http::discovery_auth::DiscoveryQueryContext;

#[path = "../../discovery_persisted_access/authors_queries.rs"]
mod authors_queries;
#[path = "../../discovery_persisted_access/books_queries.rs"]
mod books_queries;
#[path = "../../discovery_persisted_access/common_helpers.rs"]
mod common_helpers;
#[path = "../../discovery_persisted_access/facets_queries.rs"]
mod facets_queries;
#[path = "../../discovery_persisted_access/library_mappings.rs"]
mod library_mappings;
#[path = "../../discovery_persisted_access/persisted_runtime_queries.rs"]
mod persisted_runtime_queries;
#[path = "../../discovery_persisted_access/series_queries.rs"]
mod series_queries;

#[path = "persisted/backend.rs"]
mod backend;
#[path = "persisted/delegates.rs"]
mod delegates;
#[path = "persisted/helpers.rs"]
mod helpers;
#[path = "persisted/models.rs"]
mod models;

pub(crate) use backend::persisted_backend_search_collection_ids;
pub(crate) use backend::persisted_backend_search_readlist_scored_ids;
pub use backend::{PersistedDiscoveryAccessBackend, install_persisted_discovery_access};
pub use models::{
    PersistedAuthorEntry, PersistedAuthorsScope, PersistedBookBrowseEntry,
    PersistedBookPosterSummary, PersistedBookSummary, PersistedBookTagsScope,
    PersistedSeriesSummary,
};

use backend::{
    persisted_backend_load_book_poster_summaries, persisted_backend_load_collection_memberships,
    persisted_backend_load_persisted_age_ratings, persisted_backend_load_persisted_author_names,
    persisted_backend_load_persisted_author_roles,
    persisted_backend_load_persisted_authors_by_scope, persisted_backend_load_persisted_book_count,
    persisted_backend_load_persisted_book_summaries,
    persisted_backend_load_persisted_book_summaries_by_ids,
    persisted_backend_load_persisted_book_tags, persisted_backend_load_persisted_duplicate_books,
    persisted_backend_load_persisted_genres, persisted_backend_load_persisted_languages,
    persisted_backend_load_persisted_library_ids, persisted_backend_load_persisted_ondeck_books,
    persisted_backend_load_persisted_publishers, persisted_backend_load_persisted_series_count,
    persisted_backend_load_persisted_series_release_dates,
    persisted_backend_load_persisted_series_summaries,
    persisted_backend_load_persisted_series_summaries_by_ids,
    persisted_backend_load_persisted_series_tags, persisted_backend_load_persisted_sharing_labels,
    persisted_backend_load_persisted_tags, persisted_backend_load_readlist_memberships,
    persisted_backend_load_series_read_progress_counts,
    persisted_backend_load_series_total_book_counts, persisted_backend_persisted_books_exist,
    persisted_backend_persisted_series_exist, persisted_backend_persisted_utc_date_minus_days,
    persisted_backend_search_book_ids, persisted_backend_search_series_scored_ids,
};
pub(super) use common_helpers::filter_rows;
pub(super) use delegates::*;
pub(super) use helpers::*;
pub(super) use models::{
    BooksFilterCriteria, PersistedBooksBrowseQuery, PersistedBooksSortMode,
    PersistedSeriesBrowseQuery, PersistedSeriesSortMode, RuntimeBooksFilters, RuntimeSeriesFilters,
    SeriesFilterCriteria,
};

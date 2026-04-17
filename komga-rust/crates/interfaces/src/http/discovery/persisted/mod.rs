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

use crate::http::discovery_auth::context::DiscoveryQueryContext;

pub(crate) mod authors_queries;
pub mod backend;
pub(crate) mod books_queries;
pub(crate) mod common_helpers;
pub(crate) mod delegates;
pub(crate) mod facets_queries;
pub(crate) mod helpers;
pub(crate) mod library_mappings;
pub mod models;
pub(crate) mod persisted_runtime_queries;
pub(crate) mod series_queries;

use common_helpers::filter_rows;
use delegates::*;
use helpers::*;
use models::{
    BooksFilterCriteria, PersistedAuthorEntry, PersistedAuthorsScope, PersistedBookBrowseEntry,
    PersistedBookPosterSummary, PersistedBookSummary, PersistedBookTagsScope,
    PersistedBooksBrowseQuery, PersistedBooksSortMode, PersistedSeriesBrowseQuery,
    PersistedSeriesSortMode, PersistedSeriesSummary, RuntimeBooksFilters, RuntimeSeriesFilters,
};

use backend::{
    persisted_backend_load_book_poster_summaries, persisted_backend_load_collection_memberships,
    persisted_backend_load_collection_ordering, persisted_backend_load_persisted_age_ratings,
    persisted_backend_load_persisted_author_names, persisted_backend_load_persisted_author_roles,
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
    persisted_backend_load_series_read_dates, persisted_backend_load_series_read_progress_counts,
    persisted_backend_load_series_total_book_counts, persisted_backend_persisted_series_exist,
    persisted_backend_persisted_utc_date_minus_days, persisted_backend_search_book_ids,
    persisted_backend_search_series_scored_ids,
};

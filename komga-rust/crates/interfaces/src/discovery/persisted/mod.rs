use super::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use axum::http::Uri;
use komga_application::discovery::BookReadModel;
use komga_domain::discovery::{DiscoveryError, PageEnvelope};
use serde_json::{Value, json};

use crate::discovery_auth::context::DiscoveryQueryContext;

pub(crate) mod authors_queries;
pub(crate) mod books_queries;
pub(crate) mod common_helpers;
pub(crate) mod delegates;
pub(crate) mod facets_queries;
pub(crate) mod helpers;
pub(crate) mod library_mappings;
pub mod models;
pub(crate) mod series_queries;

use common_helpers::filter_rows;
use delegates::*;
use helpers::*;
use models::{
    BooksFilterCriteria, PersistedAuthorEntry, PersistedAuthorsScope, PersistedBookBrowseEntry,
    PersistedBookPosterSummary, PersistedBookTagsScope, PersistedBooksBrowseQuery,
    PersistedBooksSortMode, PersistedSeriesBrowseQuery, PersistedSeriesSortMode,
    PersistedSeriesSummary, RuntimeBooksFilters, RuntimeSeriesFilters,
};

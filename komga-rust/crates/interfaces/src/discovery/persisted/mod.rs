use std::collections::{BTreeMap, BTreeSet, HashMap};

use komga_application::discovery::BookReadModel;
use komga_domain::discovery::PageEnvelope;
use serde_json::{Value, json};

use crate::discovery_auth::context::DiscoveryQueryContext;

pub(crate) mod authors_queries;
pub mod books_queries;
pub(crate) mod common_helpers;
pub(crate) mod helpers;
pub(crate) mod library_mappings;
pub mod models;
pub mod series_queries;
pub(crate) use common_helpers::*;
pub(crate) use helpers::*;
pub(crate) use library_mappings::*;
use models::{
    PersistedAuthorEntry, PersistedBookPosterSummary, PersistedBooksBrowseQuery,
    PersistedBooksSortMode, PersistedSeriesBrowseQuery, PersistedSeriesSortMode,
    PersistedSeriesSummary,
};

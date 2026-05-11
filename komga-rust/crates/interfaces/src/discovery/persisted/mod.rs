use komga_domain::discovery::PageEnvelope;
use serde_json::{Value, json};

pub(crate) mod authors_queries;
pub(crate) mod common_helpers;
pub(crate) mod library_mappings;
pub mod models;
pub mod series_queries;
use models::{PersistedAuthorEntry, PersistedSeriesSummary};

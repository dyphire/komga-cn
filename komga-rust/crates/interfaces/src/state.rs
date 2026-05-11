use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use axum::http::HeaderMap;
use komga_application::identity_access::{
    AuthOutcome, AuthUser, KoboStoreSyncMergeResult, KoboSyncPage, PersistedApiKey,
    PersistedApiKeyMetadata, PersistedAuthenticationActivity,
};
use komga_application::library_catalog::{
    CreateLibraryResult, LibraryCatalogMutationError, LibraryChangeSet, LibraryRecord,
    LibraryTaskResult,
};
pub use komga_application::task_processing::TaskEngine;
use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlitePool;
use tokio::sync::watch;

use crate::discovery::persisted::models::{
    PersistedAuthorEntry, PersistedAuthorsScope, PersistedBookBrowseEntry,
};
use crate::discovery_auth::state::DiscoveryAuthState;

mod app_state;
mod core;
mod discovery;
mod identity;
mod library_catalog;
mod media_assets;
mod opds;
mod operational;
mod task_queue;

pub use app_state::*;
pub use core::*;
pub use discovery::*;
pub use identity::*;
pub use library_catalog::*;
pub use media_assets::*;
pub use opds::*;
pub use operational::*;
pub use task_queue::*;

#[cfg(test)]
pub(crate) mod tests;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
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
use komga_application::task_processing::TaskQueueRecord;
use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlitePool;
use tokio::sync::watch;

use crate::discovery_persisted_access::PersistedDiscoveryService;
use crate::http::discovery::detail::{
    ExistingSeriesMetadataRecord, PersistedBookAuthorRecord,
    PersistedBookDetailRecord as DiscoveryPersistedBookDetailRecord,
    PersistedBookResourceRecord as DiscoveryPersistedBookResourceRecord,
    PersistedBookSiblingDirectionRecord, PersistedCollectionAccessRecord,
    PersistedComicrackMatchCandidateRecord,
    PersistedReadlistBookRecord as DiscoveryPersistedReadlistBookRecord,
    PersistedReadlistRecord as DiscoveryPersistedReadlistRecord, PersistedSeriesCollectionRecord,
    PersistedSeriesDetailRecord, PersistedSeriesResourceRecord, PersistedSeriesRestrictionRecord,
    SeriesMetadataUpdateRecord, SeriesSummaryRecord,
};
use crate::http::discovery::persisted::models::{
    PersistedAuthorEntry, PersistedAuthorsScope, PersistedBookBrowseEntry,
    PersistedBookPosterSummary, PersistedBookSummary, PersistedBookTagsScope,
    PersistedSeriesSummary,
};
use crate::http::discovery_auth::state::DiscoveryAuthState;
use crate::media_assets_runtime_access::{
    PersistedMediaFileRecord, RuntimeBookMetadataService, RuntimeMediaImportService,
};
use crate::opds_catalog_access::{
    BrowsePublisherEntry, BrowseSeriesNavigationEntry, OpdsBookFeedEntry, OpdsReadlistEntry,
    OpdsSeriesEntry,
};
use crate::opds_persisted_access::{
    PersistedBookFeedRecord, PersistedBookSearchRecord, PersistedLibraryRecord,
    PersistedNamedRecord, PersistedReadlistBookRecord, PersistedReadlistRecord,
    PersistedSeriesBookRecord, PersistedSeriesRecord, PersistedSeriesSearchRecord,
};
use crate::operational_runtime_access::SqlitePoolSnapshot;
use crate::operational_settings_access::{
    ClaimInitialAdminUserResult, PageHashDeleteTarget, PageHashThumbnail, TransientBookAnalysis,
    TransientBookFileMetadata, TransientBookPage,
};
use crate::runtime_identity_access::{
    AuthenticationActivityWriteInput, CreateAuthUserInput, KoboMetadataRecord,
    KoreaderBookLookupError, KoreaderBookTarget, PersistedBookMediaFile,
    PersistedReadProgressRecord, UpdateAuthUserInput, UpdateAuthUserResult,
};

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

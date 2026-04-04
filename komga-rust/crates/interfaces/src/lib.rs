mod discovery_detail_access;
pub mod http;
mod media_assets_runtime_access;
mod opds_catalog_access;
mod opds_manifest_access;
mod opds_persisted_access;
mod operational_runtime_access;
mod operational_settings_access;
mod runtime_identity_access;

pub use media_assets_runtime_access::{
    MediaAssetsRuntimeAccessBackend, RuntimeBookMetadataService, RuntimeMediaImportService,
    install_media_assets_runtime_access,
};
pub use opds_catalog_access::{
    BrowsePublisherEntry, BrowseSeriesNavigationEntry, OpdsBookFeedEntry, OpdsCatalogAccessBackend,
    OpdsReadlistEntry, OpdsSeriesEntry, install_opds_catalog_access,
};
pub use opds_manifest_access::{
    ManifestBookRecord, OpdsManifestAccessBackend, install_opds_manifest_access,
};
pub use opds_persisted_access::{
    OpdsPersistedAccessBackend, PersistedBookFeedRecord, PersistedBookSearchRecord,
    PersistedLibraryRecord, PersistedNamedRecord, PersistedReadlistBookRecord,
    PersistedReadlistRecord, PersistedSeriesBookRecord, PersistedSeriesRecord,
    PersistedSeriesSearchRecord, install_opds_persisted_access,
};
pub use operational_runtime_access::{
    BookSnapshot, CollectionSnapshot, LibrarySnapshot, OperationalRuntimeAccessBackend,
    ReadListSnapshot, SeriesSnapshot, ServerSettingsStore, SseSnapshot, ThumbnailBookSnapshot,
    ThumbnailCollectionSnapshot, ThumbnailReadListSnapshot, ThumbnailSnapshot,
    install_operational_runtime_access,
};
pub use operational_settings_access::{
    ClaimInitialAdminUserResult, OperationalSettingsAccessBackend, PageHashThumbnail,
    PersistedServerSettings, TransientBookAnalysis, TransientBookFileMetadata, TransientBookPage,
    install_operational_settings_access,
};
pub use runtime_identity_access::{
    KoboMetadataRecord, KoreaderBookLookupError, KoreaderBookTarget, PersistedBookMediaFile,
    PersistedReadProgressRecord, RuntimeIdentityAccessBackend, UpdateAuthUserResult,
    install_runtime_identity_access,
};

pub use http::state::{AuthDatabaseState, OperationalState, ReadProgress, ReadProgressState};

pub const CACHE_CONTROL_PRIVATE: &str = "max-age=0, must-revalidate, private";
pub const SEARCH_OWNERSHIP_HEADER: &str = "x-komga-runtime-search-ownership";
pub const PERSISTED_OWNERSHIP_MARKER: &str = "persisted-owned-writer";

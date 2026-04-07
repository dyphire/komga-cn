use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use komga_application::library_catalog::{
    CreateLibraryService, DeleteLibraryService, LibraryCatalogQueryService, LibraryTaskService,
    UpdateLibraryService,
};
use komga_application::task_processing::TaskProcessingError;
use komga_infrastructure::auth as infrastructure_auth;
use komga_infrastructure::discovery_detail_access::{
    books as infrastructure_detail_books, collections as infrastructure_detail_collections,
    readlists as infrastructure_detail_readlists, series as infrastructure_detail_series,
};
use komga_infrastructure::discovery_persisted_access as infrastructure_discovery;
use komga_infrastructure::filesystem as infrastructure_filesystem;
use komga_infrastructure::library_catalog::SqliteLibraryCatalogAdapter;
use komga_infrastructure::metadata as infrastructure_metadata;
use komga_infrastructure::opds_catalog_access as infrastructure_opds_catalog;
use komga_infrastructure::opds_persisted_access as infrastructure_opds_persisted;
use komga_infrastructure::operational_metrics_access as infrastructure_operational_metrics;
use komga_infrastructure::operational_settings_access as infrastructure_operational_settings;
use komga_infrastructure::page_hashes_access as infrastructure_page_hashes;
use komga_infrastructure::runtime_identity_access as infrastructure_runtime_identity;
use komga_interfaces::http::discovery::{
    DiscoveryDetailAccessBackends, DiscoveryDetailBooksAccessBackend,
    DiscoveryDetailCollectionsAccessBackend, DiscoveryDetailReadlistsAccessBackend,
    DiscoveryDetailSeriesAccessBackend, ExistingSeriesMetadataRecord, PersistedAuthorEntry,
    PersistedAuthorsScope, PersistedBookAuthorRecord, PersistedBookBrowseEntry,
    PersistedBookDetailRecord, PersistedBookPosterSummary, PersistedBookResourceRecord,
    PersistedBookSiblingDirectionRecord, PersistedBookSummary, PersistedBookTagsScope,
    PersistedCollectionAccessRecord, PersistedComicrackMatchCandidateRecord,
    PersistedDiscoveryAccessBackend,
    PersistedReadProgressRecord as PersistedBookReadProgressRecord, PersistedReadlistBookRecord,
    PersistedReadlistRecord, PersistedSeriesCollectionRecord, PersistedSeriesDetailRecord,
    PersistedSeriesResourceRecord, PersistedSeriesRestrictionRecord, PersistedSeriesSummary,
    SeriesAlternateTitleRecord, SeriesMetadataLinkRecord, SeriesSummaryRecord,
    install_discovery_detail_access_backends, install_persisted_discovery_access,
};
use komga_interfaces::http::discovery_auth::DiscoveryAuthState;
use komga_interfaces::http::identity_access::auth::configure_remember_me_store;
use komga_interfaces::http::state::{
    AuthDatabaseState, BookImportSseEvent, LibraryCatalogOperations, OAuth2ClientConfig,
    OperationalState, ReadProgressState, RemoteCacheEntry, RuntimeProfile, RuntimeState,
    SseOperationalState, TransientBooksStore,
};
use komga_interfaces::{
    BookImportSnapshot as InterfacesBookImportSnapshot, BookSnapshot as InterfacesBookSnapshot,
    ClaimInitialAdminUserResult as InterfacesClaimInitialAdminUserResult,
    CollectionSnapshot as InterfacesCollectionSnapshot,
    KoboMetadataRecord as InterfacesKoboMetadataRecord,
    KoreaderBookLookupError as InterfacesKoreaderBookLookupError,
    KoreaderBookTarget as InterfacesKoreaderBookTarget,
    LibrarySnapshot as InterfacesLibrarySnapshot, MediaAssetsRuntimeAccessBackend,
    OperationalRuntimeAccessBackend, OperationalSettingsAccessBackend,
    PageHashDeleteTarget as InterfacesPageHashDeleteTarget,
    PageHashDeleteTargetPage as InterfacesPageHashDeleteTargetPage,
    PageHashThumbnail as InterfacesPageHashThumbnail,
    PersistedBookAuthorRecord as InterfacesPersistedBookAuthorRecord,
    PersistedBookFeedRecord as InterfacesPersistedBookFeedRecord,
    PersistedBookMediaFile as InterfacesPersistedBookMediaFile,
    PersistedBookSearchRecord as InterfacesPersistedBookSearchRecord,
    PersistedLibraryRecord as InterfacesPersistedLibraryRecord, PersistedMediaFileRecord,
    PersistedNamedRecord as InterfacesPersistedNamedRecord,
    PersistedReadProgressRecord as InterfacesPersistedReadProgressRecord,
    PersistedReadlistBookRecord as InterfacesPersistedReadlistBookRecord,
    PersistedReadlistRecord as InterfacesPersistedReadlistRecord,
    PersistedSeriesBookRecord as InterfacesPersistedSeriesBookRecord,
    PersistedSeriesRecord as InterfacesPersistedSeriesRecord,
    PersistedSeriesSearchRecord as InterfacesPersistedSeriesSearchRecord,
    PersistedServerSettings as InterfacesPersistedServerSettings,
    ReadListSnapshot as InterfacesReadListSnapshot, RuntimeBookMetadataService,
    RuntimeIdentityAccessBackend, RuntimeMediaImportService,
    SeriesSnapshot as InterfacesSeriesSnapshot,
    ServerSettingsStore as InterfacesServerSettingsStore, SseSnapshot as InterfacesSseSnapshot,
    ThumbnailBookSnapshot as InterfacesThumbnailBookSnapshot,
    ThumbnailCollectionSnapshot as InterfacesThumbnailCollectionSnapshot,
    ThumbnailReadListSnapshot as InterfacesThumbnailReadListSnapshot,
    ThumbnailSnapshot as InterfacesThumbnailSnapshot,
    TransientBookAnalysis as InterfacesTransientBookAnalysis,
    TransientBookFileMetadata as InterfacesTransientBookFileMetadata,
    TransientBookPage as InterfacesTransientBookPage, install_media_assets_runtime_access,
    install_operational_runtime_access, install_operational_settings_access,
    install_runtime_identity_access,
};
use komga_interfaces::{
    BrowsePublisherEntry as InterfacesBrowsePublisherEntry,
    BrowseSeriesNavigationEntry as InterfacesBrowseSeriesNavigationEntry,
    OpdsBookFeedEntry as InterfacesOpdsBookFeedEntry, OpdsCatalogAccessBackend,
    OpdsPersistedAccessBackend, OpdsReadlistEntry as InterfacesOpdsReadlistEntry,
    OpdsSeriesEntry as InterfacesOpdsSeriesEntry, install_opds_catalog_access,
    install_opds_persisted_access,
};
use tokio::sync::watch;

use crate::config::{RuntimeConfig, RuntimeProfile as ConfigRuntimeProfile};
use crate::runtime::background_workers::{RuntimeBackgroundState, SharedTaskQueue};

mod http_state_discovery;
mod http_state_media_assets;
mod http_state_opds;
mod http_state_operational_access;
mod http_state_operational_state;
mod http_state_runtime_config;
mod http_state_runtime_identity;

pub struct HttpRuntimeState {
    pub profile: RuntimeProfile,
    pub read_progress: ReadProgressState,
    pub discovery_auth: DiscoveryAuthState,
    pub auth_db: AuthDatabaseState,
    pub operational: OperationalState,
}

pub fn compose_http_runtime(
    config: &RuntimeConfig,
    background: RuntimeBackgroundState,
    shutdown_trigger: Option<watch::Sender<bool>>,
) -> HttpRuntimeState {
    install_runtime_identity_access(
        http_state_runtime_identity::compose_runtime_identity_access_backend(),
    );
    install_operational_runtime_access(
        http_state_operational_access::compose_operational_runtime_access_backend(),
    );
    install_operational_settings_access(
        http_state_operational_access::compose_operational_settings_access_backend(),
    );
    install_media_assets_runtime_access(
        http_state_media_assets::compose_media_assets_runtime_access_backend(),
    );
    install_discovery_detail_access_backends(
        http_state_discovery::compose_discovery_detail_access_backends(),
    );
    install_persisted_discovery_access(
        http_state_discovery::compose_persisted_discovery_access_backend(
            config.database_file.as_path(),
            config.lucene_data_directory.as_path(),
        ),
    );
    http_state_opds::install_opds_access_backends(config.lucene_data_directory.as_path());

    let remember_me_store_root = config
        .config_dir
        .as_deref()
        .or_else(|| config.database_file.parent())
        .unwrap_or_else(|| Path::new("."));
    let remember_me_namespace = configure_remember_me_store(remember_me_store_root);

    let read_progress = ReadProgressState {
        progress_by_token: Arc::new(Mutex::new(HashMap::new())),
    };
    let profile = http_state_runtime_config::runtime_profile(config);
    let discovery_auth = DiscoveryAuthState::default();
    let auth_db = AuthDatabaseState {
        database_file: config.database_file.clone(),
        demo_mode: config.demo_mode,
        remember_me_namespace,
    };
    let operational = http_state_operational_state::compose_operational_state(
        config,
        background.task_queue,
        shutdown_trigger,
    );

    HttpRuntimeState {
        profile,
        read_progress,
        discovery_auth,
        auth_db,
        operational,
    }
}

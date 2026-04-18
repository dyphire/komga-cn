use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use komga_application::library_catalog::{
    CreateLibraryService, DeleteLibraryService, LibraryCatalogQueryService, LibraryTaskService,
    UpdateLibraryService,
};
use komga_infrastructure::discovery_detail_access::{
    books as infrastructure_detail_books, collections as infrastructure_detail_collections,
    readlists as infrastructure_detail_readlists, series as infrastructure_detail_series,
};
use komga_infrastructure::discovery_persisted_access::{
    authors as infrastructure_discovery_authors, books as infrastructure_discovery_books,
    facets as infrastructure_discovery_facets,
    library_mappings as infrastructure_discovery_library_mappings,
    models as infrastructure_discovery_models,
    runtime_queries as infrastructure_discovery_runtime_queries,
    series as infrastructure_discovery_series,
};
use komga_infrastructure::library_catalog::SqliteLibraryCatalogAdapter;
use komga_infrastructure::metadata as infrastructure_metadata;
use komga_infrastructure::opds_catalog_access as infrastructure_opds_catalog;
use komga_infrastructure::opds_persisted_access as infrastructure_opds_persisted;
use komga_infrastructure::operational_metrics_access as infrastructure_operational_metrics;
use komga_infrastructure::operational_settings_access as infrastructure_operational_settings;
use komga_infrastructure::page_hashes_access as infrastructure_page_hashes;
use komga_infrastructure::runtime_identity_access as infrastructure_runtime_identity;
use komga_interfaces::http::discovery::detail::{
    DiscoveryDetailAccessBackends, DiscoveryDetailBooksAccessBackend,
    DiscoveryDetailCollectionsAccessBackend, DiscoveryDetailReadlistsAccessBackend,
    DiscoveryDetailSeriesAccessBackend, ExistingSeriesMetadataRecord, PersistedBookAuthorRecord,
    PersistedBookDetailRecord, PersistedBookResourceRecord, PersistedBookSiblingDirectionRecord,
    PersistedCollectionAccessRecord, PersistedComicrackMatchCandidateRecord,
    PersistedReadProgressRecord as PersistedBookReadProgressRecord, PersistedReadlistBookRecord,
    PersistedReadlistRecord, PersistedSeriesCollectionRecord, PersistedSeriesDetailRecord,
    PersistedSeriesResourceRecord, PersistedSeriesRestrictionRecord, SeriesAlternateTitleRecord,
    SeriesMetadataLinkRecord, SeriesSummaryRecord, install_discovery_detail_access_backends,
};
use komga_interfaces::http::discovery::persisted::backend::{
    PersistedDiscoveryAccessBackend, install_persisted_discovery_access,
};
use komga_interfaces::http::discovery::persisted::models::{
    PersistedAuthorEntry, PersistedAuthorsScope, PersistedBookBrowseEntry,
    PersistedBookPosterSummary, PersistedBookSummary, PersistedBookTagsScope,
    PersistedReadProgressSummary, PersistedSeriesSummary, PersistedWebLinkEntry,
};
use komga_interfaces::http::discovery_auth::state::DiscoveryAuthState;
use komga_interfaces::http::identity_access::auth::{
    sync_remember_me_runtime_database_file, sync_remember_me_runtime_settings,
    sync_session_runtime_settings,
};
use komga_interfaces::http::state::{
    AuthDatabaseState, BookImportSseEvent, HttpServerRequestsState, LibraryCatalogOperations,
    OAuth2ClientConfig, OperationalBuildMetadata, OperationalState, ReadProgressState,
    RemoteCacheEntry, RuntimeProfile, RuntimeState, SseOperationalState, StartupTimingState,
    TransientBooksStore,
};
use komga_interfaces::media_assets_runtime_access::{
    MediaAssetsRuntimeAccessBackend, PersistedMediaFileRecord, RuntimeBookMetadataService,
    RuntimeMediaImportService, install_media_assets_runtime_access,
};
use komga_interfaces::opds_catalog_access::{
    BrowsePublisherEntry as InterfacesBrowsePublisherEntry,
    BrowseSeriesNavigationEntry as InterfacesBrowseSeriesNavigationEntry,
    OpdsBookFeedEntry as InterfacesOpdsBookFeedEntry, OpdsCatalogAccessBackend,
    OpdsReadlistEntry as InterfacesOpdsReadlistEntry, OpdsSeriesEntry as InterfacesOpdsSeriesEntry,
    install_opds_catalog_access,
};
use komga_interfaces::opds_persisted_access::{
    OpdsPersistedAccessBackend, PersistedBookAuthorRecord as InterfacesPersistedBookAuthorRecord,
    PersistedBookFeedRecord as InterfacesPersistedBookFeedRecord,
    PersistedBookSearchRecord as InterfacesPersistedBookSearchRecord,
    PersistedLibraryRecord as InterfacesPersistedLibraryRecord,
    PersistedNamedRecord as InterfacesPersistedNamedRecord,
    PersistedReadlistBookRecord as InterfacesPersistedReadlistBookRecord,
    PersistedReadlistRecord as InterfacesPersistedReadlistRecord,
    PersistedSeriesBookRecord as InterfacesPersistedSeriesBookRecord,
    PersistedSeriesRecord as InterfacesPersistedSeriesRecord,
    PersistedSeriesSearchRecord as InterfacesPersistedSeriesSearchRecord,
    install_opds_persisted_access,
};
use komga_interfaces::operational_runtime_access::{
    OperationalRuntimeAccessBackend, ServerSettingsStore as InterfacesServerSettingsStore,
    install_operational_runtime_access,
};
use komga_interfaces::operational_settings_access::{
    ClaimInitialAdminUserResult as InterfacesClaimInitialAdminUserResult,
    OperationalSettingsAccessBackend, PageHashDeleteTarget as InterfacesPageHashDeleteTarget,
    PageHashDeleteTargetPage as InterfacesPageHashDeleteTargetPage,
    PageHashThumbnail as InterfacesPageHashThumbnail,
    PersistedServerSettings as InterfacesPersistedServerSettings,
    TransientBookAnalysis as InterfacesTransientBookAnalysis,
    TransientBookFileMetadata as InterfacesTransientBookFileMetadata,
    TransientBookPage as InterfacesTransientBookPage, install_operational_settings_access,
};
use komga_interfaces::runtime_identity_access::{
    KoboMetadataRecord as InterfacesKoboMetadataRecord,
    KoreaderBookLookupError as InterfacesKoreaderBookLookupError,
    KoreaderBookTarget as InterfacesKoreaderBookTarget,
    PersistedBookMediaFile as InterfacesPersistedBookMediaFile,
    PersistedReadProgressRecord as InterfacesPersistedReadProgressRecord,
    RuntimeIdentityAccessBackend, install_runtime_identity_access,
};
use sha2::Digest;
use tokio::sync::watch;

use crate::runtime::background_workers::RuntimeBackgroundState;
use crate::runtime::background_workers::WorkerRuntimeGuard;
use komga_config::env_config::RuntimeConfig;
use komga_config::profile::RuntimeProfile as ConfigRuntimeProfile;

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
    worker_runtime_guard: Option<WorkerRuntimeGuard>,
    shutdown_trigger: Option<watch::Sender<bool>>,
    startup_timing: StartupTimingState,
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

    let remember_me_runtime_key = runtime_identity_key(config.database_file.as_path());
    sync_remember_me_runtime_database_file(
        remember_me_runtime_key.as_str(),
        config.database_file.as_path(),
    );
    preload_remember_me_runtime_settings(config, remember_me_runtime_key.as_str());
    // The current registry still derives both token families from the same configured root,
    // but the HTTP state keeps separate runtime keys so session and remember-me semantics are explicit.
    let session_runtime_key = remember_me_runtime_key.clone();
    sync_session_runtime_settings(
        session_runtime_key.as_str(),
        config.session_max_inactive_seconds,
    );

    let read_progress = ReadProgressState {
        progress_by_token: Arc::new(Mutex::new(HashMap::new())),
    };
    let profile = http_state_runtime_config::runtime_profile(config);
    let discovery_auth = DiscoveryAuthState::default();
    let auth_db = AuthDatabaseState {
        database_file: config.database_file.clone(),
        demo_mode: config.demo_mode,
        session_runtime_key,
        remember_me_runtime_key: remember_me_runtime_key.clone(),
    };
    let operational = http_state_operational_state::compose_operational_state(
        config,
        startup_timing,
        remember_me_runtime_key.clone(),
        background.task_queue,
        background.task_wakeup,
        worker_runtime_guard,
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

fn runtime_identity_key(database_file: &Path) -> String {
    let canonical = database_file
        .canonicalize()
        .unwrap_or_else(|_| database_file.to_path_buf());
    let digest = sha2::Sha256::digest(canonical.to_string_lossy().as_bytes());
    let encoded = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("auth-runtime-{}", &encoded[..16])
}

fn preload_remember_me_runtime_settings(config: &RuntimeConfig, remember_me_runtime_key: &str) {
    let (remember_me_key, remember_me_duration_days) =
        infrastructure_operational_settings::load_remember_me_runtime_settings(
            config.database_file.as_path(),
        )
        .expect("remember-me startup settings should load");
    sync_remember_me_runtime_settings(
        remember_me_runtime_key,
        remember_me_key.as_str(),
        remember_me_duration_days,
    );
}

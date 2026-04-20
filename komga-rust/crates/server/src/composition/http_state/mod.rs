use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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
use komga_infrastructure::metadata as infrastructure_metadata;
use komga_infrastructure::opds_catalog_access as infrastructure_opds_catalog;
use komga_infrastructure::opds_persisted_access as infrastructure_opds_persisted;
use komga_infrastructure::operational_metrics_access as infrastructure_operational_metrics;
use komga_infrastructure::operational_settings_access as infrastructure_operational_settings;
use komga_infrastructure::page_hashes_access as infrastructure_page_hashes;
use komga_infrastructure::runtime_identity_access as infrastructure_runtime_identity;
use komga_interfaces::http::discovery::detail::{
    ExistingSeriesMetadataRecord, PersistedBookAuthorRecord, PersistedBookDetailRecord,
    PersistedBookResourceRecord, PersistedBookSiblingDirectionRecord,
    PersistedCollectionAccessRecord, PersistedComicrackMatchCandidateRecord,
    PersistedReadProgressRecord as PersistedBookReadProgressRecord, PersistedReadlistBookRecord,
    PersistedReadlistRecord, PersistedSeriesCollectionRecord, PersistedSeriesDetailRecord,
    PersistedSeriesResourceRecord, PersistedSeriesRestrictionRecord, SeriesAlternateTitleRecord,
    SeriesMetadataLinkRecord, SeriesMetadataUpdateRecord, SeriesSummaryRecord,
};
use komga_interfaces::http::discovery::persisted::models::{
    PersistedAuthorEntry, PersistedAuthorsScope, PersistedBookBrowseEntry,
    PersistedBookPosterSummary, PersistedBookSummary, PersistedBookTagsScope,
    PersistedReadProgressSummary, PersistedSeriesSummary, PersistedWebLinkEntry,
};
use komga_interfaces::http::discovery_auth::state::DiscoveryAuthState;
use komga_interfaces::http::state::{
    AuthDatabaseState, BookImportSseEvent, DiscoveryDetailService, HttpAppState,
    HttpServerRequestsState, HttpServices, IdentityService, LibraryCatalogService,
    MediaAssetsService, OAuth2ClientConfig, OperationalBuildMetadata, OperationalRuntimeService,
    OperationalSettingsService, OperationalState, ReadProgressState, RemoteCacheEntry,
    RuntimeProfile, RuntimeState, ServerSettingsService, SseOperationalState, StartupTimingState,
    TaskQueueService, TransientBooksStore,
};
use komga_interfaces::media_assets_runtime_access::PersistedMediaFileRecord;
use komga_interfaces::opds_catalog_access::{
    BrowsePublisherEntry as InterfacesBrowsePublisherEntry,
    BrowseSeriesNavigationEntry as InterfacesBrowseSeriesNavigationEntry,
    OpdsBookFeedEntry as InterfacesOpdsBookFeedEntry,
    OpdsReadlistEntry as InterfacesOpdsReadlistEntry, OpdsSeriesEntry as InterfacesOpdsSeriesEntry,
};
use komga_interfaces::opds_persisted_access::{
    PersistedBookAuthorRecord as InterfacesPersistedBookAuthorRecord,
    PersistedBookFeedRecord as InterfacesPersistedBookFeedRecord,
    PersistedBookSearchRecord as InterfacesPersistedBookSearchRecord,
    PersistedLibraryRecord as InterfacesPersistedLibraryRecord,
    PersistedNamedRecord as InterfacesPersistedNamedRecord,
    PersistedReadlistBookRecord as InterfacesPersistedReadlistBookRecord,
    PersistedReadlistRecord as InterfacesPersistedReadlistRecord,
    PersistedSeriesBookRecord as InterfacesPersistedSeriesBookRecord,
    PersistedSeriesRecord as InterfacesPersistedSeriesRecord,
    PersistedSeriesSearchRecord as InterfacesPersistedSeriesSearchRecord,
};
use komga_interfaces::operational_settings_access::PersistedServerSettings as InterfacesPersistedServerSettings;
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
    pub app: HttpAppState,
}

pub fn compose_http_runtime(
    config: &RuntimeConfig,
    background: RuntimeBackgroundState,
    worker_runtime_guard: Option<WorkerRuntimeGuard>,
    shutdown_trigger: Option<watch::Sender<bool>>,
    startup_timing: StartupTimingState,
) -> HttpRuntimeState {
    let runtime_identity_service = http_state_runtime_identity::compose_runtime_identity_service();
    let operational_runtime_service: Arc<dyn OperationalRuntimeService> =
        Arc::new(http_state_operational_access::compose_operational_runtime_service());
    let operational_settings_service: Arc<dyn OperationalSettingsService> =
        Arc::new(http_state_operational_access::compose_operational_settings_service());
    let media_assets_service = http_state_media_assets::compose_media_assets_service();
    let discovery_detail_service = http_state_discovery::compose_discovery_detail_service();
    let discovery_persisted = http_state_discovery::compose_persisted_discovery_service(
        config.database_file.as_path(),
        config.lucene_data_directory.as_path(),
    );
    let (opds_catalog, opds_persisted) =
        http_state_opds::compose_opds_services(config.lucene_data_directory.as_path());

    let remember_me_runtime_key = runtime_identity_key(config.database_file.as_path());
    runtime_identity_service.sync_remember_me_runtime_database_file(
        remember_me_runtime_key.clone(),
        config.database_file.clone(),
    );
    preload_remember_me_runtime_settings(
        config,
        remember_me_runtime_key.as_str(),
        runtime_identity_service.as_ref(),
    );
    // The current registry still derives both token families from the same configured root,
    // but the HTTP state keeps separate runtime keys so session and remember-me semantics are explicit.
    let session_runtime_key = remember_me_runtime_key.clone();
    runtime_identity_service.sync_session_runtime_settings(
        session_runtime_key.clone(),
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
        runtime_identity: runtime_identity_service.clone(),
    };
    let services = HttpServices {
        library_catalog: Arc::new(
            http_state_operational_state::SqliteLibraryCatalogService::new(
                config.database_file.as_path(),
            ),
        ),
        task_queue: Arc::new(http_state_operational_state::RuntimeTaskQueueService::new(
            background.task_queue,
            background.task_wakeup,
            worker_runtime_guard,
        )),
        server_settings: Arc::new(
            http_state_operational_state::RuntimeServerSettingsService::new(
                config.database_file.as_path(),
            ),
        ),
        runtime_identity: runtime_identity_service.clone(),
        operational_runtime: operational_runtime_service.clone(),
        operational_settings: operational_settings_service.clone(),
        media_assets: media_assets_service.clone(),
        opds_catalog: Arc::new(opds_catalog),
        opds_persisted: Arc::new(opds_persisted),
        discovery_persisted,
        discovery_detail: discovery_detail_service.clone(),
    };
    let operational = http_state_operational_state::compose_operational_state(
        config,
        startup_timing,
        remember_me_runtime_key.clone(),
        shutdown_trigger,
    );

    HttpRuntimeState {
        app: HttpAppState {
            profile,
            read_progress,
            discovery_auth,
            auth_db,
            operational,
            services,
        },
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

fn preload_remember_me_runtime_settings(
    config: &RuntimeConfig,
    remember_me_runtime_key: &str,
    runtime_identity: &dyn IdentityService,
) {
    let (remember_me_key, remember_me_duration_days) =
        infrastructure_operational_settings::load_remember_me_runtime_settings(
            config.database_file.as_path(),
        )
        .expect("remember-me startup settings should load");
    runtime_identity.sync_remember_me_runtime_settings(
        remember_me_runtime_key.to_string(),
        remember_me_key,
        remember_me_duration_days,
    );
}

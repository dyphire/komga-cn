use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use komga_infrastructure::database_handle::DatabaseHandle;
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
use komga_interfaces::discovery::persisted::models::{
    PersistedAuthorEntry, PersistedAuthorsScope, PersistedBookBrowseEntry,
    PersistedBookPosterSummary, PersistedBookSummary, PersistedBookTagsScope,
    PersistedReadProgressSummary, PersistedSeriesSummary, PersistedWebLinkEntry,
};
use komga_interfaces::discovery_auth::state::DiscoveryAuthState;
use komga_interfaces::state::{
    AuthDatabaseState, BookImportSseEvent, BrowsePublisherEntry as InterfacesBrowsePublisherEntry,
    BrowseSeriesNavigationEntry as InterfacesBrowseSeriesNavigationEntry, DiscoveryDetailService,
    DiscoveryPersistedReadProgressRecord as PersistedBookReadProgressRecord,
    DiscoveryPersistedReadlistBookRecord as PersistedReadlistBookRecord,
    DiscoveryPersistedReadlistRecord as PersistedReadlistRecord, ExistingSeriesMetadataRecord,
    HttpAppState, HttpServerRequestsState, HttpServices, IdentityService, LibraryCatalogService,
    OAuth2ClientConfig, OpdsBookFeedEntry as InterfacesOpdsBookFeedEntry,
    OpdsPersistedBookAuthorRecord as InterfacesPersistedBookAuthorRecord,
    OpdsReadlistEntry as InterfacesOpdsReadlistEntry, OpdsSeriesEntry as InterfacesOpdsSeriesEntry,
    OperationalBuildMetadata, OperationalRuntimeService, OperationalSettingsService,
    OperationalState, PersistedBookAuthorRecord, PersistedBookDetailRecord,
    PersistedBookFeedRecord as InterfacesPersistedBookFeedRecord, PersistedBookResourceRecord,
    PersistedBookSearchRecord as InterfacesPersistedBookSearchRecord,
    PersistedBookSiblingDirectionRecord, PersistedCollectionAccessRecord,
    PersistedComicrackMatchCandidateRecord,
    PersistedLibraryRecord as InterfacesPersistedLibraryRecord,
    PersistedNamedRecord as InterfacesPersistedNamedRecord,
    PersistedReadlistBookRecord as InterfacesPersistedReadlistBookRecord,
    PersistedReadlistRecord as InterfacesPersistedReadlistRecord,
    PersistedSeriesBookRecord as InterfacesPersistedSeriesBookRecord,
    PersistedSeriesCollectionRecord, PersistedSeriesDetailRecord,
    PersistedSeriesRecord as InterfacesPersistedSeriesRecord, PersistedSeriesResourceRecord,
    PersistedSeriesRestrictionRecord,
    PersistedSeriesSearchRecord as InterfacesPersistedSeriesSearchRecord,
    PersistedServerSettings as InterfacesPersistedServerSettings, ReadProgressState,
    RemoteCacheEntry, RuntimeProfile, RuntimeState, SeriesAlternateTitleRecord,
    SeriesMetadataLinkRecord, SeriesMetadataUpdateRecord, SeriesSummaryRecord,
    ServerSettingsService, SseOperationalState, StartupTimingState, TransientBooksStore,
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

pub fn compose_http_runtime(
    config: &RuntimeConfig,
    db: DatabaseHandle,
    tasks_db: DatabaseHandle,
    background: RuntimeBackgroundState,
    _worker_runtime_guard: Option<WorkerRuntimeGuard>,
    shutdown_trigger: Option<watch::Sender<bool>>,
    startup_timing: StartupTimingState,
) -> HttpAppState {
    let runtime_identity_service =
        http_state_runtime_identity::compose_runtime_identity_service(db.clone());
    let operational_runtime_service: Box<dyn OperationalRuntimeService> = Box::new(
        http_state_operational_access::compose_operational_runtime_service(db.clone(), tasks_db),
    );
    let media_assets_service = http_state_media_assets::compose_media_assets_service(db.clone());
    let discovery_detail_service = http_state_discovery::compose_discovery_detail_service(
        db.clone(),
        config.lucene_data_directory.clone(),
    );
    let discovery_persisted = http_state_discovery::compose_persisted_discovery_service(
        db.clone(),
        config.lucene_data_directory.clone(),
    );
    let (opds_catalog, opds_persisted) =
        http_state_opds::compose_opds_services(&db, config.lucene_data_directory.as_path());
    let operational_settings_service: Box<dyn OperationalSettingsService> =
        Box::new(http_state_operational_access::compose_operational_settings_service(db.clone()));

    let remember_me_runtime_key = runtime_identity_key(config.database_file.as_path());
    runtime_identity_service
        .sync_remember_me_runtime_database_file(remember_me_runtime_key.clone());
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
        progress_by_token: Mutex::new(HashMap::new()),
    };
    let profile = http_state_runtime_config::runtime_profile(config);
    let discovery_auth = DiscoveryAuthState::default();
    let auth_db = AuthDatabaseState {
        db: db.clone(),
        demo_mode: config.demo_mode,
        session_runtime_key,
        remember_me_runtime_key: remember_me_runtime_key.clone(),
    };
    let services = HttpServices {
        library_catalog: Box::new(
            http_state_operational_state::SqliteLibraryCatalogService::new(
                config.database_file.as_path(),
                db.write_pool().clone(),
            ),
        ),
        task_queue: http_state_operational_state::create_task_engine(
            background.task_queue,
            background.task_wakeup,
            background.task_execution_pool,
        ),
        server_settings: Box::new(
            http_state_operational_state::RuntimeServerSettingsService::new(
                config.database_file.as_path(),
            ),
        ),
        runtime_identity: runtime_identity_service,
        operational_runtime: operational_runtime_service,
        operational_settings: operational_settings_service,
        media_assets: media_assets_service,
        opds_catalog: Box::new(opds_catalog),
        opds_persisted: Box::new(opds_persisted),
        discovery_persisted,
        discovery_detail: discovery_detail_service,
    };
    let operational = http_state_operational_state::compose_operational_state(
        config,
        startup_timing,
        remember_me_runtime_key,
        shutdown_trigger,
    );

    HttpAppState {
        profile,
        read_progress,
        discovery_auth,
        auth_db,
        operational,
        services,
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

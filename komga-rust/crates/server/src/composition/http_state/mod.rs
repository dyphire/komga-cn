use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use komga_application::discovery::{DiscoveryBrowseService, DiscoveryFacetService};
use komga_infrastructure::discovery_detail_access::{
    books as infrastructure_detail_books, collections, readlists,
    series as infrastructure_detail_series,
};
use komga_infrastructure::discovery_persisted_access::{
    authors, library_mappings, models, runtime_queries, series as infrastructure_discovery_series,
};
use komga_infrastructure::metadata;
use komga_infrastructure::opds_catalog_access;
use komga_infrastructure::opds_persisted_access;
use komga_infrastructure::operational_metrics_access;
use komga_infrastructure::operational_settings_access;
use komga_infrastructure::page_hashes_access;
use komga_interfaces::discovery::persisted::models::{
    PersistedAuthorEntry, PersistedAuthorsScope, PersistedBookBrowseEntry,
};
use komga_interfaces::discovery_auth::state::DiscoveryAuthState;
use komga_interfaces::state::{
    AuthDatabaseState, AuthTokenService, BookAccessService, BookImportSseEvent,
    CollectionAccessService, DiscoveryPersistedReadProgressRecord,
    DiscoveryPersistedReadlistBookRecord, DiscoveryPersistedReadlistRecord,
    ExistingSeriesMetadataRecord, HttpAppState, HttpServerRequestsState, HttpServices,
    LibraryCatalogService, OAuth2ClientConfig, OperationalBuildMetadata, OperationalRuntimeService,
    OperationalSettingsService, OperationalState, PersistedBookAuthorRecord,
    PersistedBookDetailRecord, PersistedBookResourceRecord, PersistedBookSiblingDirectionRecord,
    PersistedCollectionAccessRecord, PersistedComicrackMatchCandidateRecord,
    PersistedSeriesCollectionRecord, PersistedSeriesDetailRecord, PersistedSeriesResourceRecord,
    PersistedSeriesRestrictionRecord, ReadProgressState, ReadlistAccessService, RemoteCacheEntry,
    RuntimeProfile, RuntimeState, SeriesAccessService, SeriesAlternateTitleRecord,
    SeriesMetadataLinkRecord, SeriesMetadataUpdateRecord, SeriesSummaryRecord,
    ServerSettingsService, SseOperationalState, StartupTimingState, TransientBooksStore,
};
use sha2::Digest;
use tokio::sync::watch;

use crate::runtime::HttpRuntimeParts;
use komga_config::env_config::RuntimeConfig;
use komga_config::profile::RuntimeProfile as ConfigRuntimeProfile;

mod http_state_discovery;
mod http_state_opds;
mod http_state_operational_access;
mod http_state_operational_state;
mod http_state_runtime_config;
mod http_state_runtime_identity;

pub fn compose_http_runtime(
    config: &RuntimeConfig,
    runtime: HttpRuntimeParts,
    shutdown_trigger: Option<watch::Sender<bool>>,
    startup_timing: StartupTimingState,
) -> HttpAppState {
    let HttpRuntimeParts {
        main_db: db,
        tasks_db,
        task_engine,
    } = runtime;
    let identity = http_state_runtime_identity::compose_identity_services(db.clone());
    let operational_runtime_service: Arc<dyn OperationalRuntimeService> = Arc::new(
        http_state_operational_access::compose_operational_runtime_service(db.clone(), tasks_db),
    );
    let discovery_detail_service =
        Arc::new(http_state_discovery::compose_discovery_detail_service(
            db.clone(),
            config.lucene_data_directory.clone(),
        ));
    let book_access: Arc<dyn BookAccessService> = discovery_detail_service.clone();
    let series_access: Arc<dyn SeriesAccessService> = discovery_detail_service.clone();
    let collection_access: Arc<dyn CollectionAccessService> = discovery_detail_service.clone();
    let readlist_access: Arc<dyn ReadlistAccessService> = discovery_detail_service;
    let discovery_search: Arc<dyn komga_interfaces::state::DiscoverySearchService> =
        Arc::from(http_state_discovery::compose_discovery_search_service(
            db.clone(),
            config.lucene_data_directory.clone(),
        ));
    let discovery_browse_service =
        Arc::new(http_state_discovery::compose_discovery_browse_service(
            db.clone(),
            config.lucene_data_directory.clone(),
        ));
    let discovery_browse: Arc<dyn DiscoveryBrowseService> = discovery_browse_service.clone();
    let discovery_facets: Arc<dyn DiscoveryFacetService> = discovery_browse_service;
    let (opds_catalog, opds_persisted) =
        http_state_opds::compose_opds_services(&db, config.lucene_data_directory.as_path());
    let operational_settings_service: Arc<dyn OperationalSettingsService> =
        Arc::new(http_state_operational_access::compose_operational_settings_service(db.clone()));

    let remember_me_runtime_key = runtime_identity_key(config.database_file.as_path());
    identity
        .auth_token
        .sync_remember_me_runtime_database_file(remember_me_runtime_key.as_str());
    preload_remember_me_runtime_settings(
        config,
        remember_me_runtime_key.as_str(),
        identity.auth_token.as_ref(),
    );
    // The current registry still derives both token families from the same configured root,
    // but the HTTP state keeps separate runtime keys so session and remember-me semantics are explicit.
    let session_runtime_key = remember_me_runtime_key.clone();
    identity.auth_token.sync_session_runtime_settings(
        session_runtime_key.as_str(),
        config.session_max_inactive_seconds,
    );

    let read_progress = ReadProgressState {
        progress_by_token: Arc::new(Mutex::new(HashMap::new())),
    };
    let profile = http_state_runtime_config::runtime_profile(config);
    let discovery_auth = DiscoveryAuthState::default();
    let auth_db = AuthDatabaseState {
        db: db.clone(),
        demo_mode: config.demo_mode,
        session_runtime_key,
        remember_me_runtime_key: remember_me_runtime_key.clone(),
    };
    let task_engine_arc: Arc<dyn komga_application::task_processing::TaskEngine> =
        Arc::from(task_engine);
    let metadata_writer = Arc::new(komga_application::media_assets::MetadataWriter::new(
        Box::new(metadata::SqliteBookMetadataPort::new(
            db.read_pool().clone(),
            db.write_pool().clone(),
        )),
        Box::new(
            komga_infrastructure::search_sync_adapter::SearchSyncAdapter::new(
                db.write_pool().clone(),
                config.database_file.clone(),
                config.lucene_data_directory.clone(),
            ),
        ),
        Box::new(
            komga_infrastructure::task_enqueue_adapter::TaskEnqueueAdapter::new(
                task_engine_arc.clone(),
            ),
        ),
        Box::new(komga_infrastructure::event_emitter_adapter::SseBookEventEmitter),
    ));
    let services = HttpServices {
        library_catalog: Arc::new(
            http_state_operational_state::SqliteLibraryCatalogService::new(
                db.read_pool().clone(),
                db.write_pool().clone(),
            ),
        ),
        task_queue: task_engine_arc,
        server_settings: Arc::new(
            http_state_operational_state::RuntimeServerSettingsService::new(
                config.database_file.as_path(),
            ),
        ),
        auth_token: identity.auth_token,
        user_management: identity.user_management,
        api_key: identity.api_key,
        auth_activity: identity.auth_activity,
        device_sync: identity.device_sync,
        operational_runtime: operational_runtime_service,
        operational_settings: operational_settings_service,
        opds_catalog: Arc::new(opds_catalog),
        opds_persisted: Arc::new(opds_persisted),
        discovery_search,
        book_access,
        series_access,
        collection_access,
        readlist_access,
        discovery_browse,
        discovery_facets,
        media_reader: komga_infrastructure::media_reader::MediaReader::new(db.read_pool().clone()),
        content_resolver: komga_infrastructure::content_resolver::ContentResolver,
        thumbnail_writer: komga_infrastructure::thumbnail_writer::ThumbnailWriter::new(
            db.write_pool().clone(),
        ),
        progress_writer: komga_infrastructure::progress_writer::ProgressWriter::new(
            db.write_pool().clone(),
        ),
        metadata_writer,
        import_service: Arc::new(komga_application::media_assets::MediaImportService::new(
            komga_infrastructure::filesystem::import::FilesystemImportPort::new(
                db.database_file().to_path_buf(),
            ),
        )),
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
    auth_token: &dyn AuthTokenService,
) {
    let (remember_me_key, remember_me_duration_days) =
        operational_settings_access::load_remember_me_runtime_settings(
            config.database_file.as_path(),
        )
        .expect("remember-me startup settings should load");
    auth_token.sync_remember_me_runtime_settings(
        remember_me_runtime_key,
        remember_me_key.as_str(),
        remember_me_duration_days,
    );
}

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use komga_application::discovery::{
    DiscoveryBrowseService, DiscoveryDetailPort, DiscoveryFacetService, DiscoverySearchService,
    PersistedAuthorEntry, PersistedAuthorsScope, PersistedBookBrowseEntry,
};
use komga_application::media_assets::{MediaImportService, MetadataWriter};
use komga_application::operational::OperationalMetricsPort;
use komga_config::env_config::RuntimeConfig;
use komga_config::profile::RuntimeProfile as ConfigRuntimeProfile;
use komga_infrastructure::content_resolver::ContentResolver;
use komga_infrastructure::database_handle::DatabaseHandle;
use komga_infrastructure::discovery_detail_access::DiscoveryDetailAccess;
use komga_infrastructure::discovery_persisted_access::browse::SqliteDiscoveryBrowseService;
use komga_infrastructure::discovery_persisted_access::{
    authors, library_mappings, models, runtime_queries,
};
use komga_infrastructure::event_emitter_adapter::SseBookEventEmitter;
use komga_infrastructure::filesystem::import::FilesystemImportPort;
use komga_infrastructure::library_catalog::LibraryCatalogAccess;
use komga_infrastructure::media_reader::MediaReader;
use komga_infrastructure::metadata::SqliteBookMetadataPort;
use komga_infrastructure::opds_catalog_access::OpdsCatalogAccess;
use komga_infrastructure::opds_persisted_access::OpdsPersistedAccess;
use komga_infrastructure::operational_metrics_access::OperationalMetricsAccess;
use komga_infrastructure::operational_settings_access::{self, OperationalSettingsAccess};
use komga_infrastructure::progress_writer::ProgressWriter;
use komga_infrastructure::runtime_identity_access::IdentityAccess;
use komga_infrastructure::search::index_dirs::{
    register_discovery_index_dir, resolve_discovery_index_dir,
};
use komga_infrastructure::search::index_lifecycle::{SearchEntityType, SearchQueryLifecycle};
use komga_infrastructure::search_sync_adapter::SearchSyncAdapter;
use komga_infrastructure::sqlite::write_models::server_settings::ServerSettingsStore;
use komga_infrastructure::task_enqueue_adapter::TaskEnqueueAdapter;
use komga_infrastructure::thumbnail_writer::ThumbnailWriter;
use komga_interfaces::discovery_auth::state::DiscoveryAuthState;
use komga_interfaces::state::{
    AuthDatabaseState, BookImportSseEvent, HttpAppState, HttpServerRequestsState, HttpServices,
    IdentityState, OAuth2ClientConfig, OperationalBuildMetadata, OperationalState,
    ReadProgressState, RemoteCacheEntry, RuntimeProfile, RuntimeState, SseOperationalState,
    StartupTimingState, TransientBooksStore,
};
use sha2::Digest;
use tokio::sync::watch;

use crate::build_metadata::current_build_metadata;
use crate::runtime::HttpRuntimeParts;

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
    let identity = IdentityState::new(Arc::new(IdentityAccess::new(db.clone())));
    let operational_runtime_service: Arc<dyn OperationalMetricsPort> =
        Arc::new(OperationalMetricsAccess::new(db.clone(), tasks_db));
    let discovery_detail: Arc<dyn DiscoveryDetailPort> = Arc::new(DiscoveryDetailAccess::new(
        db.clone(),
        config.lucene_data_directory.clone(),
    ));
    let discovery_search: Arc<dyn DiscoverySearchService> = Arc::new(
        RuntimePersistedDiscoveryAccess::new(db.clone(), config.lucene_data_directory.clone()),
    );
    let discovery_browse_service = Arc::new(compose_discovery_browse_service(
        db.clone(),
        config.lucene_data_directory.clone(),
    ));
    let discovery_browse: Arc<dyn DiscoveryBrowseService> = discovery_browse_service.clone();
    let discovery_facets: Arc<dyn DiscoveryFacetService> = discovery_browse_service;
    let opds_catalog: Arc<dyn komga_application::opds::OpdsCatalogPort> =
        Arc::new(OpdsCatalogAccess::new(db.clone()));
    let opds_persisted: Arc<dyn komga_application::opds::OpdsPersistedPort> = Arc::new(
        OpdsPersistedAccess::new(db.clone(), config.lucene_data_directory.clone()),
    );
    let operational_settings_service = Arc::new(OperationalSettingsAccess::new(db.clone()));

    let remember_me_runtime_key = runtime_identity_key(config.database_file.as_path());
    identity.sync_remember_me_runtime_database_file(remember_me_runtime_key.as_str());
    preload_remember_me_runtime_settings(config, remember_me_runtime_key.as_str(), &identity);
    // The current registry still derives both token families from the same configured root,
    // but the HTTP state keeps separate runtime keys so session and remember-me semantics are explicit.
    let session_runtime_key = remember_me_runtime_key.clone();
    identity.sync_session_runtime_settings(
        session_runtime_key.as_str(),
        config.session_max_inactive_seconds,
    );

    let read_progress = ReadProgressState {
        progress_by_token: Arc::new(Mutex::new(HashMap::new())),
    };
    let profile = runtime_profile(config);
    let discovery_auth = DiscoveryAuthState::default();
    let auth_db = AuthDatabaseState {
        database_file: db.database_file().to_path_buf(),
        demo_mode: config.demo_mode,
        session_runtime_key,
        remember_me_runtime_key: remember_me_runtime_key.clone(),
    };
    let task_engine_arc: Arc<dyn komga_application::task_processing::TaskQueueAdmin> =
        Arc::from(task_engine);
    let metadata_writer = Arc::new(MetadataWriter::new(
        Box::new(SqliteBookMetadataPort::new(
            db.read_pool().clone(),
            db.write_pool().clone(),
        )),
        Box::new(SearchSyncAdapter::new(
            db.write_pool().clone(),
            config.database_file.clone(),
            config.lucene_data_directory.clone(),
        )),
        Box::new(TaskEnqueueAdapter::new(task_engine_arc.clone())),
        Box::new(SseBookEventEmitter),
    ));
    let services = HttpServices {
        library_catalog: Arc::new(LibraryCatalogAccess::new(
            db.read_pool().clone(),
            db.write_pool().clone(),
        )),
        task_queue: task_engine_arc,
        server_settings: Arc::new(ServerSettingsStore::new(config.database_file.clone())),
        identity,
        operational_runtime: operational_runtime_service,
        operational_settings: operational_settings_service,
        opds_catalog,
        opds_persisted,
        discovery_search,
        discovery_detail,
        discovery_browse,
        discovery_facets,
        media_reader: Arc::new(MediaReader::new(db.read_pool().clone())),
        content_resolver: Arc::new(ContentResolver),
        thumbnail_writer: Arc::new(ThumbnailWriter::new(db.write_pool().clone())),
        progress_writer: Arc::new(ProgressWriter::new(db.write_pool().clone())),
        metadata_writer,
        import_service: Arc::new(MediaImportService::new(Arc::new(
            FilesystemImportPort::new(db.database_file().to_path_buf()),
        ))),
    };
    let operational = compose_operational_state(
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

fn runtime_profile(config: &RuntimeConfig) -> RuntimeProfile {
    match config.runtime_profile {
        ConfigRuntimeProfile::SnapshotAligned => RuntimeProfile::SnapshotAligned,
        ConfigRuntimeProfile::LiveLocaldb => RuntimeProfile::LiveLocaldb,
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
    identity: &IdentityState,
) {
    let (remember_me_key, remember_me_duration_days) =
        operational_settings_access::load_remember_me_runtime_settings(
            config.database_file.as_path(),
        )
        .expect("remember-me startup settings should load");
    identity.sync_remember_me_runtime_settings(
        remember_me_runtime_key,
        remember_me_key.as_str(),
        remember_me_duration_days,
    );
}

fn compose_discovery_browse_service(
    db: DatabaseHandle,
    lucene_data_directory: PathBuf,
) -> SqliteDiscoveryBrowseService {
    register_discovery_index_dir(db.database_file(), lucene_data_directory.as_path());
    SqliteDiscoveryBrowseService::new(db, lucene_data_directory)
}

fn compose_operational_state(
    config: &RuntimeConfig,
    startup_timing: StartupTimingState,
    remember_me_runtime_key: String,
    shutdown_trigger: Option<watch::Sender<bool>>,
) -> OperationalState {
    let build_metadata = current_build_metadata();
    OperationalState {
        runtime: RuntimeState {
            tasks_db_file: config.tasks_db_file.clone(),
            lucene_data_directory: config.lucene_data_directory.clone(),
            fonts_data_directory: config.fonts_data_directory.clone(),
            log_file: config.log_file.clone(),
            config_dir: config.config_dir.clone(),
            bind_address: config.bind_address,
            configuration_bind_address: config.configuration_bind_address,
            server_context_path: config.server_context_path.clone(),
            configuration_server_context_path: config.configuration_server_context_path.clone(),
        },
        startup_timing,
        http_server_requests: HttpServerRequestsState::default(),
        remember_me_runtime_key,
        build_metadata: OperationalBuildMetadata {
            version: build_metadata.version,
            build_time: build_metadata.build_time,
            git_branch: build_metadata.git_branch,
            git_commit_id: build_metadata.git_commit_id,
            git_commit_time: build_metadata.git_commit_time,
        },
        oauth2_clients: oauth2_clients(config),
        oauth2_account_creation: config.oauth2_account_creation,
        oidc_email_verification: config.oidc_email_verification,
        sse: Arc::new(Mutex::new(SseOperationalState {
            accepting_connections: true,
            book_import_events: Vec::<BookImportSseEvent>::new(),
            session_expired_events: Vec::new(),
            next_session_expired_event_id: 1,
        })),
        announcements_cache: Arc::new(Mutex::new(None::<RemoteCacheEntry>)),
        releases_cache: Arc::new(Mutex::new(None::<RemoteCacheEntry>)),
        transient_books: Arc::new(Mutex::new(TransientBooksStore::default())),
        shutdown_trigger,
    }
}

fn oauth2_clients(config: &RuntimeConfig) -> Vec<OAuth2ClientConfig> {
    config
        .oauth2_clients
        .iter()
        .map(|client| OAuth2ClientConfig {
            registration_id: client.registration_id.clone(),
            client_name: client.client_name.clone(),
            client_id: client.client_id.clone(),
            client_secret: client.client_secret.clone(),
            authorization_uri: client.authorization_uri.clone(),
            token_uri: client.token_uri.clone(),
            user_info_uri: client.user_info_uri.clone(),
            scopes: client.scopes.clone(),
        })
        .collect()
}

// Discovery search service runtime adapter — kept here because its only purpose
// is to wire infrastructure persisted-access modules into the
// `DiscoverySearchService` trait that the interfaces layer depends on.
fn search_ids_or_empty(
    index_dir: &Path,
    query: &str,
    entity_type: SearchEntityType,
    limit: usize,
) -> Vec<String> {
    let Ok(index) = SearchQueryLifecycle::bootstrap(index_dir) else {
        return Vec::new();
    };

    index
        .search_ids(query, entity_type, limit)
        .unwrap_or_default()
}

fn search_scored_ids_or_empty(
    index_dir: &Path,
    query: &str,
    entity_type: SearchEntityType,
    limit: usize,
) -> Vec<(f32, String)> {
    let Ok(index) = SearchQueryLifecycle::bootstrap(index_dir) else {
        return Vec::new();
    };

    index
        .search_scored_ids(query, entity_type, limit)
        .unwrap_or_default()
}

fn persisted_book_browse_entry(row: models::BookBrowseEntry) -> PersistedBookBrowseEntry {
    PersistedBookBrowseEntry {
        id: row.id,
        library_id: row.library_id,
        name: row.name,
        title: row.title,
    }
}

#[derive(Clone)]
struct RuntimePersistedDiscoveryAccess {
    db: DatabaseHandle,
    index_dir: PathBuf,
}

impl RuntimePersistedDiscoveryAccess {
    fn new(db: DatabaseHandle, index_dir: PathBuf) -> Self {
        Self { db, index_dir }
    }
}

#[async_trait::async_trait]
impl DiscoverySearchService for RuntimePersistedDiscoveryAccess {
    async fn load_author_names(
        &self,
        search: &str,
        authorized_library_ids: Option<&[String]>,
    ) -> Result<Vec<String>, String> {
        authors::load_persisted_author_names(self.db.read_pool(), search, authorized_library_ids)
            .await
    }

    async fn load_author_roles(
        &self,
        authorized_library_ids: Option<&[String]>,
    ) -> Result<Vec<String>, String> {
        authors::load_persisted_author_roles(self.db.read_pool(), authorized_library_ids).await
    }

    async fn load_authors_by_scope(
        &self,
        scope: PersistedAuthorsScope,
        authorized_library_ids: Option<&[String]>,
    ) -> Result<Vec<PersistedAuthorEntry>, String> {
        let mapped_scope = match scope {
            PersistedAuthorsScope::All => models::AuthorsScope::All,
            PersistedAuthorsScope::Libraries(ids) => models::AuthorsScope::Libraries(ids),
            PersistedAuthorsScope::Collection(id) => models::AuthorsScope::Collection(id),
            PersistedAuthorsScope::Series(id) => models::AuthorsScope::Series(id),
            PersistedAuthorsScope::ReadList(id) => models::AuthorsScope::ReadList(id),
        };
        let rows = authors::load_persisted_authors_by_scope(
            self.db.read_pool(),
            &mapped_scope,
            authorized_library_ids,
        )
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| PersistedAuthorEntry {
                name: row.name,
                role: row.role,
            })
            .collect())
    }

    async fn load_persisted_library_ids(&self) -> Result<Vec<String>, String> {
        library_mappings::load_persisted_library_ids(self.db.read_pool()).await
    }

    async fn search_collection_ids(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<String>, String> {
        Ok(search_ids_or_empty(
            resolve_discovery_index_dir(self.db.database_file(), self.index_dir.as_path())
                .as_path(),
            query,
            SearchEntityType::Collection,
            limit,
        ))
    }

    async fn search_readlist_scored_ids(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(f32, String)>, String> {
        Ok(search_scored_ids_or_empty(
            resolve_discovery_index_dir(self.db.database_file(), self.index_dir.as_path())
                .as_path(),
            query,
            SearchEntityType::ReadList,
            limit,
        ))
    }

    async fn load_ondeck_books(
        &self,
        user_id: &str,
    ) -> Result<Vec<PersistedBookBrowseEntry>, String> {
        runtime_queries::load_persisted_ondeck_books(self.db.read_pool(), user_id)
            .await
            .map(|rows| rows.into_iter().map(persisted_book_browse_entry).collect())
    }

    async fn load_duplicate_books(&self) -> Result<Vec<PersistedBookBrowseEntry>, String> {
        runtime_queries::load_persisted_duplicate_books(self.db.read_pool())
            .await
            .map(|rows| rows.into_iter().map(persisted_book_browse_entry).collect())
    }
}

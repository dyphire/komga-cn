use super::*;
use komga_application::discovery::{
    BookDetailPort, CollectionPort, DiscoveryBrowseService, DiscoveryFacetService, ReadlistPort,
    SeriesDetailPort,
};
use komga_application::operational::{HttpServerRequestsState, StartupTimingState};

#[derive(Clone)]
pub struct HttpServices {
    pub library_catalog: Arc<dyn komga_application::library_catalog::LibraryCatalogPort>,
    pub task_queue: Arc<dyn TaskQueueAdmin>,
    pub server_settings: Arc<dyn komga_application::operational::ServerSettingsPort>,
    pub server_settings_control: Arc<komga_application::operational::ServerSettingsService>,
    pub identity: IdentityState,
    pub operational_runtime: Arc<dyn komga_application::operational::OperationalMetricsPort>,
    pub actuator_snapshots: Arc<dyn komga_application::operational::ActuatorSnapshotPort>,
    pub remote_feeds: Arc<komga_application::operational::RemoteFeedService>,
    pub claim: Arc<dyn komga_application::operational::ClaimPort>,
    pub client_settings: Arc<dyn komga_application::operational::ClientSettingsPort>,
    pub filesystem_browse: Arc<dyn komga_application::operational::FilesystemBrowsePort>,
    pub fonts: Arc<dyn komga_application::operational::FontPort>,
    pub history: Arc<dyn komga_application::operational::HistoryPort>,
    pub page_hashes: Arc<dyn komga_application::operational::PageHashPort>,
    pub page_hash_control: Arc<komga_application::operational::PageHashService>,
    pub syncpoints: Arc<dyn komga_application::operational::SyncpointPort>,
    pub transient_books: Arc<komga_application::operational::TransientBookService>,
    pub opds_catalog: Arc<dyn komga_application::opds::OpdsCatalogPort>,
    pub opds_persisted: Arc<dyn komga_application::opds::OpdsPersistedPort>,
    pub discovery_search: Arc<dyn DiscoverySearchService>,
    pub book_detail: Arc<dyn BookDetailPort>,
    pub series_detail: Arc<dyn SeriesDetailPort>,
    pub collection: Arc<dyn CollectionPort>,
    pub readlist: Arc<dyn ReadlistPort>,
    pub discovery_browse: Arc<dyn DiscoveryBrowseService>,
    pub discovery_facets: Arc<dyn DiscoveryFacetService>,
    pub media_reader: Arc<dyn komga_application::media_assets::MediaReaderPort>,
    pub content_resolver: Arc<dyn komga_application::media_assets::ContentResolverPort>,
    pub thumbnail_writer: Arc<dyn komga_application::media_assets::ThumbnailWriterPort>,
    pub progress_writer: Arc<dyn komga_application::media_assets::ProgressWriterPort>,
    pub read_progress_service: Arc<komga_application::media_assets::ReadProgressService>,
    pub metadata_writer: Arc<komga_application::media_assets::MetadataWriter>,
    pub import_service: Arc<komga_application::media_assets::MediaImportService>,
}

pub struct HttpAppState {
    pub profile: RuntimeProfile,
    pub read_progress: ReadProgressState,
    pub discovery_auth: DiscoveryAuthState,
    pub auth_db: AuthDatabaseState,
    pub operational: OperationalState,
    pub services: HttpServices,
}

#[derive(Clone)]
pub struct OperationalState {
    pub runtime: RuntimeState,
    pub startup_timing: StartupTimingState,
    pub http_server_requests: HttpServerRequestsState,
    pub remember_me_runtime_key: String,
    pub build_metadata: OperationalBuildMetadata,
    pub oauth2_clients: Vec<OAuth2ClientConfig>,
    pub oauth2_account_creation: bool,
    pub oidc_email_verification: bool,
    pub sse: Arc<Mutex<SseOperationalState>>,
    pub shutdown_trigger: Option<watch::Sender<bool>>,
}

#[derive(Clone, Default)]
pub struct SseOperationalState {
    pub accepting_connections: bool,
    pub book_import_events: Vec<BookImportSseEvent>,
    pub session_expired_events: Vec<SessionExpiredSseEvent>,
    pub next_session_expired_event_id: u64,
}

#[derive(Clone)]
pub struct BookImportSseEvent {
    pub book_id: Option<String>,
    pub source_file: String,
    pub success: bool,
    pub message: Option<String>,
}

#[derive(Clone)]
pub struct SessionExpiredSseEvent {
    pub id: u64,
    pub user_id: String,
}

#[derive(Clone, Default)]
pub struct ReadProgressState {
    pub progress_by_token: Arc<Mutex<HashMap<String, HashMap<String, ReadProgress>>>>,
}

#[derive(Clone)]
pub struct ReadProgress;

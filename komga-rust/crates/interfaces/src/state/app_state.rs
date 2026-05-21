use super::*;
use komga_application::discovery::{
    BookDetailPort, CollectionPort, DiscoveryBrowseService, DiscoveryFacetService, ReadlistPort,
    SeriesDetailPort,
};

#[derive(Clone)]
pub struct HttpServices {
    pub library_catalog: Arc<dyn komga_application::library_catalog::LibraryCatalogPort>,
    pub task_queue: Arc<dyn TaskQueueAdmin>,
    pub server_settings: Arc<dyn komga_application::operational::ServerSettingsPort>,
    pub identity: IdentityState,
    pub operational_runtime: Arc<dyn komga_application::operational::OperationalMetricsPort>,
    pub announcements: Arc<dyn komga_application::operational::AnnouncementPort>,
    pub claim: Arc<dyn komga_application::operational::ClaimPort>,
    pub client_settings: Arc<dyn komga_application::operational::ClientSettingsPort>,
    pub filesystem_browse: Arc<dyn komga_application::operational::FilesystemBrowsePort>,
    pub fonts: Arc<dyn komga_application::operational::FontPort>,
    pub history: Arc<dyn komga_application::operational::HistoryPort>,
    pub page_hashes: Arc<dyn komga_application::operational::PageHashPort>,
    pub syncpoints: Arc<dyn komga_application::operational::SyncpointPort>,
    pub transient_books: Arc<dyn komga_application::operational::TransientBookPort>,
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
    pub announcements_cache: Arc<Mutex<Option<RemoteCacheEntry>>>,
    pub releases_cache: Arc<Mutex<Option<RemoteCacheEntry>>>,
    pub transient_books: Arc<Mutex<TransientBooksStore>>,
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

#[derive(Clone)]
pub struct OperationalSettings {
    pub delete_empty_collections: bool,
    pub delete_empty_read_lists: bool,
    pub remember_me_key: String,
    pub remember_me_duration_days: u64,
    pub thumbnail_size: &'static str,
    pub task_pool_size: u64,
    pub server_port: Option<u16>,
    pub server_context_path: Option<String>,
    pub kobo_proxy: bool,
    pub kobo_port: Option<u16>,
}

impl OperationalSettings {
    pub fn from_runtime() -> Self {
        Self {
            delete_empty_collections: false,
            delete_empty_read_lists: false,
            remember_me_key: String::new(),
            remember_me_duration_days: 365,
            thumbnail_size: "DEFAULT",
            task_pool_size: 1,
            server_port: None,
            server_context_path: None,
            kobo_proxy: false,
            kobo_port: None,
        }
    }
}

#[derive(Clone)]
pub struct RemoteCacheEntry {
    pub fetched_at_epoch_seconds: u64,
    pub payload: Value,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct TransientBooksStore {
    pub records: HashMap<String, TransientBookRecord>,
    #[serde(default)]
    last_access_epoch_seconds: HashMap<String, i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TransientBookRecord {
    pub id: String,
    pub name: String,
    pub path: String,
    pub file_last_modified_unix_nanos: i128,
    pub size_bytes: u64,
    pub status: String,
    pub media_type: String,
    #[serde(default)]
    pub page_count: u32,
    #[serde(default)]
    pub pages: Vec<TransientBookPageRecord>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub number: Option<f64>,
    #[serde(default)]
    pub series_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TransientBookPageRecord {
    pub number: u32,
    pub file_name: String,
    pub media_type: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub size_bytes: Option<u64>,
}

impl TransientBooksStore {
    pub fn with_records(records: HashMap<String, TransientBookRecord>) -> Self {
        let last_access_epoch_seconds = records
            .keys()
            .cloned()
            .map(|id| (id, current_unix_epoch_seconds()))
            .collect();
        Self {
            records,
            last_access_epoch_seconds,
        }
    }

    pub fn get_cloned(&mut self, id: &str) -> Option<TransientBookRecord> {
        self.prune_expired();
        self.touch(id)?;
        self.records.get(id).cloned()
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut TransientBookRecord> {
        self.prune_expired();
        self.touch(id)?;
        self.records.get_mut(id)
    }

    pub fn insert(&mut self, record: TransientBookRecord) {
        self.prune_expired();
        let id = record.id.clone();
        self.last_access_epoch_seconds
            .insert(id.clone(), current_unix_epoch_seconds());
        self.records.insert(id, record);
    }

    fn prune_expired(&mut self) {
        let now = current_unix_epoch_seconds();
        let expired_ids = self
            .last_access_epoch_seconds
            .iter()
            .filter(|(_, last_access)| {
                now.saturating_sub(**last_access)
                    >= TRANSIENT_BOOKS_EXPIRE_AFTER_ACCESS.as_secs() as i64
            })
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();

        for id in expired_ids {
            self.last_access_epoch_seconds.remove(&id);
            self.records.remove(&id);
        }
    }

    fn touch(&mut self, id: &str) -> Option<()> {
        if !self.records.contains_key(id) {
            self.last_access_epoch_seconds.remove(id);
            return None;
        }

        self.last_access_epoch_seconds
            .insert(id.to_string(), current_unix_epoch_seconds());
        Some(())
    }
}

const TRANSIENT_BOOKS_EXPIRE_AFTER_ACCESS: Duration = Duration::from_secs(60 * 60);

fn current_unix_epoch_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[derive(Clone, Default)]
pub struct ReadProgressState {
    pub progress_by_token: Arc<Mutex<HashMap<String, HashMap<String, ReadProgress>>>>,
}

#[derive(Clone)]
pub struct ReadProgress;

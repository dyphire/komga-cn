use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::future::BoxFuture;
use komga_application::library_catalog::{
    CreateLibraryResult, LibraryCatalogMutationError, LibraryChangeSet, LibraryRecord,
    LibraryTaskResult,
};
use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::watch;

use crate::operational_runtime_access::ServerSettingsStore;
use komga_application::task_processing::TaskQueueRecord;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeProfile {
    SnapshotAligned,
    LiveLocaldb,
}

#[derive(Clone)]
pub struct AuthDatabaseState {
    pub database_file: PathBuf,
    pub demo_mode: bool,
    pub session_runtime_key: String,
    pub remember_me_runtime_key: String,
}

#[derive(Clone)]
pub struct RuntimeState {
    pub database_file: PathBuf,
    pub tasks_db_file: PathBuf,
    pub lucene_data_directory: PathBuf,
    pub fonts_data_directory: PathBuf,
    pub log_file: PathBuf,
    pub config_dir: Option<PathBuf>,
    pub bind_address: SocketAddr,
    pub server_context_path: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StartupTimingSnapshot {
    pub application_started_time_seconds: f64,
    pub application_ready_time_seconds: f64,
}

#[derive(Clone, Default)]
pub struct StartupTimingState {
    snapshot: Arc<Mutex<StartupTimingSnapshot>>,
}

impl StartupTimingState {
    pub fn snapshot(&self) -> StartupTimingSnapshot {
        self.snapshot
            .lock()
            .expect("startup timing snapshot lock should not be poisoned")
            .clone()
    }

    pub fn record_application_started(&self, elapsed: Duration) {
        self.snapshot
            .lock()
            .expect("startup timing snapshot lock should not be poisoned")
            .application_started_time_seconds = elapsed.as_secs_f64();
    }

    pub fn record_application_ready(&self, elapsed: Duration) {
        self.snapshot
            .lock()
            .expect("startup timing snapshot lock should not be poisoned")
            .application_ready_time_seconds = elapsed.as_secs_f64();
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct HttpServerRequestMetricKey {
    pub exception: String,
    pub method: String,
    pub outcome: String,
    pub status: String,
    pub uri: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HttpServerRequestMetricSummary {
    pub count: u64,
    pub total_time_seconds: f64,
    pub max_time_seconds: f64,
}

#[derive(Clone, Default)]
pub struct HttpServerRequestsState {
    metrics: Arc<Mutex<HashMap<HttpServerRequestMetricKey, HttpServerRequestMetricSummary>>>,
}

impl HttpServerRequestsState {
    pub fn record(&self, key: HttpServerRequestMetricKey, elapsed: Duration) {
        let elapsed_seconds = elapsed.as_secs_f64();
        let mut metrics = self
            .metrics
            .lock()
            .expect("http server request metrics lock should not be poisoned");
        let entry = metrics.entry(key).or_default();
        entry.count += 1;
        entry.total_time_seconds += elapsed_seconds;
        entry.max_time_seconds = entry.max_time_seconds.max(elapsed_seconds);
    }

    pub fn snapshot(&self) -> Vec<(HttpServerRequestMetricKey, HttpServerRequestMetricSummary)> {
        self.metrics
            .lock()
            .expect("http server request metrics lock should not be poisoned")
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationalBuildMetadata {
    pub version: String,
    pub build_time: String,
    pub git_branch: Option<String>,
    pub git_commit_id: Option<String>,
    pub git_commit_time: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OAuth2ClientConfig {
    pub registration_id: String,
    pub client_name: String,
    pub client_id: String,
    pub client_secret: String,
    pub authorization_uri: String,
    pub token_uri: String,
    pub user_info_uri: Option<String>,
    pub scopes: Vec<String>,
}

pub type EnqueueTaskRecords =
    Arc<dyn Fn(Vec<TaskQueueRecord>, bool) -> Result<(), String> + Send + Sync>;

pub type ClearUnownedTasks = Arc<dyn Fn() -> usize + Send + Sync>;

pub type CountTaskQueueByType = Arc<dyn Fn() -> BTreeMap<String, usize> + Send + Sync>;

pub type ApplyTaskPoolSize = Arc<dyn Fn(usize) -> Result<(), String> + Send + Sync>;

pub type LoadTransientBooksRecords =
    Arc<dyn Fn() -> Result<HashMap<String, TransientBookRecord>, String> + Send + Sync>;

pub type PersistTransientBooksRecords =
    Arc<dyn Fn(&HashMap<String, TransientBookRecord>) -> Result<(), String> + Send + Sync>;

pub type ListLibraries = Arc<
    dyn Fn(DiscoveryQueryContext) -> BoxFuture<'static, Result<Vec<LibraryRecord>, DiscoveryError>>
        + Send
        + Sync,
>;

pub type GetLibrary = Arc<
    dyn Fn(
            DiscoveryQueryContext,
            String,
        ) -> BoxFuture<'static, Result<Option<LibraryRecord>, DiscoveryError>>
        + Send
        + Sync,
>;

pub type CreateLibrary = Arc<
    dyn Fn(
            LibraryChangeSet,
        ) -> BoxFuture<'static, Result<CreateLibraryResult, LibraryCatalogMutationError>>
        + Send
        + Sync,
>;

pub type UpdateLibrary = Arc<
    dyn Fn(
            String,
            LibraryChangeSet,
        ) -> BoxFuture<'static, Result<LibraryTaskResult, LibraryCatalogMutationError>>
        + Send
        + Sync,
>;

pub type DeleteLibrary = Arc<
    dyn Fn(String) -> BoxFuture<'static, Result<bool, LibraryCatalogMutationError>> + Send + Sync,
>;

pub type LibraryTaskOperation = Arc<
    dyn Fn(String) -> BoxFuture<'static, Result<LibraryTaskResult, LibraryCatalogMutationError>>
        + Send
        + Sync,
>;

pub type ScanLibrary = Arc<
    dyn Fn(
            String,
            bool,
        ) -> BoxFuture<'static, Result<LibraryTaskResult, LibraryCatalogMutationError>>
        + Send
        + Sync,
>;

#[derive(Clone)]
pub struct LibraryCatalogOperations {
    pub list_libraries: ListLibraries,
    pub get_library: GetLibrary,
    pub create_library: CreateLibrary,
    pub update_library: UpdateLibrary,
    pub delete_library: DeleteLibrary,
    pub scan_library: ScanLibrary,
    pub analyze_library: LibraryTaskOperation,
    pub refresh_metadata: LibraryTaskOperation,
    pub empty_trash: LibraryTaskOperation,
}

#[derive(Clone)]
pub struct OperationalState {
    pub runtime: RuntimeState,
    pub startup_timing: StartupTimingState,
    pub http_server_requests: HttpServerRequestsState,
    pub remember_me_runtime_key: String,
    pub build_metadata: OperationalBuildMetadata,
    pub settings_store: Arc<ServerSettingsStore>,
    pub oauth2_clients: Arc<Vec<OAuth2ClientConfig>>,
    pub oauth2_account_creation: bool,
    pub oidc_email_verification: bool,
    pub enqueue_task_records: EnqueueTaskRecords,
    pub clear_unowned_tasks: ClearUnownedTasks,
    pub count_task_queue_by_type: CountTaskQueueByType,
    pub apply_task_pool_size: ApplyTaskPoolSize,
    pub library_catalog: LibraryCatalogOperations,
    pub sse: Arc<Mutex<SseOperationalState>>,
    pub announcements_cache: Arc<Mutex<Option<RemoteCacheEntry>>>,
    pub releases_cache: Arc<Mutex<Option<RemoteCacheEntry>>>,
    pub load_transient_books_records: LoadTransientBooksRecords,
    pub persist_transient_books_records: PersistTransientBooksRecords,
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

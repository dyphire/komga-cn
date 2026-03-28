use axum::Router;
use axum::middleware;
use axum::routing::{delete, get, patch, post, put};
use komga_persistence::server_settings::ServerSettingsStore;
use komga_persistence::sqlite::connect_pool;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::runtime::Handle;
use tokio::sync::watch;
use tokio::time::interval;

use crate::app::discovery_auth::DiscoveryAuthState;
use crate::app::runtime_auth::{
    configure_remember_me_store, persisted_cleanup_authentication_activity,
};
use crate::config::RuntimeConfig;
use crate::task_queue::{
    LibraryScanInterval, LibraryScanScheduler, ScheduledLibraryScan, TaskQueueRecord,
    TaskQueueScheduler,
};

pub(crate) mod content;
mod device_auth;
mod operational;

const LAST_MODIFIED: &str = "Mon, 01 Jan 2024 22:04:05 GMT";
const THUMBNAIL_ETAG: &str = "\"048bbf960d13687d84948688ab74aaa59\"";
const CACHE_CONTROL_PRIVATE: &str = "max-age=0, must-revalidate, private";
const SEARCH_OWNERSHIP_HEADER: &str = "x-komga-compat-search-ownership";
const SHADOW_JAVA_WRITER_MARKER: &str = "shadow-java-writer";
const DEV_FRONTEND_ORIGIN: &str = "http://127.0.0.1:8081";
const DEV_CORS_ALLOW_METHODS: &str = "GET,POST,PATCH,DELETE,OPTIONS";
const DEV_CORS_ALLOW_HEADERS: &str = "authorization,x-auth-token,content-type,x-api-key,x-komga-email,x-komga-password,x-requested-with";

#[derive(Clone)]
struct OperationalState {
    runtime: RuntimeConfig,
    webui_assets_root: Option<PathBuf>,
    settings_store: Arc<ServerSettingsStore>,
    task_queue: Arc<Mutex<TaskQueueScheduler>>,
    sse: Arc<Mutex<SseOperationalState>>,
    announcements_cache: Arc<Mutex<Option<RemoteCacheEntry>>>,
    releases_cache: Arc<Mutex<Option<RemoteCacheEntry>>>,
    transient_books: Arc<Mutex<TransientBooksStore>>,
    oauth2_clients: Arc<Vec<crate::config::OAuth2ClientConfig>>,
    shutdown_trigger: Option<watch::Sender<bool>>,
}

#[derive(Clone)]
pub(super) struct AuthDatabaseState {
    pub(super) database_file: PathBuf,
    pub(super) remember_me_namespace: String,
}

#[derive(Clone, Default)]
struct SseOperationalState {
    accepting_connections: bool,
    book_import_events: Vec<BookImportSseEvent>,
}

#[derive(Clone)]
struct BookImportSseEvent {
    book_id: Option<String>,
    source_file: String,
    success: bool,
    message: Option<String>,
}

#[derive(Clone)]
struct OperationalSettings {
    delete_empty_collections: bool,
    delete_empty_read_lists: bool,
    remember_me_key: String,
    remember_me_duration_days: u64,
    thumbnail_size: &'static str,
    task_pool_size: u64,
    server_port: Option<u16>,
    server_context_path: Option<String>,
    kobo_proxy: bool,
    kobo_port: Option<u16>,
    kepubify_path: Option<String>,
}

impl OperationalSettings {
    fn from_runtime(_runtime: &RuntimeConfig) -> Self {
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
            kepubify_path: None,
        }
    }
}

#[derive(Clone)]
struct RemoteCacheEntry {
    fetched_at_epoch_seconds: u64,
    payload: Value,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct TransientBooksStore {
    records: HashMap<String, TransientBookRecord>,
    state_file: Option<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TransientBookRecord {
    id: String,
    name: String,
    path: String,
    file_last_modified_epoch_seconds: i64,
    size_bytes: u64,
    status: String,
    media_type: String,
    #[serde(default)]
    pages: Vec<TransientBookPageRecord>,
    #[serde(default)]
    files: Vec<String>,
    #[serde(default)]
    comment: String,
    #[serde(default)]
    number: Option<f64>,
    #[serde(default)]
    series_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TransientBookPageRecord {
    number: u32,
    file_name: String,
    media_type: String,
    width: Option<u32>,
    height: Option<u32>,
    size_bytes: Option<u64>,
}

impl TransientBooksStore {
    fn load(state_file: Option<PathBuf>) -> Self {
        let mut store = Self {
            records: HashMap::new(),
            state_file,
        };
        store.reload_from_disk();
        store
    }

    fn reload_from_disk(&mut self) {
        let Some(state_file) = self.state_file.as_ref() else {
            return;
        };
        let Ok(content) = fs::read_to_string(state_file) else {
            return;
        };
        let Ok(records) = serde_json::from_str::<HashMap<String, TransientBookRecord>>(&content)
        else {
            return;
        };
        self.records = records;
    }

    fn persist(&self) {
        let Some(state_file) = self.state_file.as_ref() else {
            return;
        };
        if let Some(parent) = state_file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(content) = serde_json::to_string_pretty(&self.records) {
            let _ = fs::write(state_file, content);
        }
    }
}

#[derive(Clone)]
struct ReadProgressState {
    progress_by_token: Arc<Mutex<HashMap<String, HashMap<String, ReadProgress>>>>,
}

#[derive(Clone)]
struct ReadProgress;

pub(super) fn build_router(
    config: &RuntimeConfig,
    shutdown_trigger: Option<watch::Sender<bool>>,
    startup_search_task: Option<&'static str>,
) -> Router {
    let remember_me_store_root = config
        .config_dir
        .as_deref()
        .or_else(|| config.database_file.parent())
        .unwrap_or_else(|| Path::new("."));
    let remember_me_namespace = configure_remember_me_store(remember_me_store_root);

    let state = ReadProgressState {
        progress_by_token: Arc::new(Mutex::new(HashMap::new())),
    };
    let profile = config.app_compat_profile();
    let discovery_auth = DiscoveryAuthState::default();
    let auth_db = AuthDatabaseState {
        database_file: config.database_file.clone(),
        remember_me_namespace,
    };
    let mut task_queue = TaskQueueScheduler::for_runtime(config.clone(), "rust-compat-runtime");
    let _ = task_queue.disown_all();
    let scheduled_scans = bootstrap_startup_library_scans(&mut task_queue, config);
    bootstrap_startup_search_task(&mut task_queue, config, startup_search_task);
    let task_queue = Arc::new(Mutex::new(task_queue));
    spawn_periodic_library_scan_workers(task_queue.clone(), config.clone(), scheduled_scans);
    spawn_background_task_worker(task_queue.clone(), config.clone());
    spawn_authentication_activity_cleanup_worker(config.clone());

    let operational = OperationalState {
        runtime: config.clone(),
        webui_assets_root: config.discover_webui_assets_layout(),
        settings_store: Arc::new(ServerSettingsStore::new(config.database_file.clone())),
        task_queue: task_queue.clone(),
        sse: Arc::new(Mutex::new(SseOperationalState {
            accepting_connections: true,
            book_import_events: Vec::new(),
        })),
        announcements_cache: Arc::new(Mutex::new(None)),
        releases_cache: Arc::new(Mutex::new(None)),
        transient_books: Arc::new(Mutex::new(TransientBooksStore::load(Some(
            transient_books_state_file(config),
        )))),
        oauth2_clients: Arc::new(config.oauth2_clients.clone()),
        shutdown_trigger,
    };

    let router = Router::new()
        .route(
            "/api/v1/settings",
            get(operational::get_server_settings).patch(operational::update_server_settings),
        )
        .route(
            "/api/v1/announcements",
            get(operational::get_announcements).put(operational::put_announcements),
        )
        .route("/api/v1/releases", get(operational::get_releases))
        .route("/api/v1/filesystem", post(operational::post_filesystem))
        .route(
            "/api/v1/fonts/families",
            get(operational::get_fonts_families),
        )
        .route(
            "/api/v1/fonts/resource/{font_family}/{font_file}",
            get(operational::get_font_file),
        )
        .route(
            "/api/v1/fonts/resource/{font_family}/css",
            get(operational::get_font_family_css),
        )
        .route("/api/v1/history", get(operational::get_history))
        .route(
            "/api/v1/page-hashes",
            get(operational::get_page_hashes).put(operational::put_page_hash),
        )
        .route(
            "/api/v1/page-hashes/unknown",
            get(operational::get_page_hashes_unknown),
        )
        .route(
            "/api/v1/page-hashes/unknown/{page_hash}/thumbnail",
            get(operational::get_page_hash_unknown_thumbnail),
        )
        .route(
            "/api/v1/page-hashes/{page_hash}",
            get(operational::get_page_hash_matches),
        )
        .route(
            "/api/v1/page-hashes/{page_hash}/delete-all",
            post(operational::post_page_hash_delete_all),
        )
        .route(
            "/api/v1/page-hashes/{page_hash}/delete-match",
            post(operational::post_page_hash_delete_match),
        )
        .route(
            "/api/v1/page-hashes/{page_hash}/thumbnail",
            get(operational::get_page_hash_thumbnail),
        )
        .route(
            "/api/v1/transient-books",
            post(operational::post_transient_books),
        )
        .route(
            "/api/v1/transient-books/{transient_book_id}/analyze",
            post(operational::post_transient_book_analyze),
        )
        .route(
            "/api/v1/transient-books/{transient_book_id}/status",
            get(operational::get_transient_book_status),
        )
        .route(
            "/api/v1/transient-books/{transient_book_id}/media",
            get(operational::get_transient_book_media),
        )
        .route(
            "/api/v1/transient-books/{transient_book_id}/pages/{page_number}",
            get(operational::get_transient_book_page),
        )
        .route(
            "/api/v1/claim",
            get(operational::get_claim_status).post(operational::post_claim),
        )
        .route(
            "/api/v1/syncpoints/me",
            delete(operational::delete_syncpoints_me),
        )
        .route(
            "/api/v1/client-settings/global/list",
            get(operational::get_client_settings_global),
        )
        .route(
            "/api/v1/client-settings/global",
            patch(operational::patch_client_settings_global)
                .delete(operational::delete_client_settings_global),
        )
        .route(
            "/api/v1/client-settings/user/list",
            get(operational::get_client_settings_user),
        )
        .route(
            "/api/v1/client-settings/user",
            patch(operational::patch_client_settings_user)
                .delete(operational::delete_client_settings_user),
        )
        .route(
            "/api/v1/oauth2/providers",
            get(operational::get_oauth2_providers),
        )
        .route(
            "/oauth2/authorization/{registration_id}",
            get(device_auth::oauth2_authorization),
        )
        .route(
            "/login/oauth2/code/{registration_id}",
            get(device_auth::oauth2_login_code),
        )
        .route("/kobo/{auth_token}/ping", get(device_auth::kobo_ping))
        .route(
            "/kobo/{auth_token}/v1/initialization",
            get(device_auth::kobo_initialization),
        )
        .route(
            "/kobo/{auth_token}/v1/auth/device",
            post(device_auth::kobo_auth_device),
        )
        .route(
            "/kobo/{auth_token}/v1/library/sync",
            get(device_auth::kobo_library_sync),
        )
        .route(
            "/kobo/{auth_token}/v1/library/{book_id}/metadata",
            get(device_auth::kobo_library_book_metadata),
        )
        .route(
            "/kobo/{auth_token}/v1/library/{book_id}/state",
            get(device_auth::kobo_library_book_state).put(device_auth::kobo_library_book_state_update),
        )
        .route(
            "/kobo/{auth_token}/v1/books/{book_id}/file/epub",
            get(device_auth::kobo_book_file_epub),
        )
        .route(
            "/kobo/{auth_token}/v1/books/{thumbnail_id}/thumbnail/{width}/{height}/{is_greyscale}/image.jpg",
            get(device_auth::kobo_book_thumbnail),
        )
        .route(
            "/kobo/{auth_token}/v1/books/{thumbnail_id}/thumbnail/{width}/{height}/{quality}/{is_greyscale}/image.jpg",
            get(device_auth::kobo_book_thumbnail_with_quality),
        )
        .route(
            "/kobo/{auth_token}/{*path}",
            get(device_auth::kobo_catch_all)
                .put(device_auth::kobo_catch_all)
                .post(device_auth::kobo_catch_all)
                .patch(device_auth::kobo_catch_all)
                .delete(device_auth::kobo_catch_all),
        )
        .route(
            "/koreader/users/create",
            post(device_auth::koreader_user_create),
        )
        .route("/koreader/users/auth", get(device_auth::koreader_user_auth))
        .route(
            "/koreader/syncs/progress/{book_hash}",
            get(device_auth::koreader_get_progress),
        )
        .route(
            "/koreader/syncs/progress",
            put(device_auth::koreader_put_progress),
        )
        .route("/api/v1/tasks", delete(operational::delete_tasks))
        .route(
            "/api/v1/libraries",
            get(content::libraries).post(content::library_create),
        )
        .route(
            "/api/v1/libraries/{library_id}",
            get(content::library_detail)
                .patch(content::library_update)
                .put(content::library_update)
                .delete(content::library_delete),
        )
        .route(
            "/api/v1/libraries/{library_id}/scan",
            post(content::library_scan),
        )
        .route(
            "/api/v1/libraries/{library_id}/analyze",
            post(content::library_analyze),
        )
        .route(
            "/api/v1/libraries/{library_id}/metadata/refresh",
            post(content::library_metadata_refresh),
        )
        .route(
            "/api/v1/libraries/{library_id}/empty-trash",
            post(content::library_empty_trash),
        )
        .route("/api/v1/authors", get(content::authors))
        .route("/api/v1/authors/names", get(content::authors_names))
        .route("/api/v1/authors/roles", get(content::authors_roles))
        .route("/api/v1/genres", get(content::genres))
        .route("/api/v1/tags", get(content::tags))
        .route("/api/v1/tags/series", get(content::series_tags))
        .route("/api/v1/languages", get(content::languages))
        .route("/api/v1/publishers", get(content::publishers))
        .route("/api/v1/age-ratings", get(content::age_ratings))
        .route("/api/v1/sharing-labels", get(content::sharing_labels))
        .route("/api/v1/series", get(content::series))
        .route("/api/v1/series/new", get(content::series_new))
        .route("/api/v1/series/updated", get(content::series_updated))
        .route(
            "/api/v1/series/release-dates",
            get(content::series_release_dates),
        )
        .route("/api/v1/series/latest", get(content::series_latest))
        .route(
            "/api/v1/series/alphabetical-groups",
            get(content::series_alphabetical_groups_deprecated),
        )
        .route("/api/v1/tags/book", get(content::book_tags))
        .route("/api/v1/series/{series_id}", get(content::series_detail))
        .route(
            "/api/v1/series/{series_id}/books",
            get(content::series_books),
        )
        .route(
            "/api/v1/series/{series_id}/collections",
            get(content::series_collections),
        )
        .route(
            "/api/v1/series/{series_id}/thumbnail",
            get(content::series_thumbnail),
        )
        .route(
            "/api/v1/series/{series_id}/thumbnails",
            get(content::series_thumbnails).post(content::series_thumbnail_upload),
        )
        .route(
            "/api/v1/series/{series_id}/thumbnails/{thumbnail_id}",
            get(content::series_thumbnail_by_id).delete(content::series_thumbnail_delete),
        )
        .route(
            "/api/v1/series/{series_id}/thumbnails/{thumbnail_id}/selected",
            put(content::series_thumbnail_select),
        )
        .route(
            "/api/v1/series/{series_id}/metadata",
            patch(content::series_metadata_update),
        )
        .route(
            "/api/v1/series/{series_id}/metadata/refresh",
            post(content::series_metadata_refresh),
        )
        .route(
            "/api/v1/series/{series_id}/analyze",
            post(content::series_analyze),
        )
        .route(
            "/api/v1/series/{series_id}/read-progress",
            post(content::series_read_progress_post).delete(content::series_read_progress_delete),
        )
        .route(
            "/api/v1/series/{series_id}/file",
            get(content::series_file).delete(content::series_file_delete),
        )
        .route("/api/v1/series/list", post(content::series_list))
        .route(
            "/api/v1/series/list/alphabetical-groups",
            post(content::series_alphabetical_groups),
        )
        .route("/api/v1/books", get(content::books))
        .route("/api/v1/books/list", post(content::books_list))
        .route("/api/v1/books/latest", get(content::books_latest))
        .route("/api/v1/books/ondeck", get(content::books_ondeck))
        .route("/api/v1/books/duplicates", get(content::books_duplicates))
        .route("/api/v1/books/{book_id}", get(content::book_detail))
        .route(
            "/api/v1/books/{book_id}/previous",
            get(content::book_sibling_previous),
        )
        .route(
            "/api/v1/books/{book_id}/next",
            get(content::book_sibling_next),
        )
        .route(
            "/api/v1/books/{book_id}/readlists",
            get(content::book_readlists),
        )
        .route(
            "/api/v1/readlists",
            get(content::readlists).post(content::readlist_create),
        )
        .route(
            "/api/v1/readlists/match/comicrack",
            post(content::readlist_match_comicrack),
        )
        .route(
            "/api/v1/readlists/{readlist_id}",
            get(content::readlist_detail)
                .patch(content::readlist_update)
                .delete(content::readlist_delete),
        )
        .route(
            "/api/v1/readlists/{readlist_id}/thumbnail",
            get(content::readlist_thumbnail),
        )
        .route(
            "/api/v1/readlists/{readlist_id}/thumbnails",
            get(content::readlist_thumbnails).post(content::readlist_thumbnail_upload),
        )
        .route(
            "/api/v1/readlists/{readlist_id}/thumbnails/{thumbnail_id}",
            get(content::readlist_thumbnail_by_id).delete(content::readlist_thumbnail_delete),
        )
        .route(
            "/api/v1/readlists/{readlist_id}/thumbnails/{thumbnail_id}/selected",
            put(content::readlist_thumbnail_select),
        )
        .route(
            "/api/v1/readlists/{readlist_id}/books",
            get(content::readlist_books),
        )
        .route(
            "/api/v1/readlists/{readlist_id}/books/{book_id}/previous",
            get(content::readlist_book_sibling_previous),
        )
        .route(
            "/api/v1/readlists/{readlist_id}/books/{book_id}/next",
            get(content::readlist_book_sibling_next),
        )
        .route(
            "/api/v1/readlists/{readlist_id}/read-progress/tachiyomi",
            get(content::readlist_tachiyomi_read_progress_get)
                .put(content::readlist_tachiyomi_read_progress_put),
        )
        .route(
            "/api/v1/readlists/{readlist_id}/file",
            get(content::readlist_file),
        )
        .route(
            "/api/v1/collections",
            get(content::collections).post(content::collection_create),
        )
        .route(
            "/api/v1/collections/{collection_id}/series",
            get(content::collection_series),
        )
        .route(
            "/api/v1/collections/{collection_id}",
            get(content::collection_detail)
                .patch(content::collection_update)
                .delete(content::collection_delete),
        )
        .route(
            "/api/v1/collections/{collection_id}/thumbnail",
            get(content::collection_thumbnail),
        )
        .route(
            "/api/v1/collections/{collection_id}/thumbnails",
            get(content::collection_thumbnails).post(content::collection_thumbnail_upload),
        )
        .route(
            "/api/v1/collections/{collection_id}/thumbnails/{thumbnail_id}",
            get(content::collection_thumbnail_by_id).delete(content::collection_thumbnail_delete),
        )
        .route(
            "/api/v1/collections/{collection_id}/thumbnails/{thumbnail_id}/selected",
            put(content::collection_thumbnail_select),
        )
        .route("/api/v1/books/{book_id}/pages", get(content::book_pages))
        .route(
            "/api/v1/books/{book_id}/positions",
            get(content::book_positions),
        )
        .route(
            "/api/v1/books/{book_id}/pages/{page_number}",
            get(content::book_page),
        )
        .route(
            "/api/v1/books/{book_id}/pages/{page_number}/raw",
            get(content::book_page_raw),
        )
        .route(
            "/api/v1/books/{book_id}/pages/{page_number}/thumbnail",
            get(content::book_page_thumbnail),
        )
        .route(
            "/api/v1/books/{book_id}/thumbnail",
            get(content::book_thumbnail),
        )
        .route(
            "/api/v1/books/{book_id}/thumbnails",
            get(content::book_thumbnails).post(content::book_thumbnail_upload),
        )
        .route(
            "/api/v1/books/{book_id}/thumbnails/{thumbnail_id}",
            get(content::book_thumbnail_by_id).delete(content::book_thumbnail_delete),
        )
        .route(
            "/api/v1/books/{book_id}/thumbnails/{thumbnail_id}/selected",
            put(content::book_thumbnail_select),
        )
        .route(
            "/api/v1/books/{book_id}/manifest",
            get(content::book_manifest),
        )
        .route(
            "/api/v1/books/{book_id}/manifest/epub",
            get(content::book_manifest_epub),
        )
        .route(
            "/api/v1/books/{book_id}/manifest/pdf",
            get(content::book_manifest_pdf),
        )
        .route(
            "/api/v1/books/{book_id}/manifest/divina",
            get(content::book_manifest_divina),
        )
        .route(
            "/api/v1/books/{book_id}/file",
            get(content::book_file).delete(content::book_file_delete),
        )
        .route(
            "/api/v1/books/{book_id}/file/{*file_name}",
            get(content::book_file_with_suffix),
        )
        .route(
            "/api/v1/books/{book_id}/resource/{*resource_path}",
            get(content::book_resource),
        )
        .route(
            "/opds/v2/books/{book_id}/resource/{*resource_path}",
            get(content::book_resource),
        )
        .route(
            "/api/v1/books/thumbnails",
            put(content::books_thumbnails_regenerate),
        )
        .route(
            "/api/v1/books/{book_id}/analyze",
            post(content::book_analyze),
        )
        .route(
            "/api/v1/books/{book_id}/metadata/refresh",
            post(content::book_metadata_refresh),
        )
        .route(
            "/api/v1/books/{book_id}/metadata",
            axum::routing::patch(content::book_metadata_update),
        )
        .route(
            "/api/v1/books/metadata",
            axum::routing::patch(content::book_metadata_batch_update),
        )
        .route("/api/v1/books/import", post(content::books_import))
        .route(
            "/api/v1/books/{book_id}/read-progress",
            get(content::book_read_progress_get)
                .patch(content::book_read_progress)
                .delete(content::book_read_progress_delete),
        )
        .route(
            "/api/v1/books/{book_id}/progression",
            get(content::book_progression_get).patch(content::book_progression),
        )
        .route(
            "/api/v2/users",
            get(content::users_list).post(content::users_create),
        )
        .route("/api/v2/users/me", get(content::users_me))
        .route(
            "/api/v2/users/{id}",
            patch(content::users_update).delete(content::users_delete),
        )
        .route(
            "/api/v2/users/me/password",
            patch(content::users_me_password),
        )
        .route(
            "/api/v2/users/me/api-keys",
            get(content::users_me_api_keys_list).post(content::users_me_api_keys_create),
        )
        .route(
            "/api/v2/users/me/api-keys/{key_id}",
            delete(content::users_me_api_keys_delete),
        )
        .route(
            "/api/v2/users/me/authentication-activity",
            get(content::users_me_authentication_activity),
        )
        .route(
            "/api/v2/users/authentication-activity",
            get(content::users_authentication_activity),
        )
        .route(
            "/api/v2/users/{id}/authentication-activity/latest",
            get(content::users_by_id_authentication_activity_latest),
        )
        .route(
            "/api/v2/series/{series_id}/read-progress/tachiyomi",
            get(content::series_tachiyomi_read_progress_get)
                .put(content::series_tachiyomi_read_progress_put),
        )
        .route(
            "/api/v2/users/{id}/password",
            patch(content::users_by_id_password),
        )
        .route("/api/v2/authors", get(content::authors_v2))
        .route("/opds/v1.2/catalog", get(content::opds_v1_catalog))
        .route("/opds/v1.2/search", get(content::opds_v1_search))
        .route("/opds/v1.2/ondeck", get(content::opds_v1_on_deck))
        .route("/opds/v1.2/keep-reading", get(content::opds_v1_keep_reading))
        .route("/opds/v1.2/series", get(content::opds_v1_series))
        .route("/opds/v1.2/series/latest", get(content::opds_v1_series_latest))
        .route("/opds/v1.2/books/latest", get(content::opds_v1_books_latest))
        .route("/opds/v1.2/libraries", get(content::opds_v1_libraries))
        .route("/opds/v1.2/collections", get(content::opds_v1_collections))
        .route("/opds/v1.2/readlists", get(content::opds_v1_readlists))
        .route("/opds/v1.2/publishers", get(content::opds_v1_publishers))
        .route("/opds/v1.2/series/{series_id}", get(content::opds_v1_series_detail))
        .route("/opds/v1.2/libraries/{library_id}", get(content::opds_v1_library_detail))
        .route("/opds/v1.2/collections/{collection_id}", get(content::opds_v1_collection_detail))
        .route("/opds/v1.2/readlists/{readlist_id}", get(content::opds_v1_readlist_detail))
        .route(
            "/opds/v1.2/books/{book_id}/file/{file_name}",
            get(content::opds_v1_book_file),
        )
        .route("/opds/v1.2/books/{book_id}/thumbnail", get(content::book_thumbnail))
        .route("/opds/v1.2/books/{book_id}/thumbnail/small", get(content::book_thumbnail))
        .route("/opds/v1.2/books/{book_id}/pages/{page_number}", get(content::book_page))
        .route("/opds/v2/auth", get(content::opds_auth))
        .route("/opds/v2/catalog", get(content::opds_catalog))
        .route("/opds/v2/libraries", get(content::opds_v2_libraries))
        .route("/opds/v2/libraries/keep-reading", get(content::opds_v2_libraries_keep_reading))
        .route("/opds/v2/libraries/on-deck", get(content::opds_v2_libraries_on_deck))
        .route("/opds/v2/libraries/books/latest", get(content::opds_v2_libraries_latest_books))
        .route("/opds/v2/libraries/series/latest", get(content::opds_v2_libraries_latest_series))
        .route("/opds/v2/libraries/browse", get(content::opds_v2_libraries_browse))
        .route("/opds/v2/libraries/collections", get(content::opds_v2_libraries_collections))
        .route("/opds/v2/libraries/readlists", get(content::opds_v2_libraries_readlists))
        .route(
            "/opds/v2/libraries/{library_id}",
            get(content::opds_v2_library),
        )
        .route(
            "/opds/v2/libraries/{library_id}/keep-reading",
            get(content::opds_v2_library_keep_reading),
        )
        .route(
            "/opds/v2/libraries/{library_id}/on-deck",
            get(content::opds_v2_library_on_deck),
        )
        .route(
            "/opds/v2/libraries/{library_id}/books/latest",
            get(content::opds_v2_library_latest_books),
        )
        .route(
            "/opds/v2/libraries/{library_id}/series/latest",
            get(content::opds_v2_library_latest_series),
        )
        .route(
            "/opds/v2/libraries/{library_id}/browse",
            get(content::opds_v2_library_browse),
        )
        .route(
            "/opds/v2/libraries/{library_id}/collections",
            get(content::opds_v2_library_collections),
        )
        .route(
            "/opds/v2/libraries/{library_id}/readlists",
            get(content::opds_v2_library_readlists),
        )
        .route("/opds/v2/collections/{collection_id}", get(content::opds_v2_collection))
        .route("/opds/v2/series/{series_id}", get(content::opds_v2_series))
        .route(
            "/opds/v2/readlists/{readlist_id}",
            get(content::opds_v2_readlist),
        )
        .route("/opds/v2/search", get(content::opds_v2_search))
        .route(
            "/opds/v2/books/{book_id}/manifest",
            get(content::opds_manifest),
        )
        .route(
            "/opds/v2/books/{book_id}/manifest/{manifest_profile}",
            get(content::opds_manifest_profile),
        )
        .route("/opds/v2/books/{book_id}/file", get(content::book_file))
        .route(
            "/opds/v2/books/{book_id}/file/{*file_name}",
            get(content::book_file_with_suffix),
        )
        .route(
            "/opds/v2/books/{book_id}/thumbnail",
            get(content::book_thumbnail),
        )
        .route(
            "/opds/v2/books/{book_id}/thumbnail/small",
            get(content::opds_v2_book_thumbnail_small),
        )
        .route(
            "/opds/v2/books/{book_id}/pages/{page_number}",
            get(content::book_page),
        )
        .route(
            "/opds/v2/books/{book_id}/pages/{page_number}/raw",
            get(content::book_page_raw),
        )
        .route(
            "/opds/v2/books/{book_id}/progression",
            get(content::book_progression_get).patch(content::book_progression),
        )
        .route("/api/v1/login/set-cookie", get(content::login_set_cookie))
        .route("/api/logout", post(content::logout))
        .route("/sse/v1/events", get(operational::sse_events))
        .route("/", get(operational::webui_entrypoint))
        .route("/{*webui_path}", get(operational::webui_asset));

    let router = if should_enable_dev_cors() {
        router.layer(middleware::from_fn(operational::dev_cors_middleware))
    } else {
        router
    };

    let router = if should_expose_actuator_default_contract() {
        router
            .route("/actuator", get(operational::actuator_root))
            .route("/actuator/health", get(operational::actuator_health))
            .route("/actuator/info", get(operational::actuator_info))
            .route("/actuator/logfile", get(operational::actuator_logfile))
            .route("/actuator/shutdown", post(operational::actuator_shutdown))
            .route(
                "/actuator/metrics",
                get(operational::actuator_metrics_index),
            )
            .route(
                "/actuator/metrics/{metric_name}",
                get(operational::actuator_metric_detail),
            )
    } else {
        router
    };

    let router = router
        .layer(axum::extract::Extension(state))
        .layer(axum::extract::Extension(auth_db))
        .layer(axum::extract::Extension(discovery_auth))
        .layer(axum::extract::Extension(operational))
        .layer(axum::extract::Extension(profile));

    router
}

fn bootstrap_startup_search_task(
    task_queue: &mut TaskQueueScheduler,
    config: &RuntimeConfig,
    startup_search_task: Option<&'static str>,
) {
    let Some(task_name) = startup_search_task else {
        return;
    };

    task_queue.enqueue(TaskQueueRecord::new(task_name.to_string(), 1_000, None));
    let _ = task_queue.process_available(config);
}

#[derive(Clone)]
struct PersistedLibraryScanProfile {
    library_id: String,
    scan_startup: bool,
    scan_interval: String,
}

fn bootstrap_startup_library_scans(
    task_queue: &mut TaskQueueScheduler,
    config: &RuntimeConfig,
) -> Vec<ScheduledLibraryScan> {
    let profiles = load_persisted_library_scan_profiles(config.database_file.as_path());
    if profiles.is_empty() {
        return Vec::new();
    }

    let mut scheduler = LibraryScanScheduler::default();
    for profile in &profiles {
        scheduler.schedule_scan(
            profile.library_id.clone(),
            library_scan_interval_from_db(profile.scan_interval.as_str()),
        );
    }

    let startup_library_ids = profiles
        .iter()
        .filter(|profile| profile.scan_startup)
        .map(|profile| profile.library_id.clone())
        .collect::<Vec<_>>();

    for library_id in startup_library_ids {
        task_queue.enqueue(TaskQueueRecord::new(
            format!("SCAN_LIBRARY:{library_id}"),
            100,
            Some(library_id.clone()),
        ));
    }
    scheduler.scheduled_tasks()
}

fn spawn_periodic_library_scan_workers(
    task_queue: Arc<Mutex<TaskQueueScheduler>>,
    config: RuntimeConfig,
    _scheduled_scans: Vec<ScheduledLibraryScan>,
) {
    let Ok(handle) = Handle::try_current() else {
        return;
    };

    handle.spawn(async move {
        let mut ticker = interval(Duration::from_secs(60));
        ticker.tick().await;
        let mut last_run_by_library: HashMap<String, tokio::time::Instant> = HashMap::new();

        loop {
            ticker.tick().await;

            let profiles = load_persisted_library_scan_profiles(config.database_file.as_path());
            let mut active_libraries = HashMap::new();

            for profile in profiles {
                let interval = library_scan_interval_from_db(profile.scan_interval.as_str());
                let Some(period) = interval.duration() else {
                    last_run_by_library.remove(&profile.library_id);
                    continue;
                };

                active_libraries.insert(profile.library_id.clone(), period);
                let next_due = last_run_by_library
                    .entry(profile.library_id.clone())
                    .or_insert_with(tokio::time::Instant::now);

                if next_due.elapsed() < period {
                    continue;
                }

                let mut queue = task_queue
                    .lock()
                    .expect("task queue state lock should not be poisoned");
                queue.enqueue(TaskQueueRecord::new(
                    format!("SCAN_LIBRARY:{}", profile.library_id),
                    100,
                    Some(profile.library_id.clone()),
                ));
                let _ = queue.process_available(&config);
                *next_due = tokio::time::Instant::now();
            }

            last_run_by_library.retain(|library_id, _| active_libraries.contains_key(library_id));
        }
    });
}

fn spawn_background_task_worker(task_queue: Arc<Mutex<TaskQueueScheduler>>, config: RuntimeConfig) {
    let Ok(handle) = Handle::try_current() else {
        return;
    };

    handle.spawn(async move {
        let mut ticker = interval(Duration::from_secs(300));
        ticker.tick().await;

        loop {
            ticker.tick().await;
            let mut task_queue = task_queue
                .lock()
                .expect("task queue state lock should not be poisoned");
            let _ = task_queue.process_available(&config);
        }
    });
}

fn spawn_authentication_activity_cleanup_worker(config: RuntimeConfig) {
    let Ok(handle) = Handle::try_current() else {
        return;
    };

    handle.spawn(async move {
        let mut ticker = interval(Duration::from_secs(86_400));
        ticker.tick().await;

        loop {
            ticker.tick().await;
            let _ = persisted_cleanup_authentication_activity(config.database_file.as_path()).await;
        }
    });
}

fn load_persisted_library_scan_profiles(database_file: &Path) -> Vec<PersistedLibraryScanProfile> {
    if !database_file.exists() {
        return Vec::new();
    }

    let database_file = database_file.to_path_buf();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        let Ok(runtime) = runtime else {
            return Vec::new();
        };

        runtime.block_on(async move {
            let Ok(pool) = connect_pool(database_file.as_path(), 1).await else {
                return Vec::new();
            };

            let rows = sqlx::query(
                "SELECT ID, SCAN_STARTUP, SCAN_INTERVAL \
                             FROM LIBRARY \
                             ORDER BY ID ASC",
            )
            .fetch_all(&pool)
            .await;

            let Ok(rows) = rows else {
                return Vec::new();
            };

            rows.into_iter()
                .map(|row| PersistedLibraryScanProfile {
                    library_id: row.get::<String, _>("ID"),
                    scan_startup: row.get::<bool, _>("SCAN_STARTUP"),
                    scan_interval: row.get::<String, _>("SCAN_INTERVAL"),
                })
                .collect::<Vec<_>>()
        })
    })
    .join()
    .unwrap_or_default()
}

fn library_scan_interval_from_db(value: &str) -> LibraryScanInterval {
    match value.trim().to_ascii_uppercase().as_str() {
        "DISABLED" => LibraryScanInterval::Disabled,
        "HOURLY" => LibraryScanInterval::Hourly,
        "EVERY_6H" => LibraryScanInterval::Every6h,
        "EVERY_12H" => LibraryScanInterval::Every12h,
        "DAILY" => LibraryScanInterval::Daily,
        "WEEKLY" => LibraryScanInterval::Weekly,
        _ => LibraryScanInterval::Every6h,
    }
}

fn should_expose_actuator_default_contract() -> bool {
    !std::env::var("KOMGA_RUST_DISABLE_ACTUATOR")
        .map(|value| value.eq_ignore_ascii_case("true") || value == "1")
        .unwrap_or(false)
}

fn should_enable_dev_cors() -> bool {
    std::env::var("SPRING_PROFILES_ACTIVE")
        .ok()
        .map(|profiles| {
            profiles
                .split(',')
                .map(str::trim)
                .any(|profile| profile.eq_ignore_ascii_case("dev"))
        })
        .unwrap_or(false)
}

fn transient_books_state_file(config: &RuntimeConfig) -> PathBuf {
    let root = config
        .config_dir
        .as_deref()
        .or_else(|| config.database_file.parent())
        .unwrap_or_else(|| Path::new("."));
    root.join("transient-books-state.json")
}

fn now_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

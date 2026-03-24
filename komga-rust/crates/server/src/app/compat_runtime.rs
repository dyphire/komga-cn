use axum::Router;
use axum::http::{Request, header};
use axum::middleware;
use axum::routing::{delete, get, patch, post, put};
use komga_persistence::server_settings::ServerSettingsStore;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::app::discovery_auth::DiscoveryAuthState;
use crate::config::RuntimeConfig;
use crate::task_queue::TaskQueueScheduler;

mod content;
mod device_auth;
mod operational;

const LAST_MODIFIED: &str = "Mon, 01 Jan 2024 22:04:05 GMT";
const PAGE_BODY: &[u8] = b"\x89PNG\r\n\x1a\nplaceholder";
const THUMBNAIL_BODY: &[u8] = b"\xff\xd8\xff\xdb\x00C\x00placeholder-jpeg\xff\xd9";
const PDF_BODY: &[u8] = b"%PDF-1.7\n%komga-rust-placeholder\n";
const THUMBNAIL_ETAG: &str = "\"048bbf960d13687d84948688ab74aaa59\"";
const CACHE_CONTROL_PRIVATE: &str = "max-age=0, must-revalidate, private";
const SEARCH_OWNERSHIP_HEADER: &str = "x-komga-compat-search-ownership";
const SHADOW_JAVA_WRITER_MARKER: &str = "shadow-java-writer";
const DEFAULT_BUILD_TIME: &str = "2026-03-20T00:00:00Z";
const DEFAULT_GIT_COMMIT_TIME: &str = "2026-03-20T00:00:00Z";
const DEFAULT_GIT_COMMIT_ID: &str = "komga-rust-dev";
const DEFAULT_GIT_BRANCH: &str = "rust-compat";
const DEFAULT_LOGFILE: &str =
    "komga-rust operational logfile\nINFO server started in compat mode\n";
const DEV_FRONTEND_ORIGIN: &str = "http://127.0.0.1:8081";
const DEV_CORS_ALLOW_METHODS: &str = "GET,POST,PATCH,DELETE,OPTIONS";
const DEV_CORS_ALLOW_HEADERS: &str = "authorization,x-auth-token,content-type,x-api-key,x-komga-email,x-komga-password,x-requested-with";

#[derive(Clone)]
struct OperationalState {
    runtime: RuntimeConfig,
    settings_store: Arc<ServerSettingsStore>,
    task_queue: Arc<Mutex<TaskQueueScheduler>>,
    sse: Arc<Mutex<SseOperationalState>>,
    client_settings: Arc<ClientSettingsState>,
    oauth2_clients: Arc<Vec<crate::config::OAuth2ClientConfig>>,
}

#[derive(Clone)]
pub(super) struct AuthDatabaseState {
    pub(super) database_file: PathBuf,
}

#[derive(Clone, Default)]
struct SseOperationalState {
    accepting_connections: bool,
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

#[derive(Clone, Default)]
struct ClientSettingsState {
    global: Value,
}

impl ClientSettingsState {
    fn bootstrap() -> Self {
        Self {
            global: json!({
                "webui.oauth2.hide_login": {
                    "value": "false",
                    "allowUnauthorized": true,
                },
                "webui.oauth2.auto_login": {
                    "value": "false",
                    "allowUnauthorized": true,
                },
            }),
        }
    }
}

#[derive(Clone)]
struct ReadProgressState {
    progress_by_token: Arc<Mutex<HashMap<String, HashMap<String, ReadProgress>>>>,
    koreader_progress_by_hash: Arc<Mutex<HashMap<String, KoreaderProgress>>>,
}

#[derive(Clone)]
struct ReadProgress {
    page: u64,
    completed: bool,
}

#[derive(Clone)]
struct KoreaderProgress {
    document: String,
    percentage: f64,
    progress: String,
    device: String,
    device_id: String,
}

pub(super) fn build_router(config: &RuntimeConfig) -> Router {
    let state = ReadProgressState {
        progress_by_token: Arc::new(Mutex::new(HashMap::new())),
        koreader_progress_by_hash: Arc::new(Mutex::new(HashMap::new())),
    };
    let profile = config.app_compat_profile();
    let discovery_auth = DiscoveryAuthState::default();
    let auth_db = AuthDatabaseState {
        database_file: config.database_file.clone(),
    };
    let operational = OperationalState {
        runtime: config.clone(),
        settings_store: Arc::new(ServerSettingsStore::new(config.database_file.clone())),
        task_queue: Arc::new(Mutex::new(TaskQueueScheduler::for_runtime(
            config.clone(),
            "rust-compat-runtime",
        ))),
        sse: Arc::new(Mutex::new(SseOperationalState {
            accepting_connections: true,
        })),
        client_settings: Arc::new(ClientSettingsState::bootstrap()),
        oauth2_clients: Arc::new(config.oauth2_clients.clone()),
    };

    let router = Router::new()
        .route("/health/live", get(operational::health_live))
        .route("/health/ready", get(operational::health_ready))
        .route("/metrics", get(operational::metrics_text))
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
        .route("/api/v1/history", get(operational::get_history))
        .route("/api/v1/page-hashes", get(operational::get_page_hashes))
        .route(
            "/api/v1/page-hashes/{page_hash}/thumbnail",
            get(operational::get_page_hash_thumbnail),
        )
        .route(
            "/api/v1/transient-books",
            post(operational::post_transient_books),
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
            "/api/v1/client-settings/user/list",
            get(operational::get_client_settings_user),
        )
        .route(
            "/api/v1/oauth2/providers",
            get(operational::get_oauth2_providers),
        )
        .route(
            "/oauth2/authorization/{registration_id}",
            get(device_auth::oauth2_authorization),
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
        .route("/api/v1/genres", get(content::genres))
        .route("/api/v1/tags", get(content::tags))
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
        .route(
            "/api/v1/series/latest",
            get(content::series_latest),
        )
        .route("/api/v1/tags/book", get(content::book_tags))
        .route("/api/v1/series/{series_id}", get(content::series_detail))
        .route(
            "/api/v1/series/{series_id}/collections",
            get(content::series_collections),
        )
        .route(
            "/api/v1/series/{series_id}/thumbnail",
            get(content::series_thumbnail),
        )
        .route(
            "/api/v1/series/{series_id}/metadata",
            patch(content::series_metadata_update),
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
        .route("/api/v1/books/{book_id}/pages", get(content::book_pages))
        .route(
            "/api/v1/books/{book_id}/pages/{page_number}",
            get(content::book_page),
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
            delete(content::book_thumbnail_delete),
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
            "/api/v1/books/{book_id}/file",
            get(content::book_file).delete(content::book_file_delete),
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
        .route("/api/v2/users", get(content::users_list))
        .route("/api/v2/users/me", get(content::users_me))
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
            "/api/v2/users/{id}/password",
            patch(content::users_by_id_password),
        )
        .route("/opds/v1.2/series", get(content::opds_v1_series))
        .route("/opds/v2/auth", get(content::opds_auth))
        .route("/opds/v2/catalog", get(content::opds_catalog))
        .route(
            "/opds/v2/books/{book_id}/manifest",
            get(content::opds_manifest),
        )
        .route("/api/v1/login/set-cookie", get(content::login_set_cookie))
        .route("/api/logout", post(content::logout))
        .route("/sse/v1/events", get(operational::sse_events))
        .layer(middleware::from_fn(operational::dev_cors_middleware))
        .layer(axum::extract::Extension(state))
        .layer(axum::extract::Extension(auth_db))
        .layer(axum::extract::Extension(discovery_auth))
        .layer(axum::extract::Extension(operational))
        .layer(axum::extract::Extension(profile));

    let router = if should_expose_actuator_default_contract() {
        router
            .route("/actuator", get(operational::actuator_root))
            .route("/actuator/beans", get(operational::actuator_beans))
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

    if should_reject_env_placeholder_bootstrap(config) {
        router.layer(middleware::map_request(strip_placeholder_bootstrap_headers))
    } else {
        router
    }
}

fn should_expose_actuator_default_contract() -> bool {
    std::env::var("KOMGA_RUST_ENABLE_ACTUATOR")
        .map(|value| value.eq_ignore_ascii_case("true") || value == "1")
        .unwrap_or(false)
}

fn should_reject_env_placeholder_bootstrap(config: &RuntimeConfig) -> bool {
    config.database_file.starts_with(std::env::temp_dir()) && !config.database_file.exists()
}

async fn strip_placeholder_bootstrap_headers<B>(mut request: Request<B>) -> Request<B> {
    request.headers_mut().remove(header::AUTHORIZATION);
    request.headers_mut().remove("x-api-key");
    request
}

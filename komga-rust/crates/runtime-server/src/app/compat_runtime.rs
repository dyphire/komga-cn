use axum::Router;
use axum::middleware;
use axum::routing::{delete, get, post};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::app::discovery_auth::DiscoveryAuthState;
use crate::config::RuntimeConfig;

mod content;
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
    settings: Arc<Mutex<OperationalSettings>>,
    client_settings: Arc<ClientSettingsState>,
}

#[derive(Clone)]
struct OperationalSettings {
    delete_empty_collections: bool,
    delete_empty_read_lists: bool,
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
}

#[derive(Clone)]
struct ReadProgress {
    page: u64,
    completed: bool,
}

pub(super) fn build_router(config: &RuntimeConfig) -> Router {
    let state = ReadProgressState {
        progress_by_token: Arc::new(Mutex::new(HashMap::new())),
    };
    let profile = config.app_compat_profile();
    let discovery_auth = DiscoveryAuthState::default();
    let operational = OperationalState {
        runtime: config.clone(),
        settings: Arc::new(Mutex::new(OperationalSettings::from_runtime(config))),
        client_settings: Arc::new(ClientSettingsState::bootstrap()),
    };

    Router::new()
        .route(
            "/api/v1/settings",
            get(operational::get_server_settings).patch(operational::update_server_settings),
        )
        .route("/api/v1/claim", get(operational::get_claim_status))
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
        .route("/api/v1/tasks", delete(operational::delete_tasks))
        .route("/api/v1/libraries", get(content::libraries))
        .route("/api/v1/series", get(content::series))
        .route("/api/v1/series/{series_id}", get(content::series_detail))
        .route(
            "/api/v1/series/{series_id}/collections",
            get(content::series_collections),
        )
        .route("/api/v1/series/list", post(content::series_list))
        .route("/api/v1/books", get(content::books))
        .route("/api/v1/books/list", post(content::books_list))
        .route("/api/v1/books/latest", get(content::books_latest))
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
            get(content::readlists),
        )
        .route(
            "/api/v1/readlists/{readlist_id}",
            get(content::readlist_detail),
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
        .route("/api/v1/books/{book_id}/file", get(content::book_file))
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
        .route("/api/v2/users/me", get(content::users_me))
        .route("/opds/v1.2/series", get(content::opds_v1_series))
        .route("/opds/v2/auth", get(content::opds_auth))
        .route("/opds/v2/catalog", get(content::opds_catalog))
        .route(
            "/opds/v2/books/{book_id}/manifest",
            get(content::opds_manifest),
        )
        .route("/api/v1/login/set-cookie", get(content::login_set_cookie))
        .route("/sse/v1/events", get(operational::sse_events))
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
        .layer(middleware::from_fn(operational::dev_cors_middleware))
        .layer(axum::extract::Extension(state))
        .layer(axum::extract::Extension(discovery_auth))
        .layer(axum::extract::Extension(operational))
        .layer(axum::extract::Extension(profile))
}

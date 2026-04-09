use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path, Query};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::CookieJar;
use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
use komga_application::identity_access::{
    AuthOutcome, AuthUser, KOBO_SYNC_ITEM_LIMIT, KoboSyncPointState, build_kobo_sync_events,
    build_komga_sync_token_payload, decode_or_passthrough_sync_token, generated_kobo_token_triplet,
    is_kobo_store_sync_token_candidate, now_sync_marker, parse_komga_sync_token_payload,
    resolve_koreader_user_id, user_id,
};
use oauth2::{
    AuthUrl, ClientId, ClientSecret, CsrfToken, RedirectUrl, Scope, TokenUrl, basic::BasicClient,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path as FsPath;

use crate::OperationalState;
use crate::http::identity_access::auth::{
    persisted_api_key_user_by_token, resolved_auth_user, session_token_for_user_with_namespace,
    user_has_role, user_is_admin,
};
use crate::http::request_urls::{
    request_base_url, request_base_url_with_port, request_context_path,
};
use crate::http::state::AuthDatabaseState;
use crate::media_assets_runtime_access::{
    load_persisted_book_media, persist_book_progression, read_media_file_bytes,
};
use crate::runtime_identity_access::{
    KoreaderBookLookupError, PersistedReadProgressRecord, configured_api_key, ensure_oauth_user,
    load_book_created_timestamp, load_book_last_epub_position_locator, load_kobo_metadata_record,
    load_kobo_sync_snapshot, load_koreader_book_target, load_read_progress, load_sync_point_marker,
    load_sync_point_state, load_thumbnail_by_id, persist_read_progress_with_locator,
    persisted_book_exists, proxy_kobo_store_library_sync, remove_sync_point, save_sync_point,
};

#[path = "device_auth/auth_resolvers.rs"]
mod auth_resolvers;
#[path = "device_auth/helpers.rs"]
mod helpers;
#[path = "device_auth/kobo_auth_routes.rs"]
mod kobo_auth_routes;
#[path = "device_auth/kobo_routes.rs"]
mod kobo_routes;
#[path = "device_auth/koreader_routes.rs"]
mod koreader_routes;
#[path = "device_auth/oauth.rs"]
mod oauth;

pub use kobo_auth_routes::{kobo_auth_device, kobo_initialization, kobo_ping};
pub use kobo_routes::{
    kobo_book_file_epub, kobo_book_thumbnail, kobo_book_thumbnail_with_quality, kobo_catch_all,
    kobo_library_book_metadata, kobo_library_book_state, kobo_library_book_state_update,
    kobo_library_sync,
};
pub use koreader_routes::{
    koreader_get_progress, koreader_put_progress, koreader_user_auth, koreader_user_create,
};
pub use oauth::{oauth2_authorization, oauth2_login_code};

use auth_resolvers::*;
use helpers::*;

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct KoboDeviceAuthResponse {
    access_token: String,
    refresh_token: String,
    token_type: &'static str,
    tracking_id: String,
    user_key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct KoboReadingStateUpdatePayload {
    #[serde(default)]
    reading_states: Vec<KoboReadingStateUpdateEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct KoboReadingStateUpdateEntry {
    last_modified: String,
    current_bookmark: KoboReadingStateBookmark,
    status_info: KoboReadingStateStatusInfo,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct KoboReadingStateBookmark {
    progress_percent: Option<f64>,
    content_source_progress_percent: Option<f64>,
    location: Option<KoboReadingStateLocation>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct KoboReadingStateLocation {
    value: Option<String>,
    #[serde(rename = "Type", default = "default_kobo_location_type")]
    location_type: String,
    source: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct KoboReadingStateStatusInfo {
    status: String,
}

fn default_kobo_location_type() -> String {
    "KoboSpan".to_string()
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
struct KoreaderProgressPayload {
    document: String,
    percentage: f64,
    progress: String,
    device: String,
    device_id: String,
}

#[derive(Deserialize, Default)]
pub struct KoboBookFileQuery {
    convert_kepub: Option<bool>,
}

#[derive(Deserialize, Default)]
pub struct OAuth2CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

fn convert_epub_to_kepub_bytes(input_file: &FsPath) -> Option<Vec<u8>> {
    komga_kepubify::convert_epub_file_to_bytes(input_file).ok()
}

fn kobo_kepub_file_name(file_name: &str) -> String {
    if let Some((base, ext)) = file_name.rsplit_once('.')
        && ext.eq_ignore_ascii_case("epub")
    {
        return format!("{base}.kepub.epub");
    }
    format!("{file_name}.kepub.epub")
}

fn kobo_sync_token_from_request(headers: &HeaderMap, _uri: &axum::http::Uri) -> Option<String> {
    let from_header = headers
        .get("x-kobo-synctoken")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if from_header.is_some() {
        return from_header.and_then(|value| decode_or_passthrough_sync_token(value.as_str()));
    }
    None
}

async fn load_kobo_proxy_enabled(state: &OperationalState) -> bool {
    state
        .settings_store
        .load_map()
        .await
        .ok()
        .and_then(|settings| settings.get("KOBO_PROXY").cloned())
        .and_then(|value| value)
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

#[cfg(test)]
#[path = "device_auth/tests.rs"]
mod tests;

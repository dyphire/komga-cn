use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path, Query};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::CookieJar;
use komga_application::identity_access::{
    AuthOutcome, AuthUser, DeviceProgressError, DeviceProgressService, KOBO_SYNC_ITEM_LIMIT,
    KoboLibrarySyncRequest, KoboReadingStateUpdate, KoreaderProgressUpdate,
    build_kobo_book_metadata_payload, build_kobo_library_sync_payload,
    decode_or_passthrough_sync_token, generated_kobo_token_triplet, now_sync_marker, user_id,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::path::Path as FsPath;

use crate::access_log::RequestConnectionInfo;
use crate::identity_access::auth::{
    persisted_api_key_metadata, persisted_api_key_user, persisted_api_key_user_by_token,
    persisted_record_successful_authentication_activity, resolved_auth_user,
    session_token_for_user_with_runtime_key, user_has_role, user_is_admin,
};
use crate::request_urls::{request_base_url, request_base_url_with_port, request_context_path};
use crate::state::{IdentityAccessState, IdentityState};

mod auth_resolvers;
mod helpers;
mod kobo_auth_routes;
mod kobo_routes;
mod koreader_routes;
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

#[cfg(test)]
pub(crate) async fn kobo_ping_for_tests(
    identity: &crate::state::IdentityState,
    auth_token: &str,
    connection_info: RequestConnectionInfo,
    headers: HeaderMap,
) -> Response {
    match auth_resolvers::required_kobo_user(
        identity,
        auth_token,
        &headers,
        connection_info.remote_addr(),
    )
    .await
    {
        Ok(_) => "pong".into_response(),
        Err(status) => status.into_response(),
    }
}

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

fn device_progress_service(app: &IdentityAccessState) -> DeviceProgressService<'_> {
    DeviceProgressService::new(
        app.identity.device_sync(),
        app.reader.as_ref(),
        app.content.as_ref(),
        app.progress.as_ref(),
    )
}

async fn load_kobo_proxy_enabled(
    server_settings: &dyn komga_application::operational::ServerSettingsPort,
) -> bool {
    server_settings
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

async fn effective_kobo_port(app: &IdentityAccessState) -> u16 {
    app.server_settings
        .load_settings()
        .await
        .ok()
        .and_then(|settings| settings.kobo_port)
        .unwrap_or_else(|| app.operational.runtime.bind_address.port())
}

async fn kobo_request_base_url(app: &IdentityAccessState, headers: &HeaderMap) -> String {
    format!(
        "{}{}",
        request_base_url_with_port(headers, Some(effective_kobo_port(app).await)),
        request_context_path(headers)
    )
}

#[cfg(test)]
mod tests;

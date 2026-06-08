use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path, Query};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::CookieJar;
use komga_application::identity_access::{
    AuthOutcome, AuthUser, DeviceProgressError, DeviceProgressService, KOBO_SYNC_ITEM_LIMIT,
    KoboLibrarySyncRequest, KoboLibrarySyncService, KoreaderProgressUpdate,
    build_kobo_book_metadata_payload, build_kobo_library_sync_payload, now_sync_marker, user_id,
};
use serde_json::{Value, json};
use std::net::SocketAddr;

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

fn device_progress_service(app: &IdentityAccessState) -> DeviceProgressService<'_> {
    DeviceProgressService::new(
        app.identity.device_sync(),
        app.reader.as_ref(),
        app.content.as_ref(),
        app.progress.as_ref(),
    )
}

fn kobo_library_sync_service(app: &IdentityAccessState) -> KoboLibrarySyncService<'_> {
    KoboLibrarySyncService::new(
        app.identity.kobo_sync_state(),
        app.identity.kobo_store_sync(),
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

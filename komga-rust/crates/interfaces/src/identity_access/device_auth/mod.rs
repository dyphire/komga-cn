use axum::http::HeaderMap;
use komga_application::identity_access::{
    DeviceProgressReaderPort, DeviceProgressService, DeviceSyncPort, KoboLibrarySyncService,
    KoboStoreSyncPort, KoboSyncStatePort,
};
use komga_application::media_assets::{EpubNavigationContentPort, ProgressWriterPort};
use komga_application::operational::ServerSettingsPort;

use crate::request_urls::{request_base_url_with_port, request_context_path};
use crate::state::RuntimeState;

mod auth_resolvers;
mod helpers;
mod kobo_auth_routes;
mod kobo_routes;
mod koreader_routes;
mod oauth;

pub(crate) use kobo_auth_routes::{kobo_auth_device, kobo_initialization, kobo_ping};
pub(crate) use kobo_routes::{
    kobo_book_file_epub, kobo_book_thumbnail, kobo_book_thumbnail_with_quality, kobo_catch_all,
    kobo_library_book_metadata, kobo_library_book_state, kobo_library_book_state_update,
    kobo_library_sync,
};
pub(crate) use koreader_routes::{
    koreader_get_progress, koreader_put_progress, koreader_user_auth, koreader_user_create,
};
pub(crate) use oauth::{oauth2_authorization, oauth2_login_code};

#[cfg(test)]
pub(crate) async fn kobo_ping_for_tests(
    identity: &crate::state::IdentityState,
    auth_token: &str,
    connection_info: crate::access_log::RequestConnectionInfo,
    headers: HeaderMap,
) -> axum::response::Response {
    use axum::response::IntoResponse;

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

fn device_progress_service<'a>(
    device_sync: &'a dyn DeviceSyncPort,
    reader: &'a dyn DeviceProgressReaderPort,
    content: &'a dyn EpubNavigationContentPort,
    progress: &'a dyn ProgressWriterPort,
) -> DeviceProgressService<'a, dyn EpubNavigationContentPort + 'a, dyn ProgressWriterPort + 'a> {
    DeviceProgressService::new(device_sync, reader, content, progress)
}

fn kobo_library_sync_service<'a>(
    state: &'a dyn KoboSyncStatePort,
    store_sync: &'a dyn KoboStoreSyncPort,
) -> KoboLibrarySyncService<'a> {
    KoboLibrarySyncService::new(state, store_sync)
}

async fn load_kobo_proxy_enabled(server_settings: &dyn ServerSettingsPort) -> anyhow::Result<bool> {
    server_settings
        .load_settings()
        .await
        .map(|settings| settings.kobo_proxy)
}

async fn effective_kobo_port(
    server_settings: &dyn ServerSettingsPort,
    runtime: &RuntimeState,
) -> anyhow::Result<u16> {
    server_settings.load_settings().await.map(|settings| {
        settings
            .kobo_port
            .unwrap_or_else(|| runtime.bind_address.port())
    })
}

async fn kobo_request_base_url(
    server_settings: &dyn ServerSettingsPort,
    runtime: &RuntimeState,
    headers: &HeaderMap,
) -> anyhow::Result<String> {
    Ok(format!(
        "{}{}",
        request_base_url_with_port(
            headers,
            Some(effective_kobo_port(server_settings, runtime).await?)
        ),
        request_context_path(headers)
    ))
}

#[cfg(test)]
mod tests;

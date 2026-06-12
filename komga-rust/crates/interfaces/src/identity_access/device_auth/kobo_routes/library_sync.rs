use axum::Json;
use axum::extract::{Extension, Path, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_application::identity_access::{
    KOBO_SYNC_ITEM_LIMIT, KoboLibrarySyncRequest, KoboProxyHeader, encode_komga_sync_token_payload,
};
use serde_json::Value;

use super::{resolved_kobo_request_api_key_metadata, wire::build_kobo_sync_event_payload};
use crate::access_log::RequestConnectionInfo;
use crate::identity_access::device_auth::auth_resolvers::required_kobo_user;
use crate::identity_access::device_auth::{
    kobo_library_sync_service, kobo_request_base_url, load_kobo_proxy_enabled,
};
use crate::state::IdentityAccessState;

pub(crate) async fn kobo_library_sync(
    State(app): State<IdentityAccessState>,
    Path(auth_token): Path<String>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let current_user = match required_kobo_user(
        &app.identity,
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
    )
    .await
    {
        Ok(current_user) => current_user,
        Err(status) => return status.into_response(),
    };
    let current_api_key_id = resolved_kobo_request_api_key_metadata(
        &app.identity,
        &current_user,
        auth_token.as_str(),
        &headers,
    )
    .await;
    let current_api_key_id = match current_api_key_id {
        Ok(metadata) => metadata.map(|metadata| metadata.id),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let sync_token = headers
        .get("x-kobo-synctoken")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let base_url = match kobo_request_base_url(
        app.server_settings.as_ref(),
        &app.operational.runtime,
        &headers,
    )
    .await
    {
        Ok(base_url) => base_url,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let forwarded_headers = headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| KoboProxyHeader::new(name.as_str(), value))
        })
        .collect::<Vec<_>>();
    let store_sync_enabled = match load_kobo_proxy_enabled(app.server_settings.as_ref()).await {
        Ok(enabled) => enabled,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let sync_service = kobo_library_sync_service(
        app.identity.kobo_sync_state(),
        app.identity.kobo_store_sync(),
    );
    let sync_response = match sync_service
        .sync_library(KoboLibrarySyncRequest {
            user: current_user,
            current_api_key_id,
            sync_token,
            store_sync_enabled,
            forwarded_headers,
            query: uri.query().map(str::to_string),
            limit: KOBO_SYNC_ITEM_LIMIT,
        })
        .await
    {
        Ok(response) => response,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let encoded_sync_token =
        encode_komga_sync_token_payload(sync_response.sync_token_payload.as_str());
    let should_continue = sync_response.should_continue;
    let events = match sync_response
        .events
        .into_iter()
        .map(|event| build_kobo_sync_event_payload(event, base_url.as_str(), auth_token.as_str()))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(events) => events,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let mut response = (
        StatusCode::OK,
        [(
            HeaderName::from_static("x-kobo-synctoken"),
            HeaderValue::from_str(encoded_sync_token.as_str())
                .unwrap_or_else(|_| HeaderValue::from_static("")),
        )],
        Json(Value::Array(events)),
    )
        .into_response();
    if should_continue {
        response.headers_mut().insert(
            HeaderName::from_static("x-kobo-sync"),
            HeaderValue::from_static("continue"),
        );
    }
    response
}

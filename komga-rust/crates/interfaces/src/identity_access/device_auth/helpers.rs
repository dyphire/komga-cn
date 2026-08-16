use axum::http::{HeaderMap, HeaderValue};
use komga_application::identity_access::{AuthUser, PersistedApiKeyMetadata};
use std::net::SocketAddr;

use crate::identity_access::auth::{
    AuthenticationActivityApiKey, authentication_activity_headers_metadata_with_remote_addr,
    authentication_activity_write_input, persisted_api_key_metadata,
    persisted_record_successful_authentication_activity,
};
use crate::state::IdentityState;

pub(super) async fn record_successful_api_key_authentication_by_token(
    identity: &IdentityState,
    headers: &HeaderMap,
    remote_addr: Option<SocketAddr>,
    user: &AuthUser,
    api_key: &str,
) -> Option<()> {
    let api_key_metadata = api_key_metadata_by_token(identity, api_key)
        .await
        .ok()
        .flatten();

    persisted_record_successful_authentication_activity(
        identity,
        user,
        authentication_activity_write_input(
            &authentication_activity_headers_metadata_with_remote_addr(headers, remote_addr),
            "ApiKey",
            AuthenticationActivityApiKey::from_persisted(api_key_metadata.as_ref()),
        ),
    )
    .await
}

pub(super) async fn api_key_metadata_by_token(
    identity: &IdentityState,
    api_key: &str,
) -> anyhow::Result<Option<PersistedApiKeyMetadata>> {
    let mut metadata_headers = HeaderMap::new();
    let Ok(header_value) = HeaderValue::from_str(api_key) else {
        return Ok(None);
    };
    metadata_headers.insert("x-api-key", header_value);
    persisted_api_key_metadata(identity, &metadata_headers).await
}

use axum::http::HeaderMap;
use komga_application::identity_access::{
    AuthOutcome, AuthUser, AuthenticationActivityApiKey, BasicAuthCredentials, PersistedApiKey,
    PersistedApiKeyMetadata, PersistedAuthenticationActivity,
};

use crate::state::{AuthenticationActivityWriteInput, IdentityState};

pub(crate) async fn persisted_api_key_comment_exists(
    identity: &IdentityState,
    user_id: &str,
    comment: &str,
) -> anyhow::Result<bool> {
    identity
        .user_admin()
        .persisted_api_key_comment_exists(user_id, comment)
        .await
}

pub(crate) async fn persisted_api_key_metadata(
    identity: &IdentityState,
    headers: &HeaderMap,
) -> anyhow::Result<Option<PersistedApiKeyMetadata>> {
    let Some(api_key) = api_key_header_value(headers) else {
        return Ok(None);
    };
    identity
        .authentication()
        .api_key_metadata_by_token(&api_key)
        .await
}

pub(crate) async fn persisted_api_key_user(
    identity: &IdentityState,
    headers: &HeaderMap,
) -> anyhow::Result<AuthOutcome> {
    let Some(api_key) = api_key_header_value(headers) else {
        return Ok(AuthOutcome::Missing);
    };
    identity
        .authentication()
        .authenticate_api_key(&api_key)
        .await
}

pub(crate) async fn persisted_api_key_user_by_token(
    identity: &IdentityState,
    api_key: &str,
) -> anyhow::Result<AuthOutcome> {
    identity
        .authentication()
        .authenticate_api_key(api_key)
        .await
}

pub(crate) async fn persisted_basic_user(
    identity: &IdentityState,
    headers: &HeaderMap,
) -> anyhow::Result<AuthOutcome> {
    let Some(credentials) = basic_credentials(headers) else {
        return Ok(AuthOutcome::Missing);
    };
    identity
        .authentication()
        .authenticate_basic(&credentials.username, &credentials.password)
        .await
}

pub(crate) async fn persisted_create_api_key(
    identity: &IdentityState,
    user_id: &str,
    comment: &str,
) -> anyhow::Result<PersistedApiKey> {
    identity
        .user_admin()
        .persisted_create_api_key(user_id, comment)
        .await
}

pub(crate) async fn persisted_delete_api_key_by_id(
    identity: &IdentityState,
    user_id: &str,
    api_key_id: &str,
) -> anyhow::Result<bool> {
    identity
        .user_admin()
        .persisted_delete_api_key_by_id(user_id, api_key_id)
        .await
}

pub(crate) async fn persisted_list_api_keys(
    identity: &IdentityState,
    user_id: &str,
) -> anyhow::Result<Vec<PersistedApiKey>> {
    identity.user_admin().persisted_list_api_keys(user_id).await
}

pub(crate) async fn persisted_list_authentication_activity(
    identity: &IdentityState,
    user_id: Option<&str>,
) -> anyhow::Result<Vec<PersistedAuthenticationActivity>> {
    identity
        .auth_activity()
        .persisted_list_authentication_activity(user_id)
        .await
}

pub(crate) async fn persisted_record_successful_authentication_activity(
    identity: &IdentityState,
    user: &AuthUser,
    input: AuthenticationActivityWriteInput,
) -> Option<()> {
    identity
        .auth_activity()
        .persisted_record_successful_authentication_activity(
            user,
            &input.source,
            AuthenticationActivityApiKey {
                id: input.api_key_id.as_deref(),
                comment: input.api_key_comment.as_deref(),
            },
            input.ip.as_deref(),
            input.user_agent.as_deref(),
        )
        .await
}

pub(crate) async fn persisted_update_password_by_user_id(
    identity: &IdentityState,
    user_id: &str,
    password: &str,
) -> anyhow::Result<bool> {
    identity
        .user_admin()
        .persisted_update_password_by_user_id(user_id, password)
        .await
}

pub(crate) async fn persisted_users(identity: &IdentityState) -> anyhow::Result<Vec<AuthUser>> {
    identity.user_admin().persisted_users().await
}

pub(crate) fn basic_credentials(headers: &HeaderMap) -> Option<BasicAuthCredentials> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};

    let value = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())?
        .trim();
    if value.is_empty() {
        return None;
    }

    let encoded = value.strip_prefix("Basic ")?;
    let decoded = STANDARD.decode(encoded).ok()?;
    let credentials = String::from_utf8(decoded).ok()?;
    BasicAuthCredentials::parse_basic_payload(&credentials)
}

pub(crate) fn api_key_header_value(headers: &HeaderMap) -> Option<String> {
    let value = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())?;
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

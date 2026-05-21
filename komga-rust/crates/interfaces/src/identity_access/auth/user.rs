use axum::http::HeaderMap;
use komga_application::identity_access::{
    AuthOutcome, AuthUser, PersistedApiKey, PersistedApiKeyMetadata,
    PersistedAuthenticationActivity,
};

use crate::state::{AuthenticationActivityWriteInput, IdentityState};

pub async fn persisted_api_key_comment_exists(
    identity: &IdentityState,
    user_id: &str,
    comment: &str,
) -> Option<bool> {
    identity
        .user_admin()
        .persisted_api_key_comment_exists(user_id, comment)
        .await
}

pub async fn persisted_api_key_metadata(
    identity: &IdentityState,
    headers: &HeaderMap,
) -> Option<PersistedApiKeyMetadata> {
    let api_key = api_key_header_value(headers)?;
    identity
        .authentication()
        .api_key_metadata_by_token(&api_key)
        .await
}

pub async fn persisted_api_key_user(
    identity: &IdentityState,
    headers: &HeaderMap,
) -> Option<AuthOutcome> {
    let Some(api_key) = api_key_header_value(headers) else {
        return Some(AuthOutcome::Missing);
    };
    identity
        .authentication()
        .authenticate_api_key(&api_key)
        .await
}

pub async fn persisted_api_key_user_by_token(
    identity: &IdentityState,
    api_key: &str,
) -> Option<AuthOutcome> {
    identity
        .authentication()
        .authenticate_api_key(api_key)
        .await
}

pub async fn persisted_basic_user(
    identity: &IdentityState,
    headers: &HeaderMap,
) -> Option<AuthOutcome> {
    let Some((username, password)) = basic_credentials(headers) else {
        return Some(AuthOutcome::Missing);
    };
    identity
        .authentication()
        .authenticate_basic(&username, &password)
        .await
}

pub async fn persisted_cleanup_authentication_activity(identity: &IdentityState) -> Option<u64> {
    identity
        .auth_activity()
        .persisted_cleanup_authentication_activity()
        .await
}

pub async fn persisted_create_api_key(
    identity: &IdentityState,
    user_id: &str,
    comment: &str,
) -> Option<PersistedApiKey> {
    identity
        .user_admin()
        .persisted_create_api_key(user_id, comment)
        .await
}

pub async fn persisted_delete_api_key_by_id(
    identity: &IdentityState,
    user_id: &str,
    api_key_id: &str,
) -> Option<bool> {
    identity
        .user_admin()
        .persisted_delete_api_key_by_id(user_id, api_key_id)
        .await
}

pub async fn persisted_latest_authentication_activity_by_user_and_api_key(
    identity: &IdentityState,
    user_id: &str,
    api_key_id: &str,
) -> Option<PersistedAuthenticationActivity> {
    identity
        .auth_activity()
        .persisted_latest_authentication_activity_by_user_and_api_key(user_id, api_key_id)
        .await
}

pub async fn persisted_list_api_keys(
    identity: &IdentityState,
    user_id: &str,
) -> Option<Vec<PersistedApiKey>> {
    identity.user_admin().persisted_list_api_keys(user_id).await
}

pub async fn persisted_list_authentication_activity(
    identity: &IdentityState,
    user_id: Option<&str>,
) -> Option<Vec<PersistedAuthenticationActivity>> {
    identity
        .auth_activity()
        .persisted_list_authentication_activity(user_id)
        .await
}

pub async fn persisted_record_failed_authentication_activity(
    identity: &IdentityState,
    email: Option<&str>,
    input: AuthenticationActivityWriteInput,
    error: &str,
) -> Option<()> {
    identity
        .auth_activity()
        .persisted_record_failed_authentication_activity(
            email,
            &input.source,
            error,
            input.ip.as_deref(),
            input.user_agent.as_deref(),
        )
        .await
}

pub async fn persisted_record_successful_authentication_activity(
    identity: &IdentityState,
    user: &AuthUser,
    input: AuthenticationActivityWriteInput,
) -> Option<()> {
    identity
        .auth_activity()
        .persisted_record_successful_authentication_activity(
            user,
            &input.source,
            input.api_key_id.as_deref(),
            input.api_key_comment.as_deref(),
            input.ip.as_deref(),
            input.user_agent.as_deref(),
        )
        .await
}

pub async fn persisted_update_password_by_user_id(
    identity: &IdentityState,
    user_id: &str,
    password: &str,
) -> Option<bool> {
    identity
        .user_admin()
        .persisted_update_password_by_user_id(user_id, password)
        .await
}

pub async fn persisted_users(identity: &IdentityState) -> Option<Vec<AuthUser>> {
    identity.user_admin().persisted_users().await
}

fn basic_credentials(headers: &HeaderMap) -> Option<(String, String)> {
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
    credentials
        .split_once(':')
        .map(|(username, password)| (username.to_string(), password.to_string()))
}

fn api_key_header_value(headers: &HeaderMap) -> Option<String> {
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

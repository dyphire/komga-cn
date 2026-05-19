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
        .api_key
        .persisted_api_key_comment_exists(user_id, comment)
        .await
}

pub async fn persisted_api_key_metadata(
    identity: &IdentityState,
    headers: &HeaderMap,
) -> Option<PersistedApiKeyMetadata> {
    identity
        .user_management
        .persisted_api_key_metadata(headers)
        .await
}

pub async fn persisted_api_key_user(
    identity: &IdentityState,
    headers: &HeaderMap,
) -> Option<AuthOutcome> {
    identity
        .user_management
        .persisted_api_key_user(headers)
        .await
}

pub async fn persisted_api_key_user_by_token(
    identity: &IdentityState,
    api_key: &str,
) -> Option<AuthOutcome> {
    identity
        .user_management
        .persisted_api_key_user_by_token(api_key)
        .await
}

pub async fn persisted_basic_user(
    identity: &IdentityState,
    headers: &HeaderMap,
) -> Option<AuthOutcome> {
    identity.user_management.persisted_basic_user(headers).await
}

pub async fn persisted_cleanup_authentication_activity(identity: &IdentityState) -> Option<u64> {
    identity
        .auth_activity
        .persisted_cleanup_authentication_activity()
        .await
}

pub async fn persisted_create_api_key(
    identity: &IdentityState,
    user_id: &str,
    comment: &str,
) -> Option<PersistedApiKey> {
    identity
        .api_key
        .persisted_create_api_key(user_id, comment)
        .await
}

pub async fn persisted_delete_api_key_by_id(
    identity: &IdentityState,
    user_id: &str,
    api_key_id: &str,
) -> Option<bool> {
    identity
        .api_key
        .persisted_delete_api_key_by_id(user_id, api_key_id)
        .await
}

pub async fn persisted_latest_authentication_activity_by_user_and_api_key(
    identity: &IdentityState,
    user_id: &str,
    api_key_id: &str,
) -> Option<PersistedAuthenticationActivity> {
    identity
        .auth_activity
        .persisted_latest_authentication_activity_by_user_and_api_key(user_id, api_key_id)
        .await
}

pub async fn persisted_list_api_keys(
    identity: &IdentityState,
    user_id: &str,
) -> Option<Vec<PersistedApiKey>> {
    identity.api_key.persisted_list_api_keys(user_id).await
}

pub async fn persisted_list_authentication_activity(
    identity: &IdentityState,
    user_id: Option<&str>,
) -> Option<Vec<PersistedAuthenticationActivity>> {
    identity
        .auth_activity
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
        .auth_activity
        .persisted_record_failed_authentication_activity(email, input, error)
        .await
}

pub async fn persisted_record_successful_authentication_activity(
    identity: &IdentityState,
    user: &AuthUser,
    input: AuthenticationActivityWriteInput,
) -> Option<()> {
    identity
        .auth_activity
        .persisted_record_successful_authentication_activity(user, input)
        .await
}

pub async fn persisted_update_password_by_user_id(
    identity: &IdentityState,
    user_id: &str,
    password: &str,
) -> Option<bool> {
    identity
        .user_management
        .persisted_update_password_by_user_id(user_id, password)
        .await
}

pub async fn persisted_users(identity: &IdentityState) -> Option<Vec<AuthUser>> {
    identity.user_management.persisted_users().await
}

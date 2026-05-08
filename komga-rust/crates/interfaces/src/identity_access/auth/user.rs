use axum::http::HeaderMap;
use komga_application::identity_access::{
    AuthOutcome, AuthUser, PersistedApiKey, PersistedApiKeyMetadata,
    PersistedAuthenticationActivity,
};

use crate::state::{AuthenticationActivityWriteInput, IdentityService};

pub async fn persisted_api_key_comment_exists(
    identity: &dyn IdentityService,
    user_id: &str,
    comment: &str,
) -> Option<bool> {
    identity
        .persisted_api_key_comment_exists(user_id, comment)
        .await
}

pub async fn persisted_api_key_metadata(
    identity: &dyn IdentityService,
    headers: &HeaderMap,
) -> Option<PersistedApiKeyMetadata> {
    identity.persisted_api_key_metadata(headers).await
}

pub async fn persisted_api_key_user(
    identity: &dyn IdentityService,
    headers: &HeaderMap,
) -> Option<AuthOutcome> {
    identity.persisted_api_key_user(headers).await
}

pub async fn persisted_api_key_user_by_token(
    identity: &dyn IdentityService,
    api_key: &str,
) -> Option<AuthOutcome> {
    identity.persisted_api_key_user_by_token(api_key).await
}

pub async fn persisted_basic_user(
    identity: &dyn IdentityService,
    headers: &HeaderMap,
) -> Option<AuthOutcome> {
    identity.persisted_basic_user(headers).await
}

pub async fn persisted_cleanup_authentication_activity(
    identity: &dyn IdentityService,
) -> Option<u64> {
    identity.persisted_cleanup_authentication_activity().await
}

pub async fn persisted_create_api_key(
    identity: &dyn IdentityService,
    user_id: &str,
    comment: &str,
) -> Option<PersistedApiKey> {
    identity.persisted_create_api_key(user_id, comment).await
}

pub async fn persisted_delete_api_key_by_id(
    identity: &dyn IdentityService,
    user_id: &str,
    api_key_id: &str,
) -> Option<bool> {
    identity
        .persisted_delete_api_key_by_id(user_id, api_key_id)
        .await
}

pub async fn persisted_latest_authentication_activity_by_user_and_api_key(
    identity: &dyn IdentityService,
    user_id: &str,
    api_key_id: &str,
) -> Option<PersistedAuthenticationActivity> {
    identity
        .persisted_latest_authentication_activity_by_user_and_api_key(user_id, api_key_id)
        .await
}

pub async fn persisted_list_api_keys(
    identity: &dyn IdentityService,
    user_id: &str,
) -> Option<Vec<PersistedApiKey>> {
    identity.persisted_list_api_keys(user_id).await
}

pub async fn persisted_list_authentication_activity(
    identity: &dyn IdentityService,
    user_id: Option<&str>,
) -> Option<Vec<PersistedAuthenticationActivity>> {
    identity
        .persisted_list_authentication_activity(user_id)
        .await
}

pub async fn persisted_record_failed_authentication_activity(
    identity: &dyn IdentityService,
    email: Option<&str>,
    input: AuthenticationActivityWriteInput,
    error: &str,
) -> Option<()> {
    identity
        .persisted_record_failed_authentication_activity(email, input, error)
        .await
}

pub async fn persisted_record_successful_authentication_activity(
    identity: &dyn IdentityService,
    user: &AuthUser,
    input: AuthenticationActivityWriteInput,
) -> Option<()> {
    identity
        .persisted_record_successful_authentication_activity(user, input)
        .await
}

pub async fn persisted_update_password_by_user_id(
    identity: &dyn IdentityService,
    user_id: &str,
    password: &str,
) -> Option<bool> {
    identity
        .persisted_update_password_by_user_id(user_id, password)
        .await
}

pub async fn persisted_users(identity: &dyn IdentityService) -> Option<Vec<AuthUser>> {
    identity.persisted_users().await
}

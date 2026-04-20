use axum::http::HeaderMap;
use komga_application::identity_access::{
    AuthOutcome, AuthUser, PersistedApiKey, PersistedApiKeyMetadata,
    PersistedAuthenticationActivity,
};
use std::path::Path;

use crate::runtime_identity_access::AuthenticationActivityWriteInput;

pub async fn persisted_api_key_comment_exists(
    database_file: &Path,
    user_id: &str,
    comment: &str,
) -> Option<bool> {
    komga_infrastructure::auth::runtime_identity_access::persisted_api_key_comment_exists(
        database_file,
        user_id,
        comment,
    )
    .await
}

pub async fn persisted_api_key_metadata(
    headers: &HeaderMap,
    database_file: &Path,
) -> Option<PersistedApiKeyMetadata> {
    komga_infrastructure::auth::runtime_identity_access::persisted_api_key_metadata(
        headers,
        database_file,
    )
    .await
}

pub async fn persisted_api_key_user(
    headers: &HeaderMap,
    database_file: &Path,
) -> Option<AuthOutcome> {
    komga_infrastructure::auth::runtime_identity_access::persisted_api_key_user(
        headers,
        database_file,
    )
    .await
}

pub async fn persisted_api_key_user_by_token(
    api_key: &str,
    database_file: &Path,
) -> Option<AuthOutcome> {
    komga_infrastructure::auth::runtime_identity_access::persisted_api_key_user_by_token(
        api_key,
        database_file,
    )
    .await
}

pub async fn persisted_basic_user(
    headers: &HeaderMap,
    database_file: &Path,
) -> Option<AuthOutcome> {
    komga_infrastructure::auth::runtime_identity_access::persisted_basic_user(
        headers,
        database_file,
    )
    .await
}

pub async fn persisted_cleanup_authentication_activity(database_file: &Path) -> Option<u64> {
    komga_infrastructure::auth::runtime_identity_access::persisted_cleanup_authentication_activity(
        database_file,
    )
    .await
}

pub async fn persisted_create_api_key(
    database_file: &Path,
    user_id: &str,
    comment: &str,
) -> Option<PersistedApiKey> {
    komga_infrastructure::auth::runtime_identity_access::persisted_create_api_key(
        database_file,
        user_id,
        comment,
    )
    .await
}

pub async fn persisted_delete_api_key_by_id(
    database_file: &Path,
    user_id: &str,
    api_key_id: &str,
) -> Option<bool> {
    komga_infrastructure::auth::runtime_identity_access::persisted_delete_api_key_by_id(
        database_file,
        user_id,
        api_key_id,
    )
    .await
}

pub async fn persisted_latest_authentication_activity_by_user_and_api_key(
    database_file: &Path,
    user_id: &str,
    api_key_id: &str,
) -> Option<PersistedAuthenticationActivity> {
    komga_infrastructure::auth::runtime_identity_access::persisted_latest_authentication_activity_by_user_and_api_key(
        database_file,
        user_id,
        api_key_id,
    )
    .await
}

pub async fn persisted_list_api_keys(
    database_file: &Path,
    user_id: &str,
) -> Option<Vec<PersistedApiKey>> {
    komga_infrastructure::auth::runtime_identity_access::persisted_list_api_keys(
        database_file,
        user_id,
    )
    .await
}

pub async fn persisted_list_authentication_activity(
    database_file: &Path,
    user_id: Option<&str>,
) -> Option<Vec<PersistedAuthenticationActivity>> {
    komga_infrastructure::auth::runtime_identity_access::persisted_list_authentication_activity(
        database_file,
        user_id,
    )
    .await
}

pub async fn persisted_record_failed_authentication_activity(
    database_file: &Path,
    email: Option<&str>,
    input: AuthenticationActivityWriteInput,
    error: &str,
) -> Option<()> {
    komga_infrastructure::auth::runtime_identity_access::persisted_record_failed_authentication_activity(
        database_file,
        email,
        input.source.as_str(),
        error,
        input.ip.as_deref(),
        input.user_agent.as_deref(),
    )
    .await
}

pub async fn persisted_record_successful_authentication_activity(
    database_file: &Path,
    user: &AuthUser,
    input: AuthenticationActivityWriteInput,
) -> Option<()> {
    komga_infrastructure::auth::runtime_identity_access::persisted_record_successful_authentication_activity(
        database_file,
        user,
        input.source.as_str(),
        input.api_key_id.as_deref(),
        input.api_key_comment.as_deref(),
        input.ip.as_deref(),
        input.user_agent.as_deref(),
    )
    .await
}

pub async fn persisted_update_password_by_user_id(
    database_file: &Path,
    user_id: &str,
    password: &str,
) -> Option<bool> {
    komga_infrastructure::auth::runtime_identity_access::persisted_update_password_by_user_id(
        database_file,
        user_id,
        password,
    )
    .await
}

pub async fn persisted_users(database_file: &Path) -> Option<Vec<AuthUser>> {
    komga_infrastructure::auth::runtime_identity_access::persisted_users(database_file).await
}

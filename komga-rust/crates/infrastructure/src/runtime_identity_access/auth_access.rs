use std::path::Path;

use axum::http::HeaderMap;
use komga_application::identity_access::{
    AuthOutcome, AuthUser, PersistedApiKey, PersistedApiKeyMetadata,
    PersistedAuthenticationActivity,
};
use sqlx::SqlitePool;

use super::backend_state::backend;

pub fn auth_token_user(headers: &HeaderMap) -> Option<AuthUser> {
    (backend().auth_token_user)(headers.clone())
}

pub fn session_token_for_user_with_namespace(user: &AuthUser, namespace: &str) -> String {
    (backend().session_token_for_user_with_namespace)(user.clone(), namespace.to_string())
}

pub fn remember_me_token_for_user_with_namespace(
    user: &AuthUser,
    namespace: &str,
) -> Option<String> {
    (backend().remember_me_token_for_user_with_namespace)(user.clone(), namespace.to_string())
}

pub fn configure_remember_me_store(store_root: &Path) -> String {
    (backend().configure_remember_me_store)(store_root.to_path_buf())
}

pub fn invalidate_user_sessions(user_id: &str) {
    (backend().invalidate_user_sessions)(user_id.to_string())
}

pub fn invalidate_session_token(token: &str) {
    (backend().invalidate_session_token)(token.to_string())
}

pub fn invalidate_remember_me_token(token: &str) {
    (backend().invalidate_remember_me_token)(token.to_string())
}

pub async fn persisted_basic_user(
    headers: &HeaderMap,
    database_file: &Path,
) -> Option<AuthOutcome> {
    (backend().persisted_basic_user)(headers.clone(), database_file.to_path_buf()).await
}

pub async fn persisted_api_key_user(
    headers: &HeaderMap,
    database_file: &Path,
) -> Option<AuthOutcome> {
    (backend().persisted_api_key_user)(headers.clone(), database_file.to_path_buf()).await
}

pub async fn persisted_api_key_user_by_token(
    api_key: &str,
    database_file: &Path,
) -> Option<AuthOutcome> {
    (backend().persisted_api_key_user_by_token)(api_key.to_string(), database_file.to_path_buf())
        .await
}

pub async fn persisted_api_key_metadata(
    headers: &HeaderMap,
    database_file: &Path,
) -> Option<PersistedApiKeyMetadata> {
    (backend().persisted_api_key_metadata)(headers.clone(), database_file.to_path_buf()).await
}

pub async fn persisted_users(database_file: &Path) -> Option<Vec<AuthUser>> {
    (backend().persisted_users)(database_file.to_path_buf()).await
}

pub async fn persisted_update_password_by_user_id(
    database_file: &Path,
    user_id: &str,
    password: &str,
) -> Option<bool> {
    (backend().persisted_update_password_by_user_id)(
        database_file.to_path_buf(),
        user_id.to_string(),
        password.to_string(),
    )
    .await
}

pub async fn persisted_create_api_key(
    database_file: &Path,
    user_id: &str,
    comment: &str,
) -> Option<PersistedApiKey> {
    (backend().persisted_create_api_key)(
        database_file.to_path_buf(),
        user_id.to_string(),
        comment.to_string(),
    )
    .await
}

pub async fn persisted_api_key_comment_exists(
    database_file: &Path,
    user_id: &str,
    comment: &str,
) -> Option<bool> {
    (backend().persisted_api_key_comment_exists)(
        database_file.to_path_buf(),
        user_id.to_string(),
        comment.to_string(),
    )
    .await
}

pub async fn persisted_list_api_keys(
    database_file: &Path,
    user_id: &str,
) -> Option<Vec<PersistedApiKey>> {
    (backend().persisted_list_api_keys)(database_file.to_path_buf(), user_id.to_string()).await
}

pub async fn persisted_delete_api_key_by_id(
    database_file: &Path,
    user_id: &str,
    api_key_id: &str,
) -> Option<bool> {
    (backend().persisted_delete_api_key_by_id)(
        database_file.to_path_buf(),
        user_id.to_string(),
        api_key_id.to_string(),
    )
    .await
}

pub async fn persisted_list_authentication_activity(
    database_file: &Path,
    user_id: Option<&str>,
) -> Option<Vec<PersistedAuthenticationActivity>> {
    (backend().persisted_list_authentication_activity)(
        database_file.to_path_buf(),
        user_id.map(str::to_string),
    )
    .await
}

pub async fn persisted_cleanup_authentication_activity(database_file: &Path) -> Option<u64> {
    (backend().persisted_cleanup_authentication_activity)(database_file.to_path_buf()).await
}

pub async fn persisted_latest_authentication_activity_by_user_and_api_key(
    database_file: &Path,
    user_id: &str,
    api_key_id: &str,
) -> Option<PersistedAuthenticationActivity> {
    (backend().persisted_latest_authentication_activity_by_user_and_api_key)(
        database_file.to_path_buf(),
        user_id.to_string(),
        api_key_id.to_string(),
    )
    .await
}

pub async fn persisted_record_successful_authentication_activity(
    database_file: &Path,
    user: &AuthUser,
    source: &str,
    api_key_id: Option<&str>,
    api_key_comment: Option<&str>,
) -> Option<()> {
    (backend().persisted_record_successful_authentication_activity)(
        database_file.to_path_buf(),
        user.clone(),
        source.to_string(),
        api_key_id.map(str::to_string),
        api_key_comment.map(str::to_string),
    )
    .await
}

pub async fn ensure_oauth_user(
    database_file: &Path,
    email: &str,
    allow_create: bool,
) -> Result<Option<AuthUser>, sqlx::Error> {
    (backend().ensure_oauth_user)(database_file.to_path_buf(), email.to_string(), allow_create)
        .await
}

pub fn configured_api_key() -> Option<String> {
    (backend().configured_api_key)()
}

pub async fn open_auth_pool(database_file: &Path) -> Result<SqlitePool, sqlx::Error> {
    (backend().open_auth_pool)(database_file.to_path_buf()).await
}

mod request_metadata;
mod response;
mod token;
mod user;

use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use std::path::Path;

use crate::http::access_log;

pub use crate::runtime_identity_access::AuthenticationActivityWriteInput;
pub use komga_application::identity_access::{
    AuthOutcome, AuthUser, PersistedApiKey, PersistedApiKeyMetadata,
    PersistedAuthenticationActivity, user_has_role, user_id, user_is_admin, user_payload_json,
    user_shared_all_libraries, user_shared_library_ids,
};
pub use komga_infrastructure::auth::session_store::RememberMeRuntimeSettings;
pub use request_metadata::{
    authentication_activity_headers_metadata_with_remote_addr,
    authentication_activity_request_metadata, authentication_activity_write_input,
};
pub use response::{
    bootstrap_api_key_user, bootstrap_user, bootstrap_user_with_remember_me_cookies,
    bootstrap_user_with_remember_me_token, expired_remember_me_cookie, expired_session_cookie,
    unauthorized_json_response,
};
pub use token::{
    auth_token_user, empty_auth_token_supplied, remember_me_requested,
    remember_me_token_for_user_with_runtime_key, remember_me_token_from_headers, resolved_token,
    session_token_for_user_with_runtime_key, session_token_from_headers,
};
pub use user::{
    persisted_api_key_comment_exists, persisted_api_key_metadata, persisted_api_key_user,
    persisted_api_key_user_by_token, persisted_basic_user,
    persisted_cleanup_authentication_activity, persisted_create_api_key,
    persisted_delete_api_key_by_id, persisted_latest_authentication_activity_by_user_and_api_key,
    persisted_list_api_keys, persisted_list_authentication_activity,
    persisted_record_failed_authentication_activity,
    persisted_record_successful_authentication_activity, persisted_update_password_by_user_id,
    persisted_users,
};

fn record_resolved_auth_user(auth_user: Option<AuthUser>) -> Option<AuthUser> {
    access_log::record_resolved_auth_user_id(auth_user.as_ref().map(user_id));
    auth_user
}

pub fn require_auth(headers: &HeaderMap) -> Option<Response> {
    if resolved_auth_user(headers).is_some() {
        None
    } else {
        Some(StatusCode::UNAUTHORIZED.into_response())
    }
}

pub fn require_admin(headers: &HeaderMap) -> Option<Response> {
    match resolved_auth_user(headers) {
        Some(user) if user_is_admin(&user) => None,
        Some(_) => Some(StatusCode::FORBIDDEN.into_response()),
        None => Some(StatusCode::UNAUTHORIZED.into_response()),
    }
}

pub fn require_file_download(headers: &HeaderMap) -> Option<Response> {
    match resolved_auth_user(headers) {
        Some(user) if user_is_admin(&user) || user_has_role(&user, "FILE_DOWNLOAD") => None,
        Some(_) => Some(StatusCode::FORBIDDEN.into_response()),
        None => Some(StatusCode::UNAUTHORIZED.into_response()),
    }
}

pub fn resolved_auth_user(headers: &HeaderMap) -> Option<AuthUser> {
    record_resolved_auth_user(
        komga_infrastructure::auth::runtime_identity_access::auth_token_user(headers),
    )
}

pub async fn resolved_request_auth_user(
    headers: &HeaderMap,
    database_file: &Path,
) -> Option<AuthUser> {
    let auth_user = match persisted_api_key_user(headers, database_file)
        .await
        .unwrap_or(AuthOutcome::Missing)
    {
        AuthOutcome::Valid(user) => Some(*user),
        AuthOutcome::Invalid => None,
        AuthOutcome::Missing => {
            match komga_infrastructure::auth::runtime_identity_access::auth_token_user(headers) {
                Some(user) => Some(user),
                None => match persisted_basic_user(headers, database_file)
                    .await
                    .unwrap_or(AuthOutcome::Missing)
                {
                    AuthOutcome::Valid(user) => Some(*user),
                    AuthOutcome::Invalid | AuthOutcome::Missing => None,
                },
            }
        }
    };

    record_resolved_auth_user(auth_user)
}

pub async fn require_request_auth(headers: &HeaderMap, database_file: &Path) -> Option<Response> {
    if resolved_request_auth_user(headers, database_file)
        .await
        .is_some()
    {
        None
    } else {
        Some(StatusCode::UNAUTHORIZED.into_response())
    }
}

pub async fn require_request_admin(headers: &HeaderMap, database_file: &Path) -> Option<Response> {
    match resolved_request_auth_user(headers, database_file).await {
        Some(user) if user_is_admin(&user) => None,
        Some(_) => Some(StatusCode::FORBIDDEN.into_response()),
        None => Some(StatusCode::UNAUTHORIZED.into_response()),
    }
}

pub async fn require_request_file_download(
    headers: &HeaderMap,
    database_file: &Path,
) -> Option<Response> {
    match resolved_request_auth_user(headers, database_file).await {
        Some(user) if user_is_admin(&user) || user_has_role(&user, "FILE_DOWNLOAD") => None,
        Some(_) => Some(StatusCode::FORBIDDEN.into_response()),
        None => Some(StatusCode::UNAUTHORIZED.into_response()),
    }
}

pub fn sync_remember_me_runtime_settings(runtime_key: &str, key: &str, duration_days: u64) {
    komga_infrastructure::auth::runtime_identity_access::sync_remember_me_runtime_settings(
        runtime_key,
        RememberMeRuntimeSettings {
            key: key.to_string(),
            duration_days,
        },
    )
}

pub fn sync_remember_me_runtime_database_file(runtime_key: &str, database_file: &Path) {
    komga_infrastructure::auth::runtime_identity_access::sync_remember_me_runtime_database_file(
        runtime_key,
        database_file,
    )
}

pub fn sync_session_runtime_settings(runtime_key: &str, max_inactive_seconds: u64) {
    komga_infrastructure::auth::runtime_identity_access::sync_session_runtime_settings(
        runtime_key,
        max_inactive_seconds,
    )
}

pub fn remember_me_max_age_seconds(runtime_key: &str) -> u64 {
    komga_infrastructure::auth::runtime_identity_access::remember_me_max_age_seconds(runtime_key)
}

pub fn invalidate_user_sessions(user_id: &str) {
    komga_infrastructure::auth::runtime_identity_access::invalidate_user_sessions(user_id)
}

pub fn invalidate_user_sessions_for_runtime_key(user_id: &str, runtime_key: &str) {
    komga_infrastructure::auth::runtime_identity_access::invalidate_user_sessions_with_runtime_key(
        user_id,
        runtime_key,
    )
}

pub fn invalidate_session_token(token: &str) {
    komga_infrastructure::auth::runtime_identity_access::invalidate_session_token(token)
}

pub fn invalidate_remember_me_token(token: &str) {
    komga_infrastructure::auth::runtime_identity_access::invalidate_remember_me_token(token)
}

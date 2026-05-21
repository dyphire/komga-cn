mod extractors;
mod request_metadata;
mod response;
mod token;
mod user;

use axum::http::HeaderMap;

use crate::access_log;
use crate::state::IdentityState;

pub use crate::state::AuthenticationActivityWriteInput;
pub use extractors::{Admin, Authenticated, FileDownload};
pub use komga_application::identity_access::{
    AuthOutcome, AuthTokenSource, AuthUser, PersistedApiKey, PersistedApiKeyMetadata,
    PersistedAuthenticationActivity, user_has_role, user_id, user_is_admin, user_payload_json,
    user_shared_all_libraries, user_shared_library_ids,
};
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

pub fn resolved_auth_user(identity: &IdentityState, headers: &HeaderMap) -> Option<AuthUser> {
    resolved_auth_token(identity, headers).map(|resolved| resolved.user)
}

pub fn resolved_auth_token(
    identity: &IdentityState,
    headers: &HeaderMap,
) -> Option<komga_application::identity_access::ResolvedAuthToken> {
    let session_token = token::session_token_from_headers(headers);
    let remember_me_token = token::remember_me_token_from_headers(headers);
    let resolved = identity
        .session_resolver()
        .resolve_auth_token(session_token.as_deref(), remember_me_token.as_deref());
    access_log::record_resolved_auth_user_id(
        resolved.as_ref().map(|resolved| user_id(&resolved.user)),
    );
    resolved
}

pub async fn resolved_request_auth_user(
    identity: &IdentityState,
    headers: &HeaderMap,
) -> Option<AuthUser> {
    let auth_user = match persisted_api_key_user(identity, headers)
        .await
        .unwrap_or(AuthOutcome::Missing)
    {
        AuthOutcome::Valid(user) => Some(*user),
        AuthOutcome::Invalid => None,
        AuthOutcome::Missing => match auth_token_user(identity, headers) {
            Some(user) => Some(user),
            None => match persisted_basic_user(identity, headers)
                .await
                .unwrap_or(AuthOutcome::Missing)
            {
                AuthOutcome::Valid(user) => Some(*user),
                AuthOutcome::Invalid | AuthOutcome::Missing => None,
            },
        },
    };

    record_resolved_auth_user(auth_user)
}

pub fn sync_remember_me_runtime_settings(
    identity: &IdentityState,
    runtime_key: &str,
    key: &str,
    duration_days: u64,
) {
    identity
        .session_lifecycle()
        .sync_remember_me_runtime_settings(runtime_key, key, duration_days)
}

pub fn sync_remember_me_runtime_database_file(identity: &IdentityState, runtime_key: &str) {
    identity
        .session_lifecycle()
        .sync_remember_me_runtime_database_file(runtime_key);
}

pub fn sync_session_runtime_settings(
    identity: &IdentityState,
    runtime_key: &str,
    max_inactive_seconds: u64,
) {
    identity
        .session_lifecycle()
        .sync_session_runtime_settings(runtime_key, max_inactive_seconds);
}

pub fn remember_me_max_age_seconds(identity: &IdentityState, runtime_key: &str) -> u64 {
    identity
        .session_lifecycle()
        .remember_me_max_age_seconds(runtime_key)
}

pub fn invalidate_user_sessions(identity: &IdentityState, user_id: &str) {
    identity
        .session_lifecycle()
        .invalidate_user_sessions(user_id);
}

pub fn invalidate_user_sessions_for_runtime_key(
    identity: &IdentityState,
    user_id: &str,
    runtime_key: &str,
) {
    identity
        .session_lifecycle()
        .invalidate_user_sessions_with_runtime_key(user_id, runtime_key);
}

pub fn invalidate_session_token(identity: &IdentityState, token: &str) {
    identity.session_lifecycle().invalidate_session_token(token);
}

pub fn invalidate_remember_me_token(identity: &IdentityState, token: &str) {
    identity
        .session_lifecycle()
        .invalidate_remember_me_token(token);
}

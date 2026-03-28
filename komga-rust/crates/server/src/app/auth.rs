#[path = "auth/response.rs"]
mod response;
#[path = "auth/session.rs"]
mod session;
#[path = "auth/token.rs"]
mod token;
#[path = "auth/user.rs"]
mod user;

use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use std::path::Path;

pub(super) use response::{
    bootstrap_api_key_user, bootstrap_user, bootstrap_user_with_remember_me_cookies,
    bootstrap_user_with_remember_me_token, expired_remember_me_cookie, expired_session_cookie,
    unauthorized_json_response,
};
use session::SESSION_REGISTRY;
pub(super) use token::{
    auth_token_user, empty_auth_token_supplied, remember_me_requested,
    remember_me_token_for_user_with_namespace, remember_me_token_from_headers, resolved_token,
    session_token_for_user_with_namespace, session_token_from_headers,
};
pub(super) use user::{
    AuthOutcome, AuthUser, PersistedAuthenticationActivity, persisted_api_key_comment_exists,
    persisted_api_key_metadata, persisted_api_key_user, persisted_api_key_user_by_token,
    persisted_basic_user, persisted_cleanup_authentication_activity, persisted_create_api_key,
    persisted_delete_api_key_by_id, persisted_latest_authentication_activity_by_user_and_api_key,
    persisted_list_api_keys, persisted_list_authentication_activity,
    persisted_record_successful_authentication_activity, persisted_update_password_by_user_id,
    persisted_users, user_has_role, user_id, user_is_admin, user_payload_json,
    user_shared_all_libraries, user_shared_library_ids,
};

pub(super) fn require_auth(headers: &HeaderMap) -> Option<Response> {
    if resolved_auth_user(headers).is_some() {
        None
    } else {
        Some(StatusCode::UNAUTHORIZED.into_response())
    }
}

pub(super) fn require_admin(headers: &HeaderMap) -> Option<Response> {
    match resolved_auth_user(headers) {
        Some(user) if user_is_admin(&user) => None,
        Some(_) => Some(StatusCode::FORBIDDEN.into_response()),
        None => Some(StatusCode::UNAUTHORIZED.into_response()),
    }
}

pub(super) fn require_file_download(headers: &HeaderMap) -> Option<Response> {
    match resolved_auth_user(headers) {
        Some(user) if user_is_admin(&user) || user_has_role(&user, "FILE_DOWNLOAD") => None,
        Some(_) => Some(StatusCode::FORBIDDEN.into_response()),
        None => Some(StatusCode::UNAUTHORIZED.into_response()),
    }
}

pub(super) fn resolved_auth_user(headers: &HeaderMap) -> Option<AuthUser> {
    auth_token_user(headers)
}

pub(in crate::app) fn configure_remember_me_store(store_root: &Path) -> String {
    SESSION_REGISTRY.configure_remember_me_store(store_root)
}

pub(in crate::app) fn invalidate_user_sessions(user_id: &str) {
    SESSION_REGISTRY.invalidate_user_sessions(user_id)
}

pub(in crate::app) fn invalidate_session_token(token: &str) {
    SESSION_REGISTRY.invalidate_session_token(token)
}

pub(in crate::app) fn invalidate_remember_me_token(token: &str) {
    SESSION_REGISTRY.invalidate_remember_me_token(token)
}

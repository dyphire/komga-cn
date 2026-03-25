#[path = "placeholder_auth/response.rs"]
mod response;
#[path = "placeholder_auth/session.rs"]
mod session;
#[path = "placeholder_auth/token.rs"]
mod token;
#[path = "placeholder_auth/user.rs"]
mod user;

use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

pub(super) use response::{
    bootstrap_api_key_user, bootstrap_user, bootstrap_user_with_remember_me_cookies,
    bootstrap_user_with_remember_me_token, expired_remember_me_cookie, expired_session_cookie,
    unauthorized_json_response,
};
pub(in crate::app) use session::SESSION_REGISTRY;
pub(super) use token::{
    auth_token_user, empty_auth_token_supplied, remember_me_requested,
    remember_me_token_for_user_with_namespace, remember_me_token_from_headers, resolved_token,
    session_token_for_user, session_token_from_headers,
};
pub(super) use user::{
    AuthOutcome, PersistedAuthenticationActivity, PlaceholderUser, api_key_user, basic_user,
    configured_users, persisted_api_key_user, persisted_basic_user, persisted_create_api_key,
    persisted_delete_api_key_by_id, persisted_latest_authentication_activity_by_user_and_api_key,
    persisted_list_api_keys, persisted_list_authentication_activity,
    persisted_record_successful_authentication_activity, persisted_update_password_by_user_id,
    persisted_users, placeholder_user_json, user_id, user_is_admin, user_shared_all_libraries,
    user_shared_library_ids,
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

pub(super) fn resolved_auth_user(headers: &HeaderMap) -> Option<PlaceholderUser> {
    auth_token_user(headers)
}

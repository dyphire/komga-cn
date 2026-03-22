#[path = "placeholder_auth/response.rs"]
mod response;
#[path = "placeholder_auth/token.rs"]
mod token;
#[path = "placeholder_auth/user.rs"]
mod user;

use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

pub(super) use response::{
    bootstrap_api_key_user, bootstrap_user, bootstrap_user_with_remember_me_cookies,
    bootstrap_user_with_remember_me_token, unauthorized_json_response,
};
pub(super) use token::{
    auth_token_user, empty_auth_token_supplied, remember_me_requested, resolved_token,
    session_token_for_user,
};
pub(super) use user::{
    AuthOutcome, PlaceholderUser, api_key_user, basic_user, placeholder_user_json, user_is_admin,
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
        Some(user) if user_is_admin(user) => None,
        Some(_) => Some(StatusCode::FORBIDDEN.into_response()),
        None => Some(StatusCode::UNAUTHORIZED.into_response()),
    }
}

pub(super) fn resolved_auth_user(headers: &HeaderMap) -> Option<PlaceholderUser> {
    match api_key_user(headers) {
        AuthOutcome::Valid(user) => return Some(user),
        AuthOutcome::Invalid => return None,
        AuthOutcome::Missing => {}
    }

    if let Some(user) = auth_token_user(headers) {
        return Some(user);
    }

    match basic_user(headers) {
        AuthOutcome::Valid(user) => Some(user),
        AuthOutcome::Invalid | AuthOutcome::Missing => None,
    }
}

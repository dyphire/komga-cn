use axum::http::{HeaderMap, Uri};
use axum_extra::extract::cookie::CookieJar;
use komga_application::identity_access::AuthUser;

use crate::state::IdentityService;

pub fn auth_token_user(identity: &dyn IdentityService, headers: &HeaderMap) -> Option<AuthUser> {
    identity.auth_token_user(headers)
}

pub fn resolved_token(headers: &HeaderMap) -> String {
    session_token_from_headers(headers).unwrap_or_default()
}

pub fn session_token_from_headers(headers: &HeaderMap) -> Option<String> {
    x_auth_token(headers).or_else(|| session_cookie_token(headers))
}

pub fn empty_auth_token_supplied(headers: &HeaderMap) -> bool {
    headers
        .get("X-Auth-Token")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim().is_empty())
}

pub fn remember_me_token_from_headers(headers: &HeaderMap) -> Option<String> {
    remember_me_cookie_token(headers)
}

pub fn remember_me_requested(uri: &Uri) -> bool {
    uri.query()
        .is_some_and(|query| query.split('&').any(|pair| pair == "remember-me=true"))
}

pub fn session_token_for_user_with_runtime_key(
    identity: &dyn IdentityService,
    user: &AuthUser,
    runtime_key: &str,
) -> String {
    identity.session_token_for_user_with_runtime_key(user, runtime_key)
}

pub fn remember_me_token_for_user_with_runtime_key(
    identity: &dyn IdentityService,
    user: &AuthUser,
    runtime_key: &str,
) -> Option<String> {
    identity.remember_me_token_for_user_with_runtime_key(user, runtime_key)
}

fn x_auth_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("X-Auth-Token")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.to_string())
}

fn session_cookie_token(headers: &HeaderMap) -> Option<String> {
    let jar = CookieJar::from_headers(headers);
    jar.get("KOMGA-SESSION")
        .map(|cookie| cookie.value().trim().to_string())
        .filter(|value| !value.is_empty())
}

fn remember_me_cookie_token(headers: &HeaderMap) -> Option<String> {
    let jar = CookieJar::from_headers(headers);
    jar.get("komga-remember-me")
        .map(|cookie| cookie.value().trim().to_string())
        .filter(|value| !value.is_empty())
}

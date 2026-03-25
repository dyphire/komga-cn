use axum::http::{HeaderMap, Uri};
use axum_extra::extract::cookie::CookieJar;

use super::session::SESSION_REGISTRY;
use super::user::PlaceholderUser;

pub(in crate::app) fn auth_token_user(headers: &HeaderMap) -> Option<PlaceholderUser> {
    if let Some(session_token) = session_token_from_headers(headers)
        && let Some(user) = SESSION_REGISTRY.resolve_user(&session_token)
    {
        return Some(user);
    }

    let remember_me_token = remember_me_cookie_token(headers)?;
    SESSION_REGISTRY.resolve_user_by_remember_me_token(&remember_me_token)
}

pub(in crate::app) fn resolved_token(headers: &HeaderMap) -> String {
    session_token_from_headers(headers).unwrap_or_default()
}

pub(in crate::app) fn session_token_from_headers(headers: &HeaderMap) -> Option<String> {
    x_auth_token(headers).or_else(|| session_cookie_token(headers))
}

pub(in crate::app) fn empty_auth_token_supplied(headers: &HeaderMap) -> bool {
    headers
        .get("X-Auth-Token")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim().is_empty())
}

pub(in crate::app) fn remember_me_token_from_headers(headers: &HeaderMap) -> Option<String> {
    remember_me_cookie_token(headers)
}

pub(in crate::app) fn remember_me_requested(uri: &Uri) -> bool {
    uri.query()
        .is_some_and(|query| query.split('&').any(|pair| pair == "remember-me=true"))
}

pub(in crate::app) fn session_token_for_user(user: &PlaceholderUser) -> String {
    SESSION_REGISTRY.issue_session_token(user)
}

pub(in crate::app) fn remember_me_token_for_user_with_namespace(
    user: &PlaceholderUser,
    namespace: &str,
) -> Option<String> {
    if namespace.trim().is_empty() {
        return None;
    }
    SESSION_REGISTRY.issue_remember_me_token(user, namespace)
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

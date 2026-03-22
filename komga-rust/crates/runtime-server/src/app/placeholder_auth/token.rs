use axum::http::{HeaderMap, Uri, header};

use super::user::{PlaceholderUser, default_placeholder_user, placeholder_users};

pub(in crate::app) fn auth_token_user(headers: &HeaderMap) -> Option<PlaceholderUser> {
    let token = resolved_token(headers);
    placeholder_users()
        .iter()
        .copied()
        .find(|user| token == session_token(*user))
        .or_else(|| {
            (!token.trim().is_empty() && token != "generated-token")
                .then_some(default_placeholder_user())
        })
}

pub(in crate::app) fn resolved_token(headers: &HeaderMap) -> String {
    x_auth_token(headers)
        .or_else(|| session_cookie_token(headers))
        .unwrap_or_else(|| "generated-token".to_string())
}

pub(in crate::app) fn empty_auth_token_supplied(headers: &HeaderMap) -> bool {
    headers
        .get("X-Auth-Token")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim().is_empty())
}

pub(in crate::app) fn remember_me_requested(uri: &Uri) -> bool {
    uri.query()
        .is_some_and(|query| query.split('&').any(|pair| pair == "remember-me=true"))
}

pub(in crate::app) fn session_token_for_user(user: PlaceholderUser) -> String {
    session_token(user)
}

pub(super) fn session_token(user: PlaceholderUser) -> String {
    format!("komga-{}-token", user.id())
}

pub(super) fn remember_me_token(user: PlaceholderUser) -> String {
    format!("komga-{}-remember-me-token", user.id())
}

fn x_auth_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("X-Auth-Token")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.to_string())
}

fn session_cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .split(';')
                .map(str::trim)
                .find_map(|cookie| cookie.strip_prefix("KOMGA-SESSION="))
        })
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.to_string())
}

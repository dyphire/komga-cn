use axum::http::{HeaderMap, header};

pub(super) fn intersection(requested: &[String], authorized: &[String]) -> Vec<String> {
    requested
        .iter()
        .filter(|candidate| authorized.contains(*candidate))
        .cloned()
        .collect::<Vec<_>>()
}

pub(super) fn session_token_from_headers(headers: &HeaderMap) -> Option<String> {
    x_auth_token(headers).or_else(|| session_cookie_token(headers))
}

fn x_auth_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("X-Auth-Token")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn session_cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| {
            cookies
                .split(';')
                .map(str::trim)
                .find_map(|cookie| cookie.strip_prefix("KOMGA-SESSION="))
        })
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
}

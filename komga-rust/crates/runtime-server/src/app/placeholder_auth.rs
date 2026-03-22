use axum::http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};

#[derive(Clone, Copy)]
pub(super) struct PlaceholderUser {
    id: &'static str,
    email: &'static str,
    password: &'static str,
    shared_all_libraries: bool,
    shared_library_ids: &'static [&'static str],
}

const PLACEHOLDER_USERS: &[PlaceholderUser] = &[
    PlaceholderUser {
        id: "admin",
        email: "admin@example.org",
        password: "admin",
        shared_all_libraries: true,
        shared_library_ids: &[],
    },
    PlaceholderUser {
        id: "user",
        email: "user@example.org",
        password: "user",
        shared_all_libraries: true,
        shared_library_ids: &[],
    },
    PlaceholderUser {
        id: "limited",
        email: "limited@example.org",
        password: "limited",
        shared_all_libraries: false,
        shared_library_ids: &["1"],
    },
    PlaceholderUser {
        id: "restricted",
        email: "restricted@example.org",
        password: "restricted",
        shared_all_libraries: true,
        shared_library_ids: &[],
    },
];

pub(super) enum AuthOutcome {
    Valid(PlaceholderUser),
    Invalid,
    Missing,
}

pub(super) fn default_placeholder_user() -> PlaceholderUser {
    PLACEHOLDER_USERS[0]
}

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

pub(super) fn auth_token_user(headers: &HeaderMap) -> Option<PlaceholderUser> {
    let token = resolved_token(headers);
    PLACEHOLDER_USERS
        .iter()
        .copied()
        .find(|user| token == session_token(*user))
        .or_else(|| {
            (!token.trim().is_empty() && token != "generated-token")
                .then_some(default_placeholder_user())
        })
}

pub(super) fn resolved_token(headers: &HeaderMap) -> String {
    x_auth_token(headers)
        .or_else(|| session_cookie_token(headers))
        .unwrap_or_else(|| "generated-token".to_string())
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

pub(super) fn empty_auth_token_supplied(headers: &HeaderMap) -> bool {
    headers
        .get("X-Auth-Token")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim().is_empty())
}

pub(super) fn basic_user(headers: &HeaderMap) -> AuthOutcome {
    let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return AuthOutcome::Missing;
    };

    let value = value.trim();
    if value.is_empty() {
        return AuthOutcome::Missing;
    }

    let Some(encoded) = value.strip_prefix("Basic ") else {
        return AuthOutcome::Invalid;
    };

    let decoded = match STANDARD.decode(encoded) {
        Ok(decoded) => decoded,
        Err(_) => return AuthOutcome::Invalid,
    };

    let credentials = match String::from_utf8(decoded) {
        Ok(credentials) => credentials,
        Err(_) => return AuthOutcome::Invalid,
    };

    let Some((username, password)) = credentials.split_once(':') else {
        return AuthOutcome::Invalid;
    };

    PLACEHOLDER_USERS
        .iter()
        .copied()
        .find(|user| user.email == username && user.password == password)
        .map(AuthOutcome::Valid)
        .unwrap_or(AuthOutcome::Invalid)
}

pub(super) fn api_key_user(headers: &HeaderMap) -> AuthOutcome {
    let Some(value) = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
    else {
        return AuthOutcome::Missing;
    };

    let value = value.trim();
    if value.is_empty() {
        return AuthOutcome::Invalid;
    }

    if value == configured_api_key().as_str() {
        AuthOutcome::Valid(PLACEHOLDER_USERS[1])
    } else {
        AuthOutcome::Invalid
    }
}

pub(super) fn remember_me_requested(uri: &Uri) -> bool {
    uri.query()
        .is_some_and(|query| query.split('&').any(|pair| pair == "remember-me=true"))
}

pub(super) fn bootstrap_user(user: PlaceholderUser, token: String) -> Response {
    let cookie = format!("KOMGA-SESSION={token}; Path=/");

    (
        StatusCode::OK,
        [
            (
                HeaderName::from_static("x-auth-token"),
                HeaderValue::from_str(&token)
                    .unwrap_or_else(|_| HeaderValue::from_static("generated-token")),
            ),
            (
                header::SET_COOKIE,
                HeaderValue::from_str(&cookie)
                    .unwrap_or_else(|_| HeaderValue::from_static("KOMGA-SESSION=; Path=/")),
            ),
        ],
        Json(placeholder_user_json(user)),
    )
        .into_response()
}

pub(super) fn bootstrap_user_with_remember_me_cookies(user: PlaceholderUser) -> Response {
    let session_cookie = format!(
        "KOMGA-SESSION={}; Path=/; HttpOnly; SameSite=Lax",
        session_token(user)
    );
    let remember_me_cookie = format!(
        "komga-remember-me={}; Path=/; HttpOnly; Max-Age=2592000; Expires=Sun, 18 Apr 2038 23:59:59 GMT",
        remember_me_token(user)
    );

    let mut response = (StatusCode::OK, Json(placeholder_user_json(user))).into_response();
    let headers = response.headers_mut();
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&remember_me_cookie)
            .unwrap_or_else(|_| HeaderValue::from_static("komga-remember-me=; Path=/; HttpOnly")),
    );
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&session_cookie).unwrap_or_else(|_| {
            HeaderValue::from_static("KOMGA-SESSION=; Path=/; HttpOnly; SameSite=Lax")
        }),
    );
    response
}

pub(super) fn bootstrap_user_with_remember_me_token(
    user: PlaceholderUser,
    token: String,
) -> Response {
    let remember_me_cookie = format!(
        "komga-remember-me={}; Path=/; HttpOnly; Max-Age=2592000; Expires=Sun, 18 Apr 2038 23:59:59 GMT",
        remember_me_token(user)
    );

    let mut response = (StatusCode::OK, Json(placeholder_user_json(user))).into_response();
    let headers = response.headers_mut();
    headers.insert(
        HeaderName::from_static("x-auth-token"),
        HeaderValue::from_str(&token)
            .unwrap_or_else(|_| HeaderValue::from_static("generated-token")),
    );
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&remember_me_cookie)
            .unwrap_or_else(|_| HeaderValue::from_static("komga-remember-me=; Path=/; HttpOnly")),
    );
    response
}

pub(super) fn bootstrap_api_key_user(user: PlaceholderUser) -> Response {
    let session_cookie = format!(
        "KOMGA-SESSION={}; Path=/; HttpOnly; SameSite=Lax",
        session_token(user)
    );

    let mut response = (StatusCode::OK, Json(placeholder_user_json(user))).into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_str(&session_cookie).unwrap_or_else(|_| {
            HeaderValue::from_static("KOMGA-SESSION=; Path=/; HttpOnly; SameSite=Lax")
        }),
    );
    response
}

pub(super) fn unauthorized_json_response(path: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "error": "Unauthorized",
            "message": "Unauthorized",
            "path": path,
            "status": 401,
            "timestamp": "1970-01-01T00:00:00.000+00:00",
        })),
    )
        .into_response()
}

pub(super) fn session_token_for_user(user: PlaceholderUser) -> String {
    session_token(user)
}

pub(super) fn user_is_admin(user: PlaceholderUser) -> bool {
    user.id == "admin"
}

pub(super) fn user_shared_all_libraries(user: PlaceholderUser) -> bool {
    user.shared_all_libraries
}

pub(super) fn user_shared_library_ids(user: PlaceholderUser) -> &'static [&'static str] {
    user.shared_library_ids
}

fn session_token(user: PlaceholderUser) -> String {
    format!("komga-{}-token", user.id)
}

fn remember_me_token(user: PlaceholderUser) -> String {
    format!("komga-{}-remember-me-token", user.id)
}

fn configured_api_key() -> String {
    std::env::var("KOMGA_COMPAT_API_KEY").unwrap_or_else(|_| "compat-api-key".to_string())
}

pub(super) fn placeholder_user_json(user: PlaceholderUser) -> Value {
    if user.email == "admin@example.org" {
        json!({
            "id": user.id,
            "email": user.email,
            "roles": ["ADMIN", "FILE_DOWNLOAD", "PAGE_STREAMING", "USER"],
            "sharedAllLibraries": user.shared_all_libraries,
            "sharedLibrariesIds": user.shared_library_ids,
            "labelsAllow": [],
            "labelsExclude": [],
            "ageRestriction": null,
        })
    } else if user.email == "user@example.org" || user.email == "limited@example.org" {
        json!({
            "id": if user.email == "user@example.org" { "0PV32486S7X3J" } else { "1PXGX4XP02A26" },
            "email": user.email,
            "roles": ["FILE_DOWNLOAD", "PAGE_STREAMING", "USER"],
            "sharedAllLibraries": user.shared_all_libraries,
            "sharedLibrariesIds": user.shared_library_ids,
            "labelsAllow": [],
            "labelsExclude": [],
            "ageRestriction": null,
        })
    } else if user.email == "restricted@example.org" {
        json!({
            "id": "2R3STR1CT3D",
            "email": user.email,
            "roles": ["FILE_DOWNLOAD", "PAGE_STREAMING", "USER"],
            "sharedAllLibraries": true,
            "sharedLibrariesIds": [],
            "labelsAllow": [],
            "labelsExclude": ["adult"],
            "ageRestriction": null,
        })
    } else {
        json!({
            "id": user.id,
            "email": user.email
        })
    }
}

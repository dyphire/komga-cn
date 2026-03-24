use axum::http::{header, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use axum_extra::extract::cookie::{Cookie, SameSite};
use serde_json::json;

use super::token::remember_me_token_for_session;
use super::user::{placeholder_user_json, PlaceholderUser};

pub(in crate::app) fn bootstrap_user(user: PlaceholderUser, token: String) -> Response {
    let session_cookie = Cookie::build(("KOMGA-SESSION", token.clone()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .build()
        .to_string();

    (
        StatusCode::OK,
        [
            (
                HeaderName::from_static("x-auth-token"),
                HeaderValue::from_str(&token).unwrap_or_else(|_| HeaderValue::from_static("")),
            ),
            (
                header::SET_COOKIE,
                HeaderValue::from_str(&session_cookie).unwrap_or_else(|_| {
                    HeaderValue::from_static("KOMGA-SESSION=; Path=/; HttpOnly; SameSite=Lax")
                }),
            ),
        ],
        Json(placeholder_user_json(&user)),
    )
        .into_response()
}

pub(in crate::app) fn bootstrap_user_with_remember_me_cookies(
    user: PlaceholderUser,
    session_token: String,
) -> Response {
    let session_cookie = Cookie::build(("KOMGA-SESSION", session_token.clone()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .build()
        .to_string();
    let remember_me_cookie = format!(
        "komga-remember-me={}; Path=/; HttpOnly; Max-Age=2592000; Expires=Sun, 18 Apr 2038 23:59:59 GMT",
        remember_me_token_for_session(&session_token)
    );

    let mut response = (StatusCode::OK, Json(placeholder_user_json(&user))).into_response();
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

pub(in crate::app) fn bootstrap_user_with_remember_me_token(
    user: PlaceholderUser,
    token: String,
) -> Response {
    let remember_me_cookie = format!(
        "komga-remember-me={}; Path=/; HttpOnly; Max-Age=2592000; Expires=Sun, 18 Apr 2038 23:59:59 GMT",
        remember_me_token_for_session(&token)
    );

    let mut response = (StatusCode::OK, Json(placeholder_user_json(&user))).into_response();
    let headers = response.headers_mut();
    headers.insert(
        HeaderName::from_static("x-auth-token"),
        HeaderValue::from_str(&token).unwrap_or_else(|_| HeaderValue::from_static("")),
    );
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&remember_me_cookie)
            .unwrap_or_else(|_| HeaderValue::from_static("komga-remember-me=; Path=/; HttpOnly")),
    );
    response
}

pub(in crate::app) fn bootstrap_api_key_user(user: PlaceholderUser, token: String) -> Response {
    let session_cookie = Cookie::build(("KOMGA-SESSION", token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .build()
        .to_string();

    let mut response = (StatusCode::OK, Json(placeholder_user_json(&user))).into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_str(&session_cookie).unwrap_or_else(|_| {
            HeaderValue::from_static("KOMGA-SESSION=; Path=/; HttpOnly; SameSite=Lax")
        }),
    );
    response
}

pub(in crate::app) fn unauthorized_json_response(path: &str) -> Response {
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

pub(in crate::app) fn expired_session_cookie() -> HeaderValue {
    HeaderValue::from_static("KOMGA-SESSION=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax")
}

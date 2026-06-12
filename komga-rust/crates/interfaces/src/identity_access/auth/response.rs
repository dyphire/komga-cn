use axum::Json;
use axum::http::{HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::{Cookie, SameSite};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};
use time::Duration;

use crate::identity_access::user_payload_json;
use komga_application::identity_access::AuthUser;

pub(crate) fn bootstrap_user(user: AuthUser, token: String) -> Response {
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
        Json(user_payload_json(&user)),
    )
        .into_response()
}

pub(crate) fn bootstrap_user_with_remember_me_cookies(
    user: AuthUser,
    session_token: String,
    remember_me_token: String,
    remember_me_max_age_seconds: u64,
) -> Response {
    let session_cookie = Cookie::build(("KOMGA-SESSION", session_token.clone()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .build()
        .to_string();
    let remember_me_cookie = Cookie::build(("komga-remember-me", remember_me_token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(Duration::seconds(remember_me_max_age_seconds as i64))
        .build()
        .to_string();

    let mut response = (StatusCode::OK, Json(user_payload_json(&user))).into_response();
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

pub(crate) fn bootstrap_user_with_remember_me_token(
    user: AuthUser,
    token: String,
    remember_me_token: String,
    remember_me_max_age_seconds: u64,
) -> Response {
    let remember_me_cookie = Cookie::build(("komga-remember-me", remember_me_token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(Duration::seconds(remember_me_max_age_seconds as i64))
        .build()
        .to_string();

    let mut response = (StatusCode::OK, Json(user_payload_json(&user))).into_response();
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

pub(crate) fn bootstrap_api_key_user(user: AuthUser, token: String) -> Response {
    let session_cookie = Cookie::build(("KOMGA-SESSION", token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .build()
        .to_string();

    let mut response = (StatusCode::OK, Json(user_payload_json(&user))).into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_str(&session_cookie).unwrap_or_else(|_| {
            HeaderValue::from_static("KOMGA-SESSION=; Path=/; HttpOnly; SameSite=Lax")
        }),
    );
    response
}

pub(crate) fn unauthorized_json_response(path: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "error": "Unauthorized",
            "message": "Unauthorized",
            "path": path,
            "status": 401,
            "timestamp": now_epoch_millis(),
        })),
    )
        .into_response()
}

fn now_epoch_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

pub(crate) fn expired_session_cookie() -> HeaderValue {
    HeaderValue::from_static("KOMGA-SESSION=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax")
}

pub(crate) fn expired_remember_me_cookie() -> HeaderValue {
    HeaderValue::from_static("komga-remember-me=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax")
}

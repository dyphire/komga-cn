use axum::Json;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};

use crate::app::placeholder_auth::{
    AuthOutcome, api_key_user, auth_token_user, basic_user, bootstrap_api_key_user,
    bootstrap_user, bootstrap_user_with_remember_me_cookies, bootstrap_user_with_remember_me_token,
    empty_auth_token_supplied, remember_me_requested, require_auth, resolved_token,
    session_token_for_user,
    unauthorized_json_response,
};

pub(super) async fn users_me(headers: HeaderMap, uri: Uri) -> Response {
    match api_key_user(&headers) {
        AuthOutcome::Valid(user) => return bootstrap_api_key_user(user),
        AuthOutcome::Invalid => return unauthorized_json_response(uri.path()),
        AuthOutcome::Missing => {}
    }

    if let Some(user) = auth_token_user(&headers) {
        return Json(crate::app::placeholder_auth::placeholder_user_json(user)).into_response();
    }

    match basic_user(&headers) {
        AuthOutcome::Valid(user) if remember_me_requested(&uri) => {
            if empty_auth_token_supplied(&headers) {
                bootstrap_user_with_remember_me_token(user, resolved_token(&headers))
            } else {
                bootstrap_user_with_remember_me_cookies(user)
            }
        }
        AuthOutcome::Valid(user) => bootstrap_user(user, session_token_for_user(user)),
        AuthOutcome::Invalid => StatusCode::UNAUTHORIZED.into_response(),
        AuthOutcome::Missing => StatusCode::UNAUTHORIZED.into_response(),
    }
}

pub(super) async fn login_set_cookie(headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let token = resolved_token(&headers);
    let cookie = format!("KOMGA-SESSION={token}; Path=/");

    (
        StatusCode::NO_CONTENT,
        [(
            header::SET_COOKIE,
            HeaderValue::from_str(&cookie)
                .unwrap_or_else(|_| HeaderValue::from_static("KOMGA-SESSION=; Path=/")),
        )],
    )
        .into_response()
}

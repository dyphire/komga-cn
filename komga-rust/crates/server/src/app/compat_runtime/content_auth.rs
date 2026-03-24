use axum::Json;
use axum::extract::Path;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::{Cookie, SameSite};
use serde_json::Value;
use serde_json::json;

use super::super::AuthDatabaseState;
use crate::app::discovery_auth::{DiscoveryAuthState, principal_from_user_payload};
use crate::app::placeholder_auth::{
    AuthOutcome, PersistedAuthenticationActivity, PlaceholderUser, SESSION_REGISTRY, api_key_user,
    auth_token_user, basic_user, bootstrap_api_key_user, bootstrap_user,
    bootstrap_user_with_remember_me_cookies, bootstrap_user_with_remember_me_token,
    configured_users, empty_auth_token_supplied, expired_session_cookie, persisted_api_key_user,
    persisted_basic_user, persisted_create_api_key, persisted_delete_api_key_by_id,
    persisted_latest_authentication_activity_by_user_and_api_key, persisted_list_api_keys,
    persisted_list_authentication_activity, persisted_record_successful_authentication_activity,
    persisted_update_password_by_user_id, persisted_users, placeholder_user_json,
    remember_me_requested, require_admin, require_auth, resolved_token, session_token_for_user,
    session_token_from_headers, unauthorized_json_response, user_id, user_is_admin,
};

pub(super) async fn users_me(
    headers: HeaderMap,
    uri: Uri,
    auth_state: DiscoveryAuthState,
    auth_db: AuthDatabaseState,
) -> Response {
    match persisted_api_key_user(&headers, auth_db.database_file.as_path())
        .await
        .unwrap_or_else(|| api_key_user(&headers))
    {
        AuthOutcome::Valid(user) => {
            let token = session_token_for_user(&user);
            register_discovery_principal(
                &auth_state,
                &crate::app::placeholder_auth::placeholder_user_json(&user),
                &token,
            );
            return bootstrap_api_key_user(user, token);
        }
        AuthOutcome::Invalid => return unauthorized_json_response(uri.path()),
        AuthOutcome::Missing => {}
    }

    if let Some(user) = auth_token_user(&headers) {
        let token = resolved_token(&headers);
        let payload = crate::app::placeholder_auth::placeholder_user_json(&user);
        register_discovery_principal(&auth_state, &payload, &token);
        return Json(payload).into_response();
    }

    match persisted_basic_user(&headers, auth_db.database_file.as_path())
        .await
        .unwrap_or_else(|| basic_user(&headers))
    {
        AuthOutcome::Valid(user) if remember_me_requested(&uri) => {
            let _ = persisted_record_successful_authentication_activity(
                auth_db.database_file.as_path(),
                &user,
                "BASIC",
                None,
                None,
            )
            .await;
            if empty_auth_token_supplied(&headers) {
                let token = session_token_for_user(&user);
                register_discovery_principal(
                    &auth_state,
                    &crate::app::placeholder_auth::placeholder_user_json(&user),
                    &token,
                );
                bootstrap_user_with_remember_me_token(user, token)
            } else {
                let token = session_token_for_user(&user);
                register_discovery_principal(
                    &auth_state,
                    &crate::app::placeholder_auth::placeholder_user_json(&user),
                    &token,
                );
                bootstrap_user_with_remember_me_cookies(user, token)
            }
        }
        AuthOutcome::Valid(user) => {
            let _ = persisted_record_successful_authentication_activity(
                auth_db.database_file.as_path(),
                &user,
                "BASIC",
                None,
                None,
            )
            .await;
            let token = session_token_for_user(&user);
            register_discovery_principal(
                &auth_state,
                &crate::app::placeholder_auth::placeholder_user_json(&user),
                &token,
            );
            bootstrap_user(user, token)
        }
        AuthOutcome::Invalid => StatusCode::UNAUTHORIZED.into_response(),
        AuthOutcome::Missing => StatusCode::UNAUTHORIZED.into_response(),
    }
}

pub(super) async fn login_set_cookie(headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let token = resolved_token(&headers);
    let cookie = Cookie::build(("KOMGA-SESSION", token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .build()
        .to_string();

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

pub(super) async fn users_list(headers: HeaderMap, auth_db: AuthDatabaseState) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let users = persisted_users(auth_db.database_file.as_path())
        .await
        .unwrap_or_else(configured_users);

    Json(users.iter().map(placeholder_user_json).collect::<Vec<_>>()).into_response()
}

pub(super) async fn logout(headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if let Some(token) = session_token_from_headers(&headers) {
        SESSION_REGISTRY.invalidate_session_token(&token);
    }

    (
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, expired_session_cookie())],
    )
        .into_response()
}

pub(super) async fn users_me_password(
    headers: HeaderMap,
    body: Value,
    auth_db: AuthDatabaseState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let Some(password) = password_from_request(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    match persisted_update_password_by_user_id(
        auth_db.database_file.as_path(),
        user_id(&current_user),
        password,
    )
    .await
    {
        Some(true) => StatusCode::NO_CONTENT.into_response(),
        Some(false) => StatusCode::NOT_FOUND.into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub(super) async fn users_by_id_password(
    headers: HeaderMap,
    Path(target_user_id): Path<String>,
    body: Value,
    auth_db: AuthDatabaseState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !user_is_admin(&current_user) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let Some(password) = password_from_request(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    match persisted_update_password_by_user_id(
        auth_db.database_file.as_path(),
        &target_user_id,
        password,
    )
    .await
    {
        Some(true) => StatusCode::NO_CONTENT.into_response(),
        Some(false) => StatusCode::NOT_FOUND.into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub(super) async fn users_me_api_keys_create(
    headers: HeaderMap,
    body: Value,
    auth_db: AuthDatabaseState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(comment) = api_key_comment_from_request(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    match persisted_create_api_key(
        auth_db.database_file.as_path(),
        user_id(&current_user),
        comment,
    )
    .await
    {
        Some(api_key) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "id": api_key.id,
                "userId": api_key.user_id,
                "key": api_key.key,
                "comment": api_key.comment,
            })),
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub(super) async fn users_me_api_keys_list(
    headers: HeaderMap,
    auth_db: AuthDatabaseState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let api_keys = persisted_list_api_keys(auth_db.database_file.as_path(), user_id(&current_user))
        .await
        .unwrap_or_default();

    Json(
        api_keys
            .iter()
            .map(|api_key| {
                serde_json::json!({
                    "id": api_key.id,
                    "userId": api_key.user_id,
                    "key": api_key.key,
                    "comment": api_key.comment,
                })
            })
            .collect::<Vec<_>>(),
    )
    .into_response()
}

pub(super) async fn users_me_api_keys_delete(
    headers: HeaderMap,
    Path(api_key_id): Path<String>,
    auth_db: AuthDatabaseState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    match persisted_delete_api_key_by_id(
        auth_db.database_file.as_path(),
        user_id(&current_user),
        &api_key_id,
    )
    .await
    {
        Some(true) => StatusCode::NO_CONTENT.into_response(),
        Some(false) => StatusCode::NOT_FOUND.into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub(super) async fn users_me_authentication_activity(
    headers: HeaderMap,
    uri: Uri,
    auth_db: AuthDatabaseState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let unpaged = query_bool(uri.query().unwrap_or_default(), "unpaged");
    let rows = persisted_list_authentication_activity(
        auth_db.database_file.as_path(),
        Some(user_id(&current_user)),
    )
    .await
    .unwrap_or_default();

    Json(authentication_activity_page_payload(rows, unpaged)).into_response()
}

pub(super) async fn users_authentication_activity(
    headers: HeaderMap,
    uri: Uri,
    auth_db: AuthDatabaseState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !user_is_admin(&current_user) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let unpaged = query_bool(uri.query().unwrap_or_default(), "unpaged");
    let rows = persisted_list_authentication_activity(auth_db.database_file.as_path(), None)
        .await
        .unwrap_or_default();

    Json(authentication_activity_page_payload(rows, unpaged)).into_response()
}

pub(super) async fn users_by_id_authentication_activity_latest(
    headers: HeaderMap,
    Path(target_user_id): Path<String>,
    uri: Uri,
    auth_db: AuthDatabaseState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !user_is_admin(&current_user) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let api_key_id = query_value(uri.query().unwrap_or_default(), "apikey_id")
        .and_then(|value| (!value.is_empty()).then_some(value));

    let activity = if let Some(api_key_id) = api_key_id {
        persisted_latest_authentication_activity_by_user_and_api_key(
            auth_db.database_file.as_path(),
            &target_user_id,
            api_key_id,
        )
        .await
    } else {
        persisted_list_authentication_activity(auth_db.database_file.as_path(), Some(&target_user_id))
            .await
            .and_then(|rows| rows.into_iter().next())
    };

    let Some(activity) = activity else {
        return StatusCode::NOT_FOUND.into_response();
    };

    Json(authentication_activity_payload(&activity)).into_response()
}

fn register_discovery_principal(
    auth_state: &DiscoveryAuthState,
    payload: &serde_json::Value,
    token: &str,
) {
    if let Some(principal) = principal_from_user_payload(payload) {
        auth_state.register_session_principal(token, principal);
    }
}

fn password_from_request(body: &Value) -> Option<&str> {
    body.get("password")?
        .as_str()
        .filter(|password| !password.is_empty())
}

fn api_key_comment_from_request(body: &Value) -> Option<&str> {
    body.get("comment")?
        .as_str()
        .filter(|comment| !comment.is_empty())
}

async fn authenticated_user(
    headers: &HeaderMap,
    auth_db: &AuthDatabaseState,
) -> Option<PlaceholderUser> {
    match persisted_api_key_user(headers, auth_db.database_file.as_path())
        .await
        .unwrap_or_else(|| api_key_user(headers))
    {
        AuthOutcome::Valid(user) => return Some(user),
        AuthOutcome::Invalid => return None,
        AuthOutcome::Missing => {}
    }

    if let Some(user) = auth_token_user(headers) {
        return Some(user);
    }

    match persisted_basic_user(headers, auth_db.database_file.as_path())
        .await
        .unwrap_or_else(|| basic_user(headers))
    {
        AuthOutcome::Valid(user) => Some(user),
        AuthOutcome::Invalid | AuthOutcome::Missing => None,
    }
}

fn authentication_activity_page_payload(
    rows: Vec<PersistedAuthenticationActivity>,
    unpaged: bool,
) -> Value {
    let content = rows
        .iter()
        .map(authentication_activity_payload)
        .collect::<Vec<_>>();
    let number_of_elements = content.len() as u64;
    let page_size = if unpaged { number_of_elements } else { 20 };
    let total_pages = if unpaged {
        1
    } else if number_of_elements == 0 {
        0
    } else {
        number_of_elements.div_ceil(page_size)
    };

    json!({
        "content": content,
        "pageable": {
            "pageNumber": 0,
            "pageSize": page_size,
            "sort": {
                "empty": false,
                "sorted": true,
                "unsorted": false
            },
            "offset": 0,
            "paged": !unpaged,
            "unpaged": unpaged
        },
        "last": true,
        "totalElements": number_of_elements,
        "totalPages": total_pages,
        "first": true,
        "size": page_size,
        "number": 0,
        "sort": {
            "empty": false,
            "sorted": true,
            "unsorted": false
        },
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0
    })
}

fn authentication_activity_payload(activity: &PersistedAuthenticationActivity) -> Value {
    json!({
        "userId": activity.user_id,
        "email": activity.email,
        "ip": activity.ip,
        "userAgent": activity.user_agent,
        "success": activity.success,
        "error": activity.error,
        "dateTime": sqlite_datetime_to_utc(&activity.date_time),
        "source": activity.source,
        "apiKeyId": activity.api_key_id,
        "apiKeyComment": activity.api_key_comment,
    })
}

fn sqlite_datetime_to_utc(value: &str) -> String {
    if value.ends_with('Z') || value.contains('T') {
        value.to_string()
    } else if let Some((date, time)) = value.split_once(' ') {
        format!("{date}T{time}Z")
    } else {
        value.to_string()
    }
}

fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let name = parts.next().unwrap_or_default();
        if name != key {
            return None;
        }
        Some(parts.next().unwrap_or_default())
    })
}

fn query_bool(query: &str, key: &str) -> bool {
    query_value(query, key)
        .map(|value| value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

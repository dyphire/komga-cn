use axum::Json;
use axum::extract::{Extension, Path, Request};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use bcrypt::{DEFAULT_COST, hash as hash_bcrypt_password};
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeSet;

use crate::http::access_log::RequestConnectionInfo;
use crate::http::discovery_auth::principal::principal_from_user_payload;

use crate::http::identity_access::auth::{
    AuthOutcome, AuthUser, PersistedAuthenticationActivity, bootstrap_api_key_user, bootstrap_user,
    bootstrap_user_with_remember_me_cookies, bootstrap_user_with_remember_me_token,
    empty_auth_token_supplied, expired_remember_me_cookie, expired_session_cookie,
    invalidate_session_token, invalidate_user_sessions_for_runtime_key,
    persisted_api_key_comment_exists, persisted_api_key_metadata, persisted_api_key_user,
    persisted_basic_user, persisted_create_api_key, persisted_delete_api_key_by_id,
    persisted_list_api_keys, persisted_list_authentication_activity,
    persisted_record_successful_authentication_activity, persisted_update_password_by_user_id,
    persisted_users, remember_me_max_age_seconds, remember_me_requested,
    remember_me_token_for_user_with_runtime_key, require_admin, require_auth, resolved_auth_user,
    resolved_token, session_token_for_user_with_runtime_key, session_token_from_headers,
    unauthorized_json_response, user_id, user_is_admin, user_payload_json,
};
use crate::http::operational::register_session_expired_event;
use crate::http::state::HttpAppState;
use crate::runtime_identity_access::{
    AuthUserAgeRestrictionInput, CreateAuthUserInput, SharedLibrariesInput, UpdateAuthUserInput,
};

mod activity_routes;
mod helpers;

use activity_routes::{
    users_authentication_activity, users_by_id_authentication_activity_latest,
    users_me_api_keys_create, users_me_api_keys_delete, users_me_api_keys_list,
    users_me_authentication_activity,
};
use helpers::*;

pub(super) async fn users_me(app: &HttpAppState, request: Request) -> Response {
    let auth_db = &app.auth_db;
    let auth_state = &app.discovery_auth;
    let uri = request.uri().clone();
    let headers = request.headers().clone();
    let request_metadata = authentication_activity_request_metadata(&request);

    match persisted_api_key_user(&headers, auth_db.database_file.as_path())
        .await
        .unwrap_or(AuthOutcome::Missing)
    {
        AuthOutcome::Valid(user) => {
            let api_key_metadata =
                persisted_api_key_metadata(&headers, auth_db.database_file.as_path()).await;
            let (api_key_id, api_key_comment) = api_key_metadata
                .as_ref()
                .map(|metadata| (Some(metadata.id()), Some(metadata.comment())))
                .unwrap_or((None, None));
            let _ = persisted_record_successful_authentication_activity(
                auth_db.database_file.as_path(),
                &user,
                authentication_activity_write_input(
                    &request_metadata,
                    "ApiKey",
                    api_key_id,
                    api_key_comment,
                ),
            )
            .await;
            let token = session_token_for_user_with_runtime_key(
                &user,
                auth_db.session_runtime_key.as_str(),
            );
            register_discovery_principal(
                auth_state,
                &crate::http::identity_access::auth::user_payload_json(&user),
                &token,
            );
            return bootstrap_api_key_user(*user, token);
        }
        AuthOutcome::Invalid => return unauthorized_json_response(uri.path()),
        AuthOutcome::Missing => {}
    }

    if let Some(user) = resolved_auth_user(&headers) {
        let remember_me_cookie_present = CookieJar::from_headers(&headers)
            .get("komga-remember-me")
            .is_some_and(|cookie| !cookie.value().trim().is_empty());
        if session_token_from_headers(&headers).is_none() && remember_me_cookie_present {
            let _ = persisted_record_successful_authentication_activity(
                auth_db.database_file.as_path(),
                &user,
                authentication_activity_write_input(&request_metadata, "RememberMe", None, None),
            )
            .await;
        }
        let token = session_token_from_headers(&headers).unwrap_or_else(|| {
            session_token_for_user_with_runtime_key(&user, auth_db.session_runtime_key.as_str())
        });
        let payload = crate::http::identity_access::auth::user_payload_json(&user);
        register_discovery_principal(auth_state, &payload, &token);
        if session_token_from_headers(&headers).is_some() {
            return Json(payload).into_response();
        }
        return bootstrap_user(user, token);
    }

    // Kotlin persists both success and failure authentication events. This HTTP path only aligns
    // successful-source vocabulary for now; the remaining failure-persistence gap is documented by
    // the auth-session contract suite instead of being left implicit.
    match persisted_basic_user(&headers, auth_db.database_file.as_path())
        .await
        .unwrap_or(AuthOutcome::Missing)
    {
        AuthOutcome::Valid(user) if remember_me_requested(&uri) => {
            let _ = persisted_record_successful_authentication_activity(
                auth_db.database_file.as_path(),
                &user,
                authentication_activity_write_input(&request_metadata, "Password", None, None),
            )
            .await;
            let Some(remember_me_token) = remember_me_token_for_user_with_runtime_key(
                &user,
                auth_db.remember_me_runtime_key.as_str(),
            ) else {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            };
            let remember_me_cookie_max_age_seconds =
                remember_me_max_age_seconds(auth_db.remember_me_runtime_key.as_str());
            if empty_auth_token_supplied(&headers) {
                let token = session_token_for_user_with_runtime_key(
                    &user,
                    auth_db.session_runtime_key.as_str(),
                );
                register_discovery_principal(
                    auth_state,
                    &crate::http::identity_access::auth::user_payload_json(&user),
                    &token,
                );
                bootstrap_user_with_remember_me_token(
                    *user,
                    token,
                    remember_me_token,
                    remember_me_cookie_max_age_seconds,
                )
            } else {
                let token = session_token_for_user_with_runtime_key(
                    &user,
                    auth_db.session_runtime_key.as_str(),
                );
                register_discovery_principal(
                    auth_state,
                    &crate::http::identity_access::auth::user_payload_json(&user),
                    &token,
                );
                bootstrap_user_with_remember_me_cookies(
                    *user,
                    token,
                    remember_me_token,
                    remember_me_cookie_max_age_seconds,
                )
            }
        }
        AuthOutcome::Valid(user) => {
            let _ = persisted_record_successful_authentication_activity(
                auth_db.database_file.as_path(),
                &user,
                authentication_activity_write_input(&request_metadata, "Password", None, None),
            )
            .await;
            let token = session_token_for_user_with_runtime_key(
                &user,
                auth_db.session_runtime_key.as_str(),
            );
            register_discovery_principal(
                auth_state,
                &crate::http::identity_access::auth::user_payload_json(&user),
                &token,
            );
            if empty_auth_token_supplied(&headers) {
                bootstrap_user(*user, token)
            } else {
                bootstrap_api_key_user(*user, token)
            }
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

pub(super) async fn users_list(headers: HeaderMap, app: &HttpAppState) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let users = persisted_users(app.auth_db.database_file.as_path())
        .await
        .unwrap_or_default();

    Json(users.iter().map(user_payload_json).collect::<Vec<_>>()).into_response()
}

pub(super) async fn users_create(
    app: HttpAppState,
    headers: HeaderMap,
    connection_info: RequestConnectionInfo,
    body: Value,
) -> Response {
    let auth_db = &app.auth_db;
    let Some(current_user) = authenticated_user(&headers, connection_info, &app).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !user_is_admin(&current_user) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let Some(payload) = body.as_object() else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let Some(email) = payload.get("email").and_then(Value::as_str).map(str::trim) else {
        return validation_error("email", "must be a well-formed email address");
    };
    if email.is_empty() || !looks_like_kotlin_user_email(email) {
        return validation_error("email", "must be a well-formed email address");
    }

    let Some(password) = payload
        .get("password")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return validation_error("password", "must not be blank");
    };

    let roles = match parse_roles_array(payload.get("roles")) {
        Ok(roles) => roles,
        Err(response) => return response,
    };
    let labels_allow = match parse_string_set_optional(payload.get("labelsAllow")) {
        Ok(labels) => labels,
        Err(response) => return response,
    }
    .unwrap_or_default();
    let labels_exclude = match parse_string_set_optional(payload.get("labelsExclude")) {
        Ok(labels) => labels,
        Err(response) => return response,
    }
    .unwrap_or_default();
    let age_restriction = match parse_age_restriction_optional(payload.get("ageRestriction")) {
        Ok(age_restriction) => age_restriction,
        Err(response) => return response,
    };

    let new_user_id = generated_user_id();
    let hashed_password = match hash_bcrypt_password(password, DEFAULT_COST) {
        Ok(hash) => hash,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let shared_libraries = match parse_shared_libraries_create(payload.get("sharedLibraries")) {
        Ok(shared_libraries) => shared_libraries,
        Err(response) => return response,
    };

    match app
        .services
        .runtime_identity
        .create_auth_user(
            auth_db.database_file.clone(),
            CreateAuthUserInput {
                user_id: new_user_id,
                email: email.to_string(),
                password_hash: hashed_password,
                roles,
                shared_libraries: SharedLibrariesInput {
                    all: shared_libraries.all,
                    library_ids: shared_libraries.library_ids,
                },
                labels_allow,
                labels_exclude,
                age_restriction: age_restriction.map(|value| AuthUserAgeRestrictionInput {
                    age: value.age,
                    allow_only: value.allow_only,
                }),
            },
        )
        .await
    {
        Ok(Some(user)) => (StatusCode::CREATED, Json(user_payload_json(&user))).into_response(),
        Ok(None) => spring_error(
            StatusCode::BAD_REQUEST,
            "A user with this email already exists",
            "/api/v2/users",
        ),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(super) async fn users_delete(
    app: HttpAppState,
    headers: HeaderMap,
    connection_info: RequestConnectionInfo,
    Path(target_user_id): Path<String>,
) -> Response {
    let auth_db = &app.auth_db;
    let Some(current_user) = authenticated_user(&headers, connection_info, &app).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !user_is_admin(&current_user) && user_id(&current_user) != target_user_id {
        return StatusCode::FORBIDDEN.into_response();
    }
    if user_id(&current_user) == target_user_id {
        return StatusCode::FORBIDDEN.into_response();
    }

    match app
        .services
        .runtime_identity
        .delete_auth_user(auth_db.database_file.clone(), target_user_id.clone())
        .await
    {
        Ok(true) => {
            invalidate_user_sessions_for_runtime_key(
                &target_user_id,
                auth_db.session_runtime_key.as_str(),
            );
            register_session_expired_event(&target_user_id);
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(super) async fn users_update(
    app: HttpAppState,
    headers: HeaderMap,
    connection_info: RequestConnectionInfo,
    Path(target_user_id): Path<String>,
    body: Value,
) -> Response {
    let auth_db = &app.auth_db;
    let Some(current_user) = authenticated_user(&headers, connection_info, &app).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !user_is_admin(&current_user) && user_id(&current_user) != target_user_id {
        return StatusCode::FORBIDDEN.into_response();
    }
    if user_id(&current_user) == target_user_id {
        return StatusCode::FORBIDDEN.into_response();
    }

    let Some(payload) = body.as_object() else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let roles_patch = if payload.contains_key("roles") {
        match parse_roles_array(payload.get("roles")) {
            Ok(roles) => Some(roles),
            Err(response) => return response,
        }
    } else {
        None
    };

    let shared_libraries_patch_raw = if payload.contains_key("sharedLibraries") {
        match parse_shared_libraries_patch(payload.get("sharedLibraries")) {
            Ok(shared_libraries) => Some(shared_libraries),
            Err(response) => return response,
        }
    } else {
        None
    };

    let labels_allow_patch = if payload.contains_key("labelsAllow") {
        match parse_string_set_optional(payload.get("labelsAllow")) {
            Ok(labels) => Some(labels.unwrap_or_default()),
            Err(response) => return response,
        }
    } else {
        None
    };

    let labels_exclude_patch = if payload.contains_key("labelsExclude") {
        match parse_string_set_optional(payload.get("labelsExclude")) {
            Ok(labels) => Some(labels.unwrap_or_default()),
            Err(response) => return response,
        }
    } else {
        None
    };

    let age_restriction_patch = if payload.contains_key("ageRestriction") {
        match parse_age_restriction_optional(payload.get("ageRestriction")) {
            Ok(age_restriction) => Some(age_restriction),
            Err(response) => return response,
        }
    } else {
        None
    };

    match app
        .services
        .runtime_identity
        .update_auth_user(
            auth_db.database_file.clone(),
            target_user_id.clone(),
            UpdateAuthUserInput {
                roles: roles_patch,
                shared_libraries: shared_libraries_patch_raw.map(|shared_libraries| {
                    SharedLibrariesInput {
                        all: shared_libraries.all,
                        library_ids: shared_libraries.library_ids,
                    }
                }),
                labels_allow: labels_allow_patch,
                labels_exclude: labels_exclude_patch,
                age_restriction: age_restriction_patch.map(|patch| {
                    patch.map(|value| AuthUserAgeRestrictionInput {
                        age: value.age,
                        allow_only: value.allow_only,
                    })
                }),
            },
        )
        .await
    {
        Ok(result) if !result.updated => StatusCode::NOT_FOUND.into_response(),
        Ok(result) => {
            if result.expire_sessions {
                invalidate_user_sessions_for_runtime_key(
                    &target_user_id,
                    auth_db.session_runtime_key.as_str(),
                );
                register_session_expired_event(&target_user_id);
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(super) async fn logout(headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if let Some(token) = session_token_from_headers(&headers) {
        invalidate_session_token(&token);
    }

    let mut response = StatusCode::NO_CONTENT.into_response();
    let headers = response.headers_mut();
    headers.append(header::SET_COOKIE, expired_session_cookie());
    headers.append(header::SET_COOKIE, expired_remember_me_cookie());
    response
}

pub(super) async fn users_me_password(
    headers: HeaderMap,
    connection_info: RequestConnectionInfo,
    body: Value,
    app: &HttpAppState,
) -> Response {
    let auth_db = &app.auth_db;
    let Some(current_user) = authenticated_user(&headers, connection_info, app).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let Some(password) = password_from_request(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if auth_db.demo_mode {
        return StatusCode::FORBIDDEN.into_response();
    }

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
    connection_info: RequestConnectionInfo,
    Path(target_user_id): Path<String>,
    body: Value,
    app: &HttpAppState,
) -> Response {
    let auth_db = &app.auth_db;
    let Some(current_user) = authenticated_user(&headers, connection_info, app).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !user_is_admin(&current_user) && user_id(&current_user) != target_user_id {
        return StatusCode::FORBIDDEN.into_response();
    }

    let Some(password) = password_from_request(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if auth_db.demo_mode {
        return StatusCode::FORBIDDEN.into_response();
    }

    match persisted_update_password_by_user_id(
        auth_db.database_file.as_path(),
        &target_user_id,
        password,
    )
    .await
    {
        Some(true) => {
            if user_id(&current_user) != target_user_id {
                invalidate_user_sessions_for_runtime_key(
                    &target_user_id,
                    auth_db.session_runtime_key.as_str(),
                );
                register_session_expired_event(&target_user_id);
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Some(false) => StatusCode::NOT_FOUND.into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub(crate) async fn users_me_route(
    Extension(app): Extension<HttpAppState>,
    request: Request,
) -> Response {
    users_me(&app, request).await
}

pub(crate) async fn users_list_route(
    Extension(app): Extension<HttpAppState>,
    headers: HeaderMap,
) -> Response {
    users_list(headers, &app).await
}

pub(crate) async fn users_create_route(
    Extension(app): Extension<HttpAppState>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    users_create(app, headers, connection_info, body).await
}

pub(crate) async fn users_update_route(
    Extension(app): Extension<HttpAppState>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    path: Path<String>,
    Json(body): Json<Value>,
) -> Response {
    users_update(app, headers, connection_info, path, body).await
}

pub(crate) async fn users_delete_route(
    Extension(app): Extension<HttpAppState>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    path: Path<String>,
) -> Response {
    users_delete(app, headers, connection_info, path).await
}

pub(crate) async fn users_me_password_route(
    Extension(app): Extension<HttpAppState>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    users_me_password(headers, connection_info, body, &app).await
}

pub(crate) async fn users_by_id_password_route(
    Extension(app): Extension<HttpAppState>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    path: Path<String>,
    Json(body): Json<Value>,
) -> Response {
    users_by_id_password(headers, connection_info, path, body, &app).await
}

pub(crate) async fn users_me_api_keys_create_route(
    Extension(app): Extension<HttpAppState>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    users_me_api_keys_create(headers, connection_info, body, &app).await
}

pub(crate) async fn users_me_api_keys_list_route(
    Extension(app): Extension<HttpAppState>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
) -> Response {
    users_me_api_keys_list(headers, connection_info, &app).await
}

pub(crate) async fn users_me_api_keys_delete_route(
    Extension(app): Extension<HttpAppState>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    path: Path<String>,
) -> Response {
    users_me_api_keys_delete(headers, connection_info, path, &app).await
}

pub(crate) async fn users_me_authentication_activity_route(
    Extension(app): Extension<HttpAppState>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    users_me_authentication_activity(headers, connection_info, uri, &app).await
}

pub(crate) async fn users_authentication_activity_route(
    Extension(app): Extension<HttpAppState>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    users_authentication_activity(headers, connection_info, uri, &app).await
}

pub(crate) async fn users_by_id_authentication_activity_latest_route(
    Extension(app): Extension<HttpAppState>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    path: Path<String>,
    uri: Uri,
) -> Response {
    users_by_id_authentication_activity_latest(headers, connection_info, path, uri, &app).await
}

pub(crate) async fn login_set_cookie_route(headers: HeaderMap) -> Response {
    login_set_cookie(headers).await
}

pub(crate) async fn logout_route(headers: HeaderMap) -> Response {
    logout(headers).await
}

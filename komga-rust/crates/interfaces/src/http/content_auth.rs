use axum::Json;
use axum::extract::{Extension, Path};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::{Cookie, SameSite};
use bcrypt::{DEFAULT_COST, hash as hash_bcrypt_password};
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeSet;

use super::super::AuthDatabaseState;
use crate::http::discovery_auth::{DiscoveryAuthState, principal_from_user_payload};
use crate::http::identity_access::auth::{
    AuthOutcome, AuthUser, PersistedAuthenticationActivity, auth_token_user,
    bootstrap_api_key_user, bootstrap_user, bootstrap_user_with_remember_me_cookies,
    bootstrap_user_with_remember_me_token, empty_auth_token_supplied, expired_remember_me_cookie,
    expired_session_cookie, invalidate_remember_me_token, invalidate_session_token,
    invalidate_user_sessions, persisted_api_key_comment_exists, persisted_api_key_metadata,
    persisted_api_key_user, persisted_basic_user, persisted_create_api_key,
    persisted_delete_api_key_by_id, persisted_list_api_keys,
    persisted_list_authentication_activity, persisted_record_successful_authentication_activity,
    persisted_update_password_by_user_id, persisted_users, remember_me_requested,
    remember_me_token_for_user_with_namespace, remember_me_token_from_headers, require_admin,
    require_auth, resolved_token, session_token_for_user_with_namespace,
    session_token_from_headers, unauthorized_json_response, user_id, user_is_admin,
    user_payload_json,
};
use crate::http::operational::register_session_expired_event;
use crate::http::state::OperationalState;
use crate::runtime_identity_access::{
    AuthUserAgeRestrictionInput, CreateAuthUserInput, SharedLibrariesInput, UpdateAuthUserInput,
    create_auth_user, delete_auth_user, update_auth_user,
};

#[path = "content_auth/activity_routes.rs"]
mod activity_routes;
#[path = "content_auth/helpers.rs"]
mod helpers;

pub(crate) use activity_routes::{
    users_authentication_activity, users_by_id_authentication_activity_latest,
    users_me_api_keys_create, users_me_api_keys_delete, users_me_api_keys_list,
    users_me_authentication_activity,
};

use helpers::*;

pub(super) async fn users_me(
    headers: HeaderMap,
    uri: Uri,
    auth_state: DiscoveryAuthState,
    auth_db: AuthDatabaseState,
) -> Response {
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
                "API_KEY",
                api_key_id,
                api_key_comment,
            )
            .await;
            let token = session_token_for_user_with_namespace(
                &user,
                auth_db.remember_me_namespace.as_str(),
            );
            register_discovery_principal(
                &auth_state,
                &crate::http::identity_access::auth::user_payload_json(&user),
                &token,
            );
            return bootstrap_api_key_user(*user, token);
        }
        AuthOutcome::Invalid => return unauthorized_json_response(uri.path()),
        AuthOutcome::Missing => {}
    }

    if let Some(user) = auth_token_user(&headers) {
        let token = session_token_from_headers(&headers).unwrap_or_else(|| {
            session_token_for_user_with_namespace(&user, auth_db.remember_me_namespace.as_str())
        });
        let payload = crate::http::identity_access::auth::user_payload_json(&user);
        register_discovery_principal(&auth_state, &payload, &token);
        if session_token_from_headers(&headers).is_some() {
            return Json(payload).into_response();
        }
        return bootstrap_user(user, token);
    }

    match persisted_basic_user(&headers, auth_db.database_file.as_path())
        .await
        .unwrap_or(AuthOutcome::Missing)
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
            let Some(remember_me_token) = remember_me_token_for_user_with_namespace(
                &user,
                auth_db.remember_me_namespace.as_str(),
            ) else {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            };
            if empty_auth_token_supplied(&headers) {
                let token = session_token_for_user_with_namespace(
                    &user,
                    auth_db.remember_me_namespace.as_str(),
                );
                register_discovery_principal(
                    &auth_state,
                    &crate::http::identity_access::auth::user_payload_json(&user),
                    &token,
                );
                bootstrap_user_with_remember_me_token(*user, token, remember_me_token)
            } else {
                let token = session_token_for_user_with_namespace(
                    &user,
                    auth_db.remember_me_namespace.as_str(),
                );
                register_discovery_principal(
                    &auth_state,
                    &crate::http::identity_access::auth::user_payload_json(&user),
                    &token,
                );
                bootstrap_user_with_remember_me_cookies(*user, token, remember_me_token)
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
            let token = session_token_for_user_with_namespace(
                &user,
                auth_db.remember_me_namespace.as_str(),
            );
            register_discovery_principal(
                &auth_state,
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

pub(super) async fn users_list(headers: HeaderMap, auth_db: AuthDatabaseState) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let users = persisted_users(auth_db.database_file.as_path())
        .await
        .unwrap_or_default();

    Json(users.iter().map(user_payload_json).collect::<Vec<_>>()).into_response()
}

pub(super) async fn users_create(
    headers: HeaderMap,
    body: Value,
    auth_db: AuthDatabaseState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
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

    match create_auth_user(
        auth_db.database_file.as_path(),
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
    headers: HeaderMap,
    Path(target_user_id): Path<String>,
    auth_db: AuthDatabaseState,
    state: OperationalState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !user_is_admin(&current_user) && user_id(&current_user) != target_user_id {
        return StatusCode::FORBIDDEN.into_response();
    }
    if user_id(&current_user) == target_user_id {
        return StatusCode::FORBIDDEN.into_response();
    }

    match delete_auth_user(auth_db.database_file.as_path(), &target_user_id).await {
        Ok(true) => {
            invalidate_user_sessions(&target_user_id);
            register_session_expired_event(&state, &target_user_id);
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(super) async fn users_update(
    headers: HeaderMap,
    Path(target_user_id): Path<String>,
    body: Value,
    auth_db: AuthDatabaseState,
    state: OperationalState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
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

    match update_auth_user(
        auth_db.database_file.as_path(),
        &target_user_id,
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
                invalidate_user_sessions(&target_user_id);
                register_session_expired_event(&state, &target_user_id);
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
    if let Some(remember_me_token) = remember_me_token_from_headers(&headers) {
        invalidate_remember_me_token(&remember_me_token);
    }

    let mut response = StatusCode::NO_CONTENT.into_response();
    let headers = response.headers_mut();
    headers.append(header::SET_COOKIE, expired_session_cookie());
    headers.append(header::SET_COOKIE, expired_remember_me_cookie());
    response
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
    Path(target_user_id): Path<String>,
    body: Value,
    auth_db: AuthDatabaseState,
    state: OperationalState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
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
                invalidate_user_sessions(&target_user_id);
                register_session_expired_event(&state, &target_user_id);
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Some(false) => StatusCode::NOT_FOUND.into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub(crate) async fn users_me_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    users_me(headers, uri, auth_state, auth_db).await
}

pub(crate) async fn users_list_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    users_list(headers, auth_db).await
}

pub(crate) async fn users_create_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    users_create(headers, body, auth_db).await
}

pub(crate) async fn users_update_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    path: Path<String>,
    Json(body): Json<Value>,
) -> Response {
    users_update(headers, path, body, auth_db, state).await
}

pub(crate) async fn users_delete_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    path: Path<String>,
) -> Response {
    users_delete(headers, path, auth_db, state).await
}

pub(crate) async fn users_me_password_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    users_me_password(headers, body, auth_db).await
}

pub(crate) async fn users_by_id_password_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    path: Path<String>,
    Json(body): Json<Value>,
) -> Response {
    users_by_id_password(headers, path, body, auth_db, state).await
}

pub(crate) async fn users_me_api_keys_create_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    users_me_api_keys_create(headers, body, auth_db).await
}

pub(crate) async fn users_me_api_keys_list_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    users_me_api_keys_list(headers, auth_db).await
}

pub(crate) async fn users_me_api_keys_delete_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    path: Path<String>,
) -> Response {
    users_me_api_keys_delete(headers, path, auth_db).await
}

pub(crate) async fn users_me_authentication_activity_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    users_me_authentication_activity(headers, uri, auth_db).await
}

pub(crate) async fn users_authentication_activity_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    users_authentication_activity(headers, uri, auth_db).await
}

pub(crate) async fn users_by_id_authentication_activity_latest_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    path: Path<String>,
    uri: Uri,
) -> Response {
    users_by_id_authentication_activity_latest(headers, path, uri, auth_db).await
}

pub(crate) async fn login_set_cookie_route(headers: HeaderMap) -> Response {
    login_set_cookie(headers).await
}

pub(crate) async fn logout_route(headers: HeaderMap) -> Response {
    logout(headers).await
}

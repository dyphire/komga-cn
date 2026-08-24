use axum::Json;
use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_application::identity_access::{user_id, user_is_admin};
use serde_json::Value;

use super::helpers::{
    api_key_comment_from_request, authentication_activity_page_payload, query_value,
    required_authenticated_user,
};
use crate::access_log::RequestConnectionInfo;
use crate::contracts::common::MessageDto;
use crate::contracts::identity_access::{ApiKeyDto, AuthenticationActivityDto};
use crate::identity_access::auth::{
    persisted_api_key_comment_exists, persisted_create_api_key, persisted_delete_api_key_by_id,
    persisted_list_api_keys, persisted_list_authentication_activity, persisted_users,
};
use crate::state::IdentityAccessState;

pub(crate) async fn users_me_api_keys_create(
    headers: HeaderMap,
    connection_info: RequestConnectionInfo,
    body: Value,
    app: &IdentityAccessState,
) -> Response {
    let auth_db = &app.auth_db;
    let identity = &app.identity;
    let current_user = match required_authenticated_user(&headers, connection_info, app).await {
        Ok(user) => user,
        Err(status) => return status.into_response(),
    };
    let Some(comment) = api_key_comment_from_request(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if auth_db.demo_mode && !user_is_admin(&current_user) {
        return StatusCode::FORBIDDEN.into_response();
    }

    match persisted_api_key_comment_exists(identity, user_id(&current_user), &comment).await {
        Ok(true) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(MessageDto {
                    message: "api key comment already exists for this user".to_string(),
                }),
            )
                .into_response();
        }
        Ok(false) => {}
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }

    match persisted_create_api_key(identity, user_id(&current_user), comment.as_str()).await {
        Ok(api_key) => match ApiKeyDto::from_persisted(&api_key, false) {
            Ok(payload) => (StatusCode::OK, Json(payload)).into_response(),
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        },
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn users_me_api_keys_list(
    headers: HeaderMap,
    connection_info: RequestConnectionInfo,
    app: &IdentityAccessState,
) -> Response {
    let auth_db = &app.auth_db;
    let identity = &app.identity;
    let current_user = match required_authenticated_user(&headers, connection_info, app).await {
        Ok(user) => user,
        Err(status) => return status.into_response(),
    };
    if auth_db.demo_mode && !user_is_admin(&current_user) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let api_keys = match persisted_list_api_keys(identity, user_id(&current_user)).await {
        Ok(api_keys) => api_keys,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let payload = match api_keys
        .iter()
        .map(|api_key| ApiKeyDto::from_persisted(api_key, true))
        .collect::<anyhow::Result<Vec<_>>>()
    {
        Ok(payload) => payload,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(payload).into_response()
}

pub(crate) async fn users_me_api_keys_delete(
    headers: HeaderMap,
    connection_info: RequestConnectionInfo,
    Path(api_key_id): Path<String>,
    app: &IdentityAccessState,
) -> Response {
    let identity = &app.identity;
    let current_user = match required_authenticated_user(&headers, connection_info, app).await {
        Ok(user) => user,
        Err(status) => return status.into_response(),
    };

    match persisted_delete_api_key_by_id(identity, user_id(&current_user), &api_key_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn users_me_authentication_activity(
    headers: HeaderMap,
    connection_info: RequestConnectionInfo,
    uri: Uri,
    app: &IdentityAccessState,
) -> Response {
    let auth_db = &app.auth_db;
    let identity = &app.identity;
    let current_user = match required_authenticated_user(&headers, connection_info, app).await {
        Ok(user) => user,
        Err(status) => return status.into_response(),
    };
    if auth_db.demo_mode && !user_is_admin(&current_user) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let query = uri.query().unwrap_or_default();
    let mut rows = match persisted_list_authentication_activity(identity, None).await {
        Ok(rows) => rows,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    rows.retain(|activity| {
        activity.user_id.as_deref() == Some(user_id(&current_user))
            || activity.email.as_deref() == Some(current_user.email.as_str())
    });

    match authentication_activity_page_payload(rows, query) {
        Ok(payload) => Json(payload).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn users_authentication_activity(
    headers: HeaderMap,
    connection_info: RequestConnectionInfo,
    uri: Uri,
    app: &IdentityAccessState,
) -> Response {
    let identity = &app.identity;
    let current_user = match required_authenticated_user(&headers, connection_info, app).await {
        Ok(user) => user,
        Err(status) => return status.into_response(),
    };
    if !user_is_admin(&current_user) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let query = uri.query().unwrap_or_default();
    let rows = match persisted_list_authentication_activity(identity, None).await {
        Ok(rows) => rows,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    match authentication_activity_page_payload(rows, query) {
        Ok(payload) => Json(payload).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn users_by_id_authentication_activity_latest(
    headers: HeaderMap,
    connection_info: RequestConnectionInfo,
    Path(target_user_id): Path<String>,
    uri: Uri,
    app: &IdentityAccessState,
) -> Response {
    let identity = &app.identity;
    let current_user = match required_authenticated_user(&headers, connection_info, app).await {
        Ok(user) => user,
        Err(status) => return status.into_response(),
    };
    if !user_is_admin(&current_user) && user_id(&current_user) != target_user_id {
        return StatusCode::FORBIDDEN.into_response();
    }

    let users = match persisted_users(identity).await {
        Ok(users) => users,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let Some(target_user) = users
        .into_iter()
        .find(|user| user_id(user) == target_user_id)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let api_key_id = query_value(uri.query().unwrap_or_default(), "apikey_id");

    let rows = match persisted_list_authentication_activity(identity, None).await {
        Ok(rows) => rows,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let activity = rows.into_iter().find(|activity| {
        let user_matches = activity.user_id.as_deref() == Some(target_user_id.as_str())
            || activity.email.as_deref() == Some(target_user.email.as_str());
        let api_key_matches = match api_key_id {
            Some(api_key_id) => activity.api_key_id.as_deref() == Some(api_key_id),
            None => true,
        };
        user_matches && api_key_matches
    });

    let Some(activity) = activity else {
        return StatusCode::NOT_FOUND.into_response();
    };

    match AuthenticationActivityDto::from_persisted(&activity) {
        Ok(payload) => Json(payload).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

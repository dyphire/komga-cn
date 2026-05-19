use super::*;

const REDACTED_API_KEY_VALUE: &str = "******";

pub(crate) async fn users_me_api_keys_create(
    headers: HeaderMap,
    connection_info: RequestConnectionInfo,
    body: Value,
    app: &IdentityAccessState,
) -> Response {
    let auth_db = &app.auth_db;
    let identity = &app.identity;
    let Some(current_user) = authenticated_user(&headers, connection_info, app).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(comment) = api_key_comment_from_request(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if auth_db.demo_mode && !user_is_admin(&current_user) {
        return StatusCode::FORBIDDEN.into_response();
    }

    match persisted_api_key_comment_exists(identity, user_id(&current_user), &comment).await {
        Some(true) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "message": "api key comment already exists for this user" })),
            )
                .into_response();
        }
        Some(false) => {}
        None => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }

    match persisted_create_api_key(identity, user_id(&current_user), comment.as_str()).await {
        Some(api_key) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "id": api_key.id(),
                "userId": api_key.user_id(),
                "key": api_key.key(),
                "comment": api_key.comment(),
                "createdDate": api_key.created_date().map(sqlite_datetime_to_utc),
                "lastModifiedDate": api_key.last_modified_date().map(sqlite_datetime_to_utc),
            })),
        )
            .into_response(),
        None => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

pub(crate) async fn users_me_api_keys_list(
    headers: HeaderMap,
    connection_info: RequestConnectionInfo,
    app: &IdentityAccessState,
) -> Response {
    let auth_db = &app.auth_db;
    let identity = &app.identity;
    let Some(current_user) = authenticated_user(&headers, connection_info, app).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if auth_db.demo_mode && !user_is_admin(&current_user) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let api_keys = persisted_list_api_keys(identity, user_id(&current_user))
        .await
        .unwrap_or_default();

    Json(
        api_keys
            .iter()
            .map(|api_key| {
                serde_json::json!({
                    "id": api_key.id(),
                    "userId": api_key.user_id(),
                    "key": REDACTED_API_KEY_VALUE,
                    "comment": api_key.comment(),
                    "createdDate": api_key.created_date().map(sqlite_datetime_to_utc),
                    "lastModifiedDate": api_key.created_date().map(sqlite_datetime_to_utc),
                })
            })
            .collect::<Vec<_>>(),
    )
    .into_response()
}

pub(crate) async fn users_me_api_keys_delete(
    headers: HeaderMap,
    connection_info: RequestConnectionInfo,
    Path(api_key_id): Path<String>,
    app: &IdentityAccessState,
) -> Response {
    let identity = &app.identity;
    let Some(current_user) = authenticated_user(&headers, connection_info, app).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    match persisted_delete_api_key_by_id(identity, user_id(&current_user), &api_key_id).await {
        Some(true) => StatusCode::NO_CONTENT.into_response(),
        Some(false) => StatusCode::NOT_FOUND.into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
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
    let Some(current_user) = authenticated_user(&headers, connection_info, app).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if auth_db.demo_mode && !user_is_admin(&current_user) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let query = uri.query().unwrap_or_default();
    let mut rows = persisted_list_authentication_activity(identity, None)
        .await
        .unwrap_or_default();
    rows.retain(|activity| {
        activity.user_id.as_deref() == Some(user_id(&current_user))
            || activity.email.as_deref() == Some(current_user.email.as_str())
    });

    Json(authentication_activity_page_payload(rows, query)).into_response()
}

pub(crate) async fn users_authentication_activity(
    headers: HeaderMap,
    connection_info: RequestConnectionInfo,
    uri: Uri,
    app: &IdentityAccessState,
) -> Response {
    let identity = &app.identity;
    let Some(current_user) = authenticated_user(&headers, connection_info, app).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !user_is_admin(&current_user) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let query = uri.query().unwrap_or_default();
    let rows = persisted_list_authentication_activity(identity, None)
        .await
        .unwrap_or_default();

    Json(authentication_activity_page_payload(rows, query)).into_response()
}

pub(crate) async fn users_by_id_authentication_activity_latest(
    headers: HeaderMap,
    connection_info: RequestConnectionInfo,
    Path(target_user_id): Path<String>,
    uri: Uri,
    app: &IdentityAccessState,
) -> Response {
    let identity = &app.identity;
    let Some(current_user) = authenticated_user(&headers, connection_info, app).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !user_is_admin(&current_user) && user_id(&current_user) != target_user_id {
        return StatusCode::FORBIDDEN.into_response();
    }

    let Some(target_user) = persisted_users(identity).await.and_then(|users| {
        users
            .into_iter()
            .find(|user| user_id(user) == target_user_id)
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let api_key_id = query_value(uri.query().unwrap_or_default(), "apikey_id");

    let activity = persisted_list_authentication_activity(identity, None)
        .await
        .and_then(|rows| {
            rows.into_iter().find(|activity| {
                let user_matches = activity.user_id.as_deref() == Some(target_user_id.as_str())
                    || activity.email.as_deref() == Some(target_user.email.as_str());
                let api_key_matches = match api_key_id {
                    Some(api_key_id) => activity.api_key_id.as_deref() == Some(api_key_id),
                    None => true,
                };
                user_matches && api_key_matches
            })
        });

    let Some(activity) = activity else {
        return StatusCode::NOT_FOUND.into_response();
    };

    Json(authentication_activity_payload(&activity)).into_response()
}

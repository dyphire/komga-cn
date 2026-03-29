use super::*;

pub(crate) async fn users_me_api_keys_create(
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

    match persisted_api_key_comment_exists(&auth_db.database_file, user_id(&current_user), &comment)
        .await
    {
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

    match persisted_create_api_key(
        &auth_db.database_file,
        user_id(&current_user),
        comment.as_str(),
    )
    .await
    {
        Some(api_key) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "id": api_key.id(),
                "userId": api_key.user_id(),
                "key": api_key.key(),
                "comment": api_key.comment(),
            })),
        )
            .into_response(),
        None => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

pub(crate) async fn users_me_api_keys_list(
    headers: HeaderMap,
    auth_db: AuthDatabaseState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let api_keys = persisted_list_api_keys(&auth_db.database_file, user_id(&current_user))
        .await
        .unwrap_or_default();

    Json(
        api_keys
            .iter()
            .map(|api_key| {
                serde_json::json!({
                    "id": api_key.id(),
                    "userId": api_key.user_id(),
                    "key": api_key.key(),
                    "comment": api_key.comment(),
                })
            })
            .collect::<Vec<_>>(),
    )
    .into_response()
}

pub(crate) async fn users_me_api_keys_delete(
    headers: HeaderMap,
    Path(api_key_id): Path<String>,
    auth_db: AuthDatabaseState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    match persisted_delete_api_key_by_id(
        &auth_db.database_file,
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

pub(crate) async fn users_me_authentication_activity(
    headers: HeaderMap,
    uri: Uri,
    auth_db: AuthDatabaseState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let unpaged = query_bool(uri.query().unwrap_or_default(), "unpaged");
    let rows = persisted_list_authentication_activity(
        &auth_db.database_file,
        Some(user_id(&current_user)),
    )
    .await
    .unwrap_or_default();

    Json(authentication_activity_page_payload(rows, unpaged)).into_response()
}

pub(crate) async fn users_authentication_activity(
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
    let rows = persisted_list_authentication_activity(&auth_db.database_file, None)
        .await
        .unwrap_or_default();

    Json(authentication_activity_page_payload(rows, unpaged)).into_response()
}

pub(crate) async fn users_by_id_authentication_activity_latest(
    headers: HeaderMap,
    Path(target_user_id): Path<String>,
    uri: Uri,
    auth_db: AuthDatabaseState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !user_is_admin(&current_user) && user_id(&current_user) != target_user_id {
        return StatusCode::FORBIDDEN.into_response();
    }

    let api_key_id = query_value(uri.query().unwrap_or_default(), "apikey_id")
        .and_then(|value| (!value.is_empty()).then_some(value));

    let activity = if let Some(api_key_id) = api_key_id {
        persisted_latest_authentication_activity_by_user_and_api_key(
            &auth_db.database_file,
            &target_user_id,
            api_key_id,
        )
        .await
    } else {
        persisted_list_authentication_activity(&auth_db.database_file, Some(&target_user_id))
            .await
            .and_then(|rows| rows.into_iter().next())
    };

    let Some(activity) = activity else {
        return StatusCode::NOT_FOUND.into_response();
    };

    Json(authentication_activity_payload(&activity)).into_response()
}

use super::*;
use crate::identity_access::auth::Authenticated;
use crate::state::MediaAssetsState;
use axum::extract::State;

pub async fn series_read_progress_post(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path(series_id): Path<String>,
) -> Response {
    let resolved_series_id = resolve_series_id_for_persisted(&app, &series_id).await;
    match user_can_access_series_media(&app, &resolved_series_id, &user).await {
        Ok(true) => {}
        Ok(false) => return StatusCode::FORBIDDEN.into_response(),
        Err(error) => return internal_error_response(error),
    }

    if let Err(error) = app
        .read_progress_service
        .mark_series_complete(&resolved_series_id, user_id(&user))
        .await
    {
        return internal_error_response(error);
    }

    StatusCode::NO_CONTENT.into_response()
}

pub async fn series_read_progress_delete(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path(series_id): Path<String>,
) -> Response {
    let resolved_series_id = resolve_series_id_for_persisted(&app, &series_id).await;
    let unrestricted_all_libraries = user_shared_all_libraries(&user)
        && principal_from_user_payload(&user_payload_json(&user))
            .is_none_or(|principal| !principal.restrictions.is_restricted());
    if unrestricted_all_libraries {
        if !app
            .reader
            .series_exists(&resolved_series_id)
            .await
            .unwrap_or(false)
        {
            return StatusCode::NO_CONTENT.into_response();
        }
    } else {
        if !app
            .reader
            .series_exists(&resolved_series_id)
            .await
            .unwrap_or(false)
        {
            return StatusCode::NOT_FOUND.into_response();
        }
        match user_can_access_series_media(&app, &resolved_series_id, &user).await {
            Ok(true) => {}
            Ok(false) => return StatusCode::FORBIDDEN.into_response(),
            Err(error) => return internal_error_response(error),
        }
    }

    if let Err(error) = app
        .read_progress_service
        .delete_series_progress(&resolved_series_id, user_id(&user))
        .await
    {
        return internal_error_response(error);
    }

    StatusCode::NO_CONTENT.into_response()
}

pub async fn series_tachiyomi_read_progress_get(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path(series_id): Path<String>,
) -> Response {
    let resolved_series_id = resolve_series_id_for_persisted(&app, &series_id).await;
    let unrestricted_all_libraries = user_shared_all_libraries(&user)
        && principal_from_user_payload(&user_payload_json(&user))
            .is_none_or(|principal| !principal.restrictions.is_restricted());
    if !unrestricted_all_libraries {
        if !app
            .reader
            .series_exists(&resolved_series_id)
            .await
            .unwrap_or(false)
        {
            return StatusCode::NOT_FOUND.into_response();
        }
        match user_can_access_series_media(&app, &resolved_series_id, &user).await {
            Ok(true) => {}
            Ok(false) => return StatusCode::FORBIDDEN.into_response(),
            Err(error) => return internal_error_response(error),
        }
    }

    match app
        .reader
        .series_tachiyomi_progress(&resolved_series_id, user_id(&user))
        .await
    {
        Ok(Some(payload)) => Json(payload).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn series_tachiyomi_read_progress_put(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path(series_id): Path<String>,
    body: Bytes,
) -> Response {
    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid tachiyomi series read progress payload" })),
        )
            .into_response();
    };
    let Some(last_number_sort_read) = payload
        .get("lastBookNumberSortRead")
        .and_then(Value::as_f64)
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "lastBookNumberSortRead must be a number" })),
        )
            .into_response();
    };

    let resolved_series_id = resolve_series_id_for_persisted(&app, &series_id).await;
    let unrestricted_all_libraries = user_shared_all_libraries(&user)
        && principal_from_user_payload(&user_payload_json(&user))
            .is_none_or(|principal| !principal.restrictions.is_restricted());
    if unrestricted_all_libraries {
        if !app
            .reader
            .series_exists(&resolved_series_id)
            .await
            .unwrap_or(false)
        {
            return StatusCode::NO_CONTENT.into_response();
        }
    } else {
        if !app
            .reader
            .series_exists(&resolved_series_id)
            .await
            .unwrap_or(false)
        {
            return StatusCode::NOT_FOUND.into_response();
        }
        match user_can_access_series_media(&app, &resolved_series_id, &user).await {
            Ok(true) => {}
            Ok(false) => return StatusCode::FORBIDDEN.into_response(),
            Err(error) => return internal_error_response(error),
        }
    }

    if let Err(error) = app
        .read_progress_service
        .mark_series_tachiyomi_progress(&resolved_series_id, user_id(&user), last_number_sort_read)
        .await
    {
        return internal_error_response(error);
    }

    StatusCode::NO_CONTENT.into_response()
}

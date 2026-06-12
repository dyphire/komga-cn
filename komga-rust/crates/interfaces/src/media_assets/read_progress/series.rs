use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use komga_application::discovery::resolve_persisted_series_id;
use komga_application::identity_access::user_id;
use serde_json::{Value, json};

use crate::identity_access::auth::Authenticated;
use crate::media_assets::access_control::{
    user_can_access_series_media, user_has_unrestricted_all_libraries,
};
use crate::media_assets::http_helpers::internal_error_response;
use crate::state::MediaAssetsState;

fn series_tachiyomi_progress_payload(
    progress: komga_application::media_assets::SeriesTachiyomiProgress,
) -> Value {
    json!({
        "booksCount": progress.books_count,
        "booksReadCount": progress.books_read_count,
        "booksUnreadCount": progress.books_unread_count,
        "booksInProgressCount": progress.books_in_progress_count,
        "lastReadContinuousNumberSort": progress.last_read_continuous_number_sort,
        "maxNumberSort": progress.max_number_sort,
    })
}

async fn series_exists(app: &MediaAssetsState, series_id: &str) -> Result<bool, Response> {
    app.read_progress_reader
        .series_exists(series_id)
        .await
        .map_err(internal_error_response)
}

pub(crate) async fn series_read_progress_post(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path(series_id): Path<String>,
) -> Response {
    let resolved_series_id =
        resolve_persisted_series_id(app.series_id_resolver.as_ref(), &series_id).await;
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

pub(crate) async fn series_read_progress_delete(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path(series_id): Path<String>,
) -> Response {
    let resolved_series_id =
        resolve_persisted_series_id(app.series_id_resolver.as_ref(), &series_id).await;
    let unrestricted_all_libraries = user_has_unrestricted_all_libraries(&user);
    if unrestricted_all_libraries {
        match series_exists(&app, &resolved_series_id).await {
            Ok(true) => {}
            Ok(false) => return StatusCode::NO_CONTENT.into_response(),
            Err(response) => return response,
        }
    } else {
        match series_exists(&app, &resolved_series_id).await {
            Ok(true) => {}
            Ok(false) => return StatusCode::NOT_FOUND.into_response(),
            Err(response) => return response,
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

pub(crate) async fn series_tachiyomi_read_progress_get(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path(series_id): Path<String>,
) -> Response {
    let resolved_series_id =
        resolve_persisted_series_id(app.series_id_resolver.as_ref(), &series_id).await;
    let unrestricted_all_libraries = user_has_unrestricted_all_libraries(&user);
    if !unrestricted_all_libraries {
        match series_exists(&app, &resolved_series_id).await {
            Ok(true) => {}
            Ok(false) => return StatusCode::NOT_FOUND.into_response(),
            Err(response) => return response,
        }
        match user_can_access_series_media(&app, &resolved_series_id, &user).await {
            Ok(true) => {}
            Ok(false) => return StatusCode::FORBIDDEN.into_response(),
            Err(error) => return internal_error_response(error),
        }
    }

    match app
        .read_progress_service
        .series_tachiyomi_progress(&resolved_series_id, user_id(&user))
        .await
    {
        Ok(progress) => Json(series_tachiyomi_progress_payload(progress)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(crate) async fn series_tachiyomi_read_progress_put(
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

    let resolved_series_id =
        resolve_persisted_series_id(app.series_id_resolver.as_ref(), &series_id).await;
    let unrestricted_all_libraries = user_has_unrestricted_all_libraries(&user);
    if unrestricted_all_libraries {
        match series_exists(&app, &resolved_series_id).await {
            Ok(true) => {}
            Ok(false) => return StatusCode::NO_CONTENT.into_response(),
            Err(response) => return response,
        }
    } else {
        match series_exists(&app, &resolved_series_id).await {
            Ok(true) => {}
            Ok(false) => return StatusCode::NOT_FOUND.into_response(),
            Err(response) => return response,
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

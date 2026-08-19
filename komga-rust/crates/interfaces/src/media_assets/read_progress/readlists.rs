use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use komga_application::identity_access::{AuthUser, user_id};
use serde_json::Value;

use crate::contracts::media_assets::TachiyomiReadListProgressDto;
use crate::helpers::spring_error_response;
use crate::identity_access::auth::Authenticated;
use crate::media_assets::access_control::visible_readlist_book_ids_for_user;
use crate::media_assets::http_helpers::internal_error_response;
use crate::state::MediaAssetsState;

async fn load_tachiyomi_readlist_book_ids(
    app: &MediaAssetsState,
    readlist_id: &str,
    user: &AuthUser,
) -> anyhow::Result<Option<Vec<String>>> {
    visible_readlist_book_ids_for_user(app, readlist_id, user).await
}

pub(crate) async fn readlist_tachiyomi_read_progress_get(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path(readlist_id): Path<String>,
) -> Response {
    let Some(ordered_book_ids) =
        (match load_tachiyomi_readlist_book_ids(&app, &readlist_id, &user).await {
            Ok(ordered_book_ids) => ordered_book_ids,
            Err(error) => return internal_error_response(error),
        })
    else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let counters = match app
        .read_progress_service
        .readlist_tachiyomi_counters(&ordered_book_ids, user_id(&user))
        .await
    {
        Ok(counters) => counters,
        Err(error) => return internal_error_response(error),
    };

    Json(TachiyomiReadListProgressDto::from(counters)).into_response()
}

pub(crate) async fn readlist_tachiyomi_read_progress_put(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path(readlist_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let Some(last_book_read) = body
        .get("lastBookRead")
        .and_then(Value::as_i64)
        .filter(|value| *value >= 0)
    else {
        return spring_error_response(
            StatusCode::BAD_REQUEST,
            "lastBookRead must be a non-negative integer",
        );
    };

    let Some(ordered_book_ids) =
        (match load_tachiyomi_readlist_book_ids(&app, &readlist_id, &user).await {
            Ok(ordered_book_ids) => ordered_book_ids,
            Err(error) => return internal_error_response(error),
        })
    else {
        return StatusCode::NOT_FOUND.into_response();
    };

    if ordered_book_ids.is_empty() {
        return StatusCode::NO_CONTENT.into_response();
    }

    match app
        .read_progress_service
        .mark_readlist_tachiyomi_progress(
            &ordered_book_ids,
            user_id(&user),
            last_book_read as usize,
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => internal_error_response(error),
    }
}

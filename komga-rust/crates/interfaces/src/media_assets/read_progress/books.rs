use super::epub::{
    load_epub_locator_for_page, locator_position, locator_progression, progression_bad_request,
    progression_is_older_than_existing, progression_locator,
};
use super::*;
use crate::identity_access::auth::Authenticated;
use crate::opds::OpdsV2Authenticated;
use crate::state::MediaAssetsState;
use axum::extract::State;

fn request_progress_token(
    identity: &crate::state::IdentityState,
    headers: &HeaderMap,
    user: &AuthUser,
) -> String {
    if resolved_auth_user(identity, headers).is_some() {
        let token = resolved_token(headers);
        if !token.trim().is_empty() {
            return token;
        }
    }

    format!("user:{}", user_id(user))
}

async fn load_accessible_book_media(
    app: &MediaAssetsState,
    book_id: &str,
    user: &AuthUser,
) -> Result<PersistedBookMedia, Response> {
    let Some(media) = (match app.reader.book_media(book_id).await {
        Ok(media) => media,
        Err(error) => return Err(internal_error_response(error)),
    }) else {
        return Err(StatusCode::NOT_FOUND.into_response());
    };

    if !user_can_access_book_media(app.reader.as_ref(), book_id, user, &media).await {
        return Err(StatusCode::FORBIDDEN.into_response());
    }

    Ok(media)
}

#[allow(clippy::too_many_arguments)]
async fn persist_and_record_read_progress(
    app: &MediaAssetsState,
    token: &str,
    book_id: &str,
    persisted_user_id: Option<&str>,
    page: u64,
    completed: bool,
    locator: Option<Value>,
) -> Response {
    if let Some(user_id) = persisted_user_id
        && app
            .progress
            .persist_read_progress(book_id, user_id, page, completed, locator)
            .await
            .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    set_read_progress(&app.read_progress, token.to_string(), book_id.to_string());
    StatusCode::NO_CONTENT.into_response()
}

pub async fn book_read_progress(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    body: Bytes,
) -> Response {
    let supports_persisted_flow = app.reader.book_exists(&book_id).await.unwrap_or(false);

    if !supports_persisted_flow {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return invalid_read_progress_payload();
    };

    if let Err(response) = load_accessible_book_media(&app, &book_id, &user).await {
        return response;
    }
    let persisted_user_id = Some(user_id(&user));
    let page_count = match app.reader.book_page_count(&book_id).await {
        Ok(Some(value)) if value > 0 => value,
        Ok(_) => 1,
        Err(error) => return internal_error_response(error),
    };

    let token = request_progress_token(&app.identity, &headers, &user);

    let page_value = payload.get("page");
    let completed_true = payload.get("completed").and_then(|value| value.as_bool()) == Some(true);

    if matches!(page_value.and_then(Value::as_i64), Some(value) if value <= 0) {
        return read_progress_validation_error_response(vec![json!({
            "fieldName": "page",
            "message": "must be greater than 0"
        })]);
    }

    if completed_true {
        return persist_and_record_read_progress(
            &app,
            &token,
            &book_id,
            persisted_user_id,
            page_count,
            true,
            None,
        )
        .await;
    }

    if page_value.is_none_or(Value::is_null) {
        return read_progress_validation_error_response(vec![]);
    }

    let Some(page) = payload.get("page").and_then(Value::as_u64) else {
        return invalid_read_progress_payload();
    };

    if page > page_count {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": format!(
                    "Page argument ({page}) must be within 1 and book page count ({page_count})"
                )
            })),
        )
            .into_response();
    }

    if !(1..=page_count).contains(&page) {
        return invalid_read_progress_payload();
    }

    let locator = match load_epub_locator_for_page(&app, &book_id, page).await {
        Ok(locator) => locator,
        Err(error) => return internal_error_response(error),
    };

    persist_and_record_read_progress(
        &app,
        &token,
        &book_id,
        persisted_user_id,
        page,
        page == page_count,
        locator,
    )
    .await
}

pub async fn book_read_progress_delete(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    let supports_persisted_flow = app.reader.book_exists(&book_id).await.unwrap_or(false);

    if !supports_persisted_flow {
        return StatusCode::NOT_FOUND.into_response();
    }

    if let Err(response) = load_accessible_book_media(&app, &book_id, &user).await {
        return response;
    }
    let token = request_progress_token(&app.identity, &headers, &user);
    {
        let mut all_progress = app
            .read_progress
            .progress_by_token
            .lock()
            .expect("read-progress state lock should not be poisoned");

        if let Some(user_progress) = all_progress.get_mut(&token) {
            user_progress.remove(&book_id);
        }
    }

    if supports_persisted_flow
        && app
            .progress
            .delete_read_progress(&book_id, user_id(&user))
            .await
            .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

pub async fn book_progression(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path(book_id): Path<String>,
    body: Bytes,
) -> Response {
    book_progression_response(&app, &user, &book_id, body).await
}

pub async fn opds_v2_book_progression(
    State(app): State<MediaAssetsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    Path(book_id): Path<String>,
    body: Bytes,
) -> Response {
    book_progression_response(&app, &user, &book_id, body).await
}

async fn book_progression_response(
    app: &MediaAssetsState,
    user: &AuthUser,
    book_id: &str,
    body: Bytes,
) -> Response {
    if !app.reader.book_exists(book_id).await.unwrap_or(false) {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Some(media) = (match app.reader.book_media(book_id).await {
        Ok(media) => media,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !user_can_access_book_media(app.reader.as_ref(), book_id, user, &media).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return invalid_progression_payload();
    };

    let Some(modified) = payload.get("modified").and_then(Value::as_str) else {
        return invalid_progression_payload();
    };
    let Some(device_id) = payload
        .get("device")
        .and_then(|value| value.get("id"))
        .and_then(Value::as_str)
    else {
        return invalid_progression_payload();
    };
    let Some(device_name) = payload
        .get("device")
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
    else {
        return invalid_progression_payload();
    };

    let is_epub = book_media_is_epub(&media);
    let page_count = media.page_count.max(1);
    let locator = progression_locator(&payload);
    let position = locator.and_then(locator_position);
    let (progression, locator_to_persist) = if is_epub {
        let Some(locator) = locator else {
            return invalid_progression_payload();
        };
        let normalized_locator =
            match normalize_book_epub_locator(app.reader.as_ref(), book_id, locator).await {
                Ok(locator) => locator,
                Err(response) => return response,
            };
        let Some(progression) = locator_progression(&normalized_locator) else {
            return invalid_progression_payload();
        };
        (progression, Some(normalized_locator))
    } else {
        let Some(position) = position else {
            return invalid_progression_payload();
        };
        if !(1..=page_count).contains(&position) {
            return progression_bad_request(format!(
                "Page argument ({position}) must be within 1 and book page count ({page_count})"
            ));
        }
        (position as f64 / page_count as f64, locator.cloned())
    };

    if is_epub && !(0.0..=1.0).contains(&progression) {
        return invalid_progression_payload();
    }

    let stale_progression = match progression_is_older_than_existing(
        app.reader.as_ref(),
        book_id,
        user_id(user),
        modified,
    )
    .await
    {
        Ok(stale) => stale,
        Err(error) => return internal_error_response(error),
    };
    if stale_progression {
        return (
            StatusCode::CONFLICT,
            Json(json!({ "error": "Progression is older than existing" })),
        )
            .into_response();
    }

    match app
        .progress
        .persist_book_progression(
            book_id,
            user_id(user),
            progression,
            !is_epub,
            Some(modified.to_owned()),
            Some(device_id.to_owned()),
            Some(device_name.to_owned()),
            locator_to_persist,
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn book_progression_get(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path(book_id): Path<String>,
) -> Response {
    book_progression_get_response(&app, &user, &book_id).await
}

pub async fn opds_v2_book_progression_get(
    State(app): State<MediaAssetsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    Path(book_id): Path<String>,
) -> Response {
    book_progression_get_response(&app, &user, &book_id).await
}

async fn book_progression_get_response(
    app: &MediaAssetsState,
    user: &AuthUser,
    book_id: &str,
) -> Response {
    if !app.reader.book_exists(book_id).await.unwrap_or(false) {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Some(media) = (match app.reader.book_media(book_id).await {
        Ok(media) => media,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !user_can_access_book_media(app.reader.as_ref(), book_id, user, &media).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    match app.reader.book_progression(book_id, user_id(user)).await {
        Ok(Some(progression)) => (
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static(READIUM_PROGRESSION_MEDIA_TYPE),
            )],
            Json(progression),
        )
            .into_response(),
        Ok(None) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => internal_error_response(error),
    }
}

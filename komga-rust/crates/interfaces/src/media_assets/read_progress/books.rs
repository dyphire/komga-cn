use super::epub::load_epub_locator_for_page;
use super::*;
use crate::identity_access::auth::Authenticated;
use crate::opds::OpdsV2Authenticated;
use crate::state::MediaAssetsState;
use axum::extract::State;
use komga_application::media_assets::{
    BookProgressionGetOutcome, BookProgressionOutcome, BookProgressionService,
    BookProgressionUpdate,
};

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
    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return invalid_progression_payload();
    };
    let Some(update) = BookProgressionUpdate::from_payload(&payload) else {
        return invalid_progression_payload();
    };

    let service = BookProgressionService::new(
        app.reader.as_ref(),
        app.content.as_ref(),
        app.progress.as_ref(),
    );
    match service.update_progression(user, book_id, update).await {
        BookProgressionOutcome::Updated => StatusCode::NO_CONTENT.into_response(),
        BookProgressionOutcome::NotFound => StatusCode::NOT_FOUND.into_response(),
        BookProgressionOutcome::Forbidden => StatusCode::FORBIDDEN.into_response(),
        BookProgressionOutcome::InvalidPayload => invalid_progression_payload(),
        BookProgressionOutcome::BadRequest(error) => progression_bad_request_response(error),
        BookProgressionOutcome::Conflict => (
            StatusCode::CONFLICT,
            Json(json!({ "error": "Progression is older than existing" })),
        )
            .into_response(),
        BookProgressionOutcome::Internal(error) => internal_error_response(error),
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
    let service = BookProgressionService::new(
        app.reader.as_ref(),
        app.content.as_ref(),
        app.progress.as_ref(),
    );
    match service.progression(user, book_id).await {
        BookProgressionGetOutcome::Progression(progression) => (
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static(READIUM_PROGRESSION_MEDIA_TYPE),
            )],
            Json(progression),
        )
            .into_response(),
        BookProgressionGetOutcome::NoContent => StatusCode::NO_CONTENT.into_response(),
        BookProgressionGetOutcome::NotFound => StatusCode::NOT_FOUND.into_response(),
        BookProgressionGetOutcome::Forbidden => StatusCode::FORBIDDEN.into_response(),
        BookProgressionGetOutcome::Internal(error) => internal_error_response(error),
    }
}

fn progression_bad_request_response(message: impl Into<String>) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": message.into() })),
    )
        .into_response()
}

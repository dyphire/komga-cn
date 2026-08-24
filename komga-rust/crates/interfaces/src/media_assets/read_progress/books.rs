use super::epub::load_epub_locator_for_page;
use crate::identity_access::auth::Authenticated;
use crate::opds_auth::OpdsV2Authenticated;
use crate::state::MediaAssetsState;
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use komga_application::identity_access::{AuthUser, user_id};
use komga_application::media_assets::{
    BookProgressionGetOutcome, BookProgressionLocator, BookProgressionOutcome,
    BookProgressionService, BookProgressionUpdate, BookProgressionUpdateInput,
};
use serde_json::Value;

use crate::contracts::common::ViolationDto;
use crate::contracts::media_assets::BookProgressionDto;
use crate::helpers::{spring_error_response, validation_error_response};
use crate::identity_access::auth::{resolved_auth_user, resolved_token};
use crate::media_assets::access_control::user_can_access_book_media;
use crate::media_assets::http_helpers::internal_error_response;
use crate::media_assets::read_progress::READIUM_PROGRESSION_MEDIA_TYPE;
use crate::media_assets::types::PersistedBookMedia;

fn request_progress_token(
    identity: &crate::state::IdentityState,
    headers: &HeaderMap,
    user: &AuthUser,
) -> Result<String, StatusCode> {
    if resolved_auth_user(identity, headers)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .is_some()
    {
        let token = resolved_token(headers);
        if !token.trim().is_empty() {
            return Ok(token);
        }
    }

    Ok(format!("user:{}", user_id(user)))
}

async fn load_accessible_book_media(
    app: &MediaAssetsState,
    book_id: &str,
    user: &AuthUser,
) -> Result<PersistedBookMedia, Box<Response>> {
    let Some(media) = (match app.book_media_reader.book_media(book_id).await {
        Ok(media) => media,
        Err(error) => return Err(Box::new(internal_error_response(error))),
    }) else {
        return Err(Box::new(StatusCode::NOT_FOUND.into_response()));
    };

    match user_can_access_book_media(app.book_media_reader.as_ref(), book_id, user, &media).await {
        Ok(true) => {}
        Ok(false) => return Err(Box::new(StatusCode::FORBIDDEN.into_response())),
        Err(error) => return Err(Box::new(internal_error_response(error))),
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

    app.read_progress
        .record(token.to_string(), book_id.to_string());
    StatusCode::NO_CONTENT.into_response()
}

pub(crate) async fn book_read_progress(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    body: Bytes,
) -> Response {
    let supports_persisted_flow = match app.read_progress_reader.book_exists(&book_id).await {
        Ok(exists) => exists,
        Err(error) => return internal_error_response(error),
    };

    if !supports_persisted_flow {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return spring_error_response(StatusCode::BAD_REQUEST, "invalid read progress payload");
    };

    if let Err(response) = load_accessible_book_media(&app, &book_id, &user).await {
        return *response;
    }
    let persisted_user_id = Some(user_id(&user));
    let page_count = match app.read_progress_reader.book_page_count(&book_id).await {
        Ok(Some(value)) if value > 0 => value,
        Ok(_) => 1,
        Err(error) => return internal_error_response(error),
    };

    let token = match request_progress_token(&app.identity, &headers, &user) {
        Ok(token) => token,
        Err(status) => return status.into_response(),
    };

    let page_value = payload.get("page");
    let completed_true = payload.get("completed").and_then(|value| value.as_bool()) == Some(true);

    if matches!(page_value.and_then(Value::as_i64), Some(value) if value <= 0) {
        return validation_error_response(vec![ViolationDto {
            field_name: Some("page".to_string()),
            message: Some("must be greater than 0".to_string()),
        }]);
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
        return validation_error_response(vec![]);
    }

    let Some(page) = payload.get("page").and_then(Value::as_u64) else {
        return spring_error_response(StatusCode::BAD_REQUEST, "invalid read progress payload");
    };

    if page > page_count {
        return spring_error_response(
            StatusCode::BAD_REQUEST,
            format!("Page argument ({page}) must be within 1 and book page count ({page_count})"),
        );
    }

    if !(1..=page_count).contains(&page) {
        return spring_error_response(StatusCode::BAD_REQUEST, "invalid read progress payload");
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

pub(crate) async fn book_read_progress_delete(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    let supports_persisted_flow = match app.read_progress_reader.book_exists(&book_id).await {
        Ok(exists) => exists,
        Err(error) => return internal_error_response(error),
    };

    if !supports_persisted_flow {
        return StatusCode::NOT_FOUND.into_response();
    }

    if let Err(response) = load_accessible_book_media(&app, &book_id, &user).await {
        return *response;
    }
    let token = match request_progress_token(&app.identity, &headers, &user) {
        Ok(token) => token,
        Err(status) => return status.into_response(),
    };
    app.read_progress.remove(&token, &book_id);

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

pub(crate) async fn book_progression(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path(book_id): Path<String>,
    body: Bytes,
) -> Response {
    book_progression_response(&app, &user, &book_id, body).await
}

pub(crate) async fn opds_v2_book_progression(
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
        return spring_error_response(StatusCode::BAD_REQUEST, "invalid progression payload");
    };
    let update = book_progression_update_input(&payload);
    let service = BookProgressionService::new(
        app.book_progression_reader.as_ref(),
        app.epub_navigation_content.as_ref(),
        app.progress.as_ref(),
    );
    match service.update_progression(user, book_id, update).await {
        BookProgressionOutcome::Updated => StatusCode::NO_CONTENT.into_response(),
        BookProgressionOutcome::NotFound => StatusCode::NOT_FOUND.into_response(),
        BookProgressionOutcome::Forbidden => StatusCode::FORBIDDEN.into_response(),
        BookProgressionOutcome::InvalidPayload => {
            spring_error_response(StatusCode::BAD_REQUEST, "invalid progression payload")
        }
        BookProgressionOutcome::BadRequest(error) => progression_bad_request_response(error),
        BookProgressionOutcome::Conflict => {
            spring_error_response(StatusCode::CONFLICT, "Progression is older than existing")
        }
        BookProgressionOutcome::Internal(error) => internal_error_response(error),
    }
}

fn book_progression_update_input(payload: &Value) -> BookProgressionUpdateInput {
    let Some(modified) = payload.get("modified").and_then(Value::as_str) else {
        return BookProgressionUpdateInput::InvalidPayload;
    };
    let Some(device_id) = payload
        .get("device")
        .and_then(|value| value.get("id"))
        .and_then(Value::as_str)
    else {
        return BookProgressionUpdateInput::InvalidPayload;
    };
    let Some(device_name) = payload
        .get("device")
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
    else {
        return BookProgressionUpdateInput::InvalidPayload;
    };

    BookProgressionUpdateInput::Update(BookProgressionUpdate {
        modified: modified.to_string(),
        device_id: device_id.to_string(),
        device_name: device_name.to_string(),
        locator: book_progression_locator(payload),
    })
}

fn book_progression_locator(payload: &Value) -> Option<BookProgressionLocator> {
    let locator = payload.get("locator")?.clone();
    let locations = locator.get("locations");
    let href = locator
        .get("href")
        .and_then(Value::as_str)
        .map(str::to_string);
    let progression = locations
        .and_then(|value| value.get("progression"))
        .and_then(Value::as_f64);
    let position = locations
        .and_then(|value| value.get("position"))
        .and_then(Value::as_u64);
    let total_progression = locations
        .and_then(|value| value.get("totalProgression"))
        .and_then(Value::as_f64);

    Some(BookProgressionLocator::new(
        locator,
        href,
        progression,
        position,
        total_progression,
    ))
}

pub(crate) async fn book_progression_get(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path(book_id): Path<String>,
) -> Response {
    book_progression_get_response(&app, &user, &book_id).await
}

pub(crate) async fn opds_v2_book_progression_get(
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
        app.book_progression_reader.as_ref(),
        app.epub_navigation_content.as_ref(),
        app.progress.as_ref(),
    );
    match service.progression(user, book_id).await {
        BookProgressionGetOutcome::Progression(progression) => (
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static(READIUM_PROGRESSION_MEDIA_TYPE),
            )],
            Json(BookProgressionDto::from(progression)),
        )
            .into_response(),
        BookProgressionGetOutcome::NoContent => StatusCode::NO_CONTENT.into_response(),
        BookProgressionGetOutcome::NotFound => StatusCode::NOT_FOUND.into_response(),
        BookProgressionGetOutcome::Forbidden => StatusCode::FORBIDDEN.into_response(),
        BookProgressionGetOutcome::Internal(error) => internal_error_response(error),
    }
}

fn progression_bad_request_response(message: impl Into<String>) -> Response {
    spring_error_response(StatusCode::BAD_REQUEST, message.into())
}

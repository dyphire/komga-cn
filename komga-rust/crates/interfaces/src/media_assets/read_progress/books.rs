use super::epub::{
    load_epub_locator_for_page, locator_position, locator_progression, progression_bad_request,
    progression_is_older_than_existing, progression_locator,
};
use super::*;
use axum::extract::State;
use std::sync::Arc;

fn request_progress_token(headers: &HeaderMap, user: &AuthUser) -> String {
    if resolved_auth_user(headers).is_some() {
        let token = resolved_token(headers);
        if !token.trim().is_empty() {
            return token;
        }
    }

    format!("user:{}", user_id(user))
}

async fn load_accessible_book_media(
    app: &HttpAppState,
    book_id: &str,
    user: &AuthUser,
) -> Result<PersistedBookMedia, Response> {
    let Some(media) = (match app
        .services
        .media_assets
        .load_persisted_book_media(book_id.to_string())
        .await
    {
        Ok(media) => media,
        Err(error) => return Err(internal_error_response(error)),
    }) else {
        return Err(StatusCode::NOT_FOUND.into_response());
    };

    if !user_can_access_book_media(app, book_id, user, &media).await {
        return Err(StatusCode::FORBIDDEN.into_response());
    }

    Ok(media)
}

#[allow(clippy::too_many_arguments)]
async fn persist_and_record_read_progress(
    app: &HttpAppState,
    token: &str,
    book_id: &str,
    persisted_user_id: Option<&str>,
    page: u64,
    completed: bool,
    locator: Option<Value>,
) -> Response {
    if let Some(user_id) = persisted_user_id
        && persist_read_progress_from_services(app, book_id, user_id, page, completed, locator)
            .await
            .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    set_read_progress(app, token.to_string(), book_id.to_string());
    StatusCode::NO_CONTENT.into_response()
}

pub async fn book_read_progress(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    body: Bytes,
) -> Response {
    let auth_db = &app.auth_db;
    if let Some(response) = require_request_auth(&headers, auth_db.db.database_file()).await {
        return response;
    }

    let supports_persisted_flow = persisted_book_exists_from_services(&app, &book_id)
        .await
        .unwrap_or(false);

    if !supports_persisted_flow {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return invalid_read_progress_payload();
    };

    let Some(user) = resolved_request_auth_user(&headers, auth_db.db.database_file()).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if let Err(response) = load_accessible_book_media(&app, &book_id, &user).await {
        return response;
    }
    let persisted_user_id = Some(user_id(&user).to_string());
    let page_count = match load_book_page_count_from_services(&app, &book_id).await {
        Ok(Some(value)) if value > 0 => value,
        Ok(_) => 1,
        Err(error) => return internal_error_response(error),
    };

    let token = request_progress_token(&headers, &user);

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
            persisted_user_id.as_deref(),
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
        persisted_user_id.as_deref(),
        page,
        page == page_count,
        locator,
    )
    .await
}

pub async fn book_read_progress_delete(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    let auth_db = &app.auth_db;
    if let Some(response) = require_request_auth(&headers, auth_db.db.database_file()).await {
        return response;
    }

    let supports_persisted_flow = persisted_book_exists_from_services(&app, &book_id)
        .await
        .unwrap_or(false);

    if !supports_persisted_flow {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Some(user) = resolved_request_auth_user(&headers, auth_db.db.database_file()).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if let Err(response) = load_accessible_book_media(&app, &book_id, &user).await {
        return response;
    }
    let token = request_progress_token(&headers, &user);
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
        && delete_persisted_read_progress_from_services(&app, &book_id, user_id(&user))
            .await
            .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

pub async fn book_progression(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    body: Bytes,
) -> Response {
    let auth_db = &app.auth_db;
    if let Some(response) = require_request_auth(&headers, auth_db.db.database_file()).await {
        return response;
    }

    if !persisted_book_exists_from_services(&app, &book_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Some(user) = resolved_request_auth_user(&headers, auth_db.db.database_file()).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(media) = (match app
        .services
        .media_assets
        .load_persisted_book_media(book_id.clone())
        .await
    {
        Ok(media) => media,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !user_can_access_book_media(&app, &book_id, &user, &media).await {
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
        let normalized_locator = match normalize_book_epub_locator(&app, &book_id, locator).await {
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

    let stale_progression =
        match progression_is_older_than_existing(&app, &book_id, user_id(&user), modified).await {
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
        .services
        .media_assets
        .persist_book_progression(
            book_id.clone(),
            user_id(&user).to_string(),
            progression,
            !is_epub,
            Some(modified.to_string()),
            Some(device_id.to_string()),
            Some(device_name.to_string()),
            locator_to_persist,
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn book_progression_get(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    let auth_db = &app.auth_db;
    if let Some(response) = require_request_auth(&headers, auth_db.db.database_file()).await {
        return response;
    }

    if !persisted_book_exists_from_services(&app, &book_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Some(user) = resolved_request_auth_user(&headers, auth_db.db.database_file()).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(media) = (match app
        .services
        .media_assets
        .load_persisted_book_media(book_id.clone())
        .await
    {
        Ok(media) => media,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !user_can_access_book_media(&app, &book_id, &user, &media).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    match load_book_progression_from_services(&app, &book_id, user_id(&user)).await {
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

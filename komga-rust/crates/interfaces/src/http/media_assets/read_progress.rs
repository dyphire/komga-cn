use super::*;
use crate::runtime_identity_access::load_read_progress;

pub async fn readlist_tachiyomi_read_progress_get(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let counters = match readlist_tachiyomi_counters(
        auth_db.database_file.as_path(),
        &readlist_id,
        user_id(&user),
    )
    .await
    {
        Ok(Some(counters)) => counters,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    };

    let (
        books_count,
        books_read_count,
        books_unread_count,
        books_in_progress_count,
        last_read_continuous_index,
    ) = counters;
    Json(json!({
        "booksCount": books_count,
        "booksReadCount": books_read_count,
        "booksUnreadCount": books_unread_count,
        "booksInProgressCount": books_in_progress_count,
        "lastReadContinuousIndex": last_read_continuous_index,
    }))
    .into_response()
}

pub async fn readlist_tachiyomi_read_progress_put(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let Some(last_book_read) = body
        .get("lastBookRead")
        .and_then(Value::as_i64)
        .filter(|value| *value >= 0)
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "lastBookRead must be a non-negative integer" })),
        )
            .into_response();
    };

    match persist_readlist_tachiyomi_progress(
        auth_db.database_file.as_path(),
        &readlist_id,
        user_id(&user),
        last_book_read as usize,
    )
    .await
    {
        Ok(Some(())) => StatusCode::NO_CONTENT.into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn series_read_progress_post(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }
    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let resolved_series_id =
        resolve_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    match user_can_access_series_media(auth_db.database_file.as_path(), &resolved_series_id, &user)
        .await
    {
        Ok(true) => {}
        Ok(false) => return StatusCode::FORBIDDEN.into_response(),
        Err(error) => return internal_error_response(error),
    }

    let book_ids =
        match load_series_book_ids(auth_db.database_file.as_path(), &resolved_series_id).await {
            Ok(book_ids) => book_ids,
            Err(error) => return internal_error_response(error),
        };

    for book_id in book_ids {
        let already_completed =
            match load_read_progress(auth_db.database_file.as_path(), &book_id, user_id(&user))
                .await
            {
                Ok(Some(progress)) => progress.completed,
                Ok(None) => false,
                Err(error) => return internal_error_response(error),
            };
        if already_completed {
            continue;
        }

        let page_count = match load_book_page_count(auth_db.database_file.as_path(), &book_id).await
        {
            Ok(Some(value)) => value,
            Ok(None) => 1,
            Err(error) => return internal_error_response(error),
        };
        if let Err(error) = persist_read_progress(
            auth_db.database_file.as_path(),
            &book_id,
            user_id(&user),
            page_count,
            true,
        )
        .await
        {
            return internal_error_response(error);
        }
    }
    if let Err(error) = delete_series_read_progress_row(
        auth_db.database_file.as_path(),
        &resolved_series_id,
        user_id(&user),
    )
    .await
    {
        return internal_error_response(error);
    }

    StatusCode::NO_CONTENT.into_response()
}

pub async fn series_read_progress_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }
    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let resolved_series_id =
        resolve_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    if !persisted_series_exists(auth_db.database_file.as_path(), &resolved_series_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    match user_can_access_series_media(auth_db.database_file.as_path(), &resolved_series_id, &user)
        .await
    {
        Ok(true) => {}
        Ok(false) => return StatusCode::FORBIDDEN.into_response(),
        Err(error) => return internal_error_response(error),
    }

    let book_ids =
        match load_series_book_ids(auth_db.database_file.as_path(), &resolved_series_id).await {
            Ok(book_ids) => book_ids,
            Err(error) => return internal_error_response(error),
        };

    for book_id in book_ids {
        if let Err(error) = delete_persisted_read_progress(
            auth_db.database_file.as_path(),
            &book_id,
            user_id(&user),
        )
        .await
        {
            return internal_error_response(error);
        }
    }
    if let Err(error) = refresh_series_read_progress_row(
        auth_db.database_file.as_path(),
        &resolved_series_id,
        user_id(&user),
    )
    .await
    {
        return internal_error_response(error);
    }

    StatusCode::NO_CONTENT.into_response()
}

pub async fn series_tachiyomi_read_progress_get(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }
    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let resolved_series_id =
        resolve_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    let unrestricted_all_libraries = user_shared_all_libraries(&user)
        && principal_from_user_payload(&user_payload_json(&user))
            .is_none_or(|principal| !principal.restrictions.is_restricted());
    if !unrestricted_all_libraries {
        if !persisted_series_exists(auth_db.database_file.as_path(), &resolved_series_id)
            .await
            .unwrap_or(false)
        {
            return StatusCode::NOT_FOUND.into_response();
        }
        match user_can_access_series_media(
            auth_db.database_file.as_path(),
            &resolved_series_id,
            &user,
        )
        .await
        {
            Ok(true) => {}
            Ok(false) => return StatusCode::FORBIDDEN.into_response(),
            Err(error) => return internal_error_response(error),
        }
    }

    match load_series_tachiyomi_progress(
        auth_db.database_file.as_path(),
        &resolved_series_id,
        user_id(&user),
    )
    .await
    {
        Ok(Some(payload)) => Json(payload).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn series_tachiyomi_read_progress_put(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
    body: Bytes,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }
    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

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
        resolve_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    if !persisted_series_exists(auth_db.database_file.as_path(), &resolved_series_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    match user_can_access_series_media(auth_db.database_file.as_path(), &resolved_series_id, &user)
        .await
    {
        Ok(true) => {}
        Ok(false) => return StatusCode::FORBIDDEN.into_response(),
        Err(error) => return internal_error_response(error),
    }

    let book_numbers =
        match load_series_book_number_sorts(auth_db.database_file.as_path(), &resolved_series_id)
            .await
        {
            Ok(book_numbers) => book_numbers,
            Err(error) => return internal_error_response(error),
        };

    for (book_id, number_sort) in book_numbers {
        if number_sort <= last_number_sort_read
            && let Err(error) = persist_read_progress(
                auth_db.database_file.as_path(),
                &book_id,
                user_id(&user),
                10,
                true,
            )
            .await
        {
            return internal_error_response(error);
        }
    }
    if let Err(error) = refresh_series_read_progress_row(
        auth_db.database_file.as_path(),
        &resolved_series_id,
        user_id(&user),
    )
    .await
    {
        return internal_error_response(error);
    }

    StatusCode::NO_CONTENT.into_response()
}

pub async fn book_read_progress(
    Extension(_profile): Extension<RuntimeProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(state): Extension<ReadProgressState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    body: Bytes,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let supports_persisted_flow = persisted_book_exists(auth_db.database_file.as_path(), &book_id)
        .await
        .unwrap_or(false);

    if !supports_persisted_flow {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return invalid_read_progress_payload();
    };

    let persisted_user_id = resolved_auth_user(&headers).map(|user| user_id(&user).to_string());
    let page_count = match load_book_page_count(auth_db.database_file.as_path(), &book_id).await {
        Ok(Some(value)) if value > 0 => value,
        Ok(_) => 1,
        Err(error) => return internal_error_response(error),
    };

    let token = resolved_token(&headers);

    if payload.get("completed").and_then(|value| value.as_bool()) == Some(true) {
        if let Some(user_id) = persisted_user_id.as_deref()
            && persist_read_progress(
                auth_db.database_file.as_path(),
                &book_id,
                user_id,
                page_count,
                true,
            )
            .await
            .is_err()
        {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        set_read_progress(&state, token, book_id);
        return StatusCode::NO_CONTENT.into_response();
    }

    if let Some(page) = payload.get("page").and_then(|value| value.as_u64())
        && (1..=page_count).contains(&page)
    {
        if let Some(user_id) = persisted_user_id.as_deref()
            && persist_read_progress(
                auth_db.database_file.as_path(),
                &book_id,
                user_id,
                page,
                false,
            )
            .await
            .is_err()
        {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        set_read_progress(&state, token, book_id);
        return StatusCode::NO_CONTENT.into_response();
    }

    invalid_read_progress_payload()
}

pub async fn book_read_progress_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(state): Extension<ReadProgressState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let supports_persisted_flow = persisted_book_exists(auth_db.database_file.as_path(), &book_id)
        .await
        .unwrap_or(false);

    if !supports_persisted_flow {
        return StatusCode::NOT_FOUND.into_response();
    }

    let token = resolved_token(&headers);
    {
        let mut all_progress = state
            .progress_by_token
            .lock()
            .expect("read-progress state lock should not be poisoned");

        if let Some(user_progress) = all_progress.get_mut(&token) {
            user_progress.remove(&book_id);
        }
    }

    if supports_persisted_flow
        && let Some(user) = resolved_auth_user(&headers)
        && delete_persisted_read_progress(auth_db.database_file.as_path(), &book_id, user_id(&user))
            .await
            .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

pub async fn book_progression(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    body: Bytes,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !persisted_book_exists(auth_db.database_file.as_path(), &book_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(media) =
        (match load_persisted_book_media(auth_db.database_file.as_path(), &book_id).await {
            Ok(media) => media,
            Err(error) => return internal_error_response(error),
        })
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !user_can_access_book_media(auth_db.database_file.as_path(), &book_id, &user, &media).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return invalid_progression_payload();
    };

    let progression = payload
        .get("locator")
        .and_then(|value| value.get("locations"))
        .and_then(|value| value.get("progression"))
        .and_then(|value| value.as_f64());

    let Some(progression) = progression else {
        return invalid_progression_payload();
    };
    if !(0.0..=1.0).contains(&progression) {
        return invalid_progression_payload();
    }

    match persist_book_progression(
        auth_db.database_file.as_path(),
        &book_id,
        user_id(&user),
        progression,
    )
    .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn book_progression_get(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(_profile): Extension<RuntimeProfile>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !persisted_book_exists(auth_db.database_file.as_path(), &book_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(media) =
        (match load_persisted_book_media(auth_db.database_file.as_path(), &book_id).await {
            Ok(media) => media,
            Err(error) => return internal_error_response(error),
        })
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !user_can_access_book_media(auth_db.database_file.as_path(), &book_id, &user, &media).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    match load_book_progression(auth_db.database_file.as_path(), &book_id, user_id(&user)).await {
        Ok(Some(progression)) => Json(json!({
            "locator": {
                "locations": {
                    "progression": progression,
                }
            }
        }))
        .into_response(),
        Ok(None) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => internal_error_response(error),
    }
}

use super::*;
use crate::identity_access::auth::Authenticated;
use crate::state::MediaAssetsState;
use axum::extract::State;

pub async fn series_read_progress_post(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path(series_id): Path<String>,
) -> Response {
    let resolved_series_id = resolve_series_id_for_persisted(app.root.as_ref(), &series_id).await;
    match user_can_access_series_media(app.root.as_ref(), &resolved_series_id, &user).await {
        Ok(true) => {}
        Ok(false) => return StatusCode::FORBIDDEN.into_response(),
        Err(error) => return internal_error_response(error),
    }

    let book_ids =
        match load_series_book_ids_from_services(app.root.as_ref(), &resolved_series_id).await {
            Ok(book_ids) => book_ids,
            Err(error) => return internal_error_response(error),
        };

    for book_id in book_ids {
        let already_completed =
            match load_read_progress_from_services(app.root.as_ref(), &book_id, user_id(&user))
                .await
            {
                Ok(Some(progress)) => progress.completed,
                Ok(None) => false,
                Err(error) => return internal_error_response(error),
            };
        if already_completed {
            continue;
        }

        let page_count = match load_book_page_count_from_services(app.root.as_ref(), &book_id).await
        {
            Ok(Some(value)) => value,
            Ok(None) => 1,
            Err(error) => return internal_error_response(error),
        };
        if let Err(error) = persist_read_progress_from_services(
            app.root.as_ref(),
            &book_id,
            user_id(&user),
            page_count,
            true,
            None,
        )
        .await
        {
            return internal_error_response(error);
        }
    }
    if let Err(error) = refresh_series_read_progress_row_from_services(
        app.root.as_ref(),
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
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path(series_id): Path<String>,
) -> Response {
    let resolved_series_id = resolve_series_id_for_persisted(app.root.as_ref(), &series_id).await;
    let unrestricted_all_libraries = user_shared_all_libraries(&user)
        && principal_from_user_payload(&user_payload_json(&user))
            .is_none_or(|principal| !principal.restrictions.is_restricted());
    if unrestricted_all_libraries {
        if !persisted_series_exists_from_services(app.root.as_ref(), &resolved_series_id)
            .await
            .unwrap_or(false)
        {
            return StatusCode::NO_CONTENT.into_response();
        }
    } else {
        if !persisted_series_exists_from_services(app.root.as_ref(), &resolved_series_id)
            .await
            .unwrap_or(false)
        {
            return StatusCode::NOT_FOUND.into_response();
        }
        match user_can_access_series_media(app.root.as_ref(), &resolved_series_id, &user).await {
            Ok(true) => {}
            Ok(false) => return StatusCode::FORBIDDEN.into_response(),
            Err(error) => return internal_error_response(error),
        }
    }

    let book_ids =
        match load_series_book_ids_from_services(app.root.as_ref(), &resolved_series_id).await {
            Ok(book_ids) => book_ids,
            Err(error) => return internal_error_response(error),
        };

    for book_id in book_ids {
        if let Err(error) = delete_persisted_read_progress_from_services(
            app.root.as_ref(),
            &book_id,
            user_id(&user),
        )
        .await
        {
            return internal_error_response(error);
        }
    }
    if let Err(error) = delete_series_read_progress_row_from_services(
        app.root.as_ref(),
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
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path(series_id): Path<String>,
) -> Response {
    let resolved_series_id = resolve_series_id_for_persisted(app.root.as_ref(), &series_id).await;
    let unrestricted_all_libraries = user_shared_all_libraries(&user)
        && principal_from_user_payload(&user_payload_json(&user))
            .is_none_or(|principal| !principal.restrictions.is_restricted());
    if !unrestricted_all_libraries {
        if !persisted_series_exists_from_services(app.root.as_ref(), &resolved_series_id)
            .await
            .unwrap_or(false)
        {
            return StatusCode::NOT_FOUND.into_response();
        }
        match user_can_access_series_media(app.root.as_ref(), &resolved_series_id, &user).await {
            Ok(true) => {}
            Ok(false) => return StatusCode::FORBIDDEN.into_response(),
            Err(error) => return internal_error_response(error),
        }
    }

    match load_series_tachiyomi_progress_from_services(
        app.root.as_ref(),
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

    let resolved_series_id = resolve_series_id_for_persisted(app.root.as_ref(), &series_id).await;
    let unrestricted_all_libraries = user_shared_all_libraries(&user)
        && principal_from_user_payload(&user_payload_json(&user))
            .is_none_or(|principal| !principal.restrictions.is_restricted());
    if unrestricted_all_libraries {
        if !persisted_series_exists_from_services(app.root.as_ref(), &resolved_series_id)
            .await
            .unwrap_or(false)
        {
            return StatusCode::NO_CONTENT.into_response();
        }
    } else {
        if !persisted_series_exists_from_services(app.root.as_ref(), &resolved_series_id)
            .await
            .unwrap_or(false)
        {
            return StatusCode::NOT_FOUND.into_response();
        }
        match user_can_access_series_media(app.root.as_ref(), &resolved_series_id, &user).await {
            Ok(true) => {}
            Ok(false) => return StatusCode::FORBIDDEN.into_response(),
            Err(error) => return internal_error_response(error),
        }
    }

    let book_numbers =
        match load_series_book_number_sorts_from_services(app.root.as_ref(), &resolved_series_id)
            .await
        {
            Ok(book_numbers) => book_numbers,
            Err(error) => return internal_error_response(error),
        };

    for (book_id, number_sort) in book_numbers {
        if number_sort > last_number_sort_read {
            continue;
        }

        let already_completed =
            match load_read_progress_from_services(app.root.as_ref(), &book_id, user_id(&user))
                .await
            {
                Ok(Some(progress)) => progress.completed,
                Ok(None) => false,
                Err(error) => return internal_error_response(error),
            };
        if already_completed {
            continue;
        }

        let page_count = match load_book_page_count_from_services(app.root.as_ref(), &book_id).await
        {
            Ok(Some(value)) => value,
            Ok(None) => 1,
            Err(error) => return internal_error_response(error),
        };
        if let Err(error) = persist_read_progress_from_services(
            app.root.as_ref(),
            &book_id,
            user_id(&user),
            page_count,
            true,
            None,
        )
        .await
        {
            return internal_error_response(error);
        }
    }
    if let Err(error) = refresh_series_read_progress_row_from_services(
        app.root.as_ref(),
        &resolved_series_id,
        user_id(&user),
    )
    .await
    {
        return internal_error_response(error);
    }

    StatusCode::NO_CONTENT.into_response()
}

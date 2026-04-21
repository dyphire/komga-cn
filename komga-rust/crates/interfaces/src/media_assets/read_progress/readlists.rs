use super::*;
use axum::extract::State;
use std::sync::Arc;

async fn load_tachiyomi_readlist_book_ids(
    app: &HttpAppState,
    readlist_id: &str,
    user: &AuthUser,
) -> Result<Option<Vec<String>>, String> {
    let readlist_books = app
        .services
        .opds_persisted
        .load_readlist_books(app.auth_db.database_file.clone(), readlist_id.to_string())
        .await?;
    if readlist_books.is_empty() {
        let readlist_exists = app
            .services
            .media_assets
            .load_persisted_readlist_name(
                app.auth_db.database_file.clone(),
                readlist_id.to_string(),
            )
            .await?
            .is_some();
        return Ok(
            (readlist_exists && (user_shared_all_libraries(user) || user_is_admin(user)))
                .then_some(Vec::new()),
        );
    }
    if !readlist_books
        .iter()
        .any(|book| user_can_access_library(user, &book.library_id))
    {
        return Ok(None);
    }

    Ok(Some(
        readlist_books.into_iter().map(|book| book.id).collect(),
    ))
}

pub async fn readlist_tachiyomi_read_progress_get(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) =
        require_request_auth(&headers, app.auth_db.database_file.as_path()).await
    {
        return response;
    }

    let Some(user) =
        resolved_request_auth_user(&headers, app.auth_db.database_file.as_path()).await
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let Some(ordered_book_ids) =
        (match load_tachiyomi_readlist_book_ids(&app, &readlist_id, &user).await {
            Ok(ordered_book_ids) => ordered_book_ids,
            Err(error) => return internal_error_response(error),
        })
    else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let counters = match app
        .services
        .media_assets
        .readlist_tachiyomi_counters(
            app.auth_db.database_file.clone(),
            ordered_book_ids,
            user_id(&user).to_string(),
        )
        .await
    {
        Ok(counters) => counters,
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
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if let Some(response) =
        require_request_auth(&headers, app.auth_db.database_file.as_path()).await
    {
        return response;
    }

    let Some(user) =
        resolved_request_auth_user(&headers, app.auth_db.database_file.as_path()).await
    else {
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

    let visible_books = match visible_readlist_books_for_user(&app, &readlist_id, &user).await {
        Ok(books) => books,
        Err(error) => return internal_error_response(error),
    };
    if visible_books.is_empty() {
        return StatusCode::NO_CONTENT.into_response();
    }

    let visible_book_ids = visible_books
        .into_iter()
        .map(|book| book.id)
        .collect::<Vec<_>>();

    match app
        .services
        .media_assets
        .persist_readlist_tachiyomi_progress(
            app.auth_db.database_file.clone(),
            visible_book_ids,
            user_id(&user).to_string(),
            last_book_read as usize,
        )
        .await
    {
        Ok(Some(())) => StatusCode::NO_CONTENT.into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

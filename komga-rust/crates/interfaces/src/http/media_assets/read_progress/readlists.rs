use super::*;

async fn load_tachiyomi_readlist_book_ids(
    database_file: &FsPath,
    readlist_id: &str,
    user: &AuthUser,
) -> Result<Option<Vec<String>>, String> {
    let readlist_books = load_readlist_books(database_file, readlist_id).await?;
    if readlist_books.is_empty() {
        let readlist_exists = load_persisted_readlist_name(database_file, readlist_id)
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
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_request_auth(&headers, auth_db.database_file.as_path()).await {
        return response;
    }

    let Some(user) = resolved_request_auth_user(&headers, auth_db.database_file.as_path()).await
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let Some(ordered_book_ids) = (match load_tachiyomi_readlist_book_ids(
        auth_db.database_file.as_path(),
        &readlist_id,
        &user,
    )
    .await
    {
        Ok(ordered_book_ids) => ordered_book_ids,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let counters = match readlist_tachiyomi_counters(
        auth_db.database_file.as_path(),
        ordered_book_ids,
        user_id(&user),
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
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if let Some(response) = require_request_auth(&headers, auth_db.database_file.as_path()).await {
        return response;
    }

    let Some(user) = resolved_request_auth_user(&headers, auth_db.database_file.as_path()).await
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

    let Some(ordered_book_ids) = (match load_tachiyomi_readlist_book_ids(
        auth_db.database_file.as_path(),
        &readlist_id,
        &user,
    )
    .await
    {
        Ok(ordered_book_ids) => ordered_book_ids,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    if ordered_book_ids.is_empty() {
        return StatusCode::NO_CONTENT.into_response();
    }

    let visible_books =
        match visible_readlist_books_for_user(auth_db.database_file.as_path(), &readlist_id, &user)
            .await
        {
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

    match persist_readlist_tachiyomi_progress(
        auth_db.database_file.as_path(),
        visible_book_ids,
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

use super::*;
use crate::identity_access::auth::Authenticated;
use crate::state::MediaAssetsState;
use axum::extract::State;

async fn load_tachiyomi_readlist_book_ids(
    app: &MediaAssetsState,
    readlist_id: &str,
    user: &AuthUser,
) -> Result<Option<Vec<String>>, String> {
    let visible_books = visible_readlist_books_for_user(app, readlist_id, user).await?;
    if visible_books.is_empty() {
        let readlist_exists = app
            .media_assets
            .load_persisted_readlist_name(readlist_id)
            .await?
            .is_some();
        return Ok(
            (readlist_exists && (user_shared_all_libraries(user) || user_is_admin(user)))
                .then_some(Vec::new()),
        );
    }
    Ok(Some(
        visible_books.into_iter().map(|book| book.id).collect(),
    ))
}

pub async fn readlist_tachiyomi_read_progress_get(
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
        .media_assets
        .readlist_tachiyomi_counters(&ordered_book_ids, user_id(&user))
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

    match app
        .media_assets
        .persist_readlist_tachiyomi_progress(
            &ordered_book_ids,
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

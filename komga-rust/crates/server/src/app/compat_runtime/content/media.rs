use std::fs;
use std::path::{Path as FsPath, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use komga_persistence::sqlite::connect_pool;
use serde_json::{Value, json};
use sqlx::Row;

use crate::app::CompatProfile;
use crate::app::compat_runtime::AuthDatabaseState;
use crate::app::placeholder_auth::{
    require_admin, require_auth, resolved_auth_user, resolved_token, user_id,
    user_shared_all_libraries, user_shared_library_ids,
};
use crate::app::snapshots::request_host;
use crate::task_queue::TaskQueueRecord;

use super::super::{CACHE_CONTROL_PRIVATE, LAST_MODIFIED, ReadProgressState, THUMBNAIL_ETAG};
use super::helpers::{
    invalid_progression_payload, invalid_read_progress_payload, mark_native,
    method_not_allowed_json_response, non_native_response, set_read_progress,
};

pub(in crate::app::compat_runtime) async fn book_page(
    Extension(_profile): Extension<CompatProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((book_id, page_number)): Path<(String, u32)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if let Ok(Some(media)) =
        load_persisted_book_media(auth_db.database_file.as_path(), &book_id).await
    {
        if let Some(user) = resolved_auth_user(&headers)
            && !user_can_access_library(&user, &media.library_id)
        {
            return non_native_response(StatusCode::FORBIDDEN.into_response());
        }

        if page_number != 1 || !book_media_supports_page_image(&media) {
            return non_native_response(StatusCode::NOT_FOUND.into_response());
        }

        if let Ok(bytes) = fs::read(&media.file_path) {
            let content_type = content_type_from_filename(&media.file_name, &media.media_type);

            if headers
                .get(header::IF_MODIFIED_SINCE)
                .and_then(|value| value.to_str().ok())
                == Some(LAST_MODIFIED)
            {
                return non_native_response(
                    (
                        StatusCode::NOT_MODIFIED,
                        [
                            (header::LAST_MODIFIED, LAST_MODIFIED),
                            (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
                        ],
                    )
                        .into_response(),
                );
            }

            return non_native_response(
                (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, content_type.as_str()),
                        (header::LAST_MODIFIED, LAST_MODIFIED),
                        (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
                    ],
                    bytes,
                )
                    .into_response(),
            );
        }
    }

    non_native_response(StatusCode::NOT_FOUND.into_response())
}

pub(in crate::app::compat_runtime) async fn book_page_thumbnail(
    Extension(_profile): Extension<CompatProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((book_id, page_number)): Path<(String, u32)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if let Ok(Some(media)) =
        load_persisted_book_media(auth_db.database_file.as_path(), &book_id).await
    {
        if let Some(user) = resolved_auth_user(&headers)
            && !user_can_access_library(&user, &media.library_id)
        {
            return non_native_response(StatusCode::FORBIDDEN.into_response());
        }

        if page_number != 1 || !book_media_supports_page_image(&media) {
            return non_native_response(StatusCode::NOT_FOUND.into_response());
        }

        if let Ok(bytes) = fs::read(&media.file_path) {
            let content_type = content_type_from_filename(&media.file_name, &media.media_type);

            if headers
                .get(header::IF_MODIFIED_SINCE)
                .and_then(|value| value.to_str().ok())
                == Some(LAST_MODIFIED)
            {
                return non_native_response(
                    (
                        StatusCode::NOT_MODIFIED,
                        [
                            (header::LAST_MODIFIED, LAST_MODIFIED),
                            (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
                        ],
                    )
                        .into_response(),
                );
            }

            return non_native_response(
                (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, content_type.as_str()),
                        (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
                        (header::LAST_MODIFIED, LAST_MODIFIED),
                        (header::ETAG, THUMBNAIL_ETAG),
                    ],
                    bytes,
                )
                    .into_response(),
            );
        }
    }

    non_native_response(StatusCode::NOT_FOUND.into_response())
}

pub(in crate::app::compat_runtime) async fn book_thumbnail(
    Extension(_profile): Extension<CompatProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if let Ok(Some(media)) =
        load_persisted_book_media(auth_db.database_file.as_path(), &book_id).await
    {
        if let Some(user) = resolved_auth_user(&headers)
            && !user_can_access_library(&user, &media.library_id)
        {
            return non_native_response(StatusCode::FORBIDDEN.into_response());
        }

        match load_selected_book_thumbnail(auth_db.database_file.as_path(), &book_id).await {
            Ok(Some(thumbnail)) => {
                return non_native_response(
                    (
                        StatusCode::OK,
                        [
                            (header::CONTENT_TYPE, thumbnail.media_type.as_str()),
                            (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
                            (header::LAST_MODIFIED, LAST_MODIFIED),
                            (header::ETAG, THUMBNAIL_ETAG),
                        ],
                        thumbnail.thumbnail,
                    )
                        .into_response(),
                );
            }
            Ok(None) => {}
            Err(error) => return internal_error_response(error),
        }

        if book_media_supports_page_image(&media)
            && let Ok(bytes) = fs::read(&media.file_path)
        {
            let content_type = content_type_from_filename(&media.file_name, &media.media_type);
            return non_native_response(
                (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, content_type.as_str()),
                        (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
                        (header::LAST_MODIFIED, LAST_MODIFIED),
                        (header::ETAG, THUMBNAIL_ETAG),
                    ],
                    bytes,
                )
                    .into_response(),
            );
        }

        return non_native_response(StatusCode::NOT_FOUND.into_response());
    }

    non_native_response(StatusCode::NOT_FOUND.into_response())
}

pub(in crate::app::compat_runtime) async fn book_thumbnails(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if let Ok(rows) =
        load_persisted_book_thumbnails(auth_db.database_file.as_path(), &book_id).await
        && !rows.is_empty()
    {
        let mut response = Json(
            rows.into_iter()
                .map(|row| {
                    json!({
                        "id": row.id,
                        "type": row.thumbnail_type,
                        "selected": row.selected,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .into_response();
        mark_native(&mut response);
        return response;
    }

    StatusCode::NOT_FOUND.into_response()
}

pub(in crate::app::compat_runtime) async fn book_thumbnail_upload(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match persisted_book_exists(auth_db.database_file.as_path(), &book_id).await {
        Ok(true) => {
            let mut response = StatusCode::OK.into_response();
            mark_native(&mut response);
            response
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn book_thumbnail_select(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((book_id, _thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match persisted_book_exists(auth_db.database_file.as_path(), &book_id).await {
        Ok(true) => {
            let mut response = StatusCode::ACCEPTED.into_response();
            mark_native(&mut response);
            response
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn book_thumbnail_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((book_id, _thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match persisted_book_exists(auth_db.database_file.as_path(), &book_id).await {
        Ok(true) => {
            let mut response = StatusCode::ACCEPTED.into_response();
            mark_native(&mut response);
            response
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn book_analyze(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(state): Extension<super::super::OperationalState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match persisted_book_exists(auth_db.database_file.as_path(), &book_id).await {
        Ok(true) => {
            let mut task_queue = state
                .task_queue
                .lock()
                .expect("task queue state lock should not be poisoned");
            task_queue.enqueue(TaskQueueRecord::new(
                format!("ANALYZE_BOOK:{book_id}"),
                90,
                Some(book_id),
            ));

            if let Err(error) = task_queue.process_available(&state.runtime) {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": error.to_string() })),
                )
                    .into_response();
            }

            let mut response = StatusCode::ACCEPTED.into_response();
            mark_native(&mut response);
            response
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn book_metadata_refresh(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match persisted_book_exists(auth_db.database_file.as_path(), &book_id).await {
        Ok(true) => {
            let mut response = StatusCode::ACCEPTED.into_response();
            mark_native(&mut response);
            response
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn book_metadata_update(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match persisted_book_exists(auth_db.database_file.as_path(), &book_id).await {
        Ok(true) => {
            let mut response = StatusCode::NO_CONTENT.into_response();
            mark_native(&mut response);
            response
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn book_metadata_batch_update(
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let mut response = StatusCode::NO_CONTENT.into_response();
    mark_native(&mut response);
    response
}

pub(in crate::app::compat_runtime) async fn books_import(headers: HeaderMap) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let mut response = StatusCode::ACCEPTED.into_response();
    mark_native(&mut response);
    response
}

pub(in crate::app::compat_runtime) async fn books_thumbnails_regenerate(
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let mut response = StatusCode::ACCEPTED.into_response();
    mark_native(&mut response);
    response
}

pub(in crate::app::compat_runtime) async fn readlist_thumbnail(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    match load_persisted_readlist_thumbnails(auth_db.database_file.as_path(), &readlist_id).await {
        Ok(rows) => {
            if let Some(thumbnail) = rows.first() {
                return (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, thumbnail.media_type.as_str()),
                        (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
                        (header::LAST_MODIFIED, LAST_MODIFIED),
                        (header::ETAG, THUMBNAIL_ETAG),
                    ],
                    thumbnail.thumbnail.clone(),
                )
                    .into_response();
            }

            if persisted_readlist_exists(auth_db.database_file.as_path(), &readlist_id)
                .await
                .unwrap_or(false)
            {
                return StatusCode::NOT_FOUND.into_response();
            }
        }
        Err(error) => return internal_error_response(error),
    }

    StatusCode::NOT_FOUND.into_response()
}

pub(in crate::app::compat_runtime) async fn readlist_thumbnails(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    match load_persisted_readlist_thumbnails(auth_db.database_file.as_path(), &readlist_id).await {
        Ok(rows) => {
            if !rows.is_empty() {
                return Json(
                    rows.into_iter()
                        .map(|row| {
                            json!({
                                "id": row.id,
                                "type": row.thumbnail_type,
                                "selected": row.selected,
                            })
                        })
                        .collect::<Vec<_>>(),
                )
                .into_response();
            }

            if persisted_readlist_exists(auth_db.database_file.as_path(), &readlist_id)
                .await
                .unwrap_or(false)
            {
                return Json(json!([])).into_response();
            }
        }
        Err(error) => return internal_error_response(error),
    }

    StatusCode::NOT_FOUND.into_response()
}

pub(in crate::app::compat_runtime) async fn readlist_thumbnail_by_id(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((readlist_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    match load_persisted_readlist_thumbnails(auth_db.database_file.as_path(), &readlist_id).await {
        Ok(rows) => {
            if let Some(thumbnail) = rows.into_iter().find(|row| row.id == thumbnail_id) {
                return (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, thumbnail.media_type.as_str()),
                        (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
                        (header::LAST_MODIFIED, LAST_MODIFIED),
                        (header::ETAG, THUMBNAIL_ETAG),
                    ],
                    thumbnail.thumbnail,
                )
                    .into_response();
            }

            if persisted_readlist_exists(auth_db.database_file.as_path(), &readlist_id)
                .await
                .unwrap_or(false)
            {
                return StatusCode::NOT_FOUND.into_response();
            }
        }
        Err(error) => return internal_error_response(error),
    }

    StatusCode::NOT_FOUND.into_response()
}

pub(in crate::app::compat_runtime) async fn readlist_thumbnail_upload(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match persisted_readlist_exists(auth_db.database_file.as_path(), &readlist_id).await {
        Ok(true) => StatusCode::OK.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn readlist_thumbnail_select(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((readlist_id, _thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match persisted_readlist_exists(auth_db.database_file.as_path(), &readlist_id).await {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn readlist_thumbnail_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((readlist_id, _thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match persisted_readlist_exists(auth_db.database_file.as_path(), &readlist_id).await {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn readlist_tachiyomi_read_progress_get(
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

pub(in crate::app::compat_runtime) async fn readlist_tachiyomi_read_progress_put(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    match persisted_readlist_exists(auth_db.database_file.as_path(), &readlist_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn readlist_file(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    match load_persisted_readlist_name(auth_db.database_file.as_path(), &readlist_id).await {
        Ok(Some(name)) => {
            let file_name = format!("{name}.zip");
            let content_disposition = attachment_disposition(&file_name);
            let body =
                match build_readlist_archive_payload(auth_db.database_file.as_path(), &readlist_id)
                    .await
                {
                    Ok(body) => body,
                    Err(error) => return internal_error_response(error),
                };

            return (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "application/zip"),
                    (header::CONTENT_DISPOSITION, content_disposition.as_str()),
                ],
                body,
            )
                .into_response();
        }
        Ok(None) => {}
        Err(error) => return internal_error_response(error),
    }

    StatusCode::NOT_FOUND.into_response()
}

pub(in crate::app::compat_runtime) async fn collection_thumbnail(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(collection_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !persisted_collection_exists(auth_db.database_file.as_path(), &collection_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    match load_persisted_collection_thumbnails(auth_db.database_file.as_path(), &collection_id)
        .await
    {
        Ok(rows) => {
            if let Some(thumbnail) = rows.first() {
                (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, thumbnail.media_type.as_str()),
                        (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
                        (header::LAST_MODIFIED, LAST_MODIFIED),
                        (header::ETAG, THUMBNAIL_ETAG),
                    ],
                    thumbnail.thumbnail.clone(),
                )
                    .into_response()
            } else {
                StatusCode::NOT_FOUND.into_response()
            }
        }
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn collection_thumbnails(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(collection_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !persisted_collection_exists(auth_db.database_file.as_path(), &collection_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    match load_persisted_collection_thumbnails(auth_db.database_file.as_path(), &collection_id)
        .await
    {
        Ok(rows) => Json(
            rows.into_iter()
                .map(|row| {
                    json!({
                        "id": row.id,
                        "type": row.thumbnail_type,
                        "selected": row.selected,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn collection_thumbnail_by_id(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((collection_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !persisted_collection_exists(auth_db.database_file.as_path(), &collection_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    match load_persisted_collection_thumbnails(auth_db.database_file.as_path(), &collection_id)
        .await
    {
        Ok(rows) => {
            if let Some(thumbnail) = rows.into_iter().find(|row| row.id == thumbnail_id) {
                (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, thumbnail.media_type.as_str()),
                        (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
                        (header::LAST_MODIFIED, LAST_MODIFIED),
                        (header::ETAG, THUMBNAIL_ETAG),
                    ],
                    thumbnail.thumbnail,
                )
                    .into_response()
            } else {
                StatusCode::NOT_FOUND.into_response()
            }
        }
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn collection_thumbnail_upload(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(collection_id): Path<String>,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    if !persisted_collection_exists(auth_db.database_file.as_path(), &collection_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let media_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("image/jpeg");
    if !media_type.starts_with("image/") && !media_type.starts_with("multipart/form-data") {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }

    if body.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "collection thumbnail upload body must not be empty",
            })),
        )
            .into_response();
    }

    let thumbnail_bytes = body.to_vec();

    match insert_collection_thumbnail(
        auth_db.database_file.as_path(),
        &collection_id,
        &thumbnail_bytes,
        media_type,
        true,
    )
    .await
    {
        Ok(thumbnail) => Json(json!({
            "id": thumbnail.id,
            "type": thumbnail.thumbnail_type,
            "selected": thumbnail.selected,
        }))
        .into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn collection_thumbnail_select(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((collection_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match select_collection_thumbnail(
        auth_db.database_file.as_path(),
        &collection_id,
        &thumbnail_id,
    )
    .await
    {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn collection_thumbnail_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((collection_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match delete_collection_thumbnail(
        auth_db.database_file.as_path(),
        &collection_id,
        &thumbnail_id,
    )
    .await
    {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn book_pages(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let resolved_book_id =
        resolve_snapshot_book_id_for_persisted(auth_db.database_file.as_path(), &book_id).await;

    let media =
        match load_persisted_book_media(auth_db.database_file.as_path(), &resolved_book_id).await {
            Ok(Some(media)) => media,
            Ok(None) => return non_native_response(StatusCode::NOT_FOUND.into_response()),
            Err(error) => return internal_error_response(error),
        };

    if let Some(user) = resolved_auth_user(&headers)
        && !user_can_access_library(&user, &media.library_id)
    {
        return non_native_response(StatusCode::FORBIDDEN.into_response());
    }

    if !book_media_supports_page_image(&media) {
        return non_native_response(StatusCode::NOT_FOUND.into_response());
    }

    let size_bytes = match fs::metadata(&media.file_path) {
        Ok(metadata) => metadata.len(),
        Err(_) => return non_native_response(StatusCode::NOT_FOUND.into_response()),
    };

    non_native_response(
        Json(vec![json!({
            "number": 1,
            "fileName": media.file_name,
            "mediaType": content_type_from_filename(&media.file_name, &media.media_type),
            "width": Value::Null,
            "height": Value::Null,
            "sizeBytes": size_bytes,
            "size": format_size_bytes(size_bytes),
        })])
        .into_response(),
    )
}

pub(in crate::app::compat_runtime) async fn series_thumbnail(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let resolved_series_id =
        resolve_snapshot_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;

    if let Ok(Some(media)) =
        load_persisted_series_thumbnail_media(auth_db.database_file.as_path(), &resolved_series_id)
            .await
        && let Ok(bytes) = fs::read(&media.file_path)
    {
        return non_native_response(
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "image/jpeg"),
                    (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
                    (header::LAST_MODIFIED, LAST_MODIFIED),
                    (header::ETAG, THUMBNAIL_ETAG),
                ],
                bytes,
            )
                .into_response(),
        );
    }

    non_native_response(StatusCode::NOT_FOUND.into_response())
}

pub(in crate::app::compat_runtime) async fn book_file(
    Extension(_profile): Extension<CompatProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if let Ok(Some(media)) =
        load_persisted_book_media(auth_db.database_file.as_path(), &book_id).await
        && let Ok(body) = fs::read(&media.file_path)
    {
        if let Some(user) = resolved_auth_user(&headers)
            && !user_can_access_library(&user, &media.library_id)
        {
            return non_native_response(StatusCode::FORBIDDEN.into_response());
        }

        let content_type = content_type_from_filename(&media.file_name, &media.media_type);
        let content_disposition = attachment_disposition(&media.file_name);

        if let Some((start, end)) = requested_byte_range(&headers, body.len()) {
            let mut response =
                (StatusCode::PARTIAL_CONTENT, body[start..=end].to_vec()).into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_str(&content_type)
                    .expect("book file content type should be valid"),
            );
            response.headers_mut().insert(
                header::CONTENT_DISPOSITION,
                HeaderValue::from_str(&content_disposition)
                    .expect("book file content disposition should be valid"),
            );
            response
                .headers_mut()
                .insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
            response.headers_mut().insert(
                header::CONTENT_RANGE,
                HeaderValue::from_str(&format!("bytes {start}-{end}/{}", body.len()))
                    .expect("book file content-range should be valid"),
            );

            return non_native_response(response);
        }

        return non_native_response(
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, content_type.as_str()),
                    (header::CONTENT_DISPOSITION, content_disposition.as_str()),
                ],
                body,
            )
                .into_response(),
        );
    }

    non_native_response(StatusCode::NOT_FOUND.into_response())
}

struct PersistedBookMedia {
    library_id: String,
    file_name: String,
    file_path: PathBuf,
    media_type: String,
}

struct PersistedBookThumbnailRow {
    id: String,
    thumbnail_type: String,
    selected: bool,
}

struct PersistedBookThumbnailBinary {
    media_type: String,
    thumbnail: Vec<u8>,
}

struct PersistedReadlistThumbnailRow {
    id: String,
    thumbnail_type: String,
    selected: bool,
    media_type: String,
    thumbnail: Vec<u8>,
}

struct PersistedCollectionThumbnailRow {
    id: String,
    thumbnail_type: String,
    selected: bool,
    media_type: String,
    thumbnail: Vec<u8>,
}

async fn load_persisted_book_media(
    database_file: &FsPath,
    book_id: &str,
) -> Result<Option<PersistedBookMedia>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book media db: {error}"))?;

    let row = sqlx::query(
        "SELECT b.LIBRARY_ID AS LIBRARY_ID, b.NAME AS FILE_NAME, l.ROOT AS LIBRARY_ROOT, COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE FROM BOOK b JOIN LIBRARY l ON l.ID = b.LIBRARY_ID LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID WHERE b.ID = ?",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted book media: {error}"))?;

    let media = row.map(|row| {
        let file_name = row.get::<String, _>("FILE_NAME");
        let library_root = row.get::<String, _>("LIBRARY_ROOT");

        PersistedBookMedia {
            library_id: row.get::<String, _>("LIBRARY_ID"),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            file_path: PathBuf::from(library_root).join(&file_name),
            file_name,
        }
    });

    pool.close().await;
    Ok(media)
}

async fn load_persisted_series_thumbnail_media(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Option<PersistedBookMedia>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series thumbnail db: {error}"))?;

    let row = sqlx::query(
        "SELECT b.NAME AS FILE_NAME, l.ROOT AS LIBRARY_ROOT, COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE FROM BOOK b JOIN LIBRARY l ON l.ID = b.LIBRARY_ID LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID WHERE b.SERIES_ID = ? AND b.DELETED_DATE IS NULL ORDER BY b.NUMBER ASC, b.ID ASC LIMIT 1",
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted series thumbnail media: {error}"))?;

    let media = row.map(|row| {
        let file_name = row.get::<String, _>("FILE_NAME");
        let library_root = row.get::<String, _>("LIBRARY_ROOT");

        PersistedBookMedia {
            library_id: String::new(),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            file_path: PathBuf::from(library_root).join(&file_name),
            file_name,
        }
    });

    pool.close().await;
    Ok(media)
}

async fn resolve_snapshot_series_id_for_persisted(
    database_file: &FsPath,
    requested_series_id: &str,
) -> String {
    let Some(index) = requested_series_id
        .strip_prefix("series-")
        .and_then(|value| value.parse::<usize>().ok())
    else {
        return requested_series_id.to_string();
    };

    if index == 0 {
        return requested_series_id.to_string();
    }

    if matches!(
        load_persisted_series_thumbnail_media(database_file, requested_series_id).await,
        Ok(Some(_))
    ) {
        return requested_series_id.to_string();
    }

    match load_series_id_by_sorted_position(database_file, index).await {
        Ok(Some(series_id)) => series_id,
        _ => requested_series_id.to_string(),
    }
}

async fn resolve_snapshot_book_id_for_persisted(
    database_file: &FsPath,
    requested_book_id: &str,
) -> String {
    let Some(index) = requested_book_id
        .strip_prefix("book-")
        .and_then(|value| value.parse::<usize>().ok())
    else {
        return requested_book_id.to_string();
    };

    if index == 0 {
        return requested_book_id.to_string();
    }

    if matches!(
        load_persisted_book_media(database_file, requested_book_id).await,
        Ok(Some(_))
    ) {
        return requested_book_id.to_string();
    }

    match load_book_id_by_sorted_position(database_file, index).await {
        Ok(Some(book_id)) => book_id,
        _ => requested_book_id.to_string(),
    }
}

async fn load_series_id_by_sorted_position(
    database_file: &FsPath,
    index: usize,
) -> Result<Option<String>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series-id remap db: {error}"))?;

    let row = sqlx::query(
        "SELECT s.ID AS ID FROM SERIES s LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID WHERE s.DELETED_DATE IS NULL ORDER BY COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) COLLATE NOCASE ASC, s.ID ASC LIMIT 1 OFFSET ?",
    )
    .bind((index - 1) as i64)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query remapped series id: {error}"))?;

    pool.close().await;
    Ok(row.map(|row| row.get::<String, _>("ID")))
}

async fn load_book_id_by_sorted_position(
    database_file: &FsPath,
    index: usize,
) -> Result<Option<String>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book-id remap db: {error}"))?;

    let row = sqlx::query(
        "SELECT b.ID AS ID FROM BOOK b LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID WHERE b.DELETED_DATE IS NULL ORDER BY COALESCE(bm.TITLE, b.NAME) COLLATE NOCASE ASC, b.ID ASC LIMIT 1 OFFSET ?",
    )
    .bind((index - 1) as i64)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query remapped book id: {error}"))?;

    pool.close().await;
    Ok(row.map(|row| row.get::<String, _>("ID")))
}

async fn persisted_book_exists(database_file: &FsPath, book_id: &str) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book-exists db: {error}"))?;

    let row = sqlx::query("SELECT 1 AS FOUND FROM BOOK WHERE ID = ? LIMIT 1")
        .bind(book_id)
        .fetch_optional(&pool)
        .await
        .map_err(|error| format!("query persisted book existence: {error}"))?;

    pool.close().await;
    Ok(row.is_some())
}

async fn persist_read_progress(
    database_file: &FsPath,
    book_id: &str,
    user_id_value: &str,
    page: u64,
    completed: bool,
) -> Result<(), String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open read-progress db: {error}"))?;

    sqlx::query(
        "INSERT OR IGNORE INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES) VALUES (?, ?, ?, ?)",
    )
    .bind(user_id_value)
    .bind(format!("{user_id_value}@compat.local"))
    .bind("compat-password")
    .bind(true)
    .execute(&pool)
    .await
    .map_err(|error| format!("ensure read-progress user row: {error}"))?;

    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) VALUES (?, ?, ?, ?) ON CONFLICT(BOOK_ID, USER_ID) DO UPDATE SET PAGE = excluded.PAGE, COMPLETED = excluded.COMPLETED, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP",
    )
    .bind(book_id)
    .bind(user_id_value)
    .bind(page as i64)
    .bind(completed)
    .execute(&pool)
    .await
    .map_err(|error| format!("persist read-progress: {error}"))?;

    pool.close().await;
    Ok(())
}

async fn delete_persisted_read_progress(
    database_file: &FsPath,
    book_id: &str,
    user_id_value: &str,
) -> Result<(), String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open read-progress delete db: {error}"))?;

    sqlx::query("DELETE FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ?")
        .bind(book_id)
        .bind(user_id_value)
        .execute(&pool)
        .await
        .map_err(|error| format!("delete read-progress: {error}"))?;

    pool.close().await;
    Ok(())
}

async fn load_persisted_book_thumbnails(
    database_file: &FsPath,
    book_id: &str,
) -> Result<Vec<PersistedBookThumbnailRow>, String> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book thumbnails db: {error}"))?;

    let rows = sqlx::query(
        "SELECT ID, TYPE, SELECTED FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? ORDER BY SELECTED DESC, LAST_MODIFIED_DATE DESC, ID ASC",
    )
    .bind(book_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted book thumbnails: {error}"))?;

    let thumbnails = rows
        .into_iter()
        .map(|row| PersistedBookThumbnailRow {
            id: row.get::<String, _>("ID"),
            thumbnail_type: row.get::<String, _>("TYPE"),
            selected: row.get::<i64, _>("SELECTED") != 0,
        })
        .collect::<Vec<_>>();

    pool.close().await;
    Ok(thumbnails)
}

async fn load_selected_book_thumbnail(
    database_file: &FsPath,
    book_id: &str,
) -> Result<Option<PersistedBookThumbnailBinary>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open selected book thumbnail db: {error}"))?;

    let row = sqlx::query(
        "SELECT MEDIA_TYPE, THUMBNAIL FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? ORDER BY SELECTED DESC, LAST_MODIFIED_DATE DESC, ID ASC LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query selected book thumbnail: {error}"))?;

    pool.close().await;

    Ok(row.map(|row| PersistedBookThumbnailBinary {
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        thumbnail: row.get::<Vec<u8>, _>("THUMBNAIL"),
    }))
}

async fn load_persisted_readlist_thumbnails(
    database_file: &FsPath,
    readlist_id: &str,
) -> Result<Vec<PersistedReadlistThumbnailRow>, String> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open readlist thumbnails db: {error}"))?;

    let rows = sqlx::query(
        "SELECT ID, TYPE, SELECTED, MEDIA_TYPE, THUMBNAIL FROM THUMBNAIL_READLIST WHERE READLIST_ID = ? ORDER BY SELECTED DESC, LAST_MODIFIED_DATE DESC, ID ASC",
    )
    .bind(readlist_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted readlist thumbnails: {error}"))?;

    let thumbnails = rows
        .into_iter()
        .map(|row| PersistedReadlistThumbnailRow {
            id: row.get::<String, _>("ID"),
            thumbnail_type: row.get::<String, _>("TYPE"),
            selected: row.get::<i64, _>("SELECTED") != 0,
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            thumbnail: row.get::<Vec<u8>, _>("THUMBNAIL"),
        })
        .collect::<Vec<_>>();

    pool.close().await;
    Ok(thumbnails)
}

async fn load_persisted_readlist_name(
    database_file: &FsPath,
    readlist_id: &str,
) -> Result<Option<String>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open readlist file db: {error}"))?;

    let row = sqlx::query("SELECT NAME FROM READLIST WHERE ID = ?")
        .bind(readlist_id)
        .fetch_optional(&pool)
        .await
        .map_err(|error| format!("query persisted readlist name: {error}"))?;

    let name = row.map(|row| row.get::<String, _>("NAME"));
    pool.close().await;
    Ok(name)
}

async fn persisted_readlist_exists(
    database_file: &FsPath,
    readlist_id: &str,
) -> Result<bool, String> {
    Ok(load_persisted_readlist_name(database_file, readlist_id)
        .await?
        .is_some())
}

async fn persisted_collection_exists(
    database_file: &FsPath,
    collection_id: &str,
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open collection exists db: {error}"))?;

    let row = sqlx::query("SELECT 1 AS FOUND FROM COLLECTION WHERE ID = ? LIMIT 1")
        .bind(collection_id)
        .fetch_optional(&pool)
        .await
        .map_err(|error| format!("query persisted collection existence: {error}"))?;

    pool.close().await;
    Ok(row.is_some())
}

async fn load_persisted_collection_thumbnails(
    database_file: &FsPath,
    collection_id: &str,
) -> Result<Vec<PersistedCollectionThumbnailRow>, String> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open collection thumbnails db: {error}"))?;

    let rows = sqlx::query(
        "SELECT ID, TYPE, SELECTED, MEDIA_TYPE, THUMBNAIL FROM THUMBNAIL_COLLECTION WHERE COLLECTION_ID = ? ORDER BY SELECTED DESC, LAST_MODIFIED_DATE DESC, ID ASC",
    )
    .bind(collection_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted collection thumbnails: {error}"))?;

    let thumbnails = rows
        .into_iter()
        .map(|row| PersistedCollectionThumbnailRow {
            id: row.get::<String, _>("ID"),
            thumbnail_type: row.get::<String, _>("TYPE"),
            selected: row.get::<i64, _>("SELECTED") != 0,
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            thumbnail: row.get::<Vec<u8>, _>("THUMBNAIL"),
        })
        .collect::<Vec<_>>();

    pool.close().await;
    Ok(thumbnails)
}

async fn insert_collection_thumbnail(
    database_file: &FsPath,
    collection_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    selected: bool,
) -> Result<PersistedCollectionThumbnailRow, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open collection thumbnail create db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin collection thumbnail create tx: {error}"))?;

    let exists = sqlx::query("SELECT 1 AS FOUND FROM COLLECTION WHERE ID = ? LIMIT 1")
        .bind(collection_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| format!("query collection existence for thumbnail create: {error}"))?
        .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback collection thumbnail create tx: {error}"))?;
        pool.close().await;
        return Err("collection does not exist".to_string());
    }

    if selected {
        sqlx::query("UPDATE THUMBNAIL_COLLECTION SET SELECTED = 0 WHERE COLLECTION_ID = ?")
            .bind(collection_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("clear selected collection thumbnails: {error}"))?;
    }

    let id = generated_collection_thumbnail_id();
    sqlx::query(
        "INSERT INTO THUMBNAIL_COLLECTION (ID, SELECTED, THUMBNAIL, TYPE, COLLECTION_ID, MEDIA_TYPE, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(selected)
    .bind(thumbnail)
    .bind("USER_UPLOADED")
    .bind(collection_id)
    .bind(media_type)
    .bind(thumbnail.len() as i64)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("insert collection thumbnail: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit collection thumbnail create tx: {error}"))?;
    pool.close().await;

    Ok(PersistedCollectionThumbnailRow {
        id,
        thumbnail_type: "USER_UPLOADED".to_string(),
        selected,
        media_type: media_type.to_string(),
        thumbnail: thumbnail.to_vec(),
    })
}

async fn select_collection_thumbnail(
    database_file: &FsPath,
    collection_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open collection thumbnail select db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin collection thumbnail select tx: {error}"))?;

    let exists = sqlx::query("SELECT 1 AS FOUND FROM COLLECTION WHERE ID = ? LIMIT 1")
        .bind(collection_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| format!("query collection existence for thumbnail select: {error}"))?
        .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback collection thumbnail select tx: {error}"))?;
        pool.close().await;
        return Ok(false);
    }

    let target_exists = sqlx::query(
        "SELECT 1 AS FOUND FROM THUMBNAIL_COLLECTION WHERE ID = ? AND COLLECTION_ID = ? LIMIT 1",
    )
    .bind(thumbnail_id)
    .bind(collection_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query target collection thumbnail for select: {error}"))?
    .is_some();
    if !target_exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback collection thumbnail select tx: {error}"))?;
        pool.close().await;
        return Ok(false);
    }

    sqlx::query("UPDATE THUMBNAIL_COLLECTION SET SELECTED = 0 WHERE COLLECTION_ID = ?")
        .bind(collection_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("clear selected collection thumbnails for select: {error}"))?;
    sqlx::query("UPDATE THUMBNAIL_COLLECTION SET SELECTED = 1 WHERE ID = ? AND COLLECTION_ID = ?")
        .bind(thumbnail_id)
        .bind(collection_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("mark selected collection thumbnail: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit collection thumbnail select tx: {error}"))?;
    pool.close().await;
    Ok(true)
}

async fn delete_collection_thumbnail(
    database_file: &FsPath,
    collection_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open collection thumbnail delete db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin collection thumbnail delete tx: {error}"))?;

    let exists = sqlx::query("SELECT 1 AS FOUND FROM COLLECTION WHERE ID = ? LIMIT 1")
        .bind(collection_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| format!("query collection existence for thumbnail delete: {error}"))?
        .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback collection thumbnail delete tx: {error}"))?;
        pool.close().await;
        return Ok(false);
    }

    let deleted =
        sqlx::query("DELETE FROM THUMBNAIL_COLLECTION WHERE ID = ? AND COLLECTION_ID = ?")
            .bind(thumbnail_id)
            .bind(collection_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("delete collection thumbnail: {error}"))?
            .rows_affected()
            > 0;

    if !deleted {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback collection thumbnail delete tx: {error}"))?;
        pool.close().await;
        return Ok(false);
    }

    tx.commit()
        .await
        .map_err(|error| format!("commit collection thumbnail delete tx: {error}"))?;
    pool.close().await;
    Ok(true)
}

fn generated_collection_thumbnail_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    format!("thumbnail-collection-{nanos}")
}

fn internal_error_response(error: impl std::fmt::Display) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": error.to_string() })),
    )
        .into_response()
}

fn content_type_from_filename(file_name: &str, fallback: &str) -> String {
    let extension = file_name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();

    match extension.as_str() {
        "cbz" => "application/vnd.comicbook+zip".to_string(),
        "cbr" => "application/vnd.comicbook-rar".to_string(),
        "pdf" => "application/pdf".to_string(),
        "epub" => "application/epub+zip".to_string(),
        "jpg" | "jpeg" => "image/jpeg".to_string(),
        "png" => "image/png".to_string(),
        _ => fallback.to_string(),
    }
}

fn book_media_supports_page_image(media: &PersistedBookMedia) -> bool {
    content_type_from_filename(&media.file_name, &media.media_type).starts_with("image/")
}

fn attachment_disposition(file_name: &str) -> String {
    format!("attachment; filename=\"=?UTF-8?Q?{file_name}?=\"; filename*=UTF-8''{file_name}",)
}

fn format_size_bytes(size_bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];

    if size_bytes < 1024 {
        return format!("{size_bytes} B");
    }

    let mut size = size_bytes as f64;
    let mut unit_index = 0usize;
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if (size - size.round()).abs() < 0.05 {
        format!("{} {}", size.round() as u64, UNITS[unit_index])
    } else {
        format!("{size:.1} {}", UNITS[unit_index])
    }
}

fn requested_byte_range(headers: &HeaderMap, total_len: usize) -> Option<(usize, usize)> {
    let range = headers.get(header::RANGE)?.to_str().ok()?;
    let bytes_range = range.strip_prefix("bytes=")?;
    let (start, end) = bytes_range.split_once('-')?;

    if start.is_empty() {
        return None;
    }

    let start = start.parse::<usize>().ok()?;
    let end = if end.is_empty() {
        total_len.checked_sub(1)?
    } else {
        end.parse::<usize>().ok()?
    };

    if start > end || end >= total_len {
        return None;
    }

    Some((start, end))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ManifestProfile {
    Epub,
    Pdf,
    Divina,
}

#[derive(Clone, Copy)]
enum ManifestVariant {
    Default,
    Epub,
    Pdf,
    Divina,
}

enum ManifestBuildOutcome {
    Found(&'static str, Value),
    NotFound,
    Forbidden,
}

struct PersistedManifestBook {
    library_id: String,
    title: String,
    media_type: String,
}

fn user_can_access_library(
    user: &crate::app::placeholder_auth::PlaceholderUser,
    library_id: &str,
) -> bool {
    user_shared_all_libraries(user)
        || user_shared_library_ids(user)
            .iter()
            .any(|shared_library_id| shared_library_id == library_id)
}

async fn load_persisted_manifest_book(
    database_file: &FsPath,
    book_id: &str,
) -> Result<Option<PersistedManifestBook>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open manifest book db: {error}"))?;

    let row = sqlx::query(
        "SELECT b.LIBRARY_ID AS LIBRARY_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE, b.NAME AS FILE_NAME, COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE FROM BOOK b LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID WHERE b.ID = ?",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted manifest book: {error}"))?;

    pool.close().await;

    Ok(row.map(|row| {
        let file_name = row.get::<String, _>("FILE_NAME");
        let media_type = row.get::<String, _>("MEDIA_TYPE");

        PersistedManifestBook {
            library_id: row.get::<String, _>("LIBRARY_ID"),
            title: row.get::<String, _>("TITLE"),
            media_type: content_type_from_filename(&file_name, &media_type),
        }
    }))
}

fn manifest_profile_from_media_type(media_type: &str) -> ManifestProfile {
    if media_type == "application/epub+zip" {
        ManifestProfile::Epub
    } else if media_type == "application/pdf" {
        ManifestProfile::Pdf
    } else {
        ManifestProfile::Divina
    }
}

fn manifest_content_type(profile: ManifestProfile) -> &'static str {
    match profile {
        ManifestProfile::Divina => "application/divina+json",
        ManifestProfile::Epub | ManifestProfile::Pdf => "application/webpub+json",
    }
}

fn manifest_variant_matches_profile(variant: ManifestVariant, profile: ManifestProfile) -> bool {
    match variant {
        ManifestVariant::Default => true,
        ManifestVariant::Epub => profile == ManifestProfile::Epub,
        ManifestVariant::Pdf => profile == ManifestProfile::Pdf,
        ManifestVariant::Divina => profile == ManifestProfile::Divina,
    }
}

fn persisted_manifest_payload(host: &str, book_id: &str, title: &str, media_type: &str) -> Value {
    json!({
        "context": "https://readium.org/webpub-manifest/context.jsonld",
        "metadata": {
            "title": title,
        },
        "links": [
            {
                "rel": "self",
                "href": format!("http://{host}/api/v1/books/{book_id}/manifest"),
                "type": "application/webpub+json",
            },
            {
                "rel": "http://opds-spec.org/acquisition",
                "href": format!("http://{host}/api/v1/books/{book_id}/file"),
                "type": media_type,
            }
        ],
        "readingOrder": [
            {
                "href": format!("http://{host}/api/v1/books/{book_id}/pages/1?contentNegotiation=false"),
                "type": media_type,
            }
        ],
        "resources": [
            {
                "href": format!("http://{host}/api/v1/books/{book_id}/thumbnail"),
                "type": "image/jpeg",
            }
        ],
        "toc": [],
        "landmarks": [],
        "pageList": [],
    })
}

async fn build_persisted_book_manifest(
    database_file: &FsPath,
    headers: &HeaderMap,
    book_id: &str,
    variant: ManifestVariant,
) -> Result<ManifestBuildOutcome, String> {
    let Some(user) = resolved_auth_user(headers) else {
        return Ok(ManifestBuildOutcome::NotFound);
    };

    let Some(book) = load_persisted_manifest_book(database_file, book_id).await? else {
        return Ok(ManifestBuildOutcome::NotFound);
    };

    if !user_can_access_library(&user, &book.library_id) {
        return Ok(ManifestBuildOutcome::Forbidden);
    }

    let profile = manifest_profile_from_media_type(&book.media_type);
    if !manifest_variant_matches_profile(variant, profile) {
        return Ok(ManifestBuildOutcome::NotFound);
    }

    let host = request_host(headers);
    let payload = persisted_manifest_payload(&host, book_id, &book.title, &book.media_type);
    Ok(ManifestBuildOutcome::Found(
        manifest_content_type(profile),
        payload,
    ))
}

async fn build_readlist_archive_payload(
    database_file: &FsPath,
    readlist_id: &str,
) -> Result<Vec<u8>, String> {
    let entries = load_readlist_archive_entries(database_file, readlist_id).await?;
    if entries.is_empty() {
        return Ok(vec![]);
    }

    let mut payload = Vec::new();
    for (file_name, file_path) in entries {
        if let Ok(bytes) = fs::read(&file_path) {
            payload.extend_from_slice(format!("FILE:{file_name}\n").as_bytes());
            payload.extend_from_slice(&bytes);
            payload.extend_from_slice(b"\n");
        }
    }

    Ok(payload)
}

async fn load_readlist_archive_entries(
    database_file: &FsPath,
    readlist_id: &str,
) -> Result<Vec<(String, PathBuf)>, String> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open readlist archive db: {error}"))?;

    let rows = sqlx::query(
        "SELECT b.NAME AS FILE_NAME, l.ROOT AS LIBRARY_ROOT FROM READLIST_BOOK rb JOIN BOOK b ON b.ID = rb.BOOK_ID JOIN LIBRARY l ON l.ID = b.LIBRARY_ID WHERE rb.READLIST_ID = ? ORDER BY rb.NUMBER ASC",
    )
    .bind(readlist_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query readlist archive entries: {error}"))?;

    pool.close().await;

    Ok(rows
        .into_iter()
        .map(|row| {
            let file_name = row.get::<String, _>("FILE_NAME");
            let library_root = row.get::<String, _>("LIBRARY_ROOT");
            let file_path = PathBuf::from(library_root).join(&file_name);
            (file_name, file_path)
        })
        .collect())
}

async fn readlist_tachiyomi_counters(
    database_file: &FsPath,
    readlist_id: &str,
    user_id_value: &str,
) -> Result<Option<(u64, u64, u64, u64, u64)>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open readlist tachiyomi db: {error}"))?;

    let exists = sqlx::query("SELECT 1 AS FOUND FROM READLIST WHERE ID = ? LIMIT 1")
        .bind(readlist_id)
        .fetch_optional(&pool)
        .await
        .map_err(|error| format!("query readlist exists for tachiyomi counters: {error}"))?
        .is_some();

    if !exists {
        pool.close().await;
        return Ok(None);
    }

    let rows = sqlx::query(
        "SELECT rb.NUMBER AS ORDINAL, COALESCE(rp.PAGE, 0) AS PAGE, COALESCE(rp.COMPLETED, 0) AS COMPLETED FROM READLIST_BOOK rb LEFT JOIN READ_PROGRESS rp ON rp.BOOK_ID = rb.BOOK_ID AND rp.USER_ID = ? WHERE rb.READLIST_ID = ? ORDER BY rb.NUMBER ASC",
    )
    .bind(user_id_value)
    .bind(readlist_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query readlist tachiyomi counters: {error}"))?;

    pool.close().await;

    let books_count = rows.len() as u64;
    let books_read_count = rows
        .iter()
        .filter(|row| row.get::<i64, _>("COMPLETED") != 0)
        .count() as u64;
    let books_in_progress_count = rows
        .iter()
        .filter(|row| row.get::<i64, _>("COMPLETED") == 0 && row.get::<i64, _>("PAGE") > 0)
        .count() as u64;
    let books_unread_count = books_count.saturating_sub(books_read_count + books_in_progress_count);

    let mut last_read_continuous_index = 0_u64;
    for row in rows {
        if row.get::<i64, _>("COMPLETED") != 0 {
            last_read_continuous_index += 1;
        } else {
            break;
        }
    }

    Ok(Some((
        books_count,
        books_read_count,
        books_unread_count,
        books_in_progress_count,
        last_read_continuous_index,
    )))
}

pub(in crate::app::compat_runtime) async fn book_manifest(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    match build_persisted_book_manifest(
        auth_db.database_file.as_path(),
        &headers,
        &book_id,
        ManifestVariant::Default,
    )
    .await
    {
        Ok(ManifestBuildOutcome::Found(content_type, payload)) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, content_type)],
            Json(payload),
        )
            .into_response(),
        Ok(ManifestBuildOutcome::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Ok(ManifestBuildOutcome::Forbidden) => StatusCode::FORBIDDEN.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn book_manifest_epub(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    match build_persisted_book_manifest(
        auth_db.database_file.as_path(),
        &headers,
        &book_id,
        ManifestVariant::Epub,
    )
    .await
    {
        Ok(ManifestBuildOutcome::Found(content_type, payload)) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, content_type)],
            Json(payload),
        )
            .into_response(),
        Ok(ManifestBuildOutcome::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Ok(ManifestBuildOutcome::Forbidden) => StatusCode::FORBIDDEN.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn book_manifest_pdf(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    match build_persisted_book_manifest(
        auth_db.database_file.as_path(),
        &headers,
        &book_id,
        ManifestVariant::Pdf,
    )
    .await
    {
        Ok(ManifestBuildOutcome::Found(content_type, payload)) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, content_type)],
            Json(payload),
        )
            .into_response(),
        Ok(ManifestBuildOutcome::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Ok(ManifestBuildOutcome::Forbidden) => StatusCode::FORBIDDEN.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn book_manifest_divina(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    match build_persisted_book_manifest(
        auth_db.database_file.as_path(),
        &headers,
        &book_id,
        ManifestVariant::Divina,
    )
    .await
    {
        Ok(ManifestBuildOutcome::Found(content_type, payload)) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, content_type)],
            Json(payload),
        )
            .into_response(),
        Ok(ManifestBuildOutcome::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Ok(ManifestBuildOutcome::Forbidden) => StatusCode::FORBIDDEN.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn book_read_progress(
    Extension(_profile): Extension<CompatProfile>,
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
        return non_native_response(StatusCode::NOT_FOUND.into_response());
    }

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return non_native_response(invalid_read_progress_payload());
    };

    let persisted_user_id = resolved_auth_user(&headers).map(|user| user_id(&user).to_string());

    let token = resolved_token(&headers);

    if payload.get("completed").and_then(|value| value.as_bool()) == Some(true) {
        if let Some(user_id) = persisted_user_id.as_deref() {
            if persist_read_progress(auth_db.database_file.as_path(), &book_id, user_id, 10, true)
                .await
                .is_err()
            {
                return non_native_response(StatusCode::INTERNAL_SERVER_ERROR.into_response());
            }
        }
        set_read_progress(&state, token, book_id, 10, true);
        return non_native_response(StatusCode::NO_CONTENT.into_response());
    }

    if let Some(page) = payload.get("page").and_then(|value| value.as_u64())
        && (1..=10).contains(&page)
    {
        if let Some(user_id) = persisted_user_id.as_deref() {
            if persist_read_progress(
                auth_db.database_file.as_path(),
                &book_id,
                user_id,
                page,
                false,
            )
            .await
            .is_err()
            {
                return non_native_response(StatusCode::INTERNAL_SERVER_ERROR.into_response());
            }
        }
        set_read_progress(&state, token, book_id, page, false);
        return non_native_response(StatusCode::NO_CONTENT.into_response());
    }

    non_native_response(invalid_read_progress_payload())
}

pub(in crate::app::compat_runtime) async fn book_read_progress_get(
    Extension(_profile): Extension<CompatProfile>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let path = format!("/api/v1/books/{book_id}/read-progress");
    non_native_response(method_not_allowed_json_response(&path))
}

pub(in crate::app::compat_runtime) async fn book_read_progress_delete(
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
        return non_native_response(StatusCode::NOT_FOUND.into_response());
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
        return non_native_response(StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }

    non_native_response(StatusCode::NO_CONTENT.into_response())
}

pub(in crate::app::compat_runtime) async fn book_file_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match persisted_book_exists(auth_db.database_file.as_path(), &book_id).await {
        Ok(true) => {
            let mut response = StatusCode::ACCEPTED.into_response();
            mark_native(&mut response);
            response
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn book_progression(
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
        return non_native_response(StatusCode::NOT_FOUND.into_response());
    }

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return non_native_response(invalid_progression_payload());
    };

    let progression = payload
        .get("locator")
        .and_then(|value| value.get("locations"))
        .and_then(|value| value.get("progression"))
        .and_then(|value| value.as_f64());

    if progression.is_some() {
        non_native_response(StatusCode::NO_CONTENT.into_response())
    } else {
        non_native_response(invalid_progression_payload())
    }
}

pub(in crate::app::compat_runtime) async fn book_progression_get(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(_profile): Extension<CompatProfile>,
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
        return non_native_response(StatusCode::NOT_FOUND.into_response());
    }

    non_native_response(StatusCode::NO_CONTENT.into_response())
}

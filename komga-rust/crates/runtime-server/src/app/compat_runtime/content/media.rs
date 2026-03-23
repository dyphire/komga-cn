use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::app::CompatProfile;
use crate::app::placeholder_auth::{require_auth, resolved_auth_user, resolved_token};
use crate::app::snapshots::book_pages_json;

use super::super::{
    CACHE_CONTROL_PRIVATE, LAST_MODIFIED, PAGE_BODY, PDF_BODY, ReadProgressState, THUMBNAIL_BODY,
    THUMBNAIL_ETAG,
};
use super::content_java_live;
use super::helpers::{
    invalid_progression_payload, invalid_read_progress_payload, method_not_allowed_json_response,
    non_native_response, set_read_progress,
};

pub(in crate::app::compat_runtime) async fn book_page(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

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

    if profile == CompatProfile::JavaLiveLocaldb {
        return non_native_response((
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::LAST_MODIFIED, LAST_MODIFIED),
                (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
                (
                    header::CONTENT_DISPOSITION,
                    "inline; filename=\"=?UTF-8?Q?book.cbr-1.png?=\"; filename*=UTF-8''book.cbr-1.png",
                ),
            ],
            PAGE_BODY,
        )
            .into_response());
    }

    non_native_response((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/pdf"),
            (header::LAST_MODIFIED, LAST_MODIFIED),
            (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
            (
                header::CONTENT_DISPOSITION,
                "inline; filename=\"=?UTF-8?Q?book.pdf-1.pdf?=\"; filename*=UTF-8''book.pdf-1.pdf",
            ),
        ],
        PDF_BODY,
    )
        .into_response())
}

pub(in crate::app::compat_runtime) async fn book_page_thumbnail(
    Extension(_profile): Extension<CompatProfile>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

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

    non_native_response(
        (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
                (header::LAST_MODIFIED, LAST_MODIFIED),
                (header::ETAG, THUMBNAIL_ETAG),
            ],
            THUMBNAIL_BODY,
        )
            .into_response(),
    )
}

pub(in crate::app::compat_runtime) async fn book_thumbnail(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        return non_native_response(StatusCode::NOT_FOUND.into_response());
    }

    non_native_response(StatusCode::NOT_FOUND.into_response())
}

pub(in crate::app::compat_runtime) async fn readlist_thumbnail(
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !is_seeded_readlist_id(&readlist_id) {
        return StatusCode::NOT_FOUND.into_response();
    }

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/jpeg"),
            (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
            (header::LAST_MODIFIED, LAST_MODIFIED),
            (header::ETAG, THUMBNAIL_ETAG),
        ],
        THUMBNAIL_BODY,
    )
        .into_response()
}

pub(in crate::app::compat_runtime) async fn readlist_thumbnails(
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !is_seeded_readlist_id(&readlist_id) {
        return StatusCode::NOT_FOUND.into_response();
    }

    Json(json!([
        {
            "id": "thumbnail-1",
            "type": "SIDECAR",
            "selected": true,
        }
    ]))
    .into_response()
}

pub(in crate::app::compat_runtime) async fn readlist_thumbnail_by_id(
    headers: HeaderMap,
    Path((readlist_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !is_seeded_readlist_id(&readlist_id) {
        return StatusCode::NOT_FOUND.into_response();
    }

    if thumbnail_id != "thumbnail-1" {
        return StatusCode::NOT_FOUND.into_response();
    }

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/jpeg"),
            (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
            (header::LAST_MODIFIED, LAST_MODIFIED),
            (header::ETAG, THUMBNAIL_ETAG),
        ],
        THUMBNAIL_BODY,
    )
        .into_response()
}

pub(in crate::app::compat_runtime) async fn readlist_thumbnail_upload(
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !is_seeded_readlist_id(&readlist_id) {
        return StatusCode::NOT_FOUND.into_response();
    }

    StatusCode::OK.into_response()
}

pub(in crate::app::compat_runtime) async fn readlist_thumbnail_select(
    headers: HeaderMap,
    Path((readlist_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !is_seeded_readlist_id(&readlist_id) || thumbnail_id != "thumbnail-1" {
        return StatusCode::NOT_FOUND.into_response();
    }

    StatusCode::OK.into_response()
}

pub(in crate::app::compat_runtime) async fn readlist_thumbnail_delete(
    headers: HeaderMap,
    Path((readlist_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !is_seeded_readlist_id(&readlist_id) || thumbnail_id != "thumbnail-1" {
        return StatusCode::NOT_FOUND.into_response();
    }

    StatusCode::OK.into_response()
}

pub(in crate::app::compat_runtime) async fn readlist_tachiyomi_read_progress_get(
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some((books_count, books_read_count)) = readlist_tachiyomi_counters(&readlist_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let books_unread_count = books_count.saturating_sub(books_read_count);
    Json(json!({
        "booksCount": books_count,
        "booksReadCount": books_read_count,
        "booksUnreadCount": books_unread_count,
        "booksInProgressCount": 0,
        "lastReadContinuousIndex": books_read_count,
    }))
    .into_response()
}

pub(in crate::app::compat_runtime) async fn readlist_tachiyomi_read_progress_put(
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !is_seeded_readlist_id(&readlist_id) {
        return StatusCode::NOT_FOUND.into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

pub(in crate::app::compat_runtime) async fn readlist_file(
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !is_seeded_readlist_id(&readlist_id) {
        return StatusCode::NOT_FOUND.into_response();
    }

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/zip"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"=?UTF-8?Q?ReadList.zip?=\"; filename*=UTF-8''ReadList.zip",
            ),
        ],
        PAGE_BODY,
    )
        .into_response()
}

pub(in crate::app::compat_runtime) async fn book_pages(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        let user = resolved_auth_user(&headers)
            .expect("authorized book pages request should resolve user");
        return match content_java_live::fetch_json(user, "/api/v1/books/book-1/pages", "book pages")
            .await
        {
            Ok(pages) => non_native_response(Json(pages).into_response()),
            Err(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": message })),
            )
                .into_response(),
        };
    }

    non_native_response(Json(book_pages_json(profile)).into_response())
}

fn is_seeded_readlist_id(readlist_id: &str) -> bool {
    matches!(readlist_id, "readlist-1" | "readlist-2" | "readlist-3")
}

fn readlist_tachiyomi_counters(readlist_id: &str) -> Option<(u64, u64)> {
    match readlist_id {
        "readlist-1" => Some((5, 0)),
        "readlist-2" => Some((2, 1)),
        "readlist-3" => Some((1, 0)),
        _ => None,
    }
}

pub(in crate::app::compat_runtime) async fn book_file(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        return non_native_response(
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "application/zip"),
                    (
                        header::CONTENT_DISPOSITION,
                        "attachment; filename=\"=?UTF-8?Q?book.cbr?=\"; filename*=UTF-8''book.cbr",
                    ),
                ],
                PAGE_BODY,
            )
                .into_response(),
        );
    }

    non_native_response(
        (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/pdf"),
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"=?UTF-8?Q?book.pdf?=\"; filename*=UTF-8''book.pdf",
                ),
            ],
            PDF_BODY,
        )
            .into_response(),
    )
}

pub(in crate::app::compat_runtime) async fn book_read_progress(
    Extension(_profile): Extension<CompatProfile>,
    Extension(state): Extension<ReadProgressState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    body: Bytes,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if book_id != "book-1" {
        return non_native_response(StatusCode::NOT_FOUND.into_response());
    }

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return non_native_response(invalid_read_progress_payload());
    };

    let token = resolved_token(&headers);

    if payload.get("completed").and_then(|value| value.as_bool()) == Some(true) {
        set_read_progress(&state, token, book_id, 10, true);
        return non_native_response(StatusCode::NO_CONTENT.into_response());
    }

    if let Some(page) = payload.get("page").and_then(|value| value.as_u64())
        && (1..=10).contains(&page)
    {
        set_read_progress(&state, token, book_id, page, false);
        return non_native_response(StatusCode::NO_CONTENT.into_response());
    }

    non_native_response(invalid_read_progress_payload())
}

pub(in crate::app::compat_runtime) async fn book_read_progress_get(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let path = format!("/api/v1/books/{book_id}/read-progress");

    if profile == CompatProfile::JavaLiveLocaldb {
        let user =
            resolved_auth_user(&headers).expect("authorized read-progress GET should resolve user");
        return match content_java_live::fetch_text_response(user, &path, "book read-progress").await
        {
            Ok(response) => non_native_response(response),
            Err(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": message })),
            )
                .into_response(),
        };
    }

    non_native_response(method_not_allowed_json_response(&path))
}

pub(in crate::app::compat_runtime) async fn book_read_progress_delete(
    Extension(state): Extension<ReadProgressState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if book_id != "book-1" {
        return non_native_response(StatusCode::NOT_FOUND.into_response());
    }

    let token = resolved_token(&headers);
    let mut all_progress = state
        .progress_by_token
        .lock()
        .expect("read-progress state lock should not be poisoned");

    if let Some(user_progress) = all_progress.get_mut(&token) {
        user_progress.remove(&book_id);
    }

    non_native_response(StatusCode::NO_CONTENT.into_response())
}

pub(in crate::app::compat_runtime) async fn book_progression(
    headers: HeaderMap,
    Path(book_id): Path<String>,
    body: Bytes,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if book_id != "book-1" {
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
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if book_id != "book-1" {
        return non_native_response(StatusCode::NOT_FOUND.into_response());
    }

    let path = format!("/api/v1/books/{book_id}/progression");

    if profile == CompatProfile::JavaLiveLocaldb {
        let user =
            resolved_auth_user(&headers).expect("authorized progression GET should resolve user");
        return match content_java_live::fetch_text_response(user, &path, "book progression").await {
            Ok(response) => non_native_response(response),
            Err(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": message })),
            )
                .into_response(),
        };
    }

    non_native_response(StatusCode::NO_CONTENT.into_response())
}

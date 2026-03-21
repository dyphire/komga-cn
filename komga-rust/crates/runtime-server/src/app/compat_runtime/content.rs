use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::app::CompatProfile;
use crate::app::placeholder_auth::{
    require_auth, resolved_auth_user, resolved_token,
};
use crate::app::snapshots::{
    book_pages_json, books_latest_json, snapshot_json,
};

#[path = "content_libraries.rs"]
mod content_libraries;
#[path = "content_auth.rs"]
mod content_auth;
#[path = "content_opds.rs"]
mod content_opds;
#[path = "content_java_live.rs"]
mod content_java_live;

use super::{
    CACHE_CONTROL_PRIVATE, LAST_MODIFIED, PAGE_BODY, PDF_BODY, ReadProgress, ReadProgressState,
    SEARCH_OWNERSHIP_HEADER, SHADOW_JAVA_WRITER_MARKER, THUMBNAIL_BODY, THUMBNAIL_ETAG,
};

pub(super) async fn libraries(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
) -> Response {
    content_libraries::response(profile, headers).await
}

pub(super) async fn series(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        let user =
            resolved_auth_user(&headers).expect("authorized series request should resolve user");
        let path = uri.path_and_query().map_or(uri.path(), |value| value.as_str());
        return match content_java_live::fetch_json(user, path, "series").await {
            Ok(series) => Json(series).into_response(),
            Err(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": message })),
            )
                .into_response(),
        };
    }

    Json(series_json_for_request(profile, &uri, None)).into_response()
}

pub(super) async fn series_list(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let payload = serde_json::from_slice::<Value>(&body).ok();
    let full_text_search = payload.as_ref().and_then(|payload| {
        payload
            .get("fullTextSearch")
            .and_then(|value| value.as_str())
            .map(str::to_owned)
    });
    let ownership = payload
        .as_ref()
        .and_then(|payload| payload.get("ownership"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_ascii_lowercase());

    let requested_shadow_marker = headers
        .get(SEARCH_OWNERSHIP_HEADER)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == SHADOW_JAVA_WRITER_MARKER);

    let is_shadow_ownership = ownership
        .as_deref()
        .is_some_and(|value| value == "shadow");

    let mut response = Json(series_json_for_request(profile, &uri, full_text_search)).into_response();

    if is_shadow_ownership || requested_shadow_marker {
        response.headers_mut().insert(
            HeaderName::from_static(SEARCH_OWNERSHIP_HEADER),
            HeaderValue::from_static(SHADOW_JAVA_WRITER_MARKER),
        );
    }

    response
}

pub(super) async fn books(
    Extension(profile): Extension<CompatProfile>,
    Extension(state): Extension<ReadProgressState>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let token = resolved_token(&headers);
    let mut books = if profile == CompatProfile::JavaLiveLocaldb {
        let user = resolved_auth_user(&headers).expect("authorized books request should resolve user");
        match content_java_live::fetch_json(user, "/api/v1/books", "books").await {
            Ok(books) => books,
            Err(message) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": message })),
                )
                    .into_response();
            }
        }
    } else {
        snapshot_json("books-list.json", profile)
    };
    let read_progress = state
        .progress_by_token
        .lock()
        .expect("read-progress state lock should not be poisoned")
        .get(&token)
        .and_then(|books| books.get("book-1"))
        .cloned();

    overlay_book_read_progress(&mut books, read_progress);

    Json(books).into_response()
}

pub(super) async fn books_latest(
    Extension(profile): Extension<CompatProfile>,
    Extension(state): Extension<ReadProgressState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let token = resolved_token(&headers);
    let read_progress = state
        .progress_by_token
        .lock()
        .expect("read-progress state lock should not be poisoned")
        .get(&token)
        .and_then(|books| books.get("book-1"))
        .cloned();

    let mut books = if profile == CompatProfile::JavaLiveLocaldb {
        let user =
            resolved_auth_user(&headers).expect("authorized books-latest request should resolve user");
        let path = uri.path_and_query().map_or(uri.path(), |value| value.as_str());
        match content_java_live::fetch_json(user, path, "books latest").await {
            Ok(books) => books,
            Err(message) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": message })),
                )
                    .into_response();
            }
        }
    } else {
        books_latest_json(profile)
    };

    overlay_book_read_progress(&mut books, read_progress);

    Json(books).into_response()
}

pub(super) async fn users_me(headers: HeaderMap, uri: Uri) -> Response {
    content_auth::users_me(headers, uri).await
}

pub(super) async fn login_set_cookie(headers: HeaderMap) -> Response {
    content_auth::login_set_cookie(headers).await
}

pub(super) async fn book_page(
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
        return (
            StatusCode::NOT_MODIFIED,
            [
                (header::LAST_MODIFIED, LAST_MODIFIED),
                (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
            ],
        )
            .into_response();
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        return (
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
            .into_response();
    }

    (
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
        .into_response()
}

pub(super) async fn book_page_thumbnail(
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
        return (
            StatusCode::NOT_MODIFIED,
            [
                (header::LAST_MODIFIED, LAST_MODIFIED),
                (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
            ],
        )
            .into_response();
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

pub(super) async fn book_thumbnail(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        return StatusCode::NOT_FOUND.into_response();
    }

    StatusCode::NOT_FOUND.into_response()
}

pub(super) async fn book_pages(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        let user =
            resolved_auth_user(&headers).expect("authorized book pages request should resolve user");
        return match content_java_live::fetch_json(user, "/api/v1/books/book-1/pages", "book pages")
            .await
        {
            Ok(pages) => Json(pages).into_response(),
            Err(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": message })),
            )
                .into_response(),
        };
    }

    Json(book_pages_json(profile)).into_response()
}

pub(super) async fn book_file(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        return (
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
            .into_response();
    }

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
        .into_response()
}

pub(super) async fn book_read_progress(
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
        return StatusCode::NOT_FOUND.into_response();
    }

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return invalid_read_progress_payload();
    };

    let token = resolved_token(&headers);

    if payload.get("completed").and_then(|value| value.as_bool()) == Some(true) {
        set_read_progress(&state, token, book_id, 10, true);
        return StatusCode::NO_CONTENT.into_response();
    }

    if let Some(page) = payload.get("page").and_then(|value| value.as_u64())
        && (1..=10).contains(&page)
    {
        set_read_progress(&state, token, book_id, page, false);
        return StatusCode::NO_CONTENT.into_response();
    }

    invalid_read_progress_payload()
}

pub(super) async fn book_read_progress_get(
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
        return match content_java_live::fetch_text_response(user, &path, "book read-progress")
            .await
        {
            Ok(response) => response,
            Err(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": message })),
            )
                .into_response(),
        };
    }

    method_not_allowed_json_response(&path)
}

pub(super) async fn book_read_progress_delete(
    Extension(state): Extension<ReadProgressState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if book_id != "book-1" {
        return StatusCode::NOT_FOUND.into_response();
    }

    let token = resolved_token(&headers);
    let mut all_progress = state
        .progress_by_token
        .lock()
        .expect("read-progress state lock should not be poisoned");

    if let Some(user_progress) = all_progress.get_mut(&token) {
        user_progress.remove(&book_id);
    }

    StatusCode::NO_CONTENT.into_response()
}

pub(super) async fn book_progression(
    headers: HeaderMap,
    Path(book_id): Path<String>,
    body: Bytes,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if book_id != "book-1" {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return invalid_progression_payload();
    };

    let progression = payload
        .get("locator")
        .and_then(|value| value.get("locations"))
        .and_then(|value| value.get("progression"))
        .and_then(|value| value.as_f64());

    if progression.is_some() {
        StatusCode::NO_CONTENT.into_response()
    } else {
        invalid_progression_payload()
    }
}

pub(super) async fn book_progression_get(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if book_id != "book-1" {
        return StatusCode::NOT_FOUND.into_response();
    }

    let path = format!("/api/v1/books/{book_id}/progression");

    if profile == CompatProfile::JavaLiveLocaldb {
        let user =
            resolved_auth_user(&headers).expect("authorized progression GET should resolve user");
        return match content_java_live::fetch_text_response(user, &path, "book progression").await {
            Ok(response) => response,
            Err(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": message })),
            )
                .into_response(),
        };
    }

    StatusCode::NO_CONTENT.into_response()
}

pub(super) async fn opds_manifest(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
) -> Response {
    content_opds::opds_manifest(profile, headers).await
}

pub(super) async fn opds_auth(headers: HeaderMap) -> Response {
    content_opds::opds_auth(headers).await
}

pub(super) async fn opds_catalog(headers: HeaderMap) -> Response {
    content_opds::opds_catalog(headers).await
}

pub(super) async fn opds_v1_series(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
) -> Response {
    content_opds::opds_v1_series(profile, headers).await
}

fn series_json_for_request(profile: CompatProfile, uri: &Uri, full_text_search: Option<String>) -> Value {
    let mut series = snapshot_json("series-list.json", profile);
    let query = uri.query().unwrap_or_default();

    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let sort = query_value(query, "sort");
    let search_term = query_value(query, "search")
        .map(str::to_owned)
        .or(full_text_search);
    let search_regex = query_value(query, "search_regex").and_then(parse_search_regex);
    let has_author_filters = query_has_key(query, "authors") || query_has_key(query, "author");

    let mut filtered_content = series
        .pointer("/content")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();

    if has_author_filters {
        filtered_content.clear();
    }

    if let Some(term) = search_term {
        let normalized = term.to_ascii_lowercase();
        if !normalized.trim().is_empty() && !"series".contains(normalized.trim()) {
            filtered_content.clear();
        }
    }

    if let Some((pattern, field)) = search_regex {
        let candidate = if field == "title_sort" {
            "series"
        } else {
            "series"
        };
        if !matches_search_pattern(candidate, &pattern) {
            filtered_content.clear();
        }
    }

    let total_elements = filtered_content.len();
    let start = page.saturating_mul(size);
    let end = start.saturating_add(size).min(total_elements);
    let page_content = if start >= total_elements {
        Vec::new()
    } else {
        filtered_content[start..end].to_vec()
    };
    let number_of_elements = page_content.len();
    let total_pages = if total_elements == 0 {
        0
    } else {
        total_elements.div_ceil(size)
    };
    let first = page == 0;
    let last = total_pages == 0 || page + 1 >= total_pages;
    let empty = number_of_elements == 0;
    let sorted = sort.is_some();

    series["content"] = Value::Array(page_content);
    series["number"] = Value::Number((page as u64).into());
    series["size"] = Value::Number((size as u64).into());
    series["first"] = Value::Bool(first);
    series["last"] = Value::Bool(last);
    series["empty"] = Value::Bool(empty);
    series["numberOfElements"] = Value::Number((number_of_elements as u64).into());
    series["totalElements"] = Value::Number((total_elements as u64).into());
    series["totalPages"] = Value::Number((total_pages as u64).into());
    series["pageable"]["pageNumber"] = Value::Number((page as u64).into());
    series["pageable"]["pageSize"] = Value::Number((size as u64).into());
    series["pageable"]["offset"] = Value::Number((start as u64).into());
    series["sort"]["empty"] = Value::Bool(!sorted);
    series["sort"]["sorted"] = Value::Bool(sorted);
    series["sort"]["unsorted"] = Value::Bool(!sorted);
    series["pageable"]["sort"]["empty"] = Value::Bool(!sorted);
    series["pageable"]["sort"]["sorted"] = Value::Bool(sorted);
    series["pageable"]["sort"]["unsorted"] = Value::Bool(!sorted);

    series
}

fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let name = parts.next().unwrap_or_default();
        if name != key {
            return None;
        }
        Some(parts.next().unwrap_or_default())
    })
}

fn query_has_key(query: &str, key: &str) -> bool {
    query
        .split('&')
        .any(|pair| pair.split('=').next().unwrap_or_default() == key)
}

fn parse_search_regex(value: &str) -> Option<(String, String)> {
    let mut parts = value.splitn(2, ',');
    let pattern = parts.next()?.trim();
    let field = parts.next()?.trim().to_ascii_lowercase();
    if pattern.is_empty() || (field != "title" && field != "title_sort") {
        return None;
    }
    Some((pattern.to_string(), field))
}

fn matches_search_pattern(candidate: &str, pattern: &str) -> bool {
    let text = candidate.to_ascii_lowercase();
    let mut expected = pattern.to_ascii_lowercase();

    let anchored_start = expected.starts_with('^');
    let anchored_end = expected.ends_with('$');
    if anchored_start {
        expected.remove(0);
    }
    if anchored_end {
        expected.pop();
    }

    if anchored_start && anchored_end {
        text == expected
    } else if anchored_start {
        text.starts_with(&expected)
    } else if anchored_end {
        text.ends_with(&expected)
    } else {
        text.contains(&expected)
    }
}

fn invalid_read_progress_payload() -> Response {
    (
        StatusCode::BAD_REQUEST,
        [(header::CONTENT_TYPE, "application/json")],
        Json(json!({
            "error": "invalid read progress payload",
        })),
    )
        .into_response()
}

fn invalid_progression_payload() -> Response {
    (
        StatusCode::BAD_REQUEST,
        [(header::CONTENT_TYPE, "application/json")],
        Json(json!({
            "error": "invalid progression payload",
        })),
    )
        .into_response()
}

fn method_not_allowed_json_response(path: &str) -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        [(header::CONTENT_TYPE, "application/json")],
        Json(json!({
            "error": "Method Not Allowed",
            "message": "Method 'GET' is not supported.",
            "path": path,
            "status": 405,
            "timestamp": "1970-01-01T00:00:00.000+00:00",
            "trace": "org.springframework.web.HttpRequestMethodNotSupportedException: Request method 'GET' is not supported",
        })),
    )
        .into_response()
}

fn set_read_progress(
    state: &ReadProgressState,
    token: String,
    book_id: String,
    page: u64,
    completed: bool,
) {
    let mut all_progress = state
        .progress_by_token
        .lock()
        .expect("read-progress state lock should not be poisoned");

    let user_progress = all_progress.entry(token).or_default();
    user_progress.insert(book_id, ReadProgress { page, completed });
}

fn overlay_book_read_progress(books: &mut Value, read_progress: Option<ReadProgress>) {
    if let Some(slot) = books.pointer_mut("/content/0/readProgress") {
        *slot = match read_progress {
            Some(read_progress) => json!({
                "page": read_progress.page,
                "completed": read_progress.completed,
            }),
            None => Value::Null,
        };
    }
}

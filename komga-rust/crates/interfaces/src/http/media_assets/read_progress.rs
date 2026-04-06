use super::*;
use crate::http::helpers::read_progress_validation_error_response;
use crate::opds_persisted_access::load_readlist_books;
use crate::runtime_identity_access::load_read_progress;
use flate2::read::GzDecoder;
use std::io::Read;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

fn decode_epub_extension_positions_and_layout(blob: &[u8]) -> Result<(Vec<Value>, bool), String> {
    let mut decoder = GzDecoder::new(blob);
    let mut json = String::new();
    decoder
        .read_to_string(&mut json)
        .map_err(|error| format!("decode epub extension blob: {error}"))?;
    let payload = serde_json::from_str::<Value>(&json)
        .map_err(|error| format!("parse epub extension blob json: {error}"))?;
    let positions = payload
        .get("positions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let is_fixed_layout = payload
        .get("isFixedLayout")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok((positions, is_fixed_layout))
}

fn progression_bad_request(message: impl Into<String>) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": message.into() })),
    )
        .into_response()
}

fn progression_locator(payload: &Value) -> Option<&Value> {
    payload.get("locator")
}

fn locator_progression(locator: &Value) -> Option<f64> {
    locator
        .get("locations")
        .and_then(|value| value.get("progression"))
        .and_then(Value::as_f64)
}

fn locator_position(locator: &Value) -> Option<u64> {
    locator
        .get("locations")
        .and_then(|value| value.get("position"))
        .and_then(Value::as_u64)
}

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

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hi = bytes[index + 1] as char;
            let lo = bytes[index + 2] as char;
            let parsed = hi
                .to_digit(16)
                .and_then(|hi| lo.to_digit(16).map(|lo| ((hi << 4) | lo) as u8));
            if let Some(byte) = parsed {
                decoded.push(byte);
                index += 3;
                continue;
            }
        }
        if bytes[index] == b'+' {
            decoded.push(b' ');
        } else {
            decoded.push(bytes[index]);
        }
        index += 1;
    }
    String::from_utf8(decoded).unwrap_or_else(|_| value.to_string())
}

fn normalized_href_base(href: &str) -> String {
    let base = href.split('#').next().unwrap_or(href).trim_end_matches('#');
    percent_decode(base).trim_start_matches('/').to_string()
}

fn position_progression(position: &Value) -> Option<f64> {
    position
        .get("locations")
        .and_then(|value| value.get("progression"))
        .and_then(Value::as_f64)
}

fn position_number(position: &Value) -> Option<i64> {
    position
        .get("locations")
        .and_then(|value| value.get("position"))
        .and_then(Value::as_i64)
}

fn position_matches_href(position: &Value, href_base: &str) -> bool {
    position
        .get("href")
        .and_then(Value::as_str)
        .map(|value| normalized_href_base(value) == href_base)
        .unwrap_or(false)
}

fn matched_epub_position(
    positions: &[Value],
    href_base: &str,
    locator_progression: f64,
    is_fixed_layout: bool,
) -> Option<Value> {
    let matching_positions = positions
        .iter()
        .filter(|position| position_matches_href(position, href_base))
        .cloned()
        .collect::<Vec<_>>();

    matching_positions
        .iter()
        .find(|position| position_progression(position) == Some(locator_progression))
        .cloned()
        .or_else(|| {
            if is_fixed_layout && matching_positions.len() == 1 {
                return matching_positions.first().cloned();
            }

            let before = matching_positions
                .iter()
                .filter(|position| {
                    position_progression(position).is_some_and(|value| value < locator_progression)
                })
                .max_by_key(|position| position_number(position))
                .cloned();
            let after = matching_positions
                .iter()
                .filter(|position| {
                    position_progression(position).is_some_and(|value| value > locator_progression)
                })
                .min_by_key(|position| position_number(position))
                .cloned();

            match (before, after) {
                (Some(before), Some(_)) => Some(before),
                _ => None,
            }
        })
}

fn normalized_epub_locator(locator: &Value, matched_position: &Value) -> Value {
    let mut locator = locator.clone();
    let Some(locator_map) = locator.as_object_mut() else {
        return locator;
    };

    locator_map.insert(
        "type".to_string(),
        matched_position
            .get("type")
            .cloned()
            .unwrap_or_else(|| Value::String(String::new())),
    );

    let current_kobo_span_missing = locator_map.get("koboSpan").is_none_or(Value::is_null);
    if current_kobo_span_missing && let Some(kobo_span) = matched_position.get("koboSpan").cloned()
    {
        locator_map.insert("koboSpan".to_string(), kobo_span);
    }

    if let Some(locations) = locator_map
        .get_mut("locations")
        .and_then(Value::as_object_mut)
        && let Some(total_progression) = matched_position
            .get("locations")
            .and_then(|value| value.get("totalProgression"))
            .cloned()
    {
        locations.insert("totalProgression".to_string(), total_progression);
    }

    locator
}

pub(crate) async fn normalize_book_epub_locator(
    database_file: &FsPath,
    book_id: &str,
    locator: &Value,
) -> Result<Value, Response> {
    let href_base = normalized_href_base(
        locator
            .get("href")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    if href_base.is_empty() {
        return Err(progression_bad_request("Resource does not exist in book: "));
    }

    let Some(locator_progression) = locator_progression(locator) else {
        return Err(progression_bad_request("location.progression is required"));
    };

    let persisted_media_files = match load_persisted_book_media_files(database_file, book_id).await
    {
        Ok(files) => files,
        Err(error) => return Err(internal_error_response(error)),
    };
    let persisted_resource_exists = (!persisted_media_files.is_empty()).then(|| {
        persisted_media_files
            .iter()
            .any(|file_name| normalized_href_base(file_name) == href_base)
    });
    if persisted_resource_exists == Some(false) {
        return Err(progression_bad_request(format!(
            "Resource does not exist in book: {href_base}"
        )));
    }

    let extension = match load_persisted_epub_extension_blob(database_file, book_id).await {
        Ok(extension) => extension,
        Err(error) => return Err(internal_error_response(error)),
    };
    let Some((_class, blob)) = extension else {
        return Err(progression_bad_request("Epub extension not found"));
    };
    let (positions, is_fixed_layout) = match decode_epub_extension_positions_and_layout(&blob) {
        Ok(decoded) => decoded,
        Err(error) => return Err(internal_error_response(error)),
    };

    if persisted_resource_exists.is_none()
        && !positions
            .iter()
            .any(|position| position_matches_href(position, href_base.as_str()))
    {
        return Err(progression_bad_request(format!(
            "Resource does not exist in book: {href_base}"
        )));
    }

    let Some(matched_position) = matched_epub_position(
        &positions,
        href_base.as_str(),
        locator_progression,
        is_fixed_layout,
    ) else {
        return Err(progression_bad_request("Invalid progression"));
    };

    Ok(normalized_epub_locator(locator, &matched_position))
}

pub(crate) async fn progression_is_older_than_existing(
    database_file: &FsPath,
    book_id: &str,
    user_id: &str,
    modified: &str,
) -> Result<bool, String> {
    let Ok(new_modified) = OffsetDateTime::parse(modified, &Rfc3339) else {
        return Ok(false);
    };
    let Some(existing_progression) = load_book_progression(database_file, book_id, user_id).await?
    else {
        return Ok(false);
    };
    let Some(existing_modified) = existing_progression.get("modified").and_then(Value::as_str)
    else {
        return Ok(false);
    };
    let Ok(existing_modified) = OffsetDateTime::parse(existing_modified, &Rfc3339) else {
        return Ok(false);
    };

    Ok(new_modified <= existing_modified)
}

async fn load_epub_locator_for_page(
    database_file: &FsPath,
    book_id: &str,
    page: u64,
) -> Result<Option<Value>, String> {
    match load_persisted_epub_extension_blob(database_file, book_id).await {
        Ok(Some((_class, blob))) => Ok(decode_epub_positions(&blob)
            .ok()
            .and_then(|positions| positions.get(page.saturating_sub(1) as usize).cloned())),
        Ok(None) => Ok(None),
        Err(error) => Err(error),
    }
}

async fn persist_and_record_read_progress(
    database_file: &FsPath,
    state: &ReadProgressState,
    token: &str,
    book_id: &str,
    persisted_user_id: Option<&str>,
    page: u64,
    completed: bool,
    locator: Option<Value>,
) -> Response {
    if let Some(user_id) = persisted_user_id
        && persist_read_progress(database_file, book_id, user_id, page, completed, locator)
            .await
            .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    set_read_progress(state, token.to_string(), book_id.to_string());
    StatusCode::NO_CONTENT.into_response()
}

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
            None,
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
    let unrestricted_all_libraries = user_shared_all_libraries(&user)
        && principal_from_user_payload(&user_payload_json(&user))
            .is_none_or(|principal| !principal.restrictions.is_restricted());
    if unrestricted_all_libraries {
        if !persisted_series_exists(auth_db.database_file.as_path(), &resolved_series_id)
            .await
            .unwrap_or(false)
        {
            return StatusCode::NO_CONTENT.into_response();
        }
    } else {
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
    let unrestricted_all_libraries = user_shared_all_libraries(&user)
        && principal_from_user_payload(&user_payload_json(&user))
            .is_none_or(|principal| !principal.restrictions.is_restricted());
    if unrestricted_all_libraries {
        if !persisted_series_exists(auth_db.database_file.as_path(), &resolved_series_id)
            .await
            .unwrap_or(false)
        {
            return StatusCode::NO_CONTENT.into_response();
        }
    } else {
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

    let book_numbers =
        match load_series_book_number_sorts(auth_db.database_file.as_path(), &resolved_series_id)
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
            None,
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
            auth_db.database_file.as_path(),
            &state,
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

    let locator =
        match load_epub_locator_for_page(auth_db.database_file.as_path(), &book_id, page).await {
            Ok(locator) => locator,
            Err(error) => return internal_error_response(error),
        };

    persist_and_record_read_progress(
        auth_db.database_file.as_path(),
        &state,
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
        let normalized_locator =
            match normalize_book_epub_locator(auth_db.database_file.as_path(), &book_id, locator)
                .await
            {
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

    let stale_progression = match progression_is_older_than_existing(
        auth_db.database_file.as_path(),
        &book_id,
        user_id(&user),
        modified,
    )
    .await
    {
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

    match persist_book_progression(
        auth_db.database_file.as_path(),
        &book_id,
        user_id(&user),
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
        Ok(Some(progression)) => Json(progression).into_response(),
        Ok(None) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => internal_error_response(error),
    }
}

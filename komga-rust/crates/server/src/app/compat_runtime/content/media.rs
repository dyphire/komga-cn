use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path as FsPath, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path, Query};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use flate2::read::GzDecoder;
use image::ImageFormat;
use komga_persistence::sqlite::connect_pool;
use lopdf::Document as PdfDocument;
use quick_xml::Reader as XmlReader;
use quick_xml::events::Event as XmlEvent;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use zip::ZipArchive;

use crate::app::CompatProfile;
use crate::app::compat_runtime::AuthDatabaseState;
use crate::app::discovery_auth::principal_from_user_payload;
use crate::app::runtime_auth::{
    AuthUser, require_admin, require_auth, require_file_download, resolved_auth_user,
    resolved_token, user_has_role, user_id, user_is_admin, user_payload_json,
    user_shared_all_libraries, user_shared_library_ids,
};
use crate::app::snapshots::app_absolute_url;
use crate::task_queue::TaskQueueRecord;

use super::super::{
    CACHE_CONTROL_PRIVATE, LAST_MODIFIED, OperationalState, ReadProgressState, THUMBNAIL_ETAG,
};
use super::helpers::{
    invalid_progression_payload, invalid_read_progress_payload, mark_native,
    method_not_allowed_json_response, set_read_progress,
};

pub(in crate::app::compat_runtime) async fn book_page(
    Extension(_profile): Extension<CompatProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Query(query): Query<BookPageQuery>,
    Path((book_id, page_number)): Path<(String, u32)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let resolved_book_id =
        resolve_snapshot_book_id_for_persisted(auth_db.database_file.as_path(), &book_id).await;

    let requested_page_number = if query.zero_based {
        page_number.saturating_add(1)
    } else {
        page_number
    };
    let requested_convert = query
        .convert
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let content_negotiation = query.content_negotiation;

    if let Some(requested_convert) = requested_convert
        && !matches!(requested_convert, "jpeg" | "png")
    {
        return StatusCode::BAD_REQUEST.into_response();
    }

    if let Ok(Some(media)) =
        load_persisted_book_media(auth_db.database_file.as_path(), &resolved_book_id).await
    {
        if !book_media_is_ready_status(auth_db.database_file.as_path(), &resolved_book_id)
            .await
            .unwrap_or(false)
        {
            return StatusCode::NOT_FOUND.into_response();
        }

        if let Some(user) = resolved_auth_user(&headers) {
            if !user_is_admin(&user) && !user_has_role(&user, "PAGE_STREAMING") {
                return StatusCode::FORBIDDEN.into_response();
            }
            if !user_can_access_book_media(
                auth_db.database_file.as_path(),
                &resolved_book_id,
                &user,
                &media,
            )
            .await
            {
                return StatusCode::FORBIDDEN.into_response();
            }
        }

        if !book_media_supports_page_api(&media) {
            return StatusCode::NOT_FOUND.into_response();
        }

        if book_media_is_pdf(&media) && content_negotiation && accept_header_prefers_pdf(&headers) {
            if requested_page_number == 0 {
                return StatusCode::BAD_REQUEST.into_response();
            }
            let page_count = detect_pdf_page_count(&media).unwrap_or(media.page_count);
            if requested_page_number as u64 > page_count {
                return StatusCode::BAD_REQUEST.into_response();
            }
            if let Some(bytes) =
                read_pdf_page_as_single_page_pdf(&media, requested_page_number as u64)
            {
                return (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, "application/pdf"),
                        (header::LAST_MODIFIED, LAST_MODIFIED),
                        (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
                    ],
                    bytes,
                )
                    .into_response();
            }
            return StatusCode::NOT_FOUND.into_response();
        }

        let page_row = match load_persisted_book_page_row(
            auth_db.database_file.as_path(),
            &resolved_book_id,
            requested_page_number as u64,
        )
        .await
        {
            Ok(Some(row)) => row,
            Ok(None) if book_media_is_single_image(&media) && requested_page_number == 1 => {
                PersistedBookPageRow {
                    number: requested_page_number as u64,
                    file_name: media.file_name.clone(),
                    media_type: content_type_from_filename(&media.file_name, &media.media_type),
                    width: None,
                    height: None,
                    file_size: fs::metadata(&media.file_path)
                        .ok()
                        .map(|value| value.len() as i64)
                        .unwrap_or(0),
                }
            }
            Ok(None) => {
                if let Some(row) = load_archive_page_row(&media, requested_page_number as u64) {
                    row
                } else if let Some(row) = load_pdf_page_row(&media, requested_page_number as u64) {
                    row
                } else {
                    return StatusCode::NOT_FOUND.into_response();
                }
            }
            Err(error) => return internal_error_response(error),
        };

        if let Some(bytes) =
            resolve_book_page_bytes(&media, &page_row, requested_page_number as u64)
        {
            let mut effective_bytes = bytes;
            let content_type = if page_row.media_type.is_empty() {
                content_type_from_filename(&page_row.file_name, &media.media_type)
            } else {
                page_row.media_type
            };

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

            let mut effective_content_type = content_type;
            if let Some(requested_convert) = requested_convert {
                let target_content_type = match requested_convert {
                    "jpeg" => "image/jpeg",
                    "png" => "image/png",
                    _ => unreachable!("validated convert query should be jpeg|png"),
                };

                let Some(converted) = convert_page_image_bytes(
                    &effective_bytes,
                    &effective_content_type,
                    target_content_type,
                ) else {
                    return StatusCode::NOT_FOUND.into_response();
                };
                effective_bytes = converted;
                effective_content_type = target_content_type.to_string();
            }

            return (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, effective_content_type.as_str()),
                    (header::LAST_MODIFIED, LAST_MODIFIED),
                    (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
                ],
                effective_bytes,
            )
                .into_response();
        }
    }

    StatusCode::NOT_FOUND.into_response()
}

pub(in crate::app::compat_runtime) async fn book_page_raw(
    Extension(_profile): Extension<CompatProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((book_id, page_number)): Path<(String, u32)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let resolved_book_id =
        resolve_snapshot_book_id_for_persisted(auth_db.database_file.as_path(), &book_id).await;

    if let Ok(Some(media)) =
        load_persisted_book_media(auth_db.database_file.as_path(), &resolved_book_id).await
    {
        if !book_media_is_ready_status(auth_db.database_file.as_path(), &resolved_book_id)
            .await
            .unwrap_or(false)
        {
            return StatusCode::NOT_FOUND.into_response();
        }

        if !book_media_is_pdf(&media) {
            return StatusCode::BAD_REQUEST.into_response();
        }

        if let Some(user) = resolved_auth_user(&headers) {
            if !user_is_admin(&user) && !user_has_role(&user, "PAGE_STREAMING") {
                return StatusCode::FORBIDDEN.into_response();
            }
            if !user_can_access_book_media(
                auth_db.database_file.as_path(),
                &resolved_book_id,
                &user,
                &media,
            )
            .await
            {
                return StatusCode::FORBIDDEN.into_response();
            }
        }

        let page_count = detect_pdf_page_count(&media).unwrap_or(media.page_count);
        if page_number == 0 || page_number as u64 > page_count {
            return StatusCode::BAD_REQUEST.into_response();
        }

        if let Some(bytes) = read_pdf_page_as_single_page_pdf(&media, page_number as u64) {
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

            return (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "application/pdf"),
                    (header::LAST_MODIFIED, LAST_MODIFIED),
                    (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
                ],
                bytes,
            )
                .into_response();
        }
    }

    StatusCode::NOT_FOUND.into_response()
}

#[derive(Deserialize, Default)]
pub(in crate::app::compat_runtime) struct BookPageQuery {
    #[serde(default)]
    convert: Option<String>,

    #[serde(default)]
    zero_based: bool,

    #[serde(default = "book_page_content_negotiation_default")]
    #[serde(rename = "contentNegotiation")]
    content_negotiation: bool,
}

fn book_page_content_negotiation_default() -> bool {
    true
}

fn accept_header_prefers_pdf(headers: &HeaderMap) -> bool {
    let Some(raw) = headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };

    #[derive(Clone, Copy)]
    struct Candidate {
        rank: i32,
        quality: f32,
        is_pdf: bool,
    }

    fn parse_quality(params: &str) -> f32 {
        for part in params.split(';') {
            let part = part.trim();
            if let Some(value) = part.strip_prefix("q=")
                && let Ok(parsed) = value.parse::<f32>()
            {
                return parsed.clamp(0.0, 1.0);
            }
        }
        1.0
    }

    let mut best: Option<Candidate> = None;
    for entry in raw.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }

        let mut parts = entry.split(';');
        let media_type = parts
            .next()
            .map(str::trim)
            .unwrap_or_default()
            .to_ascii_lowercase();
        let params = parts.collect::<Vec<_>>().join(";");
        let quality = parse_quality(&params);
        if quality <= 0.0 {
            continue;
        }

        let candidate = if media_type == "application/pdf" {
            Some(Candidate {
                rank: 3,
                quality,
                is_pdf: true,
            })
        } else if media_type.starts_with("image/") && media_type != "image/*" {
            Some(Candidate {
                rank: 3,
                quality,
                is_pdf: false,
            })
        } else if media_type == "image/*" {
            Some(Candidate {
                rank: 2,
                quality,
                is_pdf: false,
            })
        } else if media_type == "*/*" {
            Some(Candidate {
                rank: 1,
                quality,
                is_pdf: false,
            })
        } else {
            None
        };

        let Some(candidate) = candidate else {
            continue;
        };
        let replace = match best {
            None => true,
            Some(current) => {
                candidate.rank > current.rank
                    || (candidate.rank == current.rank && candidate.quality > current.quality)
            }
        };
        if replace {
            best = Some(candidate);
        }
    }

    best.map(|candidate| candidate.is_pdf).unwrap_or(false)
}

fn convert_page_image_bytes(
    bytes: &[u8],
    source_content_type: &str,
    target_content_type: &str,
) -> Option<Vec<u8>> {
    if source_content_type.eq_ignore_ascii_case(target_content_type) {
        return Some(bytes.to_vec());
    }

    if !source_content_type
        .to_ascii_lowercase()
        .starts_with("image/")
    {
        return None;
    }

    let source = image::load_from_memory(bytes).ok()?;
    let mut output = std::io::Cursor::new(Vec::new());
    let target_format = match target_content_type {
        "image/jpeg" => ImageFormat::Jpeg,
        "image/png" => ImageFormat::Png,
        _ => return None,
    };
    source.write_to(&mut output, target_format).ok()?;
    Some(output.into_inner())
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

    let resolved_book_id =
        resolve_snapshot_book_id_for_persisted(auth_db.database_file.as_path(), &book_id).await;

    if let Ok(Some(media)) =
        load_persisted_book_media(auth_db.database_file.as_path(), &resolved_book_id).await
    {
        if let Some(user) = resolved_auth_user(&headers)
            && !user_can_access_book_media(
                auth_db.database_file.as_path(),
                &resolved_book_id,
                &user,
                &media,
            )
            .await
        {
            return StatusCode::FORBIDDEN.into_response();
        }

        if !book_media_supports_page_api(&media) {
            return StatusCode::NOT_FOUND.into_response();
        }

        let page_row = match load_persisted_book_page_row(
            auth_db.database_file.as_path(),
            &resolved_book_id,
            page_number as u64,
        )
        .await
        {
            Ok(Some(row)) => row,
            Ok(None) if book_media_is_single_image(&media) && page_number == 1 => {
                PersistedBookPageRow {
                    number: page_number as u64,
                    file_name: media.file_name.clone(),
                    media_type: content_type_from_filename(&media.file_name, &media.media_type),
                    width: None,
                    height: None,
                    file_size: fs::metadata(&media.file_path)
                        .ok()
                        .map(|value| value.len() as i64)
                        .unwrap_or(0),
                }
            }
            Ok(None) => {
                if let Some(row) = load_archive_page_row(&media, page_number as u64) {
                    row
                } else {
                    return StatusCode::NOT_FOUND.into_response();
                }
            }
            Err(error) => return internal_error_response(error),
        };

        if let Some(bytes) = resolve_book_page_bytes(&media, &page_row, page_number as u64) {
            let content_type = if page_row.media_type.is_empty() {
                content_type_from_filename(&page_row.file_name, &media.media_type)
            } else {
                page_row.media_type
            };

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

            return (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, content_type.as_str()),
                    (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
                    (header::LAST_MODIFIED, LAST_MODIFIED),
                    (header::ETAG, THUMBNAIL_ETAG),
                ],
                bytes,
            )
                .into_response();
        }
    }

    StatusCode::NOT_FOUND.into_response()
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
            && !user_can_access_book_media(auth_db.database_file.as_path(), &book_id, &user, &media)
                .await
        {
            return StatusCode::FORBIDDEN.into_response();
        }

        match load_selected_book_thumbnail(auth_db.database_file.as_path(), &book_id).await {
            Ok(Some(thumbnail)) => {
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
            Ok(None) => {}
            Err(error) => return internal_error_response(error),
        }

        if book_media_supports_page_image(&media)
            && let Ok(bytes) = fs::read(&media.file_path)
        {
            let content_type = content_type_from_filename(&media.file_name, &media.media_type);
            return (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, content_type.as_str()),
                    (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
                    (header::LAST_MODIFIED, LAST_MODIFIED),
                    (header::ETAG, THUMBNAIL_ETAG),
                ],
                bytes,
            )
                .into_response();
        }

        return StatusCode::NOT_FOUND.into_response();
    }

    StatusCode::NOT_FOUND.into_response()
}

pub(in crate::app::compat_runtime) async fn book_thumbnail_by_id(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((book_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if let Ok(Some(media)) =
        load_persisted_book_media(auth_db.database_file.as_path(), &book_id).await
    {
        if let Some(user) = resolved_auth_user(&headers)
            && !user_can_access_book_media(auth_db.database_file.as_path(), &book_id, &user, &media)
                .await
        {
            return StatusCode::FORBIDDEN.into_response();
        }

        return match load_book_thumbnail_by_id(
            auth_db.database_file.as_path(),
            &book_id,
            &thumbnail_id,
        )
        .await
        {
            Ok(Some(thumbnail)) => (
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
            Ok(None) => StatusCode::NOT_FOUND.into_response(),
            Err(error) => internal_error_response(error),
        };
    }

    StatusCode::NOT_FOUND.into_response()
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
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    if !persisted_book_exists(auth_db.database_file.as_path(), &book_id)
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
                "error": "book thumbnail upload body must not be empty",
            })),
        )
            .into_response();
    }

    let thumbnail_bytes = body.to_vec();

    match insert_book_thumbnail(
        auth_db.database_file.as_path(),
        &book_id,
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

pub(in crate::app::compat_runtime) async fn book_thumbnail_select(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((book_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match select_book_thumbnail(auth_db.database_file.as_path(), &book_id, &thumbnail_id).await {
        Ok(true) => {
            let mut response = StatusCode::ACCEPTED.into_response();
            mark_native(&mut response);
            response
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn book_thumbnail_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((book_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match delete_book_thumbnail(auth_db.database_file.as_path(), &book_id, &thumbnail_id).await {
        Ok(true) => {
            let mut response = StatusCode::ACCEPTED.into_response();
            mark_native(&mut response);
            response
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
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
    Extension(state): Extension<super::super::OperationalState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match persisted_book_exists(auth_db.database_file.as_path(), &book_id).await {
        Ok(true) => enqueue_task_records(
            &state,
            vec![
                TaskQueueRecord::new(
                    format!("REFRESH_BOOK_METADATA:{book_id}"),
                    80,
                    Some(book_id.clone()),
                ),
                TaskQueueRecord::new(
                    format!("REFRESH_BOOK_LOCAL_ARTWORK:{book_id}"),
                    80,
                    Some(book_id),
                ),
            ],
        ),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn book_metadata_update(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(state): Extension<super::super::OperationalState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let patch = match body.as_object() {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "book metadata update payload must be a JSON object" })),
            )
                .into_response();
        }
    };

    let existing =
        match load_existing_book_metadata(auth_db.database_file.as_path(), &book_id).await {
            Ok(Some(existing)) => existing,
            Ok(None) => return StatusCode::NOT_FOUND.into_response(),
            Err(error) => return internal_error_response(error),
        };

    let series_id = match load_book_series_id(auth_db.database_file.as_path(), &book_id).await {
        Ok(series_id) => series_id,
        Err(error) => return internal_error_response(error),
    };

    let patched = match apply_book_metadata_patch(existing, patch) {
        Ok(value) => value,
        Err(error) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": error }))).into_response();
        }
    };

    match persist_book_metadata_update(auth_db.database_file.as_path(), &book_id, &patched).await {
        Ok(true) => {
            if let Some(series_id) = series_id {
                let task = TaskQueueRecord::new(
                    format!("AGGREGATE_SERIES_METADATA:{series_id}"),
                    80,
                    Some(series_id),
                );
                if let Err(error) = process_task_side_effects(&state, vec![task]) {
                    return internal_error_response(error);
                }
            }

            let mut response = StatusCode::NO_CONTENT.into_response();
            mark_native(&mut response);
            response
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn book_metadata_batch_update(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(state): Extension<super::super::OperationalState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let batch = match body.as_object() {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "book metadata batch update payload must be a JSON object map",
                })),
            )
                .into_response();
        }
    };

    let mut affected_series_ids = BTreeSet::new();

    for (book_id, patch_value) in batch {
        let patch = match patch_value.as_object() {
            Some(value) => value,
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": format!("book metadata patch for {book_id} must be a JSON object"),
                    })),
                )
                    .into_response();
            }
        };

        let existing =
            match load_existing_book_metadata(auth_db.database_file.as_path(), book_id).await {
                Ok(Some(existing)) => existing,
                Ok(None) => continue,
                Err(error) => return internal_error_response(error),
            };

        match load_book_series_id(auth_db.database_file.as_path(), book_id).await {
            Ok(Some(series_id)) => {
                affected_series_ids.insert(series_id);
            }
            Ok(None) => {}
            Err(error) => return internal_error_response(error),
        }

        let patched = match apply_book_metadata_patch(existing, patch) {
            Ok(value) => value,
            Err(error) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": format!("invalid metadata patch for {book_id}: {error}"),
                    })),
                )
                    .into_response();
            }
        };

        match persist_book_metadata_update(auth_db.database_file.as_path(), book_id, &patched).await
        {
            Ok(true) | Ok(false) => {}
            Err(error) => return internal_error_response(error),
        }
    }

    if !affected_series_ids.is_empty() {
        let tasks = affected_series_ids
            .into_iter()
            .map(|series_id| {
                TaskQueueRecord::new(
                    format!("AGGREGATE_SERIES_METADATA:{series_id}"),
                    80,
                    Some(series_id),
                )
            })
            .collect::<Vec<_>>();
        if let Err(error) = process_task_side_effects(&state, tasks) {
            return internal_error_response(error);
        }
    }

    let mut response = StatusCode::NO_CONTENT.into_response();
    mark_native(&mut response);
    response
}

pub(in crate::app::compat_runtime) async fn books_import(
    Extension(_auth_db): Extension<AuthDatabaseState>,
    Extension(state): Extension<super::super::OperationalState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let payload = match parse_books_import_payload(&body) {
        Ok(payload) => payload,
        Err(error) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": error }))).into_response();
        }
    };

    let mut task_queue = state
        .task_queue
        .lock()
        .expect("task queue state lock should not be poisoned");

    for book in payload.books {
        let group_id = book.series_id.clone();
        let task_payload = match serde_json::to_string(&QueuedBookImportPayload {
            copy_mode: payload.copy_mode,
            book,
        }) {
            Ok(payload) => payload,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("serialize books import payload: {error}") })),
                )
                    .into_response();
            }
        };

        task_queue.enqueue(
            TaskQueueRecord::new(
                format!("IMPORT_BOOK:{}", random_prefixed_id("import-book")),
                100,
                Some(group_id),
            )
            .with_payload(task_payload),
        );
    }

    let mut response = StatusCode::ACCEPTED.into_response();
    mark_native(&mut response);
    response
}

pub(crate) async fn process_queued_books_import_task(
    database_file: &FsPath,
    task_payload: &str,
) -> Result<Vec<TaskQueueRecord>, String> {
    let payload = serde_json::from_str::<BooksImportPayload>(task_payload)
        .map_err(|error| format!("parse queued import payload: {error}"))?;
    process_books_import_payload(database_file, payload).await
}

pub(crate) async fn process_queued_book_import_task(
    database_file: &FsPath,
    task_payload: &str,
) -> Result<Vec<TaskQueueRecord>, String> {
    let payload = serde_json::from_str::<QueuedBookImportPayload>(task_payload)
        .map_err(|error| format!("parse queued import payload: {error}"))?;
    process_books_import_payload(
        database_file,
        BooksImportPayload {
            copy_mode: payload.copy_mode,
            books: vec![payload.book],
        },
    )
    .await
}

pub(crate) async fn hash_book_pages_with_media_content(
    database_file: &FsPath,
    book_id: &str,
) -> Result<(), String> {
    let media = load_persisted_book_media(database_file, book_id)
        .await?
        .ok_or_else(|| "book media missing for page hash task".to_string())?;
    let pages = load_persisted_book_pages(database_file, book_id).await?;

    let mut hashes: Vec<(i64, String)> = Vec::new();
    for page in pages {
        let Some(bytes) = resolve_book_page_bytes(&media, &page, page.number) else {
            continue;
        };
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let digest = hasher.finalize();
        let hash = digest
            .iter()
            .map(|value| format!("{value:02x}"))
            .collect::<String>();
        hashes.push((page.number as i64, hash));
    }

    if hashes.is_empty() {
        return Ok(());
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open media-page hash db: {error}"))?;
    for (number, hash) in hashes {
        sqlx::query(
            "UPDATE MEDIA_PAGE \
             SET FILE_HASH = ? \
             WHERE BOOK_ID = ? \
             AND NUMBER = ?",
        )
        .bind(hash)
        .bind(book_id)
        .bind(number)
        .execute(&pool)
        .await
        .map_err(|error| format!("persist media-page hash: {error}"))?;
    }
    Ok(())
}

async fn process_books_import_payload(
    database_file: &FsPath,
    payload: BooksImportPayload,
) -> Result<Vec<TaskQueueRecord>, String> {
    let library_roots = load_library_roots(database_file).await.unwrap_or_default();

    let mut library_ids = BTreeSet::new();
    let mut deferred_tasks: Vec<TaskQueueRecord> = Vec::new();
    for entry in payload.books {
        if source_inside_library_roots(entry.source_file.as_path(), &library_roots) {
            continue;
        }

        let target = match load_import_series_target(database_file, &entry.series_id).await {
            Ok(Some(target)) => target,
            Ok(None) => continue,
            Err(_) => continue,
        };

        let mut upgrade_destination_name: Option<String> = None;

        if let Some(upgrade_book_id) = entry.upgrade_book_id.as_deref()
            && let Ok(Some(upgrade_target)) =
                load_import_upgrade_book_target(database_file, upgrade_book_id).await
        {
            if upgrade_target.series_id != entry.series_id {
                continue;
            }

            upgrade_destination_name = Some(upgrade_target.file_name.clone());

            let upgrade_file = PathBuf::from(upgrade_target.library_root)
                .join(upgrade_target.series_url)
                .join(upgrade_target.file_name);
            let _ = fs::remove_file(upgrade_file);
        }

        let destination_name = match resolve_import_destination_name(
            entry.source_file.as_path(),
            upgrade_destination_name
                .as_deref()
                .or(entry.destination_name.as_deref()),
        ) {
            Some(value) => value,
            None => continue,
        };

        let destination_dir = PathBuf::from(&target.library_root).join(&target.series_url);
        if fs::create_dir_all(&destination_dir).is_err() {
            continue;
        }

        let destination_file = destination_dir.join(destination_name);
        if let Err(error) = apply_import_copy_mode(
            payload.copy_mode,
            entry.source_file.as_path(),
            &destination_file,
        ) {
            let _ = error;
            continue;
        }

        let sidecar_imported = match import_book_sidecars(
            payload.copy_mode,
            entry.source_file.as_path(),
            &destination_file,
        ) {
            Ok(sidecar_imported) => sidecar_imported,
            Err(_) => false,
        };

        let imported_book_id = scanner_book_id_for_path(&destination_file);

        if let Some(upgrade_book_id) = entry.upgrade_book_id.as_deref() {
            let _ = migrate_upgraded_book_identity(
                database_file,
                upgrade_book_id,
                imported_book_id.as_str(),
                &destination_file,
            )
            .await;
        }

        let _ = persist_book_imported_event(
            database_file,
            imported_book_id.as_str(),
            target.series_id.as_str(),
            &destination_file,
            entry.source_file.as_path(),
            entry.upgrade_book_id.is_some(),
        )
        .await;

        if sidecar_imported {
            deferred_tasks.push(TaskQueueRecord::new(
                format!("REFRESH_BOOK_METADATA:{imported_book_id}"),
                80,
                Some(imported_book_id.clone()),
            ));
        }

        library_ids.insert(target.library_id);
    }

    let mut follow_up_tasks = library_ids
        .into_iter()
        .map(|library_id| {
            TaskQueueRecord::new(format!("SCAN_LIBRARY:{library_id}"), 100, Some(library_id))
        })
        .collect::<Vec<_>>();
    follow_up_tasks.extend(deferred_tasks);

    Ok(follow_up_tasks)
}

pub(in crate::app::compat_runtime) async fn books_thumbnails_regenerate(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(state): Extension<super::super::OperationalState>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match persisted_book_ids(auth_db.database_file.as_path()).await {
        Ok(book_ids) => enqueue_task_records(
            &state,
            book_ids
                .into_iter()
                .map(|book_id| {
                    TaskQueueRecord::new(
                        format!("REFRESH_BOOK_LOCAL_ARTWORK:{book_id}"),
                        10,
                        Some(book_id),
                    )
                })
                .collect(),
        ),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
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
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    if !persisted_readlist_exists(auth_db.database_file.as_path(), &readlist_id)
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
                "error": "readlist thumbnail upload body must not be empty",
            })),
        )
            .into_response();
    }

    match insert_readlist_thumbnail(
        auth_db.database_file.as_path(),
        &readlist_id,
        body.as_ref(),
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

pub(in crate::app::compat_runtime) async fn readlist_thumbnail_select(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((readlist_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match select_readlist_thumbnail(auth_db.database_file.as_path(), &readlist_id, &thumbnail_id)
        .await
    {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn readlist_thumbnail_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((readlist_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match delete_readlist_thumbnail(auth_db.database_file.as_path(), &readlist_id, &thumbnail_id)
        .await
    {
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

pub(in crate::app::compat_runtime) async fn readlist_file(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_file_download(&headers) {
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

pub(in crate::app::compat_runtime) async fn series_file(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    if let Some(response) = require_file_download(&headers) {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    match load_series_archive_entries(auth_db.database_file.as_path(), &series_id).await {
        Ok(Some((series_title, library_id, entries))) => {
            if !user_can_access_library(&user, &library_id) {
                return StatusCode::FORBIDDEN.into_response();
            }

            let archive_entries = entries
                .into_iter()
                .filter_map(|(file_name, file_path)| {
                    fs::read(file_path).ok().map(|bytes| (file_name, bytes))
                })
                .collect::<Vec<_>>();
            if archive_entries.is_empty() {
                return StatusCode::NOT_FOUND.into_response();
            }

            let archive_payload = match build_stored_zip_archive(archive_entries) {
                Ok(payload) => payload,
                Err(error) => return internal_error_response(error),
            };

            let file_name = format!("{series_title}.zip");
            let content_disposition = attachment_disposition(&file_name);
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "application/zip"),
                    (header::CONTENT_DISPOSITION, content_disposition.as_str()),
                ],
                archive_payload,
            )
                .into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn series_file_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(state): Extension<super::super::OperationalState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match persisted_series_exists(auth_db.database_file.as_path(), &series_id).await {
        Ok(true) => enqueue_task_records(
            &state,
            vec![TaskQueueRecord::new(
                format!("DELETE_SERIES:{series_id}"),
                100,
                Some(series_id),
            )],
        ),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
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
            Ok(None) => return StatusCode::NOT_FOUND.into_response(),
            Err(error) => return internal_error_response(error),
        };

    if let Some(user) = resolved_auth_user(&headers)
        && !user_can_access_library(&user, &media.library_id)
    {
        return StatusCode::FORBIDDEN.into_response();
    }

    if !book_media_supports_page_api(&media) {
        return StatusCode::NOT_FOUND.into_response();
    }

    let page_rows =
        match load_persisted_book_pages(auth_db.database_file.as_path(), &resolved_book_id).await {
            Ok(rows) => rows,
            Err(error) => return internal_error_response(error),
        };

    if !page_rows.is_empty() {
        return Json(
            page_rows
                .into_iter()
                .map(|page| {
                    json!({
                        "number": page.number,
                        "fileName": page.file_name,
                        "mediaType": page.media_type,
                        "width": page.width,
                        "height": page.height,
                        "sizeBytes": page.file_size,
                        "size": format_size_bytes(page.file_size as u64),
                    })
                })
                .collect::<Vec<_>>(),
        )
        .into_response();
    }

    if let Some(archive_rows) = load_archive_page_rows(&media)
        && !archive_rows.is_empty()
    {
        return Json(
            archive_rows
                .into_iter()
                .map(|page| {
                    json!({
                        "number": page.number,
                        "fileName": page.file_name,
                        "mediaType": page.media_type,
                        "width": page.width,
                        "height": page.height,
                        "sizeBytes": page.file_size,
                        "size": format_size_bytes(page.file_size as u64),
                    })
                })
                .collect::<Vec<_>>(),
        )
        .into_response();
    }

    let generated_pdf_rows = load_generated_pdf_page_rows(&media);
    if !generated_pdf_rows.is_empty() {
        return Json(
            generated_pdf_rows
                .into_iter()
                .map(|page| {
                    json!({
                        "number": page.number,
                        "fileName": page.file_name,
                        "mediaType": page.media_type,
                        "width": page.width,
                        "height": page.height,
                        "sizeBytes": page.file_size,
                        "size": format_size_bytes(page.file_size as u64),
                    })
                })
                .collect::<Vec<_>>(),
        )
        .into_response();
    }

    if !book_media_is_single_image(&media) {
        return StatusCode::NOT_FOUND.into_response();
    }

    let size_bytes = fs::metadata(&media.file_path)
        .ok()
        .map(|metadata| metadata.len())
        .unwrap_or(0);

    Json(vec![json!({
        "number": 1,
        "fileName": media.file_name,
        "mediaType": content_type_from_filename(&media.file_name, &media.media_type),
        "width": Value::Null,
        "height": Value::Null,
        "sizeBytes": size_bytes,
        "size": format_size_bytes(size_bytes),
    })])
    .into_response()
}

pub(in crate::app::compat_runtime) async fn book_positions(
    Extension(state): Extension<OperationalState>,
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
            Ok(None) => return StatusCode::NOT_FOUND.into_response(),
            Err(error) => return internal_error_response(error),
        };

    if let Some(user) = resolved_auth_user(&headers)
        && !user_can_access_library(&user, &media.library_id)
    {
        return StatusCode::FORBIDDEN.into_response();
    }

    if book_media_is_epub(&media) {
        let effective_kepubify_path = load_effective_kepubify_path(
            auth_db.database_file.as_path(),
            state.runtime.kepubify_path.as_deref(),
        )
        .await;

        match load_persisted_epub_positions(auth_db.database_file.as_path(), &resolved_book_id)
            .await
        {
            Ok(Some(positions)) if !positions.is_empty() => {
                return Json(json!({
                    "total": positions.len(),
                    "positions": positions,
                }))
                .into_response();
            }
            Ok(_) => {}
            Err(error) => return internal_error_response(error),
        }

        if let Some(positions) =
            load_epub_archive_positions_fallback(&media, effective_kepubify_path.as_deref())
            && !positions.is_empty()
        {
            return Json(json!({
                "total": positions.len(),
                "positions": positions,
            }))
            .into_response();
        }
    }

    let persisted_page_rows =
        match load_persisted_book_pages(auth_db.database_file.as_path(), &resolved_book_id).await {
            Ok(rows) => rows,
            Err(error) => return internal_error_response(error),
        };

    let effective_page_rows = if persisted_page_rows.is_empty() {
        load_archive_page_rows(&media)
            .filter(|rows| !rows.is_empty())
            .unwrap_or_else(|| load_generated_pdf_page_rows(&media))
    } else {
        persisted_page_rows
    };

    let generated_page_numbers = if effective_page_rows.is_empty() {
        if book_media_is_single_image(&media) {
            vec![1]
        } else if media.page_count > 0 {
            (1..=media.page_count).collect::<Vec<_>>()
        } else {
            return StatusCode::NOT_FOUND.into_response();
        }
    } else {
        effective_page_rows
            .iter()
            .map(|page| page.number)
            .collect::<Vec<_>>()
    };

    let total = generated_page_numbers.len();
    let positions = generated_page_numbers
        .iter()
        .enumerate()
        .map(|(index, page)| {
            let position = index + 1;
            let progression = position as f64 / total as f64;
            let fallback_to_manifest =
                effective_page_rows.is_empty() && !book_media_is_single_image(&media);
            let href = if fallback_to_manifest {
                format!("/api/v1/books/{resolved_book_id}/manifest#position={position}")
            } else {
                format!("/api/v1/books/{resolved_book_id}/pages/{page}")
            };
            let content_type = if fallback_to_manifest {
                "application/webpub+json".to_string()
            } else {
                effective_page_rows
                    .get(index)
                    .map(|row| {
                        if row.media_type.is_empty() {
                            content_type_from_filename(&row.file_name, &media.media_type)
                        } else {
                            row.media_type.clone()
                        }
                    })
                    .unwrap_or_else(|| {
                        content_type_from_filename(&media.file_name, &media.media_type)
                    })
            };

            json!({
                "href": href,
                "type": content_type,
                "title": if effective_page_rows.is_empty() {
                    format!("{}#{page}", media.file_name)
                } else {
                    effective_page_rows[index].file_name.clone()
                },
                "locations": {
                    "position": position,
                    "progression": progression,
                    "totalProgression": progression,
                },
            })
        })
        .collect::<Vec<_>>();

    Json(json!({
        "total": total,
        "positions": positions,
    }))
    .into_response()
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

    if !persisted_series_exists(auth_db.database_file.as_path(), &resolved_series_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    match load_selected_series_thumbnail(auth_db.database_file.as_path(), &resolved_series_id).await
    {
        Ok(Some(thumbnail)) => {
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
        Ok(None) => {}
        Err(error) => return internal_error_response(error),
    }

    if let Ok(Some(media)) =
        load_persisted_series_thumbnail_media(auth_db.database_file.as_path(), &resolved_series_id)
            .await
        && let Ok(bytes) = fs::read(&media.file_path)
    {
        return (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
                (header::LAST_MODIFIED, LAST_MODIFIED),
                (header::ETAG, THUMBNAIL_ETAG),
            ],
            bytes,
        )
            .into_response();
    }

    StatusCode::NOT_FOUND.into_response()
}

pub(in crate::app::compat_runtime) async fn series_thumbnails(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let resolved_series_id =
        resolve_snapshot_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    if !persisted_series_exists(auth_db.database_file.as_path(), &resolved_series_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    match load_persisted_series_thumbnails(auth_db.database_file.as_path(), &resolved_series_id)
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

pub(in crate::app::compat_runtime) async fn series_thumbnail_by_id(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((series_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let resolved_series_id =
        resolve_snapshot_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    if !persisted_series_exists(auth_db.database_file.as_path(), &resolved_series_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    match load_series_thumbnail_by_id(
        auth_db.database_file.as_path(),
        &resolved_series_id,
        &thumbnail_id,
    )
    .await
    {
        Ok(Some(thumbnail)) => (
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
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn series_thumbnail_upload(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let resolved_series_id =
        resolve_snapshot_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    if !persisted_series_exists(auth_db.database_file.as_path(), &resolved_series_id)
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
                "error": "series thumbnail upload body must not be empty",
            })),
        )
            .into_response();
    }

    match insert_series_thumbnail(
        auth_db.database_file.as_path(),
        &resolved_series_id,
        body.as_ref(),
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

pub(in crate::app::compat_runtime) async fn series_thumbnail_select(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((series_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let resolved_series_id =
        resolve_snapshot_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    match select_series_thumbnail(
        auth_db.database_file.as_path(),
        &resolved_series_id,
        &thumbnail_id,
    )
    .await
    {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn series_thumbnail_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((series_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let resolved_series_id =
        resolve_snapshot_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    match delete_series_thumbnail(
        auth_db.database_file.as_path(),
        &resolved_series_id,
        &thumbnail_id,
    )
    .await
    {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn series_analyze(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let resolved_series_id =
        resolve_snapshot_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    if !persisted_series_exists(auth_db.database_file.as_path(), &resolved_series_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let book_ids =
        match load_series_book_ids(auth_db.database_file.as_path(), &resolved_series_id).await {
            Ok(book_ids) => book_ids,
            Err(error) => return internal_error_response(error),
        };
    let task_records = book_ids
        .into_iter()
        .map(|book_id| TaskQueueRecord::new(format!("ANALYZE_BOOK:{book_id}"), 90, Some(book_id)))
        .collect::<Vec<_>>();

    enqueue_task_records(&state, task_records)
}

pub(in crate::app::compat_runtime) async fn series_metadata_refresh(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let resolved_series_id =
        resolve_snapshot_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    if !persisted_series_exists(auth_db.database_file.as_path(), &resolved_series_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let book_ids =
        match load_series_book_ids(auth_db.database_file.as_path(), &resolved_series_id).await {
            Ok(book_ids) => book_ids,
            Err(error) => return internal_error_response(error),
        };

    let mut task_records = vec![];
    for book_id in book_ids {
        task_records.push(TaskQueueRecord::new(
            format!("REFRESH_BOOK_METADATA:{book_id}"),
            80,
            Some(book_id.clone()),
        ));
        task_records.push(TaskQueueRecord::new(
            format!("REFRESH_BOOK_LOCAL_ARTWORK:{book_id}"),
            80,
            Some(book_id),
        ));
    }
    task_records.push(TaskQueueRecord::new(
        format!("REFRESH_SERIES_LOCAL_ARTWORK:{resolved_series_id}"),
        80,
        Some(resolved_series_id),
    ));

    enqueue_task_records(&state, task_records)
}

pub(in crate::app::compat_runtime) async fn series_read_progress_post(
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
        resolve_snapshot_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    let Some(library_id) =
        (match load_series_library_id(auth_db.database_file.as_path(), &resolved_series_id).await {
            Ok(library_id) => library_id,
            Err(error) => return internal_error_response(error),
        })
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !user_can_access_library(&user, &library_id) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let book_ids =
        match load_series_book_ids(auth_db.database_file.as_path(), &resolved_series_id).await {
            Ok(book_ids) => book_ids,
            Err(error) => return internal_error_response(error),
        };

    for book_id in book_ids {
        if let Err(error) = persist_read_progress(
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

pub(in crate::app::compat_runtime) async fn series_read_progress_delete(
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
        resolve_snapshot_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    let Some(library_id) =
        (match load_series_library_id(auth_db.database_file.as_path(), &resolved_series_id).await {
            Ok(library_id) => library_id,
            Err(error) => return internal_error_response(error),
        })
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !user_can_access_library(&user, &library_id) {
        return StatusCode::FORBIDDEN.into_response();
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

pub(in crate::app::compat_runtime) async fn series_tachiyomi_read_progress_get(
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
        resolve_snapshot_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    let Some(library_id) =
        (match load_series_library_id(auth_db.database_file.as_path(), &resolved_series_id).await {
            Ok(library_id) => library_id,
            Err(error) => return internal_error_response(error),
        })
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !user_can_access_library(&user, &library_id) {
        return StatusCode::FORBIDDEN.into_response();
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

pub(in crate::app::compat_runtime) async fn series_tachiyomi_read_progress_put(
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
        resolve_snapshot_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    let Some(library_id) =
        (match load_series_library_id(auth_db.database_file.as_path(), &resolved_series_id).await {
            Ok(library_id) => library_id,
            Err(error) => return internal_error_response(error),
        })
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !user_can_access_library(&user, &library_id) {
        return StatusCode::FORBIDDEN.into_response();
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

pub(in crate::app::compat_runtime) async fn book_resource(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((book_id, resource_path)): Path<(String, String)>,
) -> Response {
    let resource_name = resource_path.trim_start_matches('/');
    if resource_name.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let is_font = is_font_resource(resource_name);
    if !is_font && let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(media) =
        (match load_persisted_book_media(auth_db.database_file.as_path(), &book_id).await {
            Ok(media) => media,
            Err(error) => return internal_error_response(error),
        })
    else {
        return StatusCode::NOT_FOUND.into_response();
    };

    if !book_media_is_epub(&media) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": format!("Book media type '{}' not compatible with requested profile", media.media_type),
            })),
        )
            .into_response();
    }

    if !is_font {
        let Some(user) = resolved_auth_user(&headers) else {
            return StatusCode::UNAUTHORIZED.into_response();
        };
        if !user_can_access_library(&user, &media.library_id) {
            return StatusCode::FORBIDDEN.into_response();
        }
    }

    let Some(bytes) = read_epub_resource_bytes(media.file_path.as_path(), resource_name) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    (
        StatusCode::OK,
        [
            (
                header::CONTENT_TYPE,
                content_type_from_filename(resource_name, "application/octet-stream"),
            ),
            (
                header::CONTENT_SECURITY_POLICY,
                "script-src 'none'; object-src 'none';".to_string(),
            ),
        ],
        bytes,
    )
        .into_response()
}

pub(in crate::app::compat_runtime) async fn book_file(
    Extension(_profile): Extension<CompatProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    book_file_response(&auth_db, &headers, &book_id).await
}

pub(in crate::app::compat_runtime) async fn book_file_with_suffix(
    Extension(_profile): Extension<CompatProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((book_id, _file_name)): Path<(String, String)>,
) -> Response {
    book_file_response(&auth_db, &headers, &book_id).await
}

async fn book_file_response(
    auth_db: &AuthDatabaseState,
    headers: &HeaderMap,
    book_id: &str,
) -> Response {
    if let Some(response) = require_file_download(headers) {
        return response;
    }

    if let Ok(Some(media)) =
        load_persisted_book_media(auth_db.database_file.as_path(), book_id).await
        && let Ok(body) = fs::read(&media.file_path)
    {
        if let Some(user) = resolved_auth_user(headers)
            && !user_can_access_library(&user, &media.library_id)
        {
            return StatusCode::FORBIDDEN.into_response();
        }

        let content_type = content_type_from_filename(&media.file_name, &media.media_type);
        let content_disposition = attachment_disposition(&media.file_name);

        if let Some((start, end)) = requested_byte_range(headers, body.len()) {
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

            return response;
        }

        return (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, content_type.as_str()),
                (header::CONTENT_DISPOSITION, content_disposition.as_str()),
            ],
            body,
        )
            .into_response();
    }

    StatusCode::NOT_FOUND.into_response()
}

struct PersistedBookMedia {
    library_id: String,
    file_name: String,
    file_path: PathBuf,
    media_type: String,
    page_count: u64,
}

struct PersistedBookPageRow {
    number: u64,
    file_name: String,
    media_type: String,
    width: Option<i64>,
    height: Option<i64>,
    file_size: i64,
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

struct PersistedSeriesThumbnailRow {
    id: String,
    thumbnail_type: String,
    selected: bool,
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

struct PersistedBookMetadata {
    title: String,
    title_lock: bool,
    summary: String,
    summary_lock: bool,
    number: String,
    number_lock: bool,
    number_sort: f64,
    number_sort_lock: bool,
    release_date: Option<String>,
    release_date_lock: bool,
    authors: Vec<(String, String)>,
    authors_lock: bool,
    tags: Vec<String>,
    tags_lock: bool,
    isbn: String,
    isbn_lock: bool,
    links: Vec<(String, String)>,
    links_lock: bool,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum ImportCopyMode {
    Move,
    Copy,
    Hardlink,
}

#[derive(Serialize, Deserialize)]
struct BooksImportPayload {
    copy_mode: ImportCopyMode,
    books: Vec<BooksImportEntry>,
}

#[derive(Serialize, Deserialize)]
struct QueuedBookImportPayload {
    copy_mode: ImportCopyMode,
    book: BooksImportEntry,
}

#[derive(Serialize, Deserialize)]
struct BooksImportEntry {
    source_file: PathBuf,
    series_id: String,
    destination_name: Option<String>,
    upgrade_book_id: Option<String>,
}

struct ImportSeriesTarget {
    series_id: String,
    library_id: String,
    library_root: String,
    series_url: String,
}

struct ImportUpgradeBookTarget {
    series_id: String,
    library_root: String,
    series_url: String,
    file_name: String,
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
        "SELECT b.LIBRARY_ID AS LIBRARY_ID, b.NAME AS FILE_NAME, b.URL AS BOOK_URL, \
                l.ROOT AS LIBRARY_ROOT, \
                COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE, \
                COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT \
         FROM BOOK b \
         JOIN LIBRARY l ON l.ID = b.LIBRARY_ID \
         LEFT \
         JOIN MEDIA m ON m.BOOK_ID = b.ID \
         WHERE b.ID = ?",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted book media: {error}"))?;

    let media = row.map(|row| {
        let file_name = row.get::<String, _>("FILE_NAME");
        let book_url = row.get::<String, _>("BOOK_URL");
        let library_root = row.get::<String, _>("LIBRARY_ROOT");

        PersistedBookMedia {
            library_id: row.get::<String, _>("LIBRARY_ID"),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            file_path: PathBuf::from(library_root).join(book_url),
            file_name,
            page_count: row.get::<i64, _>("PAGE_COUNT").max(0) as u64,
        }
    });

    Ok(media)
}

async fn book_media_is_ready_status(database_file: &FsPath, book_id: &str) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open media status db: {error}"))?;
    let row = sqlx::query(
        "SELECT STATUS \
         FROM MEDIA \
         WHERE BOOK_ID = ? \
         LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query media status: {error}"))?;

    Ok(row
        .map(|row| row.get::<String, _>("STATUS"))
        .is_some_and(|status| status.eq_ignore_ascii_case("READY")))
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
        "SELECT b.NAME AS FILE_NAME, b.URL AS BOOK_URL, l.ROOT AS LIBRARY_ROOT, \
                COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE \
         FROM BOOK b \
         JOIN LIBRARY l ON l.ID = b.LIBRARY_ID \
         LEFT \
         JOIN MEDIA m ON m.BOOK_ID = b.ID \
         WHERE b.SERIES_ID = ? \
         AND b.DELETED_DATE IS NULL \
         ORDER BY b.NUMBER ASC, b.ID ASC \
         LIMIT 1",
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted series thumbnail media: {error}"))?;

    let media = row.map(|row| {
        let file_name = row.get::<String, _>("FILE_NAME");
        let book_url = row.get::<String, _>("BOOK_URL");
        let library_root = row.get::<String, _>("LIBRARY_ROOT");

        PersistedBookMedia {
            library_id: String::new(),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            file_path: PathBuf::from(library_root).join(book_url),
            file_name,
            page_count: 0,
        }
    });

    Ok(media)
}

async fn load_persisted_book_pages(
    database_file: &FsPath,
    book_id: &str,
) -> Result<Vec<PersistedBookPageRow>, String> {
    if !database_file.exists() {
        return Ok(Vec::new());
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book pages db: {error}"))?;

    let rows = sqlx::query(
        "SELECT NUMBER, FILE_NAME, MEDIA_TYPE, WIDTH, HEIGHT, COALESCE(FILE_SIZE, 0) AS FILE_SIZE \
         FROM MEDIA_PAGE \
         WHERE BOOK_ID = ? \
         ORDER BY NUMBER ASC",
    )
    .bind(book_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted book pages: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedBookPageRow {
            number: row.get::<i64, _>("NUMBER") as u64,
            file_name: row.get::<String, _>("FILE_NAME"),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            width: row.get::<Option<i64>, _>("WIDTH"),
            height: row.get::<Option<i64>, _>("HEIGHT"),
            file_size: row.get::<i64, _>("FILE_SIZE"),
        })
        .collect())
}

async fn load_persisted_book_page_row(
    database_file: &FsPath,
    book_id: &str,
    page_number: u64,
) -> Result<Option<PersistedBookPageRow>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open single book page db: {error}"))?;

    let row = sqlx::query(
        "SELECT NUMBER, FILE_NAME, MEDIA_TYPE, WIDTH, HEIGHT, COALESCE(FILE_SIZE, 0) AS FILE_SIZE \
         FROM MEDIA_PAGE \
         WHERE BOOK_ID = ? \
         AND NUMBER = ? \
         LIMIT 1",
    )
    .bind(book_id)
    .bind(page_number as i64)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query single persisted book page: {error}"))?;

    Ok(row.map(|row| PersistedBookPageRow {
        number: row.get::<i64, _>("NUMBER") as u64,
        file_name: row.get::<String, _>("FILE_NAME"),
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        width: row.get::<Option<i64>, _>("WIDTH"),
        height: row.get::<Option<i64>, _>("HEIGHT"),
        file_size: row.get::<i64, _>("FILE_SIZE"),
    }))
}

fn resolve_book_page_bytes(
    media: &PersistedBookMedia,
    page: &PersistedBookPageRow,
    page_number: u64,
) -> Option<Vec<u8>> {
    let mut candidates = Vec::new();

    if media.file_path.is_dir() {
        candidates.push(media.file_path.join(&page.file_name));
    }
    if let Some(parent) = media.file_path.parent() {
        candidates.push(parent.join(&page.file_name));
    }
    if book_media_is_single_image(media) && page_number == 1 {
        candidates.push(media.file_path.clone());
    }

    for candidate in candidates {
        if let Ok(bytes) = fs::read(candidate) {
            return Some(bytes);
        }
    }

    if let Some(bytes) = read_zip_archive_page_bytes(media, page, page_number) {
        return Some(bytes);
    }

    if let Some(bytes) = read_rar_archive_page_bytes_cli(media, page, page_number) {
        return Some(bytes);
    }

    if let Some(bytes) = read_pdf_page_bytes(media, page_number) {
        return Some(bytes);
    }

    if book_media_is_single_image(media) && page_number == 1 {
        return fs::read(&media.file_path).ok();
    }

    None
}

fn load_archive_page_row(
    media: &PersistedBookMedia,
    page_number: u64,
) -> Option<PersistedBookPageRow> {
    if page_number == 0 {
        return None;
    }

    let page_index = usize::try_from(page_number.saturating_sub(1)).ok()?;
    load_archive_page_rows(media)?.into_iter().nth(page_index)
}

fn load_archive_page_rows(media: &PersistedBookMedia) -> Option<Vec<PersistedBookPageRow>> {
    if book_media_is_zip_archive(media) {
        return load_zip_archive_page_rows(media);
    }
    if book_media_is_rar_archive(media) {
        return load_rar_archive_page_rows_cli(media);
    }
    None
}

fn load_pdf_page_row(media: &PersistedBookMedia, page_number: u64) -> Option<PersistedBookPageRow> {
    if page_number == 0 {
        return None;
    }

    load_generated_pdf_page_rows(media)
        .into_iter()
        .nth(usize::try_from(page_number.saturating_sub(1)).ok()?)
}

fn load_generated_pdf_page_rows(media: &PersistedBookMedia) -> Vec<PersistedBookPageRow> {
    if !book_media_is_pdf(media) {
        return vec![];
    }

    let page_count = if media.page_count > 0 {
        media.page_count
    } else {
        detect_pdf_page_count(media).unwrap_or(0)
    };
    if page_count == 0 {
        return vec![];
    }

    (1..=page_count)
        .map(|number| PersistedBookPageRow {
            number,
            file_name: format!("page-{number}.pdf"),
            media_type: "application/pdf".to_string(),
            width: None,
            height: None,
            file_size: 0,
        })
        .collect()
}

fn read_pdf_page_bytes(media: &PersistedBookMedia, page_number: u64) -> Option<Vec<u8>> {
    if !book_media_is_pdf(media) || page_number == 0 {
        return None;
    }

    let document = PdfDocument::load(&media.file_path).ok()?;
    let pages = document.get_pages();
    let object_id = *pages.get(&(page_number as u32))?;
    document.get_page_content(object_id).ok()
}

fn read_pdf_page_as_single_page_pdf(
    media: &PersistedBookMedia,
    page_number: u64,
) -> Option<Vec<u8>> {
    if !book_media_is_pdf(media) || page_number == 0 {
        return None;
    }

    let mut document = PdfDocument::load(&media.file_path).ok()?;
    let pages = document.get_pages();
    if !pages.contains_key(&(page_number as u32)) {
        return None;
    }

    let to_delete = pages
        .keys()
        .copied()
        .filter(|number| *number != page_number as u32)
        .collect::<Vec<_>>();
    document.delete_pages(&to_delete);
    document.prune_objects();

    let mut bytes = Vec::new();
    document.save_to(&mut bytes).ok()?;
    Some(bytes)
}

fn detect_pdf_page_count(media: &PersistedBookMedia) -> Option<u64> {
    if !book_media_is_pdf(media) {
        return None;
    }
    let document = PdfDocument::load(&media.file_path).ok()?;
    Some(document.get_pages().len() as u64)
}

fn read_zip_archive_page_bytes(
    media: &PersistedBookMedia,
    page: &PersistedBookPageRow,
    page_number: u64,
) -> Option<Vec<u8>> {
    if !book_media_is_zip_archive(media) || page_number == 0 {
        return None;
    }

    let file = fs::File::open(&media.file_path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;

    if !page.file_name.is_empty()
        && let Ok(mut entry) = archive.by_name(&page.file_name)
        && is_supported_page_image_file_name(entry.name())
    {
        let mut bytes = Vec::new();
        if entry.read_to_end(&mut bytes).is_ok() {
            return Some(bytes);
        }
    }

    let target_index = usize::try_from(page_number.saturating_sub(1)).ok()?;
    let mut logical_index = 0usize;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).ok()?;
        if !is_supported_page_image_file_name(entry.name()) {
            continue;
        }
        if logical_index != target_index {
            logical_index += 1;
            continue;
        }

        let mut bytes = Vec::new();
        if entry.read_to_end(&mut bytes).is_ok() {
            return Some(bytes);
        }
        return None;
    }

    None
}

fn load_zip_archive_page_rows(media: &PersistedBookMedia) -> Option<Vec<PersistedBookPageRow>> {
    let file = fs::File::open(&media.file_path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let mut rows = Vec::new();

    for index in 0..archive.len() {
        let entry = archive.by_index(index).ok()?;
        let file_name = entry.name().to_string();
        if !is_supported_page_image_file_name(&file_name) {
            continue;
        }

        rows.push(PersistedBookPageRow {
            number: (rows.len() as u64) + 1,
            media_type: content_type_from_filename(&file_name, "image/jpeg"),
            file_name,
            width: None,
            height: None,
            file_size: entry.size().try_into().unwrap_or(i64::MAX),
        });
    }

    (!rows.is_empty()).then_some(rows)
}

fn load_rar_archive_page_rows_cli(media: &PersistedBookMedia) -> Option<Vec<PersistedBookPageRow>> {
    let output = Command::new("unrar")
        .arg("lb")
        .arg(&media.file_path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let rows = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && is_supported_page_image_file_name(line))
        .enumerate()
        .map(|(index, file_name)| PersistedBookPageRow {
            number: (index as u64) + 1,
            file_name: file_name.to_string(),
            media_type: content_type_from_filename(file_name, "image/jpeg"),
            width: None,
            height: None,
            file_size: 0,
        })
        .collect::<Vec<_>>();

    (!rows.is_empty()).then_some(rows)
}

fn read_rar_archive_page_bytes_cli(
    media: &PersistedBookMedia,
    page: &PersistedBookPageRow,
    page_number: u64,
) -> Option<Vec<u8>> {
    if !book_media_is_rar_archive(media) || page_number == 0 {
        return None;
    }

    if !page.file_name.is_empty()
        && let Some(bytes) = read_rar_entry_bytes_cli(&media.file_path, &page.file_name)
    {
        return Some(bytes);
    }

    let page_index = usize::try_from(page_number.saturating_sub(1)).ok()?;
    let page_file_name = load_rar_archive_page_rows_cli(media)?
        .into_iter()
        .nth(page_index)?
        .file_name;
    read_rar_entry_bytes_cli(&media.file_path, &page_file_name)
}

fn read_rar_entry_bytes_cli(archive_path: &FsPath, entry_name: &str) -> Option<Vec<u8>> {
    let output = Command::new("unrar")
        .arg("p")
        .arg("-inul")
        .arg(archive_path)
        .arg(entry_name)
        .output()
        .ok()?;
    if !output.status.success() || output.stdout.is_empty() {
        return None;
    }
    Some(output.stdout)
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
        "SELECT s.ID AS ID \
         FROM SERIES s \
         LEFT \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         WHERE s.DELETED_DATE IS NULL \
         ORDER BY COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) COLLATE NOCASE ASC, s.ID ASC \
         LIMIT 1 \
         OFFSET ?",
    )
    .bind((index - 1) as i64)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query remapped series id: {error}"))?;

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
        "SELECT b.ID AS ID \
         FROM BOOK b \
         LEFT \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         WHERE b.DELETED_DATE IS NULL \
         ORDER BY COALESCE(bm.TITLE, b.NAME) COLLATE NOCASE ASC, b.ID ASC \
         LIMIT 1 \
         OFFSET ?",
    )
    .bind((index - 1) as i64)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query remapped book id: {error}"))?;

    Ok(row.map(|row| row.get::<String, _>("ID")))
}

async fn persisted_book_exists(database_file: &FsPath, book_id: &str) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book-exists db: {error}"))?;

    let row = sqlx::query(
        "SELECT 1 AS FOUND \
                           FROM BOOK \
                           WHERE ID = ? \
                           LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted book existence: {error}"))?;

    Ok(row.is_some())
}

async fn persisted_series_exists(database_file: &FsPath, series_id: &str) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series exists db: {error}"))?;
    let row = sqlx::query(
        "SELECT 1 AS FOUND \
                           FROM SERIES \
                           WHERE ID = ? \
                           LIMIT 1",
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted series existence: {error}"))?;

    Ok(row.is_some())
}

async fn load_series_library_id(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Option<String>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series library db: {error}"))?;

    let row = sqlx::query(
        "SELECT LIBRARY_ID \
                           FROM SERIES \
                           WHERE ID = ? \
                           LIMIT 1",
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query series library id: {error}"))?;

    Ok(row.map(|row| row.get::<String, _>("LIBRARY_ID")))
}

async fn load_series_book_ids(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Vec<String>, String> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series books db: {error}"))?;

    let rows = sqlx::query(
        "SELECT b.ID AS ID \
         FROM BOOK b \
         LEFT \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         WHERE b.SERIES_ID = ? \
         AND b.DELETED_DATE IS NULL \
         ORDER BY COALESCE(bm.NUMBER_SORT, 0) ASC, b.ID ASC",
    )
    .bind(series_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query series book ids: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("ID"))
        .collect())
}

async fn load_series_book_number_sorts(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Vec<(String, f64)>, String> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series number sort db: {error}"))?;

    let rows = sqlx::query(
        "SELECT b.ID AS ID, COALESCE(bm.NUMBER_SORT, 0) AS NUMBER_SORT \
         FROM BOOK b \
         LEFT \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         WHERE b.SERIES_ID = ? \
         AND b.DELETED_DATE IS NULL \
         ORDER BY COALESCE(bm.NUMBER_SORT, 0) ASC, b.ID ASC",
    )
    .bind(series_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query series number sort rows: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| (row.get::<String, _>("ID"), row.get::<f64, _>("NUMBER_SORT")))
        .collect())
}

async fn refresh_series_read_progress_row(
    database_file: &FsPath,
    series_id: &str,
    user_id_value: &str,
) -> Result<(), String> {
    if !database_file.exists() {
        return Ok(());
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series read progress db: {error}"))?;

    let row = sqlx::query(
        "SELECT COALESCE(SUM(CASE WHEN rp.COMPLETED = 1 THEN 1 ELSE 0 END), 0) AS READ_COUNT, \
                COALESCE(SUM(CASE WHEN rp.COMPLETED = 0 \
         AND rp.PAGE > 0 THEN 1 ELSE 0 END), 0) AS IN_PROGRESS_COUNT, \
           MAX(rp.READ_DATE) AS MOST_RECENT_READ_DATE \
         FROM BOOK b \
         LEFT \
         JOIN READ_PROGRESS rp ON rp.BOOK_ID = b.ID \
         AND rp.USER_ID = ? \
         WHERE b.SERIES_ID = ? \
         AND b.DELETED_DATE IS NULL",
    )
    .bind(user_id_value)
    .bind(series_id)
    .fetch_one(&pool)
    .await
    .map_err(|error| format!("query series read progress aggregates: {error}"))?;

    let read_count = row.get::<i64, _>("READ_COUNT");
    let in_progress_count = row.get::<i64, _>("IN_PROGRESS_COUNT");
    let most_recent_read_date = row.get::<Option<String>, _>("MOST_RECENT_READ_DATE");

    sqlx::query(
        "INSERT INTO READ_PROGRESS_SERIES ( SERIES_ID, USER_ID, READ_COUNT, IN_PROGRESS_COUNT, \
           MOST_RECENT_READ_DATE, LAST_MODIFIED_DATE ) \
         VALUES (?, ?, ?, ?, ?, CURRENT_TIMESTAMP) \
         ON CONFLICT(SERIES_ID, USER_ID) DO UPDATE \
         SET READ_COUNT = excluded.READ_COUNT, IN_PROGRESS_COUNT = excluded.IN_PROGRESS_COUNT, \
             MOST_RECENT_READ_DATE = excluded.MOST_RECENT_READ_DATE, \
             LAST_MODIFIED_DATE = CURRENT_TIMESTAMP",
    )
    .bind(series_id)
    .bind(user_id_value)
    .bind(read_count)
    .bind(in_progress_count)
    .bind(most_recent_read_date)
    .execute(&pool)
    .await
    .map_err(|error| format!("upsert series read progress row: {error}"))?;

    Ok(())
}

async fn load_series_tachiyomi_progress(
    database_file: &FsPath,
    series_id: &str,
    user_id_value: &str,
) -> Result<Option<Value>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series tachiyomi db: {error}"))?;

    let rows = sqlx::query(
        "SELECT COALESCE(bm.NUMBER_SORT, 0) AS NUMBER_SORT, \
                COALESCE(rp.COMPLETED, 0) AS COMPLETED, COALESCE(rp.PAGE, 0) AS PAGE \
         FROM BOOK b \
         LEFT \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         LEFT \
         JOIN READ_PROGRESS rp ON rp.BOOK_ID = b.ID \
         AND rp.USER_ID = ? \
         WHERE b.SERIES_ID = ? \
         AND b.DELETED_DATE IS NULL \
         ORDER BY COALESCE(bm.NUMBER_SORT, 0) ASC, b.ID ASC",
    )
    .bind(user_id_value)
    .bind(series_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query series tachiyomi rows: {error}"))?;

    if rows.is_empty() {
        return Ok(None);
    }

    let mut books_count = 0usize;
    let mut books_read_count = 0usize;
    let mut books_in_progress_count = 0usize;
    let mut last_read_continuous_number_sort = 0.0f64;
    let mut max_number_sort = 0.0f64;
    let mut all_previous_completed = true;

    for row in rows {
        books_count += 1;
        let number_sort = row.get::<f64, _>("NUMBER_SORT");
        let completed = row.get::<i64, _>("COMPLETED") != 0;
        let page = row.get::<i64, _>("PAGE");
        if number_sort > max_number_sort {
            max_number_sort = number_sort;
        }

        if completed {
            books_read_count += 1;
            if all_previous_completed {
                last_read_continuous_number_sort = number_sort;
            }
        } else if page > 0 {
            books_in_progress_count += 1;
            all_previous_completed = false;
        } else {
            all_previous_completed = false;
        }
    }

    let books_unread_count = books_count
        .saturating_sub(books_read_count)
        .saturating_sub(books_in_progress_count);

    Ok(Some(json!({
        "booksCount": books_count,
        "booksReadCount": books_read_count,
        "booksUnreadCount": books_unread_count,
        "booksInProgressCount": books_in_progress_count,
        "lastReadContinuousNumberSort": last_read_continuous_number_sort,
        "maxNumberSort": max_number_sort,
    })))
}

fn is_font_resource(resource_name: &str) -> bool {
    matches!(
        resource_name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .unwrap_or_default()
            .as_str(),
        "ttf" | "otf" | "woff" | "woff2"
    )
}

fn read_epub_resource_bytes(epub_path: &FsPath, resource_name: &str) -> Option<Vec<u8>> {
    let file = fs::File::open(epub_path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    read_zip_entry_bytes(&mut archive, resource_name)
}

async fn persisted_book_ids(database_file: &FsPath) -> Result<Vec<String>, String> {
    if !database_file.exists() {
        return Ok(Vec::new());
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book list db: {error}"))?;

    let rows = sqlx::query(
        "SELECT ID \
                            FROM BOOK \
                            ORDER BY ID ASC",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted book ids: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("ID"))
        .collect())
}

fn process_task_side_effects(
    state: &super::super::OperationalState,
    task_records: Vec<TaskQueueRecord>,
) -> Result<(), String> {
    let mut task_queue = state
        .task_queue
        .lock()
        .expect("task queue state lock should not be poisoned");
    for task in task_records {
        task_queue.enqueue(task);
    }

    task_queue
        .process_available(&state.runtime)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn enqueue_task_records(
    state: &super::super::OperationalState,
    task_records: Vec<TaskQueueRecord>,
) -> Response {
    if let Err(error) = process_task_side_effects(state, task_records) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": error })),
        )
            .into_response();
    }

    let mut response = StatusCode::ACCEPTED.into_response();
    mark_native(&mut response);
    response
}

async fn load_existing_book_metadata(
    database_file: &FsPath,
    book_id: &str,
) -> Result<Option<PersistedBookMetadata>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book metadata db: {error}"))?;

    let row = sqlx::query(
        "SELECT TITLE, TITLE_LOCK, SUMMARY, SUMMARY_LOCK, NUMBER, NUMBER_LOCK, NUMBER_SORT, \
                NUMBER_SORT_LOCK, RELEASE_DATE, RELEASE_DATE_LOCK, AUTHORS_LOCK, TAGS_LOCK, ISBN, \
                ISBN_LOCK, LINKS_LOCK \
         FROM BOOK_METADATA \
         WHERE BOOK_ID = ? \
         LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query existing book metadata: {error}"))?;

    let Some(row) = row else {
        return Ok(None);
    };

    let author_rows = sqlx::query(
        "SELECT NAME, ROLE \
         FROM BOOK_METADATA_AUTHOR \
         WHERE BOOK_ID = ? \
         ORDER BY ROLE ASC, NAME ASC",
    )
    .bind(book_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query existing book metadata authors: {error}"))?;

    let tag_rows = sqlx::query(
        "SELECT TAG \
         FROM BOOK_METADATA_TAG \
         WHERE BOOK_ID = ? \
         ORDER BY TAG COLLATE NOCASE ASC",
    )
    .bind(book_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query existing book metadata tags: {error}"))?;

    let link_rows = sqlx::query(
        "SELECT LABEL, URL \
         FROM BOOK_METADATA_LINK \
         WHERE BOOK_ID = ? \
         ORDER BY LABEL COLLATE NOCASE ASC, URL ASC",
    )
    .bind(book_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query existing book metadata links: {error}"))?;

    Ok(Some(PersistedBookMetadata {
        title: row.get::<String, _>("TITLE"),
        title_lock: row.get::<i64, _>("TITLE_LOCK") != 0,
        summary: row.get::<String, _>("SUMMARY"),
        summary_lock: row.get::<i64, _>("SUMMARY_LOCK") != 0,
        number: row.get::<String, _>("NUMBER"),
        number_lock: row.get::<i64, _>("NUMBER_LOCK") != 0,
        number_sort: row.get::<f64, _>("NUMBER_SORT"),
        number_sort_lock: row.get::<i64, _>("NUMBER_SORT_LOCK") != 0,
        release_date: row.get::<Option<String>, _>("RELEASE_DATE"),
        release_date_lock: row.get::<i64, _>("RELEASE_DATE_LOCK") != 0,
        authors: author_rows
            .into_iter()
            .map(|entry| {
                (
                    entry.get::<String, _>("NAME"),
                    entry.get::<String, _>("ROLE"),
                )
            })
            .collect(),
        authors_lock: row.get::<i64, _>("AUTHORS_LOCK") != 0,
        tags: tag_rows
            .into_iter()
            .map(|entry| entry.get::<String, _>("TAG"))
            .collect(),
        tags_lock: row.get::<i64, _>("TAGS_LOCK") != 0,
        isbn: row.get::<String, _>("ISBN"),
        isbn_lock: row.get::<i64, _>("ISBN_LOCK") != 0,
        links: link_rows
            .into_iter()
            .map(|entry| {
                (
                    entry.get::<String, _>("LABEL"),
                    entry.get::<String, _>("URL"),
                )
            })
            .collect(),
        links_lock: row.get::<i64, _>("LINKS_LOCK") != 0,
    }))
}

async fn load_book_series_id(
    database_file: &FsPath,
    book_id: &str,
) -> Result<Option<String>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book series db: {error}"))?;
    let row = sqlx::query(
        "SELECT SERIES_ID \
         FROM BOOK \
         WHERE ID = ? \
         LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query book series id: {error}"))?;

    Ok(row.map(|row| row.get::<String, _>("SERIES_ID")))
}

fn apply_book_metadata_patch(
    mut existing: PersistedBookMetadata,
    patch: &serde_json::Map<String, Value>,
) -> Result<PersistedBookMetadata, String> {
    if let Some(value) = patch.get("title") {
        if !value.is_null() {
            let title = value
                .as_str()
                .ok_or_else(|| "title must be a string or null".to_string())?;
            if title.trim().is_empty() {
                return Err("title must not be blank".to_string());
            }
            existing.title = title.to_string();
        }
    }

    existing.title_lock = patched_bool_or_keep(patch, "titleLock", existing.title_lock)?;

    if let Some(value) = patch.get("summary") {
        if value.is_null() {
            existing.summary = String::new();
        } else {
            existing.summary = value
                .as_str()
                .ok_or_else(|| "summary must be a string or null".to_string())?
                .to_string();
        }
    }

    existing.summary_lock = patched_bool_or_keep(patch, "summaryLock", existing.summary_lock)?;

    if let Some(value) = patch.get("number") {
        if !value.is_null() {
            let number = value
                .as_str()
                .ok_or_else(|| "number must be a string or null".to_string())?;
            if number.trim().is_empty() {
                return Err("number must not be blank".to_string());
            }
            existing.number = number.to_string();
        }
    }

    existing.number_lock = patched_bool_or_keep(patch, "numberLock", existing.number_lock)?;

    if let Some(value) = patch.get("numberSort") {
        if !value.is_null() {
            existing.number_sort = value
                .as_f64()
                .ok_or_else(|| "numberSort must be a number or null".to_string())?;
        }
    }

    existing.number_sort_lock =
        patched_bool_or_keep(patch, "numberSortLock", existing.number_sort_lock)?;

    if let Some(value) = patch.get("releaseDate") {
        if value.is_null() {
            existing.release_date = None;
        } else {
            let release_date = value
                .as_str()
                .ok_or_else(|| "releaseDate must be a string or null".to_string())?;
            existing.release_date = Some(release_date.to_string());
        }
    }

    existing.release_date_lock =
        patched_bool_or_keep(patch, "releaseDateLock", existing.release_date_lock)?;

    if let Some(value) = patch.get("authors") {
        if value.is_null() {
            existing.authors = Vec::new();
        } else {
            let authors = value
                .as_array()
                .ok_or_else(|| "authors must be an array or null".to_string())?;
            let mut parsed = Vec::with_capacity(authors.len());
            for author in authors {
                let author = author
                    .as_object()
                    .ok_or_else(|| "authors entries must be objects".to_string())?;
                let name = author
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "author.name must be a non-empty string".to_string())?;
                let role = author
                    .get("role")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "author.role must be a non-empty string".to_string())?;
                if name.trim().is_empty() || role.trim().is_empty() {
                    return Err("author name/role must not be blank".to_string());
                }
                parsed.push((name.to_string(), role.to_string()));
            }
            existing.authors = parsed;
        }
    }

    existing.authors_lock = patched_bool_or_keep(patch, "authorsLock", existing.authors_lock)?;

    if let Some(value) = patch.get("tags") {
        if value.is_null() {
            existing.tags = Vec::new();
        } else {
            let tags = value
                .as_array()
                .ok_or_else(|| "tags must be an array or null".to_string())?;
            let mut parsed = tags
                .iter()
                .map(|tag| {
                    tag.as_str()
                        .map(ToString::to_string)
                        .ok_or_else(|| "tags entries must be strings".to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            parsed.sort();
            parsed.dedup();
            existing.tags = parsed;
        }
    }

    existing.tags_lock = patched_bool_or_keep(patch, "tagsLock", existing.tags_lock)?;

    if let Some(value) = patch.get("isbn") {
        if value.is_null() {
            existing.isbn = String::new();
        } else {
            let isbn = value
                .as_str()
                .ok_or_else(|| "isbn must be a string or null".to_string())?;
            existing.isbn = isbn
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect::<String>();
        }
    }

    existing.isbn_lock = patched_bool_or_keep(patch, "isbnLock", existing.isbn_lock)?;

    if let Some(value) = patch.get("links") {
        if value.is_null() {
            existing.links = Vec::new();
        } else {
            let links = value
                .as_array()
                .ok_or_else(|| "links must be an array or null".to_string())?;
            let mut parsed = Vec::with_capacity(links.len());
            for link in links {
                let link = link
                    .as_object()
                    .ok_or_else(|| "links entries must be objects".to_string())?;
                let label = link
                    .get("label")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "link.label must be a non-empty string".to_string())?;
                let url = link
                    .get("url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "link.url must be a non-empty string".to_string())?;
                if label.trim().is_empty() || url.trim().is_empty() {
                    return Err("link label/url must not be blank".to_string());
                }
                parsed.push((label.to_string(), url.to_string()));
            }
            existing.links = parsed;
        }
    }

    existing.links_lock = patched_bool_or_keep(patch, "linksLock", existing.links_lock)?;

    Ok(existing)
}

fn patched_bool_or_keep(
    patch: &serde_json::Map<String, Value>,
    key: &str,
    current: bool,
) -> Result<bool, String> {
    match patch.get(key) {
        Some(value) if value.is_null() => Ok(current),
        Some(value) => value
            .as_bool()
            .ok_or_else(|| format!("{key} must be a boolean or null")),
        None => Ok(current),
    }
}

async fn persist_book_metadata_update(
    database_file: &FsPath,
    book_id: &str,
    metadata: &PersistedBookMetadata,
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book metadata update db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin book metadata update tx: {error}"))?;

    let exists = sqlx::query(
        "SELECT 1 AS FOUND \
                              FROM BOOK_METADATA \
                              WHERE BOOK_ID = ? \
                              LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query book metadata existence for update: {error}"))?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback book metadata update tx: {error}"))?;
        return Ok(false);
    }

    sqlx::query(
        "UPDATE BOOK_METADATA \
         SET TITLE = ?, TITLE_LOCK = ?, SUMMARY = ?, SUMMARY_LOCK = ?, NUMBER = ?, \
             NUMBER_LOCK = ?, NUMBER_SORT = ?, NUMBER_SORT_LOCK = ?, RELEASE_DATE = ?, \
             RELEASE_DATE_LOCK = ?, AUTHORS_LOCK = ?, TAGS_LOCK = ?, ISBN = ?, ISBN_LOCK = ?, \
             LINKS_LOCK = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
         WHERE BOOK_ID = ?",
    )
    .bind(&metadata.title)
    .bind(metadata.title_lock)
    .bind(&metadata.summary)
    .bind(metadata.summary_lock)
    .bind(&metadata.number)
    .bind(metadata.number_lock)
    .bind(metadata.number_sort)
    .bind(metadata.number_sort_lock)
    .bind(metadata.release_date.as_deref())
    .bind(metadata.release_date_lock)
    .bind(metadata.authors_lock)
    .bind(metadata.tags_lock)
    .bind(&metadata.isbn)
    .bind(metadata.isbn_lock)
    .bind(metadata.links_lock)
    .bind(book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("update book metadata: {error}"))?;

    sqlx::query(
        "UPDATE BOOK \
         SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
         WHERE ID = ?",
    )
    .bind(book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("touch book last modified after metadata update: {error}"))?;

    sqlx::query(
        "DELETE \
                 FROM BOOK_METADATA_AUTHOR \
                 WHERE BOOK_ID = ?",
    )
    .bind(book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete existing book metadata authors: {error}"))?;
    for (name, role) in &metadata.authors {
        sqlx::query(
            "INSERT INTO BOOK_METADATA_AUTHOR (NAME, ROLE, BOOK_ID) \
                     VALUES (?, ?, ?)",
        )
        .bind(name)
        .bind(role)
        .bind(book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("insert book metadata author: {error}"))?;
    }

    sqlx::query(
        "DELETE \
                 FROM BOOK_METADATA_TAG \
                 WHERE BOOK_ID = ?",
    )
    .bind(book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete existing book metadata tags: {error}"))?;
    for tag in &metadata.tags {
        sqlx::query(
            "INSERT INTO BOOK_METADATA_TAG (TAG, BOOK_ID) \
                     VALUES (?, ?)",
        )
        .bind(tag)
        .bind(book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("insert book metadata tag: {error}"))?;
    }

    sqlx::query(
        "DELETE \
                 FROM BOOK_METADATA_LINK \
                 WHERE BOOK_ID = ?",
    )
    .bind(book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete existing book metadata links: {error}"))?;
    for (label, url) in &metadata.links {
        sqlx::query(
            "INSERT INTO BOOK_METADATA_LINK (LABEL, URL, BOOK_ID) \
                     VALUES (?, ?, ?)",
        )
        .bind(label)
        .bind(url)
        .bind(book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("insert book metadata link: {error}"))?;
    }

    tx.commit()
        .await
        .map_err(|error| format!("commit book metadata update tx: {error}"))?;
    Ok(true)
}

fn parse_books_import_payload(body: &Value) -> Result<BooksImportPayload, String> {
    let body = body
        .as_object()
        .ok_or_else(|| "books import payload must be a JSON object".to_string())?;

    let copy_mode = match body.get("copyMode").and_then(Value::as_str) {
        Some("MOVE") => ImportCopyMode::Move,
        Some("COPY") => ImportCopyMode::Copy,
        Some("HARDLINK") => ImportCopyMode::Hardlink,
        Some(_) => {
            return Err("copyMode must be one of MOVE, COPY, HARDLINK".to_string());
        }
        None => {
            return Err("copyMode is required".to_string());
        }
    };

    let books = body
        .get("books")
        .and_then(Value::as_array)
        .ok_or_else(|| "books must be an array".to_string())?
        .iter()
        .map(|entry| {
            let entry = entry
                .as_object()
                .ok_or_else(|| "books entries must be objects".to_string())?;

            let source_file = entry
                .get("sourceFile")
                .and_then(Value::as_str)
                .ok_or_else(|| "books[].sourceFile must be a string".to_string())?;
            let series_id = entry
                .get("seriesId")
                .and_then(Value::as_str)
                .ok_or_else(|| "books[].seriesId must be a string".to_string())?;
            if source_file.trim().is_empty() || series_id.trim().is_empty() {
                return Err("books[].sourceFile and books[].seriesId must not be blank".to_string());
            }

            let destination_name = entry
                .get("destinationName")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string);

            let upgrade_book_id = entry
                .get("upgradeBookId")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string);

            Ok(BooksImportEntry {
                source_file: PathBuf::from(source_file),
                series_id: series_id.to_string(),
                destination_name,
                upgrade_book_id,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(BooksImportPayload { copy_mode, books })
}

async fn load_import_series_target(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Option<ImportSeriesTarget>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series target db: {error}"))?;

    let row = sqlx::query(
        "SELECT s.ID AS SERIES_ID, s.LIBRARY_ID AS LIBRARY_ID, s.URL AS SERIES_URL, \
                l.ROOT AS LIBRARY_ROOT \
         FROM SERIES s \
         JOIN LIBRARY l ON l.ID = s.LIBRARY_ID \
         WHERE s.ID = ? \
         LIMIT 1",
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query series target for import: {error}"))?;

    Ok(row.map(|row| ImportSeriesTarget {
        series_id: row.get::<String, _>("SERIES_ID"),
        library_id: row.get::<String, _>("LIBRARY_ID"),
        library_root: row.get::<String, _>("LIBRARY_ROOT"),
        series_url: row.get::<String, _>("SERIES_URL"),
    }))
}

async fn load_import_upgrade_book_target(
    database_file: &FsPath,
    book_id: &str,
) -> Result<Option<ImportUpgradeBookTarget>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open upgrade book target db: {error}"))?;

    let row = sqlx::query(
        "SELECT b.SERIES_ID AS SERIES_ID, b.NAME AS FILE_NAME, s.URL AS SERIES_URL, \
                l.ROOT AS LIBRARY_ROOT \
         FROM BOOK b \
         JOIN SERIES s ON s.ID = b.SERIES_ID \
         JOIN LIBRARY l ON l.ID = b.LIBRARY_ID \
         WHERE b.ID = ? \
         LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query upgrade book target for import: {error}"))?;

    Ok(row.map(|row| ImportUpgradeBookTarget {
        series_id: row.get::<String, _>("SERIES_ID"),
        library_root: row.get::<String, _>("LIBRARY_ROOT"),
        series_url: row.get::<String, _>("SERIES_URL"),
        file_name: row.get::<String, _>("FILE_NAME"),
    }))
}

async fn load_library_roots(database_file: &FsPath) -> Result<Vec<PathBuf>, String> {
    if !database_file.exists() {
        return Ok(Vec::new());
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open library roots db: {error}"))?;

    let rows = sqlx::query(
        "SELECT ROOT \
                            FROM LIBRARY",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query library roots for import: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| PathBuf::from(row.get::<String, _>("ROOT")))
        .collect())
}

fn source_inside_library_roots(source_file: &FsPath, library_roots: &[PathBuf]) -> bool {
    let source = fs::canonicalize(source_file).unwrap_or_else(|_| source_file.to_path_buf());

    library_roots.iter().any(|root| {
        let root = fs::canonicalize(root).unwrap_or_else(|_| root.clone());
        source.starts_with(root)
    })
}

fn resolve_import_destination_name(
    source_file: &FsPath,
    destination_name: Option<&str>,
) -> Option<String> {
    if let Some(destination_name) = destination_name {
        if destination_name.contains('/') || destination_name.contains('\\') {
            return None;
        }
        return Some(destination_name.to_string());
    }

    source_file
        .file_name()
        .and_then(|value| value.to_str())
        .map(ToString::to_string)
}

fn apply_import_copy_mode(
    copy_mode: ImportCopyMode,
    source_file: &FsPath,
    destination_file: &FsPath,
) -> Result<(), String> {
    if !source_file.exists() {
        return Err("source file does not exist".to_string());
    }

    if destination_file.exists() {
        fs::remove_file(destination_file)
            .map_err(|error| format!("remove existing destination file: {error}"))?;
    }

    match copy_mode {
        ImportCopyMode::Copy => {
            fs::copy(source_file, destination_file)
                .map_err(|error| format!("copy source file for import: {error}"))?;
            Ok(())
        }
        ImportCopyMode::Move => {
            if let Err(error) = fs::rename(source_file, destination_file) {
                fs::copy(source_file, destination_file)
                    .map_err(|copy_error| {
                        format!(
                            "move source file for import failed ({error}); copy fallback failed: {copy_error}"
                        )
                    })?;
                fs::remove_file(source_file).map_err(|remove_error| {
                    format!("remove source file after move fallback copy: {remove_error}")
                })?;
            }
            Ok(())
        }
        ImportCopyMode::Hardlink => {
            if fs::hard_link(source_file, destination_file).is_err() {
                fs::copy(source_file, destination_file)
                    .map_err(|error| format!("hardlink/copy source file for import: {error}"))?;
            }
            Ok(())
        }
    }
}

fn import_book_sidecars(
    copy_mode: ImportCopyMode,
    source_file: &FsPath,
    destination_file: &FsPath,
) -> Result<bool, String> {
    let source_sidecar = source_file.with_extension("xml");
    if !source_sidecar.exists() {
        return Ok(false);
    }

    let destination_sidecar = destination_file.with_extension("xml");
    apply_import_copy_mode(
        copy_mode,
        source_sidecar.as_path(),
        destination_sidecar.as_path(),
    )?;
    Ok(true)
}

fn scanner_book_id_for_path(path: &FsPath) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    format!("book-{:016x}", hasher.finish())
}

async fn migrate_upgraded_book_identity(
    database_file: &FsPath,
    old_book_id: &str,
    new_book_id: &str,
    destination_file: &FsPath,
) -> Result<(), String> {
    if old_book_id == new_book_id || !database_file.exists() {
        return Ok(());
    }

    let destination_name = destination_file
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();
    let destination_url = destination_file.to_string_lossy().to_string();

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open import-upgrade migration db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin import-upgrade migration tx: {error}"))?;

    let source_exists = sqlx::query(
        "SELECT 1 AS FOUND \
                                     FROM BOOK \
                                     WHERE ID = ? \
                                     LIMIT 1",
    )
    .bind(old_book_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query upgraded source book for migration: {error}"))?
    .is_some();
    if !source_exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback import-upgrade migration tx: {error}"))?;
        return Ok(());
    }

    sqlx::query(
        "INSERT INTO BOOK (ID, CREATED_DATE, LAST_MODIFIED_DATE, FILE_LAST_MODIFIED, NAME, URL, \
           SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, FILE_HASH, DELETED_DATE, oneshot, \
           FILE_HASH_KOREADER) SELECT ?, \
           CREATED_DATE, LAST_MODIFIED_DATE, FILE_LAST_MODIFIED, ?, ?, SERIES_ID, FILE_SIZE, \
           NUMBER, LIBRARY_ID, FILE_HASH, DELETED_DATE, oneshot, FILE_HASH_KOREADER \
         FROM BOOK \
         WHERE ID = ? \
         ON CONFLICT(ID) DO UPDATE \
         SET FILE_LAST_MODIFIED = excluded.FILE_LAST_MODIFIED, NAME = excluded.NAME, \
             URL = excluded.URL, SERIES_ID = excluded.SERIES_ID, FILE_SIZE = excluded.FILE_SIZE, \
             NUMBER = excluded.NUMBER, LIBRARY_ID = excluded.LIBRARY_ID, \
             FILE_HASH = excluded.FILE_HASH, DELETED_DATE = excluded.DELETED_DATE, \
             oneshot = excluded.oneshot, FILE_HASH_KOREADER = excluded.FILE_HASH_KOREADER, \
             LAST_MODIFIED_DATE = CURRENT_TIMESTAMP",
    )
    .bind(new_book_id)
    .bind(destination_name)
    .bind(destination_url)
    .bind(old_book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("upsert upgraded destination book identity: {error}"))?;

    sqlx::query(
        "DELETE \
                 FROM BOOK_METADATA \
                 WHERE BOOK_ID = ?",
    )
    .bind(new_book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete destination metadata before upgrade migration: {error}"))?;
    sqlx::query(
        "UPDATE BOOK_METADATA \
                 SET BOOK_ID = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                 WHERE BOOK_ID = ?",
    )
    .bind(new_book_id)
    .bind(old_book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("move book metadata during upgrade migration: {error}"))?;

    for table in [
        "BOOK_METADATA_AUTHOR",
        "BOOK_METADATA_TAG",
        "BOOK_METADATA_LINK",
    ] {
        sqlx::query(&format!(
            "DELETE \
                              FROM {table} \
                              WHERE BOOK_ID = ?"
        ))
        .bind(new_book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            format!("delete destination {table} rows before upgrade migration: {error}")
        })?;

        sqlx::query(&format!(
            "UPDATE {table} \
                              SET BOOK_ID = ? \
                              WHERE BOOK_ID = ?"
        ))
        .bind(new_book_id)
        .bind(old_book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("move {table} rows during upgrade migration: {error}"))?;
    }

    for table in ["MEDIA", "MEDIA_FILE", "MEDIA_PAGE", "READ_PROGRESS"] {
        sqlx::query(&format!(
            "DELETE \
                              FROM {table} \
                              WHERE BOOK_ID = ?"
        ))
        .bind(new_book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            format!("delete destination {table} rows before upgrade migration: {error}")
        })?;

        sqlx::query(&format!(
            "UPDATE {table} \
                              SET BOOK_ID = ? \
                              WHERE BOOK_ID = ?"
        ))
        .bind(new_book_id)
        .bind(old_book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("move {table} rows during upgrade migration: {error}"))?;
    }

    sqlx::query(
        "DELETE \
                 FROM THUMBNAIL_BOOK \
                 WHERE BOOK_ID = ? \
                 AND TYPE = 'USER_UPLOADED'",
    )
    .bind(new_book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        format!("delete destination user-uploaded thumbnails before upgrade migration: {error}")
    })?;
    sqlx::query(
        "UPDATE THUMBNAIL_BOOK \
         SET BOOK_ID = ? \
         WHERE BOOK_ID = ? \
         AND TYPE = 'USER_UPLOADED'",
    )
    .bind(new_book_id)
    .bind(old_book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("move user-uploaded thumbnails during upgrade migration: {error}"))?;

    sqlx::query(
        "INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) SELECT READLIST_ID, ?, NUMBER \
         FROM READLIST_BOOK \
         WHERE BOOK_ID = ? \
         ON CONFLICT(READLIST_ID, BOOK_ID) DO NOTHING",
    )
    .bind(new_book_id)
    .bind(old_book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("copy readlist mapping rows during upgrade migration: {error}"))?;
    sqlx::query(
        "DELETE \
                 FROM READLIST_BOOK \
                 WHERE BOOK_ID = ?",
    )
    .bind(old_book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        format!("delete source readlist mappings during upgrade migration: {error}")
    })?;

    sqlx::query(
        "DELETE \
                 FROM THUMBNAIL_BOOK \
                 WHERE BOOK_ID = ?",
    )
    .bind(old_book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete source thumbnails during upgrade migration: {error}"))?;

    sqlx::query(
        "DELETE \
                 FROM BOOK \
                 WHERE ID = ?",
    )
    .bind(old_book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete source book after upgrade migration: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit import-upgrade migration tx: {error}"))?;
    Ok(())
}

async fn persist_book_imported_event(
    database_file: &FsPath,
    book_id: &str,
    series_id: &str,
    destination_file: &FsPath,
    source_file: &FsPath,
    upgrade: bool,
) -> Result<(), String> {
    if !database_file.exists() {
        return Ok(());
    }

    let event_id = generated_historical_event_id();
    let destination_name = destination_file.to_string_lossy().to_string();
    let source_name = source_file.to_string_lossy().to_string();
    let upgrade_value = if upgrade { "Yes" } else { "No" };

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open historical-event db for import: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin historical-event tx for import: {error}"))?;

    sqlx::query(
        "INSERT INTO HISTORICAL_EVENT (ID, TYPE, BOOK_ID, SERIES_ID) \
                 VALUES (?, ?, ?, ?)",
    )
    .bind(&event_id)
    .bind("BookImported")
    .bind(book_id)
    .bind(series_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("insert historical BookImported event: {error}"))?;

    for (key, value) in [
        ("name", destination_name.as_str()),
        ("source", source_name.as_str()),
        ("upgrade", upgrade_value),
    ] {
        sqlx::query(
            "INSERT INTO HISTORICAL_EVENT_PROPERTIES (ID, KEY, VALUE) \
                     VALUES (?, ?, ?)",
        )
        .bind(&event_id)
        .bind(key)
        .bind(value)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            format!("insert historical event property '{key}' for BookImported: {error}")
        })?;
    }

    tx.commit()
        .await
        .map_err(|error| format!("commit historical-event tx for import: {error}"))?;

    Ok(())
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

    let user_exists = sqlx::query(
        "SELECT 1 \
         FROM USER \
         WHERE ID = ? \
         LIMIT 1",
    )
    .bind(user_id_value)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query read-progress user: {error}"))?
    .is_some();

    if !user_exists {
        return Err("read-progress user not found".to_string());
    }

    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) \
         VALUES (?, ?, ?, ?) \
         ON CONFLICT(BOOK_ID, USER_ID) DO UPDATE \
         SET PAGE = excluded.PAGE, COMPLETED = excluded.COMPLETED, \
             LAST_MODIFIED_DATE = CURRENT_TIMESTAMP",
    )
    .bind(book_id)
    .bind(user_id_value)
    .bind(page as i64)
    .bind(completed)
    .execute(&pool)
    .await
    .map_err(|error| format!("persist read-progress: {error}"))?;

    Ok(())
}

async fn persist_book_progression(
    database_file: &FsPath,
    book_id: &str,
    user_id_value: &str,
    progression: f64,
) -> Result<(), String> {
    let page_count = load_book_page_count(database_file, book_id)
        .await?
        .unwrap_or(1)
        .max(1);
    let page = ((progression * page_count as f64).ceil() as u64).clamp(0, page_count as u64);
    let completed = progression >= 1.0;

    persist_read_progress(database_file, book_id, user_id_value, page, completed).await
}

async fn load_book_progression(
    database_file: &FsPath,
    book_id: &str,
    user_id_value: &str,
) -> Result<Option<f64>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book progression db: {error}"))?;

    let row = sqlx::query(
        "SELECT PAGE \
                     FROM READ_PROGRESS \
                     WHERE BOOK_ID = ? \
                     AND USER_ID = ? \
                     LIMIT 1",
    )
    .bind(book_id)
    .bind(user_id_value)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted book progression: {error}"))?;

    let Some(row) = row else {
        return Ok(None);
    };

    let page = row.get::<i64, _>("PAGE").max(0) as u64;
    let page_count = load_book_page_count(database_file, book_id)
        .await?
        .unwrap_or(1)
        .max(1) as f64;
    Ok(Some((page as f64 / page_count).clamp(0.0, 1.0)))
}

async fn load_book_page_count(
    database_file: &FsPath,
    book_id: &str,
) -> Result<Option<u64>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book page-count db: {error}"))?;

    let row = sqlx::query(
        "SELECT PAGE_COUNT \
                           FROM MEDIA \
                           WHERE BOOK_ID = ? \
                           LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query book page-count: {error}"))?;

    Ok(row.map(|row| row.get::<i64, _>("PAGE_COUNT").max(0) as u64))
}

async fn load_effective_kepubify_path(
    database_file: &FsPath,
    runtime_kepubify_path: Option<&FsPath>,
) -> Option<PathBuf> {
    if database_file.exists()
        && let Ok(pool) = connect_pool(database_file, 1).await
    {
        let persisted = sqlx::query(
            "SELECT VALUE \
                         FROM SERVER_SETTINGS \
                         WHERE KEY = 'KEPUBIFY_PATH' \
                         LIMIT 1",
        )
        .fetch_optional(&pool)
        .await
        .ok()
        .and_then(|row| row)
        .and_then(|row| row.get::<Option<String>, _>("VALUE"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);

        if persisted.is_some() {
            return persisted;
        }
    }

    runtime_kepubify_path.map(PathBuf::from)
}

async fn load_persisted_epub_positions(
    database_file: &FsPath,
    book_id: &str,
) -> Result<Option<Vec<Value>>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open epub extension db: {error}"))?;

    let row = sqlx::query(
        "SELECT EXTENSION_CLASS, EXTENSION_VALUE_BLOB \
         FROM MEDIA \
         WHERE BOOK_ID = ? \
         LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query epub extension blob: {error}"))?;

    let Some(row) = row else {
        return Ok(None);
    };

    let extension_class = row
        .get::<Option<String>, _>("EXTENSION_CLASS")
        .unwrap_or_default();
    if !extension_class.is_empty()
        && !extension_class
            .to_ascii_lowercase()
            .contains("mediaextensionepub")
    {
        return Ok(None);
    }

    let Some(blob) = row.get::<Option<Vec<u8>>, _>("EXTENSION_VALUE_BLOB") else {
        return Ok(None);
    };

    let positions = decode_epub_positions_blob(&blob)?;

    if positions.is_empty() {
        Ok(None)
    } else {
        Ok(Some(positions))
    }
}

fn decode_epub_positions_blob(blob: &[u8]) -> Result<Vec<Value>, String> {
    let mut decoder = GzDecoder::new(blob);
    let mut decoded = Vec::new();
    decoder
        .read_to_end(&mut decoded)
        .map_err(|error| format!("decode epub extension blob: {error}"))?;

    let extension = serde_json::from_slice::<Value>(&decoded)
        .map_err(|error| format!("parse epub extension blob json: {error}"))?;
    Ok(extension
        .get("positions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

fn load_epub_archive_positions_fallback(
    media: &PersistedBookMedia,
    kepubify_path: Option<&FsPath>,
) -> Option<Vec<Value>> {
    if !book_media_is_epub(media) {
        return None;
    }

    let file = fs::File::open(&media.file_path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let container_xml = read_zip_entry_bytes(&mut archive, "META-INF/container.xml")?;
    let rootfile_path = parse_epub_rootfile_path(&container_xml)?;
    let package_document = read_zip_entry_bytes(&mut archive, &rootfile_path)?;
    let spine_entries = parse_epub_spine_entries(&package_document, &rootfile_path);
    if spine_entries.is_empty() {
        return None;
    }
    let fixed_layout = parse_epub_fixed_layout(&package_document);
    let mut resources = spine_entries
        .into_iter()
        .map(|entry| {
            let bytes = read_zip_entry_bytes(&mut archive, &entry.href).unwrap_or_default();
            let kobo_spans = if fixed_layout {
                vec![]
            } else {
                parse_epub_kobo_spans(&bytes)
            };
            EpubResourceSemantics {
                entry,
                bytes,
                kobo_spans,
            }
        })
        .collect::<Vec<_>>();

    if !fixed_layout
        && resources
            .iter()
            .all(|resource| resource.kobo_spans.is_empty())
    {
        let converted_kobo_spans = load_kepub_converted_spans(
            media,
            resources
                .iter()
                .map(|resource| resource.entry.clone())
                .collect::<Vec<_>>()
                .as_slice(),
            kepubify_path,
        );
        for resource in &mut resources {
            if let Some(spans) = converted_kobo_spans.get(&resource.entry.href)
                && !spans.is_empty()
            {
                resource.kobo_spans = spans.clone();
            }
        }
    }

    let mut raw_positions = Vec::new();

    for resource in resources {
        let position_count = if fixed_layout {
            1usize
        } else {
            ((resource.bytes.len() as f64 / 1024.0).ceil() as usize).max(1)
        };

        for segment_index in 0..position_count {
            let progression = if fixed_layout {
                0.0
            } else {
                segment_index as f64 / position_count as f64
            };
            let kobo_span = if fixed_layout || position_count == 1 || segment_index == 0 {
                Some("kobo.1.1".to_string())
            } else {
                closest_kobo_span(&resource.kobo_spans, progression)
            };

            raw_positions.push((
                resource.entry.href.clone(),
                resource.entry.media_type.clone(),
                progression,
                kobo_span,
            ));
        }
    }

    if raw_positions.is_empty() {
        return None;
    }

    let total_positions = raw_positions.len() as f64;
    let positions = raw_positions
        .into_iter()
        .enumerate()
        .map(|(index, (href, media_type, progression, kobo_span))| {
            let position = index + 1;
            let mut locator = json!({
                "href": href,
                "type": media_type,
                "locations": {
                    "position": position,
                    "progression": progression,
                    "totalProgression": position as f64 / total_positions,
                },
            });
            if let Some(kobo_span) = kobo_span
                && let Some(object) = locator.as_object_mut()
            {
                object.insert("koboSpan".to_string(), Value::String(kobo_span));
            }
            locator
        })
        .collect::<Vec<_>>();

    Some(positions)
}

#[derive(Clone)]
struct EpubSpineEntry {
    href: String,
    media_type: String,
}

struct EpubResourceSemantics {
    entry: EpubSpineEntry,
    bytes: Vec<u8>,
    kobo_spans: Vec<(String, f64)>,
}

fn load_kepub_converted_spans(
    media: &PersistedBookMedia,
    spine_entries: &[EpubSpineEntry],
    kepubify_path: Option<&FsPath>,
) -> HashMap<String, Vec<(String, f64)>> {
    let Some(kepubify_path) = kepubify_path else {
        return HashMap::new();
    };

    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let output_dir = std::env::temp_dir().join(format!("komga-kepubify-{suffix}"));
    if fs::create_dir_all(&output_dir).is_err() {
        return HashMap::new();
    }

    let mut span_map = HashMap::new();
    let converted_file = if run_kepubify_to_directory(kepubify_path, &media.file_path, &output_dir)
    {
        find_first_epub_file(&output_dir)
    } else {
        None
    };

    if let Some(converted_file) = converted_file
        && let Ok(file) = fs::File::open(converted_file)
        && let Ok(mut archive) = ZipArchive::new(file)
    {
        for entry in spine_entries {
            let bytes = read_zip_entry_bytes(&mut archive, &entry.href).unwrap_or_default();
            let spans = parse_epub_kobo_spans(&bytes);
            if !spans.is_empty() {
                span_map.insert(entry.href.clone(), spans);
            }
        }
    }

    let _ = fs::remove_dir_all(output_dir);
    span_map
}

fn run_kepubify_to_directory(
    kepubify_path: &FsPath,
    input_file: &FsPath,
    output_dir: &FsPath,
) -> bool {
    let attempts = [
        vec![
            "-o".to_string(),
            output_dir.to_string_lossy().to_string(),
            input_file.to_string_lossy().to_string(),
        ],
        vec![
            "--output".to_string(),
            output_dir.to_string_lossy().to_string(),
            input_file.to_string_lossy().to_string(),
        ],
        vec![
            input_file.to_string_lossy().to_string(),
            "-o".to_string(),
            output_dir.to_string_lossy().to_string(),
        ],
        vec![
            input_file.to_string_lossy().to_string(),
            "--output".to_string(),
            output_dir.to_string_lossy().to_string(),
        ],
    ];

    for args in attempts {
        if Command::new(kepubify_path)
            .args(args)
            .status()
            .is_ok_and(|status| status.success())
        {
            return true;
        }
    }

    false
}

fn find_first_epub_file(root: &FsPath) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];

    while let Some(directory) = stack.pop() {
        let entries = fs::read_dir(directory).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }

            if path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("epub"))
            {
                return Some(path);
            }
        }
    }

    None
}

fn parse_epub_rootfile_path(container_xml: &[u8]) -> Option<String> {
    let mut reader = XmlReader::from_reader(container_xml);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer).ok()? {
            XmlEvent::Start(event) | XmlEvent::Empty(event) => {
                if !xml_name_matches(event.name().as_ref(), b"rootfile") {
                    buffer.clear();
                    continue;
                }
                for attribute in event.attributes().flatten() {
                    if xml_name_matches(attribute.key.as_ref(), b"full-path") {
                        return attribute
                            .unescape_value()
                            .ok()
                            .map(|value| value.to_string());
                    }
                }
            }
            XmlEvent::Eof => break,
            _ => {}
        }
        buffer.clear();
    }

    None
}

fn parse_epub_spine_entries(package_document: &[u8], rootfile_path: &str) -> Vec<EpubSpineEntry> {
    let mut reader = XmlReader::from_reader(package_document);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut manifest = HashMap::<String, EpubSpineEntry>::new();
    let mut spine = Vec::<String>::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(XmlEvent::Start(event)) | Ok(XmlEvent::Empty(event)) => {
                if xml_name_matches(event.name().as_ref(), b"item") {
                    let mut id = None::<String>;
                    let mut href = None::<String>;
                    let mut media_type = None::<String>;

                    for attribute in event.attributes().flatten() {
                        if xml_name_matches(attribute.key.as_ref(), b"id") {
                            id = attribute
                                .unescape_value()
                                .ok()
                                .map(|value| value.to_string());
                        } else if xml_name_matches(attribute.key.as_ref(), b"href") {
                            href = attribute
                                .unescape_value()
                                .ok()
                                .map(|value| value.to_string());
                        } else if xml_name_matches(attribute.key.as_ref(), b"media-type") {
                            media_type = attribute
                                .unescape_value()
                                .ok()
                                .map(|value| value.to_string());
                        }
                    }

                    if let (Some(id), Some(href)) = (id, href) {
                        manifest.insert(
                            id,
                            EpubSpineEntry {
                                href: normalize_epub_resource_href(rootfile_path, &href),
                                media_type: media_type
                                    .unwrap_or_else(|| "application/xhtml+xml".to_string()),
                            },
                        );
                    }
                } else if xml_name_matches(event.name().as_ref(), b"itemref") {
                    for attribute in event.attributes().flatten() {
                        if xml_name_matches(attribute.key.as_ref(), b"idref")
                            && let Some(idref) = attribute
                                .unescape_value()
                                .ok()
                                .map(|value| value.to_string())
                        {
                            spine.push(idref);
                        }
                    }
                }
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buffer.clear();
    }

    if spine.is_empty() {
        return vec![];
    }

    spine
        .into_iter()
        .filter_map(|idref| manifest.get(&idref).cloned())
        .collect::<Vec<_>>()
}

fn parse_epub_fixed_layout(package_document: &[u8]) -> bool {
    let mut reader = XmlReader::from_reader(package_document);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut awaiting_rendition_layout_text = false;

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(XmlEvent::Start(event)) => {
                if !xml_name_matches(event.name().as_ref(), b"meta") {
                    buffer.clear();
                    continue;
                }

                let mut property = None::<String>;
                let mut name = None::<String>;
                let mut content = None::<String>;
                for attribute in event.attributes().flatten() {
                    if xml_name_matches(attribute.key.as_ref(), b"property") {
                        property = attribute
                            .unescape_value()
                            .ok()
                            .map(|value| value.to_string());
                    } else if xml_name_matches(attribute.key.as_ref(), b"name") {
                        name = attribute
                            .unescape_value()
                            .ok()
                            .map(|value| value.to_string());
                    } else if xml_name_matches(attribute.key.as_ref(), b"content") {
                        content = attribute
                            .unescape_value()
                            .ok()
                            .map(|value| value.to_string());
                    }
                }

                if property
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case("rendition:layout"))
                {
                    if content
                        .as_deref()
                        .is_some_and(|value| value.eq_ignore_ascii_case("pre-paginated"))
                    {
                        return true;
                    }
                    awaiting_rendition_layout_text = true;
                }

                if name
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case("fixed-layout"))
                    && content
                        .as_deref()
                        .is_some_and(|value| value.eq_ignore_ascii_case("true"))
                {
                    return true;
                }
            }
            Ok(XmlEvent::Empty(event)) => {
                if !xml_name_matches(event.name().as_ref(), b"meta") {
                    buffer.clear();
                    continue;
                }

                let mut property = None::<String>;
                let mut name = None::<String>;
                let mut content = None::<String>;
                for attribute in event.attributes().flatten() {
                    if xml_name_matches(attribute.key.as_ref(), b"property") {
                        property = attribute
                            .unescape_value()
                            .ok()
                            .map(|value| value.to_string());
                    } else if xml_name_matches(attribute.key.as_ref(), b"name") {
                        name = attribute
                            .unescape_value()
                            .ok()
                            .map(|value| value.to_string());
                    } else if xml_name_matches(attribute.key.as_ref(), b"content") {
                        content = attribute
                            .unescape_value()
                            .ok()
                            .map(|value| value.to_string());
                    }
                }

                if property
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case("rendition:layout"))
                    && content
                        .as_deref()
                        .is_some_and(|value| value.eq_ignore_ascii_case("pre-paginated"))
                {
                    return true;
                }

                if name
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case("fixed-layout"))
                    && content
                        .as_deref()
                        .is_some_and(|value| value.eq_ignore_ascii_case("true"))
                {
                    return true;
                }
            }
            Ok(XmlEvent::Text(text)) if awaiting_rendition_layout_text => {
                let value = String::from_utf8_lossy(text.as_ref()).trim().to_string();
                if value.eq_ignore_ascii_case("pre-paginated") {
                    return true;
                }
            }
            Ok(XmlEvent::End(event)) if xml_name_matches(event.name().as_ref(), b"meta") => {
                awaiting_rendition_layout_text = false;
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }

        buffer.clear();
    }

    false
}

fn parse_epub_kobo_spans(resource_bytes: &[u8]) -> Vec<(String, f64)> {
    let content = String::from_utf8_lossy(resource_bytes);
    if content.is_empty() {
        return vec![];
    }

    let mut spans = Vec::new();
    let mut seen = HashSet::new();
    let mut cursor = 0usize;
    let total_len = content.len().max(1) as f64;

    while let Some(relative_start) = content[cursor..].find("<span") {
        let span_start = cursor + relative_start;
        let Some(relative_end) = content[span_start..].find('>') else {
            break;
        };
        let span_end = span_start + relative_end;
        let tag = &content[span_start..=span_end];

        if !tag.to_ascii_lowercase().contains("kobospan") {
            cursor = span_end.saturating_add(1);
            if cursor >= content.len() {
                break;
            }
            continue;
        }

        let id = extract_html_attribute(tag, "id").unwrap_or_default();
        if id.starts_with("kobo.") && seen.insert(id.clone()) {
            let progression = (span_end as f64 / total_len).clamp(0.0, 1.0);
            spans.push((id, progression));
        }

        cursor = span_end.saturating_add(1);
        if cursor >= content.len() {
            break;
        }
    }

    spans
}

fn extract_html_attribute(tag: &str, attribute: &str) -> Option<String> {
    let double_quoted = format!("{attribute}=\"");
    if let Some(start) = tag.find(&double_quoted) {
        let value_start = start + double_quoted.len();
        let value_end = tag[value_start..].find('"')? + value_start;
        return Some(tag[value_start..value_end].to_string());
    }

    let single_quoted = format!("{attribute}='");
    if let Some(start) = tag.find(&single_quoted) {
        let value_start = start + single_quoted.len();
        let value_end = tag[value_start..].find('\'')? + value_start;
        return Some(tag[value_start..value_end].to_string());
    }

    None
}

fn closest_kobo_span(spans: &[(String, f64)], progression: f64) -> Option<String> {
    spans
        .iter()
        .min_by(|left, right| {
            let left_distance = (left.1 - progression).abs();
            let right_distance = (right.1 - progression).abs();
            left_distance
                .partial_cmp(&right_distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(id, _)| id.clone())
}

fn read_zip_entry_bytes(archive: &mut ZipArchive<fs::File>, path: &str) -> Option<Vec<u8>> {
    if let Ok(mut entry) = archive.by_name(path) {
        let mut bytes = Vec::new();
        if entry.read_to_end(&mut bytes).is_ok() {
            return Some(bytes);
        }
    }

    let normalized = path.trim_start_matches('/');
    if normalized != path
        && let Ok(mut entry) = archive.by_name(normalized)
    {
        let mut bytes = Vec::new();
        if entry.read_to_end(&mut bytes).is_ok() {
            return Some(bytes);
        }
    }

    None
}

fn normalize_epub_resource_href(rootfile_path: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or_default();

    if href.starts_with('/') {
        return href.to_string();
    }

    let base = rootfile_path
        .rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or_default();
    let joined = if base.is_empty() {
        href.to_string()
    } else {
        format!("{base}/{href}")
    };

    format!("/{}", joined.replace('\\', "/").trim_start_matches('/'))
}

fn xml_name_matches(actual: &[u8], expected: &[u8]) -> bool {
    actual == expected || actual.ends_with(expected)
}

async fn delete_persisted_read_progress(
    database_file: &FsPath,
    book_id: &str,
    user_id_value: &str,
) -> Result<(), String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open read-progress delete db: {error}"))?;

    sqlx::query(
        "DELETE \
                 FROM READ_PROGRESS \
                 WHERE BOOK_ID = ? \
                 AND USER_ID = ?",
    )
    .bind(book_id)
    .bind(user_id_value)
    .execute(&pool)
    .await
    .map_err(|error| format!("delete read-progress: {error}"))?;

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
        "SELECT ID, TYPE, SELECTED \
         FROM THUMBNAIL_BOOK \
         WHERE BOOK_ID = ? \
         ORDER BY SELECTED DESC, LAST_MODIFIED_DATE DESC, ID ASC",
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
        "SELECT MEDIA_TYPE, THUMBNAIL \
         FROM THUMBNAIL_BOOK \
         WHERE BOOK_ID = ? \
         ORDER BY SELECTED DESC, LAST_MODIFIED_DATE DESC, ID ASC \
         LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query selected book thumbnail: {error}"))?;

    Ok(row.map(|row| PersistedBookThumbnailBinary {
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        thumbnail: row.get::<Vec<u8>, _>("THUMBNAIL"),
    }))
}

async fn load_book_thumbnail_by_id(
    database_file: &FsPath,
    book_id: &str,
    thumbnail_id: &str,
) -> Result<Option<PersistedBookThumbnailBinary>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open single book thumbnail db: {error}"))?;

    let row = sqlx::query(
        "SELECT MEDIA_TYPE, THUMBNAIL \
         FROM THUMBNAIL_BOOK \
         WHERE ID = ? \
         AND BOOK_ID = ? \
         LIMIT 1",
    )
    .bind(thumbnail_id)
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query single book thumbnail: {error}"))?;

    Ok(row.map(|row| PersistedBookThumbnailBinary {
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        thumbnail: row.get::<Vec<u8>, _>("THUMBNAIL"),
    }))
}

async fn insert_book_thumbnail(
    database_file: &FsPath,
    book_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    selected: bool,
) -> Result<PersistedBookThumbnailRow, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book thumbnail create db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin book thumbnail create tx: {error}"))?;

    let exists = sqlx::query(
        "SELECT 1 AS FOUND \
                              FROM BOOK \
                              WHERE ID = ? \
                              LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query book existence for thumbnail create: {error}"))?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback book thumbnail create tx: {error}"))?;
        return Err("book does not exist".to_string());
    }

    if selected {
        sqlx::query(
            "UPDATE THUMBNAIL_BOOK \
                     SET SELECTED = 0 \
                     WHERE BOOK_ID = ?",
        )
        .bind(book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("clear selected book thumbnails: {error}"))?;
    }

    let id = generated_book_thumbnail_id();
    sqlx::query(
        "INSERT INTO THUMBNAIL_BOOK (ID, SELECTED, THUMBNAIL, TYPE, BOOK_ID, MEDIA_TYPE, \
           FILE_SIZE) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(selected)
    .bind(thumbnail)
    .bind("USER_UPLOADED")
    .bind(book_id)
    .bind(media_type)
    .bind(thumbnail.len() as i64)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("insert book thumbnail: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit book thumbnail create tx: {error}"))?;

    Ok(PersistedBookThumbnailRow {
        id,
        thumbnail_type: "USER_UPLOADED".to_string(),
        selected,
    })
}

async fn select_book_thumbnail(
    database_file: &FsPath,
    book_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book thumbnail select db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin book thumbnail select tx: {error}"))?;

    let exists = sqlx::query(
        "SELECT 1 AS FOUND \
                              FROM BOOK \
                              WHERE ID = ? \
                              LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query book existence for thumbnail select: {error}"))?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback book thumbnail select tx: {error}"))?;
        return Ok(false);
    }

    let target_exists = sqlx::query(
        "SELECT 1 AS FOUND \
                     FROM THUMBNAIL_BOOK \
                     WHERE ID = ? \
                     AND BOOK_ID = ? \
                     LIMIT 1",
    )
    .bind(thumbnail_id)
    .bind(book_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query target book thumbnail for select: {error}"))?
    .is_some();
    if !target_exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback book thumbnail select tx: {error}"))?;
        return Ok(false);
    }

    sqlx::query(
        "UPDATE THUMBNAIL_BOOK \
                 SET SELECTED = 0 \
                 WHERE BOOK_ID = ?",
    )
    .bind(book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("clear selected book thumbnails for select: {error}"))?;
    sqlx::query(
        "UPDATE THUMBNAIL_BOOK \
                 SET SELECTED = 1 \
                 WHERE ID = ? \
                 AND BOOK_ID = ?",
    )
    .bind(thumbnail_id)
    .bind(book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("mark selected book thumbnail: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit book thumbnail select tx: {error}"))?;
    Ok(true)
}

async fn delete_book_thumbnail(
    database_file: &FsPath,
    book_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book thumbnail delete db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin book thumbnail delete tx: {error}"))?;

    let exists = sqlx::query(
        "SELECT 1 AS FOUND \
                              FROM BOOK \
                              WHERE ID = ? \
                              LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query book existence for thumbnail delete: {error}"))?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback book thumbnail delete tx: {error}"))?;
        return Ok(false);
    }

    let deleted = sqlx::query(
        "DELETE \
                               FROM THUMBNAIL_BOOK \
                               WHERE ID = ? \
                               AND BOOK_ID = ?",
    )
    .bind(thumbnail_id)
    .bind(book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete book thumbnail: {error}"))?
    .rows_affected()
        > 0;

    if !deleted {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback book thumbnail delete tx: {error}"))?;
        return Ok(false);
    }

    tx.commit()
        .await
        .map_err(|error| format!("commit book thumbnail delete tx: {error}"))?;
    Ok(true)
}

async fn load_persisted_series_thumbnails(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Vec<PersistedSeriesThumbnailRow>, String> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series thumbnails db: {error}"))?;

    let rows = sqlx::query(
        "SELECT ID, TYPE, SELECTED \
         FROM THUMBNAIL_SERIES \
         WHERE SERIES_ID = ? \
         ORDER BY SELECTED DESC, LAST_MODIFIED_DATE DESC, ID ASC",
    )
    .bind(series_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted series thumbnails: {error}"))?;

    let thumbnails = rows
        .into_iter()
        .map(|row| PersistedSeriesThumbnailRow {
            id: row.get::<String, _>("ID"),
            thumbnail_type: row.get::<String, _>("TYPE"),
            selected: row.get::<i64, _>("SELECTED") != 0,
        })
        .collect::<Vec<_>>();

    Ok(thumbnails)
}

async fn load_selected_series_thumbnail(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Option<PersistedBookThumbnailBinary>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open selected series thumbnail db: {error}"))?;

    let row = sqlx::query(
        "SELECT MEDIA_TYPE, THUMBNAIL \
         FROM THUMBNAIL_SERIES \
         WHERE SERIES_ID = ? \
         ORDER BY SELECTED DESC, LAST_MODIFIED_DATE DESC, ID ASC \
         LIMIT 1",
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query selected series thumbnail: {error}"))?;

    Ok(row.map(|row| PersistedBookThumbnailBinary {
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        thumbnail: row
            .get::<Option<Vec<u8>>, _>("THUMBNAIL")
            .unwrap_or_default(),
    }))
}

async fn load_series_thumbnail_by_id(
    database_file: &FsPath,
    series_id: &str,
    thumbnail_id: &str,
) -> Result<Option<PersistedBookThumbnailBinary>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open single series thumbnail db: {error}"))?;

    let row = sqlx::query(
        "SELECT MEDIA_TYPE, THUMBNAIL \
         FROM THUMBNAIL_SERIES \
         WHERE ID = ? \
         AND SERIES_ID = ? \
         LIMIT 1",
    )
    .bind(thumbnail_id)
    .bind(series_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query single series thumbnail: {error}"))?;

    Ok(row.map(|row| PersistedBookThumbnailBinary {
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        thumbnail: row
            .get::<Option<Vec<u8>>, _>("THUMBNAIL")
            .unwrap_or_default(),
    }))
}

async fn insert_series_thumbnail(
    database_file: &FsPath,
    series_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    selected: bool,
) -> Result<PersistedSeriesThumbnailRow, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series thumbnail create db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin series thumbnail create tx: {error}"))?;

    let exists = sqlx::query(
        "SELECT 1 AS FOUND \
                              FROM SERIES \
                              WHERE ID = ? \
                              LIMIT 1",
    )
    .bind(series_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query series existence for thumbnail create: {error}"))?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback series thumbnail create tx: {error}"))?;
        return Err("series does not exist".to_string());
    }

    if selected {
        sqlx::query(
            "UPDATE THUMBNAIL_SERIES \
                     SET SELECTED = 0 \
                     WHERE SERIES_ID = ?",
        )
        .bind(series_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("clear selected series thumbnails: {error}"))?;
    }

    let id = generated_series_thumbnail_id();
    sqlx::query(
        "INSERT INTO THUMBNAIL_SERIES (ID, SELECTED, THUMBNAIL, TYPE, SERIES_ID, MEDIA_TYPE, \
           FILE_SIZE) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(selected)
    .bind(thumbnail)
    .bind("USER_UPLOADED")
    .bind(series_id)
    .bind(media_type)
    .bind(thumbnail.len() as i64)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("insert series thumbnail: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit series thumbnail create tx: {error}"))?;

    Ok(PersistedSeriesThumbnailRow {
        id,
        thumbnail_type: "USER_UPLOADED".to_string(),
        selected,
    })
}

async fn select_series_thumbnail(
    database_file: &FsPath,
    series_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series thumbnail select db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin series thumbnail select tx: {error}"))?;

    let exists = sqlx::query(
        "SELECT 1 AS FOUND \
                              FROM SERIES \
                              WHERE ID = ? \
                              LIMIT 1",
    )
    .bind(series_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query series existence for thumbnail select: {error}"))?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback series thumbnail select tx: {error}"))?;
        return Ok(false);
    }

    let target_exists = sqlx::query(
        "SELECT 1 AS FOUND \
         FROM THUMBNAIL_SERIES \
         WHERE ID = ? \
         AND SERIES_ID = ? \
         LIMIT 1",
    )
    .bind(thumbnail_id)
    .bind(series_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query target series thumbnail for select: {error}"))?
    .is_some();
    if !target_exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback series thumbnail select tx: {error}"))?;
        return Ok(false);
    }

    sqlx::query(
        "UPDATE THUMBNAIL_SERIES \
                 SET SELECTED = 0 \
                 WHERE SERIES_ID = ?",
    )
    .bind(series_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("clear selected series thumbnails for select: {error}"))?;
    sqlx::query(
        "UPDATE THUMBNAIL_SERIES \
                 SET SELECTED = 1 \
                 WHERE ID = ? \
                 AND SERIES_ID = ?",
    )
    .bind(thumbnail_id)
    .bind(series_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("mark selected series thumbnail: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit series thumbnail select tx: {error}"))?;
    Ok(true)
}

async fn delete_series_thumbnail(
    database_file: &FsPath,
    series_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series thumbnail delete db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin series thumbnail delete tx: {error}"))?;

    let exists = sqlx::query(
        "SELECT 1 AS FOUND \
                              FROM SERIES \
                              WHERE ID = ? \
                              LIMIT 1",
    )
    .bind(series_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query series existence for thumbnail delete: {error}"))?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback series thumbnail delete tx: {error}"))?;
        return Ok(false);
    }

    let deleted = sqlx::query(
        "DELETE \
                               FROM THUMBNAIL_SERIES \
                               WHERE ID = ? \
                               AND SERIES_ID = ?",
    )
    .bind(thumbnail_id)
    .bind(series_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete series thumbnail: {error}"))?
    .rows_affected()
        > 0;
    if !deleted {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback series thumbnail delete tx: {error}"))?;
        return Ok(false);
    }

    tx.commit()
        .await
        .map_err(|error| format!("commit series thumbnail delete tx: {error}"))?;
    Ok(true)
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
        "SELECT ID, TYPE, SELECTED, MEDIA_TYPE, THUMBNAIL \
         FROM THUMBNAIL_READLIST \
         WHERE READLIST_ID = ? \
         ORDER BY SELECTED DESC, LAST_MODIFIED_DATE DESC, ID ASC",
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

    Ok(thumbnails)
}

async fn insert_readlist_thumbnail(
    database_file: &FsPath,
    readlist_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    selected: bool,
) -> Result<PersistedReadlistThumbnailRow, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open readlist thumbnail create db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin readlist thumbnail create tx: {error}"))?;

    let exists = sqlx::query(
        "SELECT 1 AS FOUND \
                              FROM READLIST \
                              WHERE ID = ? \
                              LIMIT 1",
    )
    .bind(readlist_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query readlist existence for thumbnail create: {error}"))?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback readlist thumbnail create tx: {error}"))?;
        return Err("readlist does not exist".to_string());
    }

    if selected {
        sqlx::query(
            "UPDATE THUMBNAIL_READLIST \
                     SET SELECTED = 0 \
                     WHERE READLIST_ID = ?",
        )
        .bind(readlist_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("clear selected readlist thumbnails: {error}"))?;
    }

    let id = generated_readlist_thumbnail_id();
    sqlx::query(
        "INSERT INTO THUMBNAIL_READLIST (ID, SELECTED, THUMBNAIL, TYPE, READLIST_ID, MEDIA_TYPE, \
           FILE_SIZE) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(selected)
    .bind(thumbnail)
    .bind("USER_UPLOADED")
    .bind(readlist_id)
    .bind(media_type)
    .bind(thumbnail.len() as i64)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("insert readlist thumbnail: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit readlist thumbnail create tx: {error}"))?;

    Ok(PersistedReadlistThumbnailRow {
        id,
        thumbnail_type: "USER_UPLOADED".to_string(),
        selected,
        media_type: media_type.to_string(),
        thumbnail: thumbnail.to_vec(),
    })
}

async fn select_readlist_thumbnail(
    database_file: &FsPath,
    readlist_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open readlist thumbnail select db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin readlist thumbnail select tx: {error}"))?;

    let exists = sqlx::query(
        "SELECT 1 AS FOUND \
                              FROM READLIST \
                              WHERE ID = ? \
                              LIMIT 1",
    )
    .bind(readlist_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query readlist existence for thumbnail select: {error}"))?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback readlist thumbnail select tx: {error}"))?;
        return Ok(false);
    }

    let target_exists = sqlx::query(
        "SELECT 1 AS FOUND \
         FROM THUMBNAIL_READLIST \
         WHERE ID = ? \
         AND READLIST_ID = ? \
         LIMIT 1",
    )
    .bind(thumbnail_id)
    .bind(readlist_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query readlist thumbnail select target: {error}"))?
    .is_some();
    if !target_exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback readlist thumbnail select tx: {error}"))?;
        return Ok(false);
    }

    sqlx::query(
        "UPDATE THUMBNAIL_READLIST \
                 SET SELECTED = 0 \
                 WHERE READLIST_ID = ?",
    )
    .bind(readlist_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("clear selected readlist thumbnails for select: {error}"))?;
    sqlx::query(
        "UPDATE THUMBNAIL_READLIST \
                 SET SELECTED = 1 \
                 WHERE ID = ? \
                 AND READLIST_ID = ?",
    )
    .bind(thumbnail_id)
    .bind(readlist_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("mark selected readlist thumbnail: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit readlist thumbnail select tx: {error}"))?;
    Ok(true)
}

async fn delete_readlist_thumbnail(
    database_file: &FsPath,
    readlist_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open readlist thumbnail delete db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin readlist thumbnail delete tx: {error}"))?;

    let exists = sqlx::query(
        "SELECT 1 AS FOUND \
                              FROM READLIST \
                              WHERE ID = ? \
                              LIMIT 1",
    )
    .bind(readlist_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query readlist existence for thumbnail delete: {error}"))?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback readlist thumbnail delete tx: {error}"))?;
        return Ok(false);
    }

    let deleted = sqlx::query(
        "DELETE \
                               FROM THUMBNAIL_READLIST \
                               WHERE ID = ? \
                               AND READLIST_ID = ?",
    )
    .bind(thumbnail_id)
    .bind(readlist_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete readlist thumbnail: {error}"))?
    .rows_affected()
        > 0;
    if !deleted {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback readlist thumbnail delete tx: {error}"))?;
        return Ok(false);
    }

    tx.commit()
        .await
        .map_err(|error| format!("commit readlist thumbnail delete tx: {error}"))?;
    Ok(true)
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

    let row = sqlx::query(
        "SELECT NAME \
                           FROM READLIST \
                           WHERE ID = ?",
    )
    .bind(readlist_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted readlist name: {error}"))?;

    let name = row.map(|row| row.get::<String, _>("NAME"));
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

    let row = sqlx::query(
        "SELECT 1 AS FOUND \
                           FROM COLLECTION \
                           WHERE ID = ? \
                           LIMIT 1",
    )
    .bind(collection_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted collection existence: {error}"))?;

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
        "SELECT ID, TYPE, SELECTED, MEDIA_TYPE, THUMBNAIL \
         FROM THUMBNAIL_COLLECTION \
         WHERE COLLECTION_ID = ? \
         ORDER BY SELECTED DESC, LAST_MODIFIED_DATE DESC, ID ASC",
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

    let exists = sqlx::query(
        "SELECT 1 AS FOUND \
                              FROM COLLECTION \
                              WHERE ID = ? \
                              LIMIT 1",
    )
    .bind(collection_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query collection existence for thumbnail create: {error}"))?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback collection thumbnail create tx: {error}"))?;
        return Err("collection does not exist".to_string());
    }

    if selected {
        sqlx::query(
            "UPDATE THUMBNAIL_COLLECTION \
                     SET SELECTED = 0 \
                     WHERE COLLECTION_ID = ?",
        )
        .bind(collection_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("clear selected collection thumbnails: {error}"))?;
    }

    let id = generated_collection_thumbnail_id();
    sqlx::query(
        "INSERT INTO THUMBNAIL_COLLECTION (ID, SELECTED, THUMBNAIL, TYPE, COLLECTION_ID, \
           MEDIA_TYPE, FILE_SIZE) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
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

    let exists = sqlx::query(
        "SELECT 1 AS FOUND \
                              FROM COLLECTION \
                              WHERE ID = ? \
                              LIMIT 1",
    )
    .bind(collection_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query collection existence for thumbnail select: {error}"))?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback collection thumbnail select tx: {error}"))?;
        return Ok(false);
    }

    let target_exists = sqlx::query(
        "SELECT 1 AS FOUND \
         FROM THUMBNAIL_COLLECTION \
         WHERE ID = ? \
         AND COLLECTION_ID = ? \
         LIMIT 1",
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
        return Ok(false);
    }

    sqlx::query(
        "UPDATE THUMBNAIL_COLLECTION \
                 SET SELECTED = 0 \
                 WHERE COLLECTION_ID = ?",
    )
    .bind(collection_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("clear selected collection thumbnails for select: {error}"))?;
    sqlx::query(
        "UPDATE THUMBNAIL_COLLECTION \
                 SET SELECTED = 1 \
                 WHERE ID = ? \
                 AND COLLECTION_ID = ?",
    )
    .bind(thumbnail_id)
    .bind(collection_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("mark selected collection thumbnail: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit collection thumbnail select tx: {error}"))?;
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

    let exists = sqlx::query(
        "SELECT 1 AS FOUND \
                              FROM COLLECTION \
                              WHERE ID = ? \
                              LIMIT 1",
    )
    .bind(collection_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query collection existence for thumbnail delete: {error}"))?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback collection thumbnail delete tx: {error}"))?;
        return Ok(false);
    }

    let deleted = sqlx::query(
        "DELETE \
                     FROM THUMBNAIL_COLLECTION \
                     WHERE ID = ? \
                     AND COLLECTION_ID = ?",
    )
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
        return Ok(false);
    }

    tx.commit()
        .await
        .map_err(|error| format!("commit collection thumbnail delete tx: {error}"))?;
    Ok(true)
}

fn generated_collection_thumbnail_id() -> String {
    random_prefixed_id("thumbnail-collection")
}

fn generated_series_thumbnail_id() -> String {
    random_prefixed_id("thumbnail-series")
}

fn generated_readlist_thumbnail_id() -> String {
    random_prefixed_id("thumbnail-readlist")
}

fn generated_book_thumbnail_id() -> String {
    random_prefixed_id("thumbnail-book")
}

fn generated_historical_event_id() -> String {
    random_prefixed_id("historical-event")
}

fn random_prefixed_id(prefix: &str) -> String {
    format!("{prefix}-{}", random_hex_token(12))
}

fn random_hex_token(byte_len: usize) -> String {
    let mut bytes = vec![0u8; byte_len];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        let _ = file.read_exact(&mut bytes);
    } else {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = ((seed >> ((index % 8) * 8)) as u8) ^ (index as u8).wrapping_mul(29);
        }
    }

    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
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
        "zip" => "application/zip".to_string(),
        "cbr" => "application/vnd.comicbook-rar".to_string(),
        "pdf" => "application/pdf".to_string(),
        "epub" => "application/epub+zip".to_string(),
        "jpg" | "jpeg" => "image/jpeg".to_string(),
        "png" => "image/png".to_string(),
        "gif" => "image/gif".to_string(),
        "webp" => "image/webp".to_string(),
        "avif" => "image/avif".to_string(),
        _ => fallback.to_string(),
    }
}

fn book_media_supports_page_image(media: &PersistedBookMedia) -> bool {
    content_type_from_filename(&media.file_name, &media.media_type).starts_with("image/")
}

fn book_media_is_single_image(media: &PersistedBookMedia) -> bool {
    book_media_supports_page_image(media)
}

fn book_media_is_zip_archive(media: &PersistedBookMedia) -> bool {
    matches!(
        content_type_from_filename(&media.file_name, &media.media_type).as_str(),
        "application/vnd.comicbook+zip" | "application/epub+zip" | "application/zip"
    )
}

fn book_media_is_rar_archive(media: &PersistedBookMedia) -> bool {
    content_type_from_filename(&media.file_name, &media.media_type)
        == "application/vnd.comicbook-rar"
}

fn book_media_is_epub(media: &PersistedBookMedia) -> bool {
    content_type_from_filename(&media.file_name, &media.media_type) == "application/epub+zip"
}

fn book_media_is_pdf(media: &PersistedBookMedia) -> bool {
    content_type_from_filename(&media.file_name, &media.media_type) == "application/pdf"
}

fn book_media_supports_page_api(media: &PersistedBookMedia) -> bool {
    book_media_is_single_image(media)
        || media.page_count > 0
        || book_media_is_zip_archive(media)
        || book_media_is_rar_archive(media)
        || book_media_is_pdf(media)
}

fn is_supported_page_image_file_name(file_name: &str) -> bool {
    matches!(
        file_name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .unwrap_or_default()
            .as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "avif"
    )
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

fn user_can_access_library(user: &AuthUser, library_id: &str) -> bool {
    user_shared_all_libraries(user)
        || user_shared_library_ids(user)
            .iter()
            .any(|shared_library_id| shared_library_id == library_id)
}

async fn user_can_access_book_media(
    database_file: &FsPath,
    book_id: &str,
    user: &AuthUser,
    media: &PersistedBookMedia,
) -> bool {
    if !user_can_access_library(user, &media.library_id) {
        return false;
    }

    let payload = user_payload_json(user);
    let Some(principal) = principal_from_user_payload(&payload) else {
        return true;
    };
    if !principal.restrictions.is_restricted() {
        return true;
    }

    let Ok(Some((age_rating, labels))) = load_book_restrictions(database_file, book_id).await
    else {
        return true;
    };

    principal.is_content_allowed(age_rating, &labels)
}

async fn load_book_restrictions(
    database_file: &FsPath,
    book_id: &str,
) -> Result<Option<(Option<u16>, Vec<String>)>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book restrictions db: {error}"))?;

    let row = sqlx::query(
        "SELECT sm.AGE_RATING AS AGE_RATING, COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS LABELS \
         FROM BOOK b \
         JOIN SERIES s ON s.ID = b.SERIES_ID \
         LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID \
         WHERE b.ID = ? \
         GROUP BY sm.AGE_RATING",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query book restrictions: {error}"))?;

    Ok(row.map(|row| {
        let age_rating = row
            .get::<Option<i64>, _>("AGE_RATING")
            .and_then(|value| u16::try_from(value).ok());
        let labels = row
            .get::<String, _>("LABELS")
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        (age_rating, labels)
    }))
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
        "SELECT b.LIBRARY_ID AS LIBRARY_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE, \
                b.NAME AS FILE_NAME, \
                COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE \
         FROM BOOK b \
         LEFT \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         LEFT \
         JOIN MEDIA m ON m.BOOK_ID = b.ID \
         WHERE b.ID = ?",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted manifest book: {error}"))?;

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

fn persisted_manifest_payload(
    headers: &HeaderMap,
    book_id: &str,
    title: &str,
    media_type: &str,
) -> Value {
    json!({
        "context": "https://readium.org/webpub-manifest/context.jsonld",
        "metadata": {
            "title": title,
        },
        "links": [
            {
                "rel": "self",
                "href": app_absolute_url(headers, format!("/api/v1/books/{book_id}/manifest").as_str()),
                "type": "application/webpub+json",
            },
            {
                "rel": "http://opds-spec.org/acquisition",
                "href": app_absolute_url(headers, format!("/api/v1/books/{book_id}/file").as_str()),
                "type": media_type,
            }
        ],
        "readingOrder": [
            {
                "href": app_absolute_url(headers, format!("/api/v1/books/{book_id}/pages/1?contentNegotiation=false").as_str()),
                "type": media_type,
            }
        ],
        "resources": [
            {
                "href": app_absolute_url(headers, format!("/api/v1/books/{book_id}/thumbnail").as_str()),
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

    let payload = persisted_manifest_payload(headers, book_id, &book.title, &book.media_type);
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

    let mut archive_entries = Vec::new();
    for (file_name, file_path) in entries {
        if let Ok(bytes) = fs::read(&file_path) {
            archive_entries.push((file_name, bytes));
        }
    }

    if archive_entries.is_empty() {
        return Ok(vec![]);
    }

    build_stored_zip_archive(archive_entries)
}

fn build_stored_zip_archive(entries: Vec<(String, Vec<u8>)>) -> Result<Vec<u8>, String> {
    let mut payload = Vec::new();
    let mut central_directory = Vec::new();
    let mut entries_count: usize = 0;

    for (file_name, bytes) in entries {
        let file_name_bytes = file_name.as_bytes();
        let name_len = u16::try_from(file_name_bytes.len())
            .map_err(|_| format!("zip entry name too long: {file_name}"))?;
        let size =
            u32::try_from(bytes.len()).map_err(|_| format!("zip entry too large: {file_name}"))?;
        let local_header_offset = u32::try_from(payload.len())
            .map_err(|_| "zip archive too large for legacy zip format".to_string())?;
        let crc32 = crc32_ieee(&bytes);

        push_u32_le(&mut payload, 0x0403_4b50);
        push_u16_le(&mut payload, 20);
        push_u16_le(&mut payload, 0);
        push_u16_le(&mut payload, 0);
        push_u16_le(&mut payload, 0);
        push_u16_le(&mut payload, 0);
        push_u32_le(&mut payload, crc32);
        push_u32_le(&mut payload, size);
        push_u32_le(&mut payload, size);
        push_u16_le(&mut payload, name_len);
        push_u16_le(&mut payload, 0);
        payload.extend_from_slice(file_name_bytes);
        payload.extend_from_slice(&bytes);

        push_u32_le(&mut central_directory, 0x0201_4b50);
        push_u16_le(&mut central_directory, 20);
        push_u16_le(&mut central_directory, 20);
        push_u16_le(&mut central_directory, 0);
        push_u16_le(&mut central_directory, 0);
        push_u16_le(&mut central_directory, 0);
        push_u16_le(&mut central_directory, 0);
        push_u32_le(&mut central_directory, crc32);
        push_u32_le(&mut central_directory, size);
        push_u32_le(&mut central_directory, size);
        push_u16_le(&mut central_directory, name_len);
        push_u16_le(&mut central_directory, 0);
        push_u16_le(&mut central_directory, 0);
        push_u16_le(&mut central_directory, 0);
        push_u16_le(&mut central_directory, 0);
        push_u32_le(&mut central_directory, 0);
        push_u32_le(&mut central_directory, local_header_offset);
        central_directory.extend_from_slice(file_name_bytes);
        entries_count += 1;
    }

    let central_directory_offset = u32::try_from(payload.len())
        .map_err(|_| "zip archive too large for legacy zip format".to_string())?;
    let central_directory_size = u32::try_from(central_directory.len())
        .map_err(|_| "zip central directory too large for legacy zip format".to_string())?;
    let entries_count = u16::try_from(entries_count)
        .map_err(|_| "too many zip entries for legacy zip format".to_string())?;

    payload.extend_from_slice(&central_directory);
    push_u32_le(&mut payload, 0x0605_4b50);
    push_u16_le(&mut payload, 0);
    push_u16_le(&mut payload, 0);
    push_u16_le(&mut payload, entries_count);
    push_u16_le(&mut payload, entries_count);
    push_u32_le(&mut payload, central_directory_size);
    push_u32_le(&mut payload, central_directory_offset);
    push_u16_le(&mut payload, 0);

    Ok(payload)
}
fn crc32_ieee(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            let lsb = crc & 1;
            crc >>= 1;
            if lsb != 0 {
                crc ^= 0xedb8_8320;
            }
        }
    }
    !crc
}

fn push_u16_le(buffer: &mut Vec<u8>, value: u16) {
    buffer.extend_from_slice(&value.to_le_bytes())
}

fn push_u32_le(buffer: &mut Vec<u8>, value: u32) {
    buffer.extend_from_slice(&value.to_le_bytes())
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
        "SELECT b.NAME AS FILE_NAME, b.URL AS BOOK_URL, l.ROOT AS LIBRARY_ROOT \
         FROM READLIST_BOOK rb \
         JOIN BOOK b ON b.ID = rb.BOOK_ID \
         JOIN LIBRARY l ON l.ID = b.LIBRARY_ID \
         WHERE rb.READLIST_ID = ? \
         ORDER BY rb.NUMBER ASC",
    )
    .bind(readlist_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query readlist archive entries: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let file_name = row.get::<String, _>("FILE_NAME");
            let book_url = row.get::<String, _>("BOOK_URL");
            let library_root = row.get::<String, _>("LIBRARY_ROOT");
            let file_path = PathBuf::from(library_root).join(book_url);
            (file_name, file_path)
        })
        .collect())
}

async fn load_series_archive_entries(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Option<(String, String, Vec<(String, PathBuf)>)>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series archive db: {error}"))?;

    let series_row = sqlx::query(
        "SELECT s.LIBRARY_ID AS LIBRARY_ID, COALESCE(sm.TITLE, s.NAME) AS SERIES_TITLE \
         FROM SERIES s \
         LEFT \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         WHERE s.ID = ? \
         LIMIT 1",
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query series archive metadata: {error}"))?;

    let Some(series_row) = series_row else {
        return Ok(None);
    };

    let library_id = series_row.get::<String, _>("LIBRARY_ID");
    let series_title = series_row.get::<String, _>("SERIES_TITLE");

    let rows = sqlx::query(
        "SELECT b.NAME AS FILE_NAME, b.URL AS BOOK_URL, l.ROOT AS LIBRARY_ROOT \
         FROM BOOK b \
         JOIN LIBRARY l ON l.ID = b.LIBRARY_ID \
         WHERE b.SERIES_ID = ? \
         AND b.DELETED_DATE IS NULL \
         ORDER BY b.NUMBER ASC, b.ID ASC",
    )
    .bind(series_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query series archive entries: {error}"))?;

    let entries = rows
        .into_iter()
        .map(|row| {
            let file_name = row.get::<String, _>("FILE_NAME");
            let book_url = row.get::<String, _>("BOOK_URL");
            let library_root = row.get::<String, _>("LIBRARY_ROOT");
            let file_path = PathBuf::from(library_root).join(book_url);
            (file_name, file_path)
        })
        .collect::<Vec<_>>();

    Ok(Some((series_title, library_id, entries)))
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

    let exists = sqlx::query(
        "SELECT 1 AS FOUND \
                              FROM READLIST \
                              WHERE ID = ? \
                              LIMIT 1",
    )
    .bind(readlist_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query readlist exists for tachiyomi counters: {error}"))?
    .is_some();

    if !exists {
        return Ok(None);
    }

    let rows = sqlx::query(
        "SELECT rb.NUMBER AS ORDINAL, COALESCE(rp.PAGE, 0) AS PAGE, \
                COALESCE(rp.COMPLETED, 0) AS COMPLETED \
         FROM READLIST_BOOK rb \
         LEFT \
         JOIN READ_PROGRESS rp ON rp.BOOK_ID = rb.BOOK_ID \
         AND rp.USER_ID = ? \
         WHERE rb.READLIST_ID = ? \
         ORDER BY rb.NUMBER ASC",
    )
    .bind(user_id_value)
    .bind(readlist_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query readlist tachiyomi counters: {error}"))?;

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

async fn persist_readlist_tachiyomi_progress(
    database_file: &FsPath,
    readlist_id: &str,
    user_id_value: &str,
    last_book_read: usize,
) -> Result<Option<()>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open readlist tachiyomi write db: {error}"))?;

    let exists = sqlx::query(
        "SELECT 1 AS FOUND \
                              FROM READLIST \
                              WHERE ID = ? \
                              LIMIT 1",
    )
    .bind(readlist_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query readlist exists for tachiyomi write: {error}"))?
    .is_some();
    if !exists {
        return Ok(None);
    }

    let rows = sqlx::query(
        "SELECT BOOK_ID \
                     FROM READLIST_BOOK \
                     WHERE READLIST_ID = ? \
                     ORDER BY NUMBER ASC",
    )
    .bind(readlist_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query readlist books for tachiyomi write: {error}"))?;

    for (index, row) in rows.into_iter().enumerate() {
        if index >= last_book_read {
            break;
        }

        let book_id = row.get::<String, _>("BOOK_ID");
        persist_read_progress(database_file, &book_id, user_id_value, 10, true)
            .await
            .map_err(|error| format!("persist read progress for tachiyomi write: {error}"))?;
    }

    Ok(Some(()))
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
        if let Some(user_id) = persisted_user_id.as_deref() {
            if persist_read_progress(
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
        }
        set_read_progress(&state, token, book_id, page_count, true);
        return StatusCode::NO_CONTENT.into_response();
    }

    if let Some(page) = payload.get("page").and_then(|value| value.as_u64())
        && (1..=page_count).contains(&page)
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
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
        set_read_progress(&state, token, book_id, page, false);
        return StatusCode::NO_CONTENT.into_response();
    }

    invalid_read_progress_payload()
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
    method_not_allowed_json_response(&path)
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

pub(in crate::app::compat_runtime) async fn book_file_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(state): Extension<super::super::OperationalState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match persisted_book_exists(auth_db.database_file.as_path(), &book_id).await {
        Ok(true) => enqueue_task_records(
            &state,
            vec![TaskQueueRecord::new(
                format!("DELETE_BOOK:{book_id}"),
                100,
                Some(book_id),
            )],
        ),
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

    let Some(progression) = progression else {
        return invalid_progression_payload();
    };
    if !(0.0..=1.0).contains(&progression) {
        return invalid_progression_payload();
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

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
        return StatusCode::NOT_FOUND.into_response();
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

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

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use lopdf::{Object, Stream, dictionary};
    use std::io::Write;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_path(prefix: &str) -> PathBuf {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_millis();
        std::env::temp_dir().join(format!("{prefix}-{millis}"))
    }

    fn assert_f64_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-6,
            "expected {expected}, got {actual}"
        );
    }

    fn write_single_page_pdf(path: &std::path::Path) {
        let mut document = PdfDocument::with_version("1.5");
        let pages_id = document.new_object_id();
        let page_id = document.new_object_id();
        let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
        let resources_id = document.add_object(dictionary! {});

        document.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
                "Contents" => content_id,
                "Resources" => resources_id,
            }),
        );
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );

        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        document.compress();
        document
            .save(path)
            .expect("single-page pdf should be saved");
    }

    #[test]
    fn page_api_support_depends_on_image_or_known_page_count() {
        let image_media = PersistedBookMedia {
            library_id: "lib".to_string(),
            file_name: "cover.jpg".to_string(),
            file_path: PathBuf::from("/tmp/cover.jpg"),
            media_type: "image/jpeg".to_string(),
            page_count: 0,
        };
        assert!(book_media_supports_page_api(&image_media));

        let paged_archive = PersistedBookMedia {
            library_id: "lib".to_string(),
            file_name: "book.cbz".to_string(),
            file_path: PathBuf::from("/tmp/book.cbz"),
            media_type: "application/vnd.comicbook+zip".to_string(),
            page_count: 25,
        };
        assert!(book_media_supports_page_api(&paged_archive));

        let unknown_media = PersistedBookMedia {
            library_id: "lib".to_string(),
            file_name: "book.cbz".to_string(),
            file_path: PathBuf::from("/tmp/book.cbz"),
            media_type: "application/vnd.comicbook+zip".to_string(),
            page_count: 0,
        };
        assert!(book_media_supports_page_api(&unknown_media));

        let rar_media = PersistedBookMedia {
            library_id: "lib".to_string(),
            file_name: "book.cbr".to_string(),
            file_path: PathBuf::from("/tmp/book.cbr"),
            media_type: "application/vnd.comicbook-rar".to_string(),
            page_count: 0,
        };
        assert!(book_media_supports_page_api(&rar_media));

        let pdf_media = PersistedBookMedia {
            library_id: "lib".to_string(),
            file_name: "book.pdf".to_string(),
            file_path: PathBuf::from("/tmp/book.pdf"),
            media_type: "application/pdf".to_string(),
            page_count: 0,
        };
        assert!(book_media_supports_page_api(&pdf_media));
    }

    #[test]
    fn resolve_book_page_bytes_does_not_fallback_whole_archive_for_non_image() {
        let file_path = unique_temp_path("komga-media-archive");
        fs::write(&file_path, b"archive-bytes").expect("archive test file should be written");

        let media = PersistedBookMedia {
            library_id: "lib".to_string(),
            file_name: "book.cbz".to_string(),
            file_path: file_path.clone(),
            media_type: "application/vnd.comicbook+zip".to_string(),
            page_count: 12,
        };
        let page = PersistedBookPageRow {
            number: 5,
            file_name: "page-005.jpg".to_string(),
            media_type: "image/jpeg".to_string(),
            width: None,
            height: None,
            file_size: 0,
        };

        let bytes = resolve_book_page_bytes(&media, &page, 5);
        assert!(bytes.is_none());

        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn resolve_book_page_bytes_allows_single_image_first_page_fallback() {
        let file_path = unique_temp_path("komga-media-image");
        fs::write(&file_path, b"image-bytes").expect("image test file should be written");

        let media = PersistedBookMedia {
            library_id: "lib".to_string(),
            file_name: "cover.jpg".to_string(),
            file_path: file_path.clone(),
            media_type: "image/jpeg".to_string(),
            page_count: 1,
        };
        let page = PersistedBookPageRow {
            number: 1,
            file_name: "missing-derived-page.jpg".to_string(),
            media_type: "image/jpeg".to_string(),
            width: None,
            height: None,
            file_size: 0,
        };

        let bytes = resolve_book_page_bytes(&media, &page, 1);
        assert_eq!(bytes, Some(b"image-bytes".to_vec()));

        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn load_archive_page_rows_uses_zip_image_entries_only() {
        let file_path = unique_temp_path("komga-media-zip-rows");
        let archive = build_stored_zip_archive(vec![
            ("001.jpg".to_string(), b"page-1".to_vec()),
            ("meta.txt".to_string(), b"meta".to_vec()),
            ("002.png".to_string(), b"page-2".to_vec()),
        ])
        .expect("zip payload should be created");
        fs::write(&file_path, archive).expect("zip test file should be written");

        let media = PersistedBookMedia {
            library_id: "lib".to_string(),
            file_name: "book.cbz".to_string(),
            file_path: file_path.clone(),
            media_type: "application/vnd.comicbook+zip".to_string(),
            page_count: 2,
        };

        let rows = load_archive_page_rows(&media).expect("archive rows should be parsed");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].number, 1);
        assert_eq!(rows[0].file_name, "001.jpg");
        assert_eq!(rows[1].number, 2);
        assert_eq!(rows[1].file_name, "002.png");

        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn resolve_book_page_bytes_extracts_zip_page_by_logical_index() {
        let file_path = unique_temp_path("komga-media-zip-by-index");
        let archive = build_stored_zip_archive(vec![
            ("001.jpg".to_string(), b"page-1".to_vec()),
            ("meta.txt".to_string(), b"meta".to_vec()),
            ("002.png".to_string(), b"page-2".to_vec()),
        ])
        .expect("zip payload should be created");
        fs::write(&file_path, archive).expect("zip test file should be written");

        let media = PersistedBookMedia {
            library_id: "lib".to_string(),
            file_name: "book.cbz".to_string(),
            file_path: file_path.clone(),
            media_type: "application/vnd.comicbook+zip".to_string(),
            page_count: 2,
        };
        let page = PersistedBookPageRow {
            number: 2,
            file_name: "not-present.jpg".to_string(),
            media_type: "image/jpeg".to_string(),
            width: None,
            height: None,
            file_size: 0,
        };

        let bytes = resolve_book_page_bytes(&media, &page, 2);
        assert_eq!(bytes, Some(b"page-2".to_vec()));

        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn load_epub_archive_positions_fallback_parses_spine() {
        let file_path = unique_temp_path("komga-media-epub-fallback");
        let container_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;
        let package_document = r#"<?xml version="1.0" encoding="UTF-8"?>
<package version="3.0" xmlns="http://www.idpf.org/2007/opf">
  <manifest>
    <item id="chap-1" href="chapter-1.xhtml" media-type="application/xhtml+xml"/>
    <item id="chap-2" href="chapter-2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="chap-1"/>
    <itemref idref="chap-2"/>
  </spine>
</package>"#;

        let archive = build_stored_zip_archive(vec![
            (
                "META-INF/container.xml".to_string(),
                container_xml.as_bytes().to_vec(),
            ),
            (
                "OEBPS/content.opf".to_string(),
                package_document.as_bytes().to_vec(),
            ),
            (
                "OEBPS/chapter-1.xhtml".to_string(),
                b"<html></html>".to_vec(),
            ),
            (
                "OEBPS/chapter-2.xhtml".to_string(),
                b"<html></html>".to_vec(),
            ),
        ])
        .expect("epub archive should be created");
        fs::write(&file_path, archive).expect("epub test file should be written");

        let media = PersistedBookMedia {
            library_id: "lib".to_string(),
            file_name: "book.epub".to_string(),
            file_path: file_path.clone(),
            media_type: "application/epub+zip".to_string(),
            page_count: 0,
        };

        let positions = load_epub_archive_positions_fallback(&media, None)
            .expect("epub fallback positions expected");
        assert_eq!(positions.len(), 2);
        assert_eq!(
            positions[0].get("href"),
            Some(&Value::String("/OEBPS/chapter-1.xhtml".to_string()))
        );
        assert_eq!(
            positions[1].get("href"),
            Some(&Value::String("/OEBPS/chapter-2.xhtml".to_string()))
        );
        assert!(positions[0].get("title").is_none());
        assert_eq!(
            positions[0].get("koboSpan"),
            Some(&Value::String("kobo.1.1".to_string()))
        );
        assert_f64_close(
            positions[0]
                .get("locations")
                .and_then(|value| value.get("progression"))
                .and_then(Value::as_f64)
                .expect("progression should be present"),
            0.0,
        );
        assert_f64_close(
            positions[0]
                .get("locations")
                .and_then(|value| value.get("totalProgression"))
                .and_then(Value::as_f64)
                .expect("totalProgression should be present"),
            0.5,
        );
        assert_f64_close(
            positions[1]
                .get("locations")
                .and_then(|value| value.get("totalProgression"))
                .and_then(Value::as_f64)
                .expect("totalProgression should be present"),
            1.0,
        );

        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn load_epub_archive_positions_fallback_uses_readium_style_1024_byte_segmentation() {
        let file_path = unique_temp_path("komga-media-epub-segmentation");
        let container_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;
        let package_document = r#"<?xml version="1.0" encoding="UTF-8"?>
<package version="3.0" xmlns="http://www.idpf.org/2007/opf">
  <manifest>
    <item id="chap-1" href="chapter-1.xhtml" media-type="application/xhtml+xml"/>
    <item id="chap-2" href="chapter-2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="chap-1"/>
    <itemref idref="chap-2"/>
  </spine>
</package>"#;

        let chapter_one = vec![b'a'; 2500];
        let chapter_two = vec![b'b'; 100];
        let archive = build_stored_zip_archive(vec![
            (
                "META-INF/container.xml".to_string(),
                container_xml.as_bytes().to_vec(),
            ),
            (
                "OEBPS/content.opf".to_string(),
                package_document.as_bytes().to_vec(),
            ),
            ("OEBPS/chapter-1.xhtml".to_string(), chapter_one),
            ("OEBPS/chapter-2.xhtml".to_string(), chapter_two),
        ])
        .expect("epub archive should be created");
        fs::write(&file_path, archive).expect("epub test file should be written");

        let media = PersistedBookMedia {
            library_id: "lib".to_string(),
            file_name: "book.epub".to_string(),
            file_path: file_path.clone(),
            media_type: "application/epub+zip".to_string(),
            page_count: 0,
        };

        let positions = load_epub_archive_positions_fallback(&media, None)
            .expect("epub fallback positions expected");
        assert_eq!(positions.len(), 4);
        assert_eq!(
            positions[0].get("href"),
            Some(&Value::String("/OEBPS/chapter-1.xhtml".to_string()))
        );
        assert_eq!(
            positions[1].get("href"),
            Some(&Value::String("/OEBPS/chapter-1.xhtml".to_string()))
        );
        assert_eq!(
            positions[2].get("href"),
            Some(&Value::String("/OEBPS/chapter-1.xhtml".to_string()))
        );
        assert_eq!(
            positions[3].get("href"),
            Some(&Value::String("/OEBPS/chapter-2.xhtml".to_string()))
        );

        assert_f64_close(
            positions[1]
                .get("locations")
                .and_then(|value| value.get("progression"))
                .and_then(Value::as_f64)
                .expect("progression should be present"),
            1.0 / 3.0,
        );
        assert_f64_close(
            positions[2]
                .get("locations")
                .and_then(|value| value.get("progression"))
                .and_then(Value::as_f64)
                .expect("progression should be present"),
            2.0 / 3.0,
        );
        assert!(positions[1].get("koboSpan").is_none());
        assert_eq!(
            positions[3].get("koboSpan"),
            Some(&Value::String("kobo.1.1".to_string()))
        );

        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn parse_epub_fixed_layout_detects_property_and_name_variants() {
        let by_property = br#"<package><metadata><meta property="rendition:layout">pre-paginated</meta></metadata></package>"#;
        assert!(parse_epub_fixed_layout(by_property));

        let by_name =
            br#"<package><metadata><meta name="fixed-layout" content="true"/></metadata></package>"#;
        assert!(parse_epub_fixed_layout(by_name));

        let flowing = br#"<package><metadata><meta property="rendition:layout">reflowable</meta></metadata></package>"#;
        assert!(!parse_epub_fixed_layout(flowing));
    }

    #[test]
    fn parse_epub_kobo_spans_extracts_kobospan_ids_only() {
        let html = br#"<html><body><span class="koboSpan" id="kobo.1.1"></span><span id="kobo.9.9"></span><span class="koboSpan" id="kobo.1.2"></span></body></html>"#;
        let spans = parse_epub_kobo_spans(html);

        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].0, "kobo.1.1");
        assert_eq!(spans[1].0, "kobo.1.2");
    }

    #[tokio::test]
    async fn effective_kepubify_path_prefers_persisted_server_setting() {
        let database_file = unique_temp_path("komga-media-kepub-path");
        let pool = connect_pool(database_file.as_path(), 1)
            .await
            .expect("database should open");
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS SERVER_SETTINGS (KEY varchar NOT NULL PRIMARY KEY, VALUE \
               varchar NULL)",
        )
        .execute(&pool)
        .await
        .expect("server settings table should be created");
        sqlx::query(
            "INSERT \
             OR REPLACE INTO SERVER_SETTINGS (KEY, VALUE) \
             VALUES ('KEPUBIFY_PATH', ?)",
        )
        .bind("/custom/kepubify")
        .execute(&pool)
        .await
        .expect("server setting should be inserted");

        let resolved = load_effective_kepubify_path(
            database_file.as_path(),
            Some(Path::new("/runtime/kepubify")),
        )
        .await;
        assert_eq!(resolved, Some(PathBuf::from("/custom/kepubify")));

        let _ = fs::remove_file(database_file);
    }

    #[tokio::test]
    async fn effective_kepubify_path_falls_back_to_runtime_config() {
        let database_file = unique_temp_path("komga-media-kepub-path-runtime");

        let resolved = load_effective_kepubify_path(
            database_file.as_path(),
            Some(Path::new("/runtime/kepubify")),
        )
        .await;
        assert_eq!(resolved, Some(PathBuf::from("/runtime/kepubify")));
    }

    #[test]
    fn generated_pdf_rows_use_detected_page_count_when_media_count_missing() {
        let file_path = unique_temp_path("komga-media-pdf-fallback");
        write_single_page_pdf(&file_path);

        let media = PersistedBookMedia {
            library_id: "lib".to_string(),
            file_name: "book.pdf".to_string(),
            file_path: file_path.clone(),
            media_type: "application/pdf".to_string(),
            page_count: 0,
        };

        let rows = load_generated_pdf_page_rows(&media);
        assert_eq!(rows.len(), 1);
        let bytes = read_pdf_page_bytes(&media, 1);
        assert!(bytes.is_some());

        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn decode_epub_positions_blob_returns_positions_array() {
        let payload = json!({
            "positions": [
                {
                    "href": "/chap-1.xhtml",
                    "type": "application/xhtml+xml",
                    "locations": { "position": 1, "progression": 0.1 }
                },
                {
                    "href": "/chap-2.xhtml",
                    "type": "application/xhtml+xml",
                    "locations": { "position": 2, "progression": 0.2 }
                }
            ]
        });
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(payload.to_string().as_bytes())
            .expect("gzip payload should be writable");
        let blob = encoder.finish().expect("gzip payload should finalize");

        let positions = decode_epub_positions_blob(&blob).expect("epub positions should decode");
        assert_eq!(positions.len(), 2);
        assert_eq!(
            positions[0].get("href"),
            Some(&Value::String("/chap-1.xhtml".to_string()))
        );
    }
}

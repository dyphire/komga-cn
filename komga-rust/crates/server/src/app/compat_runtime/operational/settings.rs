use axum::Json;
use axum::body::Bytes;
use axum::extract::Extension;
use axum::extract::Path as AxumPath;
use axum::http::StatusCode;
use axum::http::Uri;
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Response};
use bcrypt::{DEFAULT_COST, hash as hash_bcrypt_password};
use komga_persistence::sqlite::connect_pool;
use lopdf::Document as PdfDocument;
use reqwest::Client as AsyncHttpClient;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use time::OffsetDateTime;
use zip::ZipArchive;

use crate::app::runtime_auth::{require_admin, require_auth, resolved_auth_user, user_id};

use super::super::{
    OperationalSettings, OperationalState, TransientBookPageRecord, TransientBookRecord,
    now_epoch_seconds,
};
use super::helpers::{
    effective_kepubify_path, effective_server_context_path, effective_server_port,
    invalid_settings_payload, is_valid_context_path, multi_source_number, multi_source_string,
};

pub(in crate::app::compat_runtime) async fn get_server_settings(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let settings = match state.load_settings().await {
        Ok(settings) => settings,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "message": format!("failed to load settings: {error}") })),
            )
                .into_response();
        }
    };

    {
        let mut task_queue = state
            .task_queue
            .lock()
            .expect("task queue state lock should not be poisoned");
        task_queue.set_task_pool_size(settings.task_pool_size as usize);
        if let Err(error) = task_queue.process_available(&state.runtime) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "message": format!("failed to process queued tasks: {error}") })),
            )
                .into_response();
        }
    }

    Json(settings_json(&state.runtime, &settings)).into_response()
}

pub(in crate::app::compat_runtime) async fn get_claim_status(
    Extension(state): Extension<OperationalState>,
) -> Response {
    let is_claimed = persisted_user_count_from_db_path(state.runtime.database_file.as_path())
        .await
        .unwrap_or(0)
        > 0;

    Json(json!({ "isClaimed": is_claimed })).into_response()
}

pub(in crate::app::compat_runtime) async fn post_claim(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
) -> Response {
    let email = header_value(&headers, "x-komga-email");
    let password = header_value(&headers, "x-komga-password");
    let (Some(email), Some(password)) = (email, password) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let database_file = state.runtime.database_file.as_path();
    let existing_users = match persisted_user_count_from_db_path(database_file).await {
        Ok(count) => count,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if existing_users > 0 {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let hashed_password = match hash_bcrypt_password(password, DEFAULT_COST) {
        Ok(hash) => hash,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let created_user =
        match persisted_create_initial_admin_user(database_file, &email, &hashed_password).await {
            Ok(created_user) => created_user,
            Err(PersistedClaimError::Storage) => {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

    Json(json!({
        "id": created_user.id,
        "email": created_user.email,
        "roles": ["ADMIN"],
    }))
    .into_response()
}

pub(in crate::app::compat_runtime) async fn get_announcements(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let Some(current_user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let feed = match load_cached_announcements_feed(&state).await {
        Ok(Some(feed)) => feed,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };

    let read_ids = match persisted_announcement_read_ids(
        state.runtime.database_file.as_path(),
        user_id(&current_user),
    )
    .await
    {
        Ok(ids) => ids,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(apply_announcement_read_projection(feed, &read_ids)).into_response()
}

pub(in crate::app::compat_runtime) async fn put_announcements(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let Some(current_user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(announcement_ids) = payload.as_array() else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let mut ids = Vec::with_capacity(announcement_ids.len());
    for id in announcement_ids {
        let Some(id) = id.as_str().map(str::trim).filter(|value| !value.is_empty()) else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        ids.push(id.to_string());
    }

    if persisted_save_announcements_read(
        state.runtime.database_file.as_path(),
        user_id(&current_user),
        &ids,
    )
    .await
    .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

pub(in crate::app::compat_runtime) async fn get_releases(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let releases = match load_cached_releases(&state).await {
        Ok(Some(releases)) => releases,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };

    Json(releases).into_response()
}

pub(in crate::app::compat_runtime) async fn post_filesystem(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(requested_path) = payload
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let show_files = payload
        .get("showFiles")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let resolved_path = normalize_requested_path(requested_path, state.runtime.config_dir.as_ref());
    let directories = list_directory_entries(&resolved_path, true);
    let files = if show_files {
        list_directory_entries(&resolved_path, false)
    } else {
        Vec::new()
    };

    Json(json!({
        "path": resolved_path.to_string_lossy().to_string(),
        "directories": directories,
        "files": files,
    }))
    .into_response()
}

pub(in crate::app::compat_runtime) async fn get_fonts_families(
    Extension(state): Extension<OperationalState>,
) -> Response {
    let families = list_font_families(state.runtime.fonts_data_directory.as_path());
    Json(Value::Array(
        families.into_iter().map(Value::String).collect(),
    ))
    .into_response()
}

pub(in crate::app::compat_runtime) async fn get_font_file(
    Extension(state): Extension<OperationalState>,
    AxumPath((font_family, font_file)): AxumPath<(String, String)>,
) -> Response {
    if font_family.contains('/')
        || font_family.contains('\\')
        || font_file.contains('/')
        || font_file.contains('\\')
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Some(media_type) = font_media_type(&font_file) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let font_path = state
        .runtime
        .fonts_data_directory
        .join(&font_family)
        .join(&font_file);
    let Ok(bytes) = fs::read(&font_path) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let content_disposition = format!("attachment; filename=\"{}\"", font_file);
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, media_type),
            (header::CONTENT_DISPOSITION, content_disposition.as_str()),
        ],
        bytes,
    )
        .into_response()
}

pub(in crate::app::compat_runtime) async fn get_font_family_css(
    Extension(state): Extension<OperationalState>,
    AxumPath(font_family): AxumPath<String>,
) -> Response {
    if font_family.contains('/') || font_family.contains('\\') {
        return StatusCode::NOT_FOUND.into_response();
    }

    let family_dir = state.runtime.fonts_data_directory.join(&font_family);
    let Ok(entries) = fs::read_dir(&family_dir) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let mut blocks = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let file_name = match path.file_name().and_then(|value| value.to_str()) {
            Some(value) => value,
            None => continue,
        };
        let Some(format) = font_format(file_name) else {
            continue;
        };

        let lower = file_name.to_ascii_lowercase();
        let style = if lower.contains("italic") {
            "italic"
        } else {
            "normal"
        };
        let weight = if lower.contains("bold") {
            "bold"
        } else {
            "normal"
        };

        blocks.push(format!(
            "@font-face {{\n  font-family: '{}';\n  src: url('{}') format('{}');\n  font-weight: {};\n  font-style: {};\n}}",
            font_family, file_name, format, weight, style,
        ));
    }

    if blocks.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let css = blocks.join("\n\n");
    let content_disposition = format!("attachment; filename=\"{}.css\"", font_family);
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/css"),
            (header::CONTENT_DISPOSITION, content_disposition.as_str()),
        ],
        css,
    )
        .into_response()
}

pub(in crate::app::compat_runtime) async fn get_history(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let query = uri.query().unwrap_or_default();
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(20);

    let page_data =
        match persisted_history_page(state.runtime.database_file.as_path(), page, size).await {
            Ok(page_data) => page_data,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };

    Json(page_data).into_response()
}

pub(in crate::app::compat_runtime) async fn get_page_hashes(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let query = uri.query().unwrap_or_default();
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(20);

    let page_data =
        match persisted_page_hashes_page(state.runtime.database_file.as_path(), page, size).await {
            Ok(page_data) => page_data,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };

    Json(page_data).into_response()
}

pub(in crate::app::compat_runtime) async fn get_page_hashes_unknown(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let query = uri.query().unwrap_or_default();
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(20);

    let page_data =
        match persisted_page_hashes_unknown_page(state.runtime.database_file.as_path(), page, size)
            .await
        {
            Ok(page_data) => page_data,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };

    Json(page_data).into_response()
}

pub(in crate::app::compat_runtime) async fn get_page_hash_matches(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    AxumPath(page_hash): AxumPath<String>,
    uri: Uri,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let query = uri.query().unwrap_or_default();
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(20);

    let page_data = match persisted_page_hash_matches_page(
        state.runtime.database_file.as_path(),
        &page_hash,
        page,
        size,
    )
    .await
    {
        Ok(page_data) => page_data,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(page_data).into_response()
}

pub(in crate::app::compat_runtime) async fn get_page_hash_thumbnail(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    AxumPath(page_hash): AxumPath<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let thumbnail = match persisted_page_hash_thumbnail(
        state.runtime.database_file.as_path(),
        &page_hash,
    )
    .await
    {
        Ok(Some(thumbnail)) => thumbnail,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    ([(header::CONTENT_TYPE, "image/jpeg")], thumbnail).into_response()
}

pub(in crate::app::compat_runtime) async fn get_page_hash_unknown_thumbnail(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    AxumPath(page_hash): AxumPath<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let thumbnail = match persisted_unknown_page_hash_thumbnail(
        state.runtime.database_file.as_path(),
        &page_hash,
    )
    .await
    {
        Ok(Some(thumbnail)) => thumbnail,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    (
        [(header::CONTENT_TYPE, thumbnail.media_type.as_str())],
        thumbnail.bytes,
    )
        .into_response()
}

pub(in crate::app::compat_runtime) async fn put_page_hash(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(hash) = payload
        .get("hash")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let size = payload.get("size").and_then(Value::as_i64);
    let Some(action) = payload
        .get("action")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| matches!(*value, "DELETE_MANUAL" | "DELETE_AUTO" | "IGNORE"))
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    match persisted_upsert_page_hash(state.runtime.database_file.as_path(), hash, size, action)
        .await
    {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn post_page_hash_delete_all(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    AxumPath(page_hash): AxumPath<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match persisted_delete_all_page_hash_matches(state.runtime.database_file.as_path(), &page_hash)
        .await
    {
        Ok(_) => StatusCode::ACCEPTED.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn post_page_hash_delete_match(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    AxumPath(page_hash): AxumPath<String>,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let Some(book_id) = payload
        .get("bookId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(page_number) = payload.get("pageNumber").and_then(Value::as_i64) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if page_number <= 0 {
        return StatusCode::BAD_REQUEST.into_response();
    }

    match persisted_delete_page_hash_match(
        state.runtime.database_file.as_path(),
        &page_hash,
        book_id,
        page_number as u64,
    )
    .await
    {
        Ok(_) => StatusCode::ACCEPTED.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn post_transient_books(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(requested_path) = payload
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let resolved_path = normalize_requested_path(requested_path, state.runtime.config_dir.as_ref());
    let scanned_books = list_transient_book_entries(&resolved_path);

    let mut store = state
        .transient_books
        .lock()
        .expect("transient books state lock should not be poisoned");

    let mut payload = Vec::new();
    for scanned in scanned_books {
        let Some(path) = scanned.get("path").and_then(Value::as_str) else {
            continue;
        };
        let Some(name) = scanned.get("name").and_then(Value::as_str) else {
            continue;
        };

        let file_metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        let id = transient_book_id(path);
        let existing = store.records.get(&id).cloned();
        let status = existing
            .as_ref()
            .map(|record| record.status.clone())
            .unwrap_or_else(|| "UNPROCESSED".to_string());
        let media_type = existing
            .as_ref()
            .map(|record| record.media_type.clone())
            .unwrap_or_default();
        let pages = existing
            .as_ref()
            .map(|record| record.pages.clone())
            .unwrap_or_default();
        let files = existing
            .as_ref()
            .map(|record| record.files.clone())
            .unwrap_or_default();
        let comment = existing
            .as_ref()
            .map(|record| record.comment.clone())
            .unwrap_or_default();
        let number = existing.as_ref().and_then(|record| record.number);
        let series_id = existing
            .as_ref()
            .and_then(|record| record.series_id.clone());

        let record = TransientBookRecord {
            id: id.clone(),
            name: name.to_string(),
            path: path.to_string(),
            file_last_modified_epoch_seconds: to_unix_seconds(file_metadata.modified().ok()),
            size_bytes: file_metadata.len(),
            status,
            media_type,
            pages,
            files,
            comment,
            number,
            series_id,
        };
        store.records.insert(id, record.clone());
        payload.push(transient_book_payload(&record));
    }

    store.persist();
    payload.sort_by(|left, right| {
        left["url"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["url"].as_str().unwrap_or_default())
    });
    Json(Value::Array(payload)).into_response()
}

pub(in crate::app::compat_runtime) async fn post_transient_book_analyze(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    AxumPath(transient_book_id): AxumPath<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let record = {
        let store = state
            .transient_books
            .lock()
            .expect("transient books state lock should not be poisoned");
        store.records.get(&transient_book_id).cloned()
    };
    let Some(record) = record else {
        return StatusCode::NOT_FOUND.into_response();
    };

    if !Path::new(record.path.as_str()).exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let analysis = analyze_transient_book(record.path.as_str());
    let inferred_series_and_number = infer_transient_series_and_number(
        state.runtime.database_file.as_path(),
        record.name.as_str(),
    )
    .await;

    let mut store = state
        .transient_books
        .lock()
        .expect("transient books state lock should not be poisoned");
    let Some(entry) = store.records.get_mut(&transient_book_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    match analysis {
        Ok(analysis) => {
            let (inferred_series_id, inferred_number) = inferred_series_and_number;
            entry.status = analysis.status;
            entry.media_type = analysis.media_type;
            entry.pages = analysis.pages;
            entry.files = analysis.files;
            entry.comment = analysis.comment;
            entry.number = analysis.number.or(inferred_number);
            entry.series_id = analysis.series_id.or(inferred_series_id);
        }
        Err(comment) => {
            entry.status = "ERROR".to_string();
            entry.media_type = transient_media_type(record.path.as_str());
            entry.pages.clear();
            entry.files.clear();
            entry.comment = comment;
            entry.number = None;
            entry.series_id = None;
        }
    }

    let payload = transient_book_payload(entry);
    store.persist();

    Json(payload).into_response()
}

pub(in crate::app::compat_runtime) async fn get_transient_book_status(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    AxumPath(transient_book_id): AxumPath<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let maybe_record = {
        let store = state
            .transient_books
            .lock()
            .expect("transient books state lock should not be poisoned");
        store.records.get(&transient_book_id).cloned()
    };
    let Some(record) = maybe_record else {
        return StatusCode::NOT_FOUND.into_response();
    };

    Json(transient_book_payload(&record)).into_response()
}

pub(in crate::app::compat_runtime) async fn get_transient_book_media(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    AxumPath(transient_book_id): AxumPath<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let store = state
        .transient_books
        .lock()
        .expect("transient books state lock should not be poisoned");
    let Some(record) = store.records.get(&transient_book_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !record.status.eq_ignore_ascii_case("READY") {
        return StatusCode::NOT_FOUND.into_response();
    }

    let bytes = match fs::read(record.path.as_str()) {
        Ok(bytes) => bytes,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let content_type = transient_content_type(record.path.as_str(), record.media_type.as_str());

    ([(header::CONTENT_TYPE, content_type)], bytes).into_response()
}

pub(in crate::app::compat_runtime) async fn get_transient_book_page(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    AxumPath((transient_book_id, page_number)): AxumPath<(String, u32)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }
    let store = state
        .transient_books
        .lock()
        .expect("transient books state lock should not be poisoned");
    let Some(record) = store.records.get(&transient_book_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !record.status.eq_ignore_ascii_case("READY") {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Some((content_type, bytes)) = transient_page_content(record, page_number) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    ([(header::CONTENT_TYPE, content_type)], bytes).into_response()
}

pub(in crate::app::compat_runtime) async fn delete_syncpoints_me(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(current_user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(key_id) = query_value(uri.query().unwrap_or_default(), "key_id") else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if key_id.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }

    if persisted_delete_syncpoints_by_user_and_key_id(
        state.runtime.database_file.as_path(),
        user_id(&current_user),
        key_id,
    )
    .await
    .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

pub(in crate::app::compat_runtime) async fn get_client_settings_global(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
) -> Response {
    let include_unauthorized_only = resolved_auth_user(&headers).is_none();
    let settings = match persisted_client_settings_global(
        state.runtime.database_file.as_path(),
        include_unauthorized_only,
    )
    .await
    {
        Ok(settings) => settings,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    Json(settings).into_response()
}

pub(in crate::app::compat_runtime) async fn get_client_settings_user(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }
    let Some(current_user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let settings = match persisted_client_settings_user(
        state.runtime.database_file.as_path(),
        user_id(&current_user),
    )
    .await
    {
        Ok(settings) => settings,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    Json(settings).into_response()
}

pub(in crate::app::compat_runtime) async fn patch_client_settings_global(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let settings = match parse_client_settings_global_payload(&body) {
        Ok(settings) => settings,
        Err(response) => return response,
    };

    match persisted_upsert_client_settings_global(state.runtime.database_file.as_path(), &settings)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn patch_client_settings_user(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }
    let Some(current_user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let settings = match parse_client_settings_user_payload(&body) {
        Ok(settings) => settings,
        Err(response) => return response,
    };

    match persisted_upsert_client_settings_user(
        state.runtime.database_file.as_path(),
        user_id(&current_user),
        &settings,
    )
    .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn delete_client_settings_global(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let keys = match parse_client_settings_delete_keys(&body) {
        Ok(keys) => keys,
        Err(response) => return response,
    };

    match persisted_delete_client_settings_global(state.runtime.database_file.as_path(), &keys)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn delete_client_settings_user(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }
    let Some(current_user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let keys = match parse_client_settings_delete_keys(&body) {
        Ok(keys) => keys,
        Err(response) => return response,
    };

    match persisted_delete_client_settings_user(
        state.runtime.database_file.as_path(),
        user_id(&current_user),
        &keys,
    )
    .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn get_oauth2_providers(
    Extension(state): Extension<OperationalState>,
) -> Response {
    let providers = state
        .oauth2_clients
        .iter()
        .map(|provider| {
            json!({
                "name": provider.client_name,
                "registrationId": provider.registration_id,
            })
        })
        .collect::<Vec<_>>();

    Json(Value::Array(providers)).into_response()
}

pub(in crate::app::compat_runtime) async fn delete_tasks(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let deleted = state
        .task_queue
        .lock()
        .expect("task queue state lock should not be poisoned")
        .clear_unowned();

    Json(json!(deleted)).into_response()
}

pub(in crate::app::compat_runtime) async fn update_server_settings(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return invalid_settings_payload("invalid settings payload");
    };

    if !payload.is_object() {
        return invalid_settings_payload("invalid settings payload");
    }

    let mut settings = match state.load_settings().await {
        Ok(settings) => settings,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "message": format!("failed to load settings: {error}") })),
            )
                .into_response();
        }
    };
    let mut persistence_changes: Vec<(String, Option<String>)> = Vec::new();

    if let Some(value) = payload.get("deleteEmptyCollections") {
        if !value.is_null() {
            let Some(value) = value.as_bool() else {
                return invalid_settings_payload("deleteEmptyCollections must be a boolean");
            };
            settings.delete_empty_collections = value;
            persistence_changes.push((
                "DELETE_EMPTY_COLLECTIONS".to_string(),
                Some(value.to_string()),
            ));
        }
    }

    if let Some(value) = payload.get("deleteEmptyReadLists") {
        if !value.is_null() {
            let Some(value) = value.as_bool() else {
                return invalid_settings_payload("deleteEmptyReadLists must be a boolean");
            };
            settings.delete_empty_read_lists = value;
            persistence_changes.push((
                "DELETE_EMPTY_READLISTS".to_string(),
                Some(value.to_string()),
            ));
        }
    }

    if let Some(value) = payload.get("rememberMeDurationDays") {
        if !value.is_null() {
            let Some(value) = value.as_u64() else {
                return invalid_settings_payload(
                    "rememberMeDurationDays must be a positive integer",
                );
            };
            if value == 0 {
                return invalid_settings_payload("rememberMeDurationDays must be greater than 0");
            }
            settings.remember_me_duration_days = value;
            persistence_changes.push(("REMEMBER_ME_DURATION".to_string(), Some(value.to_string())));
        }
    }

    if let Some(value) = payload.get("renewRememberMeKey") {
        if !value.is_null() {
            let Some(value) = value.as_bool() else {
                return invalid_settings_payload("renewRememberMeKey must be a boolean");
            };
            if value {
                settings.remember_me_key = generate_remember_me_key();
                persistence_changes.push((
                    "REMEMBER_ME_KEY".to_string(),
                    Some(settings.remember_me_key.clone()),
                ));
            }
        }
    }

    if let Some(value) = payload.get("thumbnailSize") {
        if !value.is_null() {
            let Some(value) = value.as_str() else {
                return invalid_settings_payload("thumbnailSize must be a string");
            };
            if !matches!(value, "DEFAULT" | "MEDIUM" | "LARGE" | "XLARGE") {
                return invalid_settings_payload("thumbnailSize is invalid");
            }
            settings.thumbnail_size = match value {
                "DEFAULT" => "DEFAULT",
                "MEDIUM" => "MEDIUM",
                "LARGE" => "LARGE",
                "XLARGE" => "XLARGE",
                _ => unreachable!(),
            };
            persistence_changes.push((
                "THUMBNAIL_SIZE".to_string(),
                Some(settings.thumbnail_size.to_string()),
            ));
        }
    }

    if let Some(value) = payload.get("taskPoolSize") {
        if !value.is_null() {
            let Some(value) = value.as_u64() else {
                return invalid_settings_payload("taskPoolSize must be a positive integer");
            };
            if value == 0 {
                return invalid_settings_payload("taskPoolSize must be greater than 0");
            }
            settings.task_pool_size = value;
            let mut task_queue = state
                .task_queue
                .lock()
                .expect("task queue state lock should not be poisoned");
            task_queue.set_task_pool_size(value as usize);
            if let Err(error) = task_queue.process_available(&state.runtime) {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "message": format!("failed to process queued tasks: {error}") })),
                )
                    .into_response();
            }
            persistence_changes.push(("TASK_POOL_SIZE".to_string(), Some(value.to_string())));
        }
    }

    if payload.get("serverPort").is_some() {
        match payload.get("serverPort") {
            Some(Value::Null) => settings.server_port = None,
            Some(value) => {
                let Some(value) = value.as_u64() else {
                    return invalid_settings_payload(
                        "serverPort must be an integer between 1 and 65535",
                    );
                };
                if !(1..=65535).contains(&value) {
                    return invalid_settings_payload(
                        "serverPort must be an integer between 1 and 65535",
                    );
                }
                settings.server_port = Some(value as u16);
            }
            None => {}
        }
        persistence_changes.push((
            "SERVER_PORT".to_string(),
            settings.server_port.map(|value| value.to_string()),
        ));
    }

    if payload.get("serverContextPath").is_some() {
        match payload.get("serverContextPath") {
            Some(Value::Null) => settings.server_context_path = None,
            Some(value) => {
                let Some(value) = value.as_str() else {
                    return invalid_settings_payload("serverContextPath must be a string or null");
                };
                if !is_valid_context_path(value) {
                    return invalid_settings_payload("serverContextPath is invalid");
                }
                settings.server_context_path = Some(value.to_string());
            }
            None => {}
        }
        persistence_changes.push((
            "SERVER_CONTEXT_PATH".to_string(),
            settings.server_context_path.clone(),
        ));
    }

    if let Some(value) = payload.get("koboProxy") {
        if !value.is_null() {
            let Some(value) = value.as_bool() else {
                return invalid_settings_payload("koboProxy must be a boolean");
            };
            settings.kobo_proxy = value;
            persistence_changes.push(("KOBO_PROXY".to_string(), Some(value.to_string())));
        }
    }

    if payload.get("koboPort").is_some() {
        match payload.get("koboPort") {
            Some(Value::Null) => settings.kobo_port = None,
            Some(value) => {
                let Some(value) = value.as_u64() else {
                    return invalid_settings_payload(
                        "koboPort must be an integer between 1 and 65535",
                    );
                };
                if !(1..=65535).contains(&value) {
                    return invalid_settings_payload(
                        "koboPort must be an integer between 1 and 65535",
                    );
                }
                settings.kobo_port = Some(value as u16);
            }
            None => {}
        }
        persistence_changes.push((
            "KOBO_PORT".to_string(),
            settings.kobo_port.map(|value| value.to_string()),
        ));
    }

    if payload.get("kepubifyPath").is_some() {
        match payload.get("kepubifyPath") {
            Some(Value::Null) => settings.kepubify_path = None,
            Some(value) => {
                let Some(value) = value.as_str() else {
                    return invalid_settings_payload("kepubifyPath must be a string or null");
                };
                settings.kepubify_path = Some(value.to_string());
            }
            None => {}
        }
        persistence_changes.push(("KEPUBIFY_PATH".to_string(), settings.kepubify_path.clone()));
    }

    if let Err(error) = state
        .settings_store
        .apply_changes(&persistence_changes)
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "message": format!("failed to persist settings: {error}") })),
        )
            .into_response();
    }

    axum::http::StatusCode::NO_CONTENT.into_response()
}

impl OperationalState {
    async fn load_settings(&self) -> Result<OperationalSettings, sqlx::Error> {
        let persisted = self.settings_store.load_map().await?;
        let mut settings = OperationalSettings::from_runtime(&self.runtime);

        settings.delete_empty_collections =
            parse_bool(persisted.get("DELETE_EMPTY_COLLECTIONS"), false);
        settings.delete_empty_read_lists =
            parse_bool(persisted.get("DELETE_EMPTY_READLISTS"), false);
        settings.remember_me_duration_days =
            parse_u64(persisted.get("REMEMBER_ME_DURATION")).unwrap_or(365);
        settings.thumbnail_size =
            parse_thumbnail_size(persisted.get("THUMBNAIL_SIZE")).unwrap_or("DEFAULT");
        settings.task_pool_size = parse_u64(persisted.get("TASK_POOL_SIZE")).unwrap_or(1);
        settings.server_port = parse_u16(persisted.get("SERVER_PORT"));
        settings.server_context_path = parse_string(persisted.get("SERVER_CONTEXT_PATH"));
        settings.kobo_proxy = parse_bool(persisted.get("KOBO_PROXY"), false);
        settings.kobo_port = parse_u16(persisted.get("KOBO_PORT"));
        settings.kepubify_path = parse_non_blank_string(persisted.get("KEPUBIFY_PATH"));
        settings.remember_me_key = parse_non_blank_string(persisted.get("REMEMBER_ME_KEY"))
            .unwrap_or_else(generate_remember_me_key);

        if !persisted.contains_key("REMEMBER_ME_KEY")
            || persisted
                .get("REMEMBER_ME_KEY")
                .is_some_and(|value| value.as_deref().unwrap_or_default().trim().is_empty())
        {
            self.settings_store
                .apply_changes(&[(
                    "REMEMBER_ME_KEY".to_string(),
                    Some(settings.remember_me_key.clone()),
                )])
                .await?;
        }

        Ok(settings)
    }
}

struct CreatedUser {
    id: String,
    email: String,
}

enum PersistedClaimError {
    Storage,
}

struct PersistedHistoricalEvent {
    id: String,
    event_type: String,
    book_id: Option<String>,
    series_id: Option<String>,
    timestamp: String,
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
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

async fn load_cached_announcements_feed(
    state: &OperationalState,
) -> Result<Option<Value>, reqwest::Error> {
    const CACHE_TTL_SECONDS: u64 = 60 * 60;
    let now = now_epoch_seconds();
    {
        let cache = state
            .announcements_cache
            .lock()
            .expect("announcements cache lock should not be poisoned");
        if let Some(cached) = cache.as_ref() {
            if now.saturating_sub(cached.fetched_at_epoch_seconds) < CACHE_TTL_SECONDS {
                return Ok(Some(cached.payload.clone()));
            }
        }
    }

    let url = std::env::var("KOMGA_RUST_ANNOUNCEMENTS_URL")
        .unwrap_or_else(|_| "https://komga.org/blog/feed.json".to_string());
    let payload = AsyncHttpClient::new()
        .get(url)
        .send()
        .await?
        .json::<Value>()
        .await?;

    {
        let mut cache = state
            .announcements_cache
            .lock()
            .expect("announcements cache lock should not be poisoned");
        *cache = Some(super::super::RemoteCacheEntry {
            fetched_at_epoch_seconds: now,
            payload: payload.clone(),
        });
    }

    Ok(Some(payload))
}

async fn load_cached_releases(state: &OperationalState) -> Result<Option<Value>, reqwest::Error> {
    const CACHE_TTL_SECONDS: u64 = 60 * 60;
    let now = now_epoch_seconds();
    {
        let cache = state
            .releases_cache
            .lock()
            .expect("releases cache lock should not be poisoned");
        if let Some(cached) = cache.as_ref() {
            if now.saturating_sub(cached.fetched_at_epoch_seconds) < CACHE_TTL_SECONDS {
                return Ok(Some(cached.payload.clone()));
            }
        }
    }

    let url = std::env::var("KOMGA_RUST_RELEASES_URL").unwrap_or_else(|_| {
        "https://api.github.com/repos/gotson/komga/releases?per_page=20".to_string()
    });
    let upstream = AsyncHttpClient::new()
        .get(url)
        .header("User-Agent", "komga-rust-compat")
        .send()
        .await?
        .json::<Value>()
        .await?;

    let payload = map_github_releases(upstream);
    {
        let mut cache = state
            .releases_cache
            .lock()
            .expect("releases cache lock should not be poisoned");
        *cache = Some(super::super::RemoteCacheEntry {
            fetched_at_epoch_seconds: now,
            payload: payload.clone(),
        });
    }

    Ok(Some(payload))
}

fn map_github_releases(upstream: Value) -> Value {
    let Some(items) = upstream.as_array() else {
        return Value::Array(Vec::new());
    };

    Value::Array(
        items
            .iter()
            .enumerate()
            .map(|(index, release)| {
                json!({
                    "version": release.get("tag_name").cloned().unwrap_or(Value::Null),
                    "releaseDate": release.get("published_at").cloned().unwrap_or(Value::Null),
                    "url": release.get("html_url").cloned().unwrap_or(Value::Null),
                    "latest": index == 0,
                    "preRelease": release.get("prerelease").cloned().unwrap_or(Value::Bool(false)),
                    "description": release.get("body").cloned().unwrap_or(Value::Null),
                })
            })
            .collect(),
    )
}

fn apply_announcement_read_projection(feed: Value, read_ids: &[String]) -> Value {
    let mut projected = feed;
    let read_set = read_ids
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();

    if let Some(items) = projected
        .as_object_mut()
        .and_then(|object| object.get_mut("items"))
        .and_then(Value::as_array_mut)
    {
        for item in items {
            let read = item
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| read_set.contains(id));
            if let Some(object) = item.as_object_mut() {
                object.insert("_komga".to_string(), json!({ "read": read }));
            }
        }
    }

    projected
}

fn transient_book_id(path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.as_bytes());
    let digest = hasher.finalize();
    let digest_hex = digest
        .as_slice()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("transient-{digest_hex}")[..26].to_string()
}

fn transient_book_payload(record: &TransientBookRecord) -> Value {
    json!({
        "id": record.id,
        "name": record.name,
        "url": record.path,
        "fileLastModified": format_local_datetime(record.file_last_modified_epoch_seconds),
        "sizeBytes": record.size_bytes,
        "size": format_size_bytes(record.size_bytes),
        "status": record.status,
        "mediaType": record.media_type,
        "pages": record.pages.iter().map(transient_page_payload).collect::<Vec<_>>(),
        "files": record.files,
        "comment": record.comment,
        "number": record.number,
        "seriesId": record.series_id,
    })
}

fn format_local_datetime(epoch_seconds: i64) -> String {
    let datetime = OffsetDateTime::from_unix_timestamp(epoch_seconds)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH)
        .to_offset(time::UtcOffset::UTC);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        datetime.year(),
        datetime.month() as u8,
        datetime.day(),
        datetime.hour(),
        datetime.minute(),
        datetime.second(),
    )
}

fn transient_page_payload(page: &TransientBookPageRecord) -> Value {
    json!({
        "number": page.number,
        "fileName": page.file_name,
        "mediaType": page.media_type,
        "width": page.width,
        "height": page.height,
        "sizeBytes": page.size_bytes,
        "size": page
            .size_bytes
            .map(format_size_bytes)
            .unwrap_or_default(),
    })
}

fn transient_content_type(path: &str, media_type: &str) -> &'static str {
    if !media_type.is_empty() {
        return match media_type {
            "application/pdf" => "application/pdf",
            "application/epub+zip" => "application/epub+zip",
            "application/zip" => "application/zip",
            "application/vnd.comicbook-rar" => "application/vnd.comicbook-rar",
            _ => "application/octet-stream",
        };
    }

    match PathBuf::from(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("pdf") => "application/pdf",
        Some("epub") => "application/epub+zip",
        Some("cbz") | Some("zip") => "application/zip",
        Some("cbr") | Some("rar") => "application/vnd.comicbook-rar",
        _ => "application/octet-stream",
    }
}

#[derive(Clone, Debug)]
struct TransientBookAnalysis {
    status: String,
    media_type: String,
    pages: Vec<TransientBookPageRecord>,
    files: Vec<String>,
    comment: String,
    number: Option<f64>,
    series_id: Option<String>,
}

fn analyze_transient_book(path: &str) -> Result<TransientBookAnalysis, String> {
    if !Path::new(path).exists() {
        return Err("File not found, it may have moved".to_string());
    }

    let media_type = transient_media_type(path);
    let (pages, files) = if transient_media_is_image(path, &media_type) {
        analyze_transient_image(path)
    } else if transient_media_is_zip_archive(path, &media_type) {
        analyze_transient_zip_archive(path)?
    } else if transient_media_is_rar_archive(path, &media_type) {
        analyze_transient_rar_archive(path)?
    } else if transient_media_is_pdf(path, &media_type) {
        analyze_transient_pdf(path)?
    } else {
        return Err(format!("unsupported media type: {media_type}"));
    };

    if pages.is_empty() {
        return Err("Book analysis failed".to_string());
    }

    Ok(TransientBookAnalysis {
        status: "READY".to_string(),
        media_type,
        pages,
        files,
        comment: String::new(),
        number: None,
        series_id: None,
    })
}

fn analyze_transient_image(path: &str) -> (Vec<TransientBookPageRecord>, Vec<String>) {
    let file_name = PathBuf::from(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();
    let size_bytes = fs::metadata(path).ok().map(|meta| meta.len());

    (
        vec![TransientBookPageRecord {
            number: 1,
            file_name: file_name.clone(),
            media_type: transient_entry_media_type(&file_name),
            width: None,
            height: None,
            size_bytes,
        }],
        vec![file_name],
    )
}

fn analyze_transient_zip_archive(
    path: &str,
) -> Result<(Vec<TransientBookPageRecord>, Vec<String>), String> {
    let file = fs::File::open(path).map_err(|error| format!("open archive: {error}"))?;
    let mut archive = ZipArchive::new(file).map_err(|error| format!("read archive: {error}"))?;

    let mut files = Vec::new();
    let mut pages = Vec::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| format!("read archive entry: {error}"))?;
        let file_name = entry.name().trim().to_string();
        if file_name.is_empty() || file_name.ends_with('/') {
            continue;
        }

        files.push(file_name.clone());
        if !is_supported_page_image_file_name(&file_name) {
            continue;
        }

        pages.push(TransientBookPageRecord {
            number: (pages.len() as u32) + 1,
            file_name: file_name.clone(),
            media_type: transient_entry_media_type(&file_name),
            width: None,
            height: None,
            size_bytes: Some(entry.size()),
        });
    }

    files.sort();
    Ok((pages, files))
}

fn analyze_transient_rar_archive(
    path: &str,
) -> Result<(Vec<TransientBookPageRecord>, Vec<String>), String> {
    let output = Command::new("unrar")
        .arg("lb")
        .arg(path)
        .output()
        .map_err(|error| format!("list rar entries: {error}"))?;
    if !output.status.success() {
        return Err("Book analysis failed".to_string());
    }

    let mut files = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.ends_with('/'))
        .map(str::to_string)
        .collect::<Vec<_>>();
    files.sort();

    let pages = files
        .iter()
        .filter(|file_name| is_supported_page_image_file_name(file_name))
        .enumerate()
        .map(|(index, file_name)| TransientBookPageRecord {
            number: (index as u32) + 1,
            file_name: file_name.clone(),
            media_type: transient_entry_media_type(file_name),
            width: None,
            height: None,
            size_bytes: None,
        })
        .collect::<Vec<_>>();

    Ok((pages, files))
}

fn analyze_transient_pdf(
    path: &str,
) -> Result<(Vec<TransientBookPageRecord>, Vec<String>), String> {
    let document = PdfDocument::load(path).map_err(|error| format!("open pdf: {error}"))?;
    let page_count = document.get_pages().len() as u32;
    let pages = (1..=page_count)
        .map(|number| TransientBookPageRecord {
            number,
            file_name: format!("page-{number}.pdf"),
            media_type: "application/pdf".to_string(),
            width: None,
            height: None,
            size_bytes: None,
        })
        .collect::<Vec<_>>();

    let file_name = PathBuf::from(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();

    Ok((pages, vec![file_name]))
}

fn transient_page_content(
    record: &TransientBookRecord,
    page_number: u32,
) -> Option<(String, Vec<u8>)> {
    if page_number == 0 {
        return None;
    }

    let media_type = if record.media_type.is_empty() {
        transient_media_type(record.path.as_str())
    } else {
        record.media_type.clone()
    };

    if transient_media_is_image(record.path.as_str(), media_type.as_str()) {
        if page_number != 1 {
            return None;
        }
        let bytes = fs::read(record.path.as_str()).ok()?;
        return Some((media_type, bytes));
    }

    let page = record
        .pages
        .iter()
        .find(|entry| entry.number == page_number)
        .cloned()?;

    if transient_media_is_zip_archive(record.path.as_str(), media_type.as_str()) {
        let file = fs::File::open(record.path.as_str()).ok()?;
        let mut archive = ZipArchive::new(file).ok()?;
        let mut entry = archive.by_name(page.file_name.as_str()).ok()?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).ok()?;
        return Some((page.media_type, bytes));
    }

    if transient_media_is_rar_archive(record.path.as_str(), media_type.as_str()) {
        let bytes = read_rar_entry_bytes_cli(record.path.as_str(), page.file_name.as_str())?;
        return Some((page.media_type, bytes));
    }

    if transient_media_is_pdf(record.path.as_str(), media_type.as_str()) {
        let bytes = read_pdf_page_content_bytes(record.path.as_str(), page_number)?;
        return Some(("application/pdf".to_string(), bytes));
    }

    None
}

fn read_rar_entry_bytes_cli(archive_path: &str, entry_name: &str) -> Option<Vec<u8>> {
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

fn read_pdf_page_content_bytes(path: &str, page_number: u32) -> Option<Vec<u8>> {
    let document = PdfDocument::load(path).ok()?;
    let pages = document.get_pages();
    let object_id = *pages.get(&page_number)?;
    document.get_page_content(object_id).ok()
}

fn transient_media_type(path: &str) -> String {
    transient_content_type(path, "").to_string()
}

fn transient_media_is_image(path: &str, media_type: &str) -> bool {
    if media_type.starts_with("image/") {
        return true;
    }
    matches!(
        PathBuf::from(path)
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .as_deref(),
        Some("jpg") | Some("jpeg") | Some("png") | Some("gif") | Some("webp") | Some("avif")
    )
}

fn transient_media_is_zip_archive(path: &str, media_type: &str) -> bool {
    matches!(
        transient_content_type(path, media_type),
        "application/zip" | "application/epub+zip"
    )
}

fn transient_media_is_rar_archive(path: &str, media_type: &str) -> bool {
    transient_content_type(path, media_type) == "application/vnd.comicbook-rar"
}

fn transient_media_is_pdf(path: &str, media_type: &str) -> bool {
    transient_content_type(path, media_type) == "application/pdf"
}

fn transient_entry_media_type(file_name: &str) -> String {
    match file_name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg".to_string(),
        "png" => "image/png".to_string(),
        "gif" => "image/gif".to_string(),
        "webp" => "image/webp".to_string(),
        "avif" => "image/avif".to_string(),
        "pdf" => "application/pdf".to_string(),
        _ => "application/octet-stream".to_string(),
    }
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

async fn infer_transient_series_and_number(
    database_file: &Path,
    file_name: &str,
) -> (Option<String>, Option<f64>) {
    let (series_title_candidate, number) = parse_transient_series_and_number_candidate(file_name);
    if series_title_candidate.is_empty() {
        return (None, number);
    }

    let pool = match connect_pool(database_file, 1).await {
        Ok(pool) => pool,
        Err(_) => return (None, number),
    };

    let exact_match = sqlx::query(
        "SELECT s.ID AS ID \
         FROM SERIES s \
         LEFT \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         WHERE LOWER(COALESCE(sm.TITLE, s.NAME)) = LOWER(?) \
         ORDER BY s.LAST_MODIFIED_DATE DESC, s.ID ASC \
         LIMIT 1",
    )
    .bind(series_title_candidate.as_str())
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten()
    .map(|row| row.get::<String, _>("ID"));

    let fuzzy_match = if exact_match.is_none() {
        sqlx::query(
            "SELECT s.ID AS ID \
             FROM SERIES s \
             LEFT \
             JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
             WHERE LOWER(COALESCE(sm.TITLE, s.NAME)) LIKE LOWER(?) \
             ORDER BY s.LAST_MODIFIED_DATE DESC, s.ID ASC \
             LIMIT 1",
        )
        .bind(format!("%{}%", series_title_candidate))
        .fetch_optional(&pool)
        .await
        .ok()
        .flatten()
        .map(|row| row.get::<String, _>("ID"))
    } else {
        None
    };

    (exact_match.or(fuzzy_match), number)
}

fn parse_transient_series_and_number_candidate(file_name: &str) -> (String, Option<f64>) {
    let file_path = PathBuf::from(file_name);
    let stem = file_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(file_name)
        .trim();
    if stem.is_empty() {
        return (String::new(), None);
    }

    let normalized = stem
        .chars()
        .map(|ch| {
            if ch == '_' || ch == '-' || ch == '.' {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>();

    let mut parts = normalized
        .split_whitespace()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return (String::new(), None);
    }

    let mut number = None;
    if let Some(last) = parts.last()
        && let Ok(parsed_number) = last.parse::<f64>()
    {
        number = Some(parsed_number);
        let _ = parts.pop();
    }

    let series_title_candidate = if parts.is_empty() {
        normalized.trim().to_string()
    } else {
        parts.join(" ")
    };

    (series_title_candidate, number)
}

fn to_unix_seconds(time: Option<SystemTime>) -> i64 {
    time.and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_secs() as i64)
        .unwrap_or_default()
}

async fn persisted_user_count_from_db_path(database_file: &Path) -> Result<i64, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let count = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
                             FROM USER",
    )
    .fetch_one(&pool)
    .await?
    .get::<i64, _>("COUNT");
    Ok(count)
}

async fn persisted_create_initial_admin_user(
    database_file: &Path,
    email: &str,
    hashed_password: &str,
) -> Result<CreatedUser, PersistedClaimError> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|_| PersistedClaimError::Storage)?;
    let created_user_id = generate_claimed_user_id();

    let mut tx = pool
        .begin()
        .await
        .map_err(|_| PersistedClaimError::Storage)?;
    sqlx::query(
        "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES, AGE_RESTRICTION, \
           AGE_RESTRICTION_ALLOW_ONLY) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&created_user_id)
    .bind(email)
    .bind(hashed_password)
    .bind(true)
    .bind(None::<i64>)
    .bind(None::<bool>)
    .execute(&mut *tx)
    .await
    .map_err(|_| PersistedClaimError::Storage)?;

    sqlx::query(
        "INSERT INTO USER_ROLE (USER_ID, ROLE) \
                 VALUES (?, ?)",
    )
    .bind(&created_user_id)
    .bind("ADMIN")
    .execute(&mut *tx)
    .await
    .map_err(|_| PersistedClaimError::Storage)?;

    tx.commit()
        .await
        .map_err(|_| PersistedClaimError::Storage)?;

    Ok(CreatedUser {
        id: created_user_id,
        email: email.to_string(),
    })
}

async fn persisted_delete_syncpoints_by_user_and_key_id(
    database_file: &Path,
    user_id: &str,
    key_id: &str,
) -> Result<(), sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    sqlx::query(
        "DELETE \
                 FROM SYNC_POINT \
                 WHERE USER_ID = ? \
                 AND API_KEY_ID = ?",
    )
    .bind(user_id)
    .bind(key_id)
    .execute(&pool)
    .await?;
    Ok(())
}

async fn persisted_save_announcements_read(
    database_file: &Path,
    user_id: &str,
    announcement_ids: &[String],
) -> Result<(), sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let mut tx = pool.begin().await?;

    for announcement_id in announcement_ids {
        sqlx::query(
            "INSERT \
             OR IGNORE INTO ANNOUNCEMENTS_READ (USER_ID, ANNOUNCEMENT_ID) \
             VALUES (?, ?)",
        )
        .bind(user_id)
        .bind(announcement_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

async fn persisted_announcement_read_ids(
    database_file: &Path,
    user_id: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT ANNOUNCEMENT_ID \
         FROM ANNOUNCEMENTS_READ \
         WHERE USER_ID = ? \
         ORDER BY ANNOUNCEMENT_ID ASC",
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("ANNOUNCEMENT_ID"))
        .collect())
}

async fn persisted_client_settings_global(
    database_file: &Path,
    allow_unauthorized_only: bool,
) -> Result<Value, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let rows = if allow_unauthorized_only {
        sqlx::query(
            "SELECT KEY, VALUE, ALLOW_UNAUTHORIZED \
             FROM CLIENT_SETTINGS_GLOBAL \
             WHERE ALLOW_UNAUTHORIZED = 1 \
             ORDER BY KEY ASC",
        )
        .fetch_all(&pool)
        .await?
    } else {
        sqlx::query(
            "SELECT KEY, VALUE, ALLOW_UNAUTHORIZED \
             FROM CLIENT_SETTINGS_GLOBAL \
             ORDER BY KEY ASC",
        )
        .fetch_all(&pool)
        .await?
    };

    let mut map = serde_json::Map::new();
    for row in rows {
        let key = row.get::<String, _>("KEY");
        let value = row.get::<String, _>("VALUE");
        let allow_unauthorized = row.get::<bool, _>("ALLOW_UNAUTHORIZED");
        map.insert(
            key,
            json!({
                "value": value,
                "allowUnauthorized": allow_unauthorized,
            }),
        );
    }
    if !map.contains_key("webui.oauth2.hide_login") {
        map.insert(
            "webui.oauth2.hide_login".to_string(),
            json!({
                "value": "false",
                "allowUnauthorized": true,
            }),
        );
    }
    Ok(Value::Object(map))
}

async fn persisted_client_settings_user(
    database_file: &Path,
    user_id: &str,
) -> Result<Value, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT KEY, VALUE \
         FROM CLIENT_SETTINGS_USER \
         WHERE USER_ID = ? \
         ORDER BY KEY ASC",
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await?;

    let mut map = serde_json::Map::new();
    for row in rows {
        let key = row.get::<String, _>("KEY");
        let value = row.get::<String, _>("VALUE");
        map.insert(key, json!({ "value": value }));
    }
    Ok(Value::Object(map))
}

async fn persisted_upsert_client_settings_global(
    database_file: &Path,
    settings: &[(String, String, bool)],
) -> Result<(), sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let mut tx = pool.begin().await?;
    for (key, value, allow_unauthorized) in settings {
        sqlx::query(
            "INSERT INTO CLIENT_SETTINGS_GLOBAL (KEY, VALUE, ALLOW_UNAUTHORIZED) \
             VALUES (?, ?, ?) \
             ON CONFLICT(KEY) DO UPDATE \
             SET VALUE = excluded.VALUE, ALLOW_UNAUTHORIZED = excluded.ALLOW_UNAUTHORIZED",
        )
        .bind(key)
        .bind(value)
        .bind(*allow_unauthorized)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn persisted_upsert_client_settings_user(
    database_file: &Path,
    user_id: &str,
    settings: &[(String, String)],
) -> Result<(), sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let mut tx = pool.begin().await?;
    for (key, value) in settings {
        sqlx::query(
            "INSERT INTO CLIENT_SETTINGS_USER (USER_ID, KEY, VALUE) \
             VALUES (?, ?, ?) \
             ON CONFLICT(KEY, USER_ID) DO UPDATE \
             SET VALUE = excluded.VALUE",
        )
        .bind(user_id)
        .bind(key)
        .bind(value)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn persisted_delete_client_settings_global(
    database_file: &Path,
    keys: &[String],
) -> Result<(), sqlx::Error> {
    if keys.is_empty() {
        return Ok(());
    }

    let pool = connect_pool(database_file, 1).await?;
    let mut tx = pool.begin().await?;
    for key in keys {
        sqlx::query(
            "DELETE \
                     FROM CLIENT_SETTINGS_GLOBAL \
                     WHERE KEY = ?",
        )
        .bind(key)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn persisted_delete_client_settings_user(
    database_file: &Path,
    user_id: &str,
    keys: &[String],
) -> Result<(), sqlx::Error> {
    if keys.is_empty() {
        return Ok(());
    }

    let pool = connect_pool(database_file, 1).await?;
    let mut tx = pool.begin().await?;
    for key in keys {
        sqlx::query(
            "DELETE \
                     FROM CLIENT_SETTINGS_USER \
                     WHERE USER_ID = ? \
                     AND KEY = ?",
        )
        .bind(user_id)
        .bind(key)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

fn parse_client_settings_global_payload(
    body: &[u8],
) -> Result<Vec<(String, String, bool)>, Response> {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return Err(StatusCode::BAD_REQUEST.into_response());
    };
    let Some(object) = value.as_object() else {
        return Err(StatusCode::BAD_REQUEST.into_response());
    };

    let mut settings = Vec::with_capacity(object.len());
    for (key, item) in object {
        if !is_valid_client_settings_key(key) {
            return Err(StatusCode::BAD_REQUEST.into_response());
        }
        let Some(item) = item.as_object() else {
            return Err(StatusCode::BAD_REQUEST.into_response());
        };
        let Some(value) = item
            .get("value")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Err(StatusCode::BAD_REQUEST.into_response());
        };
        let Some(allow_unauthorized) = item.get("allowUnauthorized").and_then(Value::as_bool)
        else {
            return Err(StatusCode::BAD_REQUEST.into_response());
        };
        settings.push((key.to_string(), value.to_string(), allow_unauthorized));
    }

    Ok(settings)
}

fn parse_client_settings_user_payload(body: &[u8]) -> Result<Vec<(String, String)>, Response> {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return Err(StatusCode::BAD_REQUEST.into_response());
    };
    let Some(object) = value.as_object() else {
        return Err(StatusCode::BAD_REQUEST.into_response());
    };

    let mut settings = Vec::with_capacity(object.len());
    for (key, item) in object {
        if !is_valid_client_settings_key(key) {
            return Err(StatusCode::BAD_REQUEST.into_response());
        }
        let Some(item) = item.as_object() else {
            return Err(StatusCode::BAD_REQUEST.into_response());
        };
        let Some(value) = item
            .get("value")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Err(StatusCode::BAD_REQUEST.into_response());
        };
        settings.push((key.to_string(), value.to_string()));
    }

    Ok(settings)
}

fn parse_client_settings_delete_keys(body: &[u8]) -> Result<Vec<String>, Response> {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return Err(StatusCode::BAD_REQUEST.into_response());
    };
    let Some(items) = value.as_array() else {
        return Err(StatusCode::BAD_REQUEST.into_response());
    };

    let mut keys = Vec::new();
    for item in items {
        let Some(key) = item
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Err(StatusCode::BAD_REQUEST.into_response());
        };
        if !is_valid_client_settings_key(key) {
            return Err(StatusCode::BAD_REQUEST.into_response());
        }
        keys.push(key.to_string());
    }

    Ok(keys)
}

fn is_valid_client_settings_key(key: &str) -> bool {
    let mut segments = key.split('.');
    let Some(first) = segments.next() else {
        return false;
    };
    if !is_valid_client_settings_segment(first) {
        return false;
    }
    segments.all(is_valid_client_settings_segment)
}

fn is_valid_client_settings_segment(segment: &str) -> bool {
    if segment.is_empty() {
        return false;
    }
    let mut chars = segment.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    let Some(last) = segment.chars().last() else {
        return false;
    };
    if !last.is_ascii_lowercase() && !last.is_ascii_digit() {
        return false;
    }

    segment
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
}

async fn persisted_history_page(
    database_file: &Path,
    page: u64,
    size: u64,
) -> Result<Value, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;

    let total_elements = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
                                      FROM HISTORICAL_EVENT",
    )
    .fetch_one(&pool)
    .await?
    .get::<i64, _>("COUNT") as u64;

    let size = size.max(1);
    let offset = page.saturating_mul(size);

    let events = sqlx::query(
        "SELECT ID, TYPE, BOOK_ID, SERIES_ID, TIMESTAMP \
         FROM HISTORICAL_EVENT \
         ORDER BY TIMESTAMP DESC, ID DESC \
         LIMIT ? \
         OFFSET ?",
    )
    .bind((size.min(i64::MAX as u64)) as i64)
    .bind((offset.min(i64::MAX as u64)) as i64)
    .fetch_all(&pool)
    .await?
    .into_iter()
    .map(|row| PersistedHistoricalEvent {
        id: row.get::<String, _>("ID"),
        event_type: row.get::<String, _>("TYPE"),
        book_id: row.get::<Option<String>, _>("BOOK_ID"),
        series_id: row.get::<Option<String>, _>("SERIES_ID"),
        timestamp: row.get::<String, _>("TIMESTAMP"),
    })
    .collect::<Vec<_>>();

    let mut properties_by_id: HashMap<String, serde_json::Map<String, Value>> = HashMap::new();
    if !events.is_empty() {
        let placeholders = std::iter::repeat_n("?", events.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT ID, \"KEY\" AS EVENT_KEY, VALUE \
             FROM HISTORICAL_EVENT_PROPERTIES \
             WHERE ID IN ({placeholders})",
        );

        let mut query = sqlx::query(&sql);
        for event in &events {
            query = query.bind(&event.id);
        }

        let property_rows = query.fetch_all(&pool).await?;
        for row in property_rows {
            let event_id = row.get::<String, _>("ID");
            let key = row.get::<String, _>("EVENT_KEY");
            let value = row.get::<String, _>("VALUE");
            properties_by_id
                .entry(event_id)
                .or_default()
                .insert(key, Value::String(value));
        }
    }

    let content = events
        .into_iter()
        .map(|event| {
            let properties = properties_by_id.remove(&event.id).unwrap_or_default();
            json!({
                "id": event.id,
                "type": event.event_type,
                "bookId": event.book_id,
                "seriesId": event.series_id,
                "timestamp": event.timestamp,
                "properties": properties,
            })
        })
        .collect::<Vec<_>>();

    let total_pages = if total_elements == 0 {
        0
    } else {
        (total_elements + size - 1) / size
    };
    let number_of_elements = content.len() as u64;
    let first = page == 0;
    let last = total_pages == 0 || page + 1 >= total_pages;

    Ok(json!({
        "content": content,
        "pageable": {
            "pageNumber": page,
            "pageSize": size,
            "sort": {
                "empty": false,
                "sorted": true,
                "unsorted": false,
            },
            "offset": offset,
            "paged": true,
            "unpaged": false,
        },
        "last": last,
        "totalElements": total_elements,
        "totalPages": total_pages,
        "first": first,
        "size": size,
        "number": page,
        "sort": {
            "empty": false,
            "sorted": true,
            "unsorted": false,
        },
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0,
    }))
}

async fn persisted_page_hashes_page(
    database_file: &Path,
    page: u64,
    size: u64,
) -> Result<Value, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;

    let total_elements = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
                                      FROM PAGE_HASH",
    )
    .fetch_one(&pool)
    .await?
    .get::<i64, _>("COUNT") as u64;

    let size = size.max(1);
    let offset = page.saturating_mul(size);

    let content = sqlx::query(
        "SELECT ph.HASH, ph.SIZE, ph.ACTION, ph.DELETE_COUNT, ph.CREATED_DATE, \
                ph.LAST_MODIFIED_DATE, COUNT(mp.BOOK_ID) AS MATCH_COUNT \
         FROM PAGE_HASH ph \
         LEFT \
         JOIN MEDIA_PAGE mp ON mp.FILE_HASH = ph.HASH \
         GROUP BY ph.HASH, ph.SIZE, ph.ACTION, ph.DELETE_COUNT, ph.CREATED_DATE, \
                  ph.LAST_MODIFIED_DATE \
         ORDER BY ph.LAST_MODIFIED_DATE DESC, ph.HASH DESC \
         LIMIT ? \
         OFFSET ?",
    )
    .bind((size.min(i64::MAX as u64)) as i64)
    .bind((offset.min(i64::MAX as u64)) as i64)
    .fetch_all(&pool)
    .await?
    .into_iter()
    .map(|row| {
        json!({
            "hash": row.get::<String, _>("HASH"),
            "size": row.get::<Option<i64>, _>("SIZE"),
            "action": row.get::<String, _>("ACTION"),
            "deleteCount": row.get::<i64, _>("DELETE_COUNT"),
            "matchCount": row.get::<i64, _>("MATCH_COUNT"),
            "created": sqlite_datetime_to_utc(&row.get::<String, _>("CREATED_DATE")),
            "lastModified": sqlite_datetime_to_utc(&row.get::<String, _>("LAST_MODIFIED_DATE")),
        })
    })
    .collect::<Vec<_>>();

    let total_pages = if total_elements == 0 {
        0
    } else {
        total_elements.div_ceil(size)
    };
    let number_of_elements = content.len() as u64;
    let first = page == 0;
    let last = total_pages == 0 || page + 1 >= total_pages;

    Ok(json!({
        "content": content,
        "pageable": {
            "pageNumber": page,
            "pageSize": size,
            "sort": {
                "empty": false,
                "sorted": true,
                "unsorted": false,
            },
            "offset": offset,
            "paged": true,
            "unpaged": false,
        },
        "last": last,
        "totalElements": total_elements,
        "totalPages": total_pages,
        "first": first,
        "size": size,
        "number": page,
        "sort": {
            "empty": false,
            "sorted": true,
            "unsorted": false,
        },
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0,
    }))
}

async fn persisted_page_hashes_unknown_page(
    database_file: &Path,
    page: u64,
    size: u64,
) -> Result<Value, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;

    let total_elements = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
         FROM ( SELECT mp.FILE_HASH \
         FROM MEDIA_PAGE mp \
         WHERE mp.FILE_HASH <> '' \
         AND NOT EXISTS (SELECT 1 \
         FROM PAGE_HASH ph \
         WHERE ph.HASH = mp.FILE_HASH) \
         GROUP BY mp.FILE_HASH \
         HAVING COUNT(mp.BOOK_ID) > 1 ) unknown_hashes",
    )
    .fetch_one(&pool)
    .await?
    .get::<i64, _>("COUNT") as u64;

    let size = size.max(1);
    let offset = page.saturating_mul(size);

    let content = sqlx::query(
        "SELECT mp.FILE_HASH AS HASH, mp.FILE_SIZE AS SIZE, COUNT(mp.BOOK_ID) AS MATCH_COUNT \
         FROM MEDIA_PAGE mp \
         WHERE mp.FILE_HASH <> '' \
         AND NOT EXISTS (SELECT 1 \
         FROM PAGE_HASH ph \
         WHERE ph.HASH = mp.FILE_HASH) \
         GROUP BY mp.FILE_HASH, mp.FILE_SIZE \
         HAVING COUNT(mp.BOOK_ID) > 1 \
         ORDER BY MATCH_COUNT DESC, HASH ASC \
         LIMIT ? \
         OFFSET ?",
    )
    .bind((size.min(i64::MAX as u64)) as i64)
    .bind((offset.min(i64::MAX as u64)) as i64)
    .fetch_all(&pool)
    .await?
    .into_iter()
    .map(|row| {
        json!({
            "hash": row.get::<String, _>("HASH"),
            "size": row.get::<Option<i64>, _>("SIZE"),
            "matchCount": row.get::<i64, _>("MATCH_COUNT"),
        })
    })
    .collect::<Vec<_>>();

    let total_pages = if total_elements == 0 {
        0
    } else {
        total_elements.div_ceil(size)
    };
    let number_of_elements = content.len() as u64;
    let first = page == 0;
    let last = total_pages == 0 || page + 1 >= total_pages;

    Ok(json!({
        "content": content,
        "pageable": {
            "pageNumber": page,
            "pageSize": size,
            "sort": {
                "empty": false,
                "sorted": true,
                "unsorted": false,
            },
            "offset": offset,
            "paged": true,
            "unpaged": false,
        },
        "last": last,
        "totalElements": total_elements,
        "totalPages": total_pages,
        "first": first,
        "size": size,
        "number": page,
        "sort": {
            "empty": false,
            "sorted": true,
            "unsorted": false,
        },
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0,
    }))
}

async fn persisted_page_hash_matches_page(
    database_file: &Path,
    page_hash: &str,
    page: u64,
    size: u64,
) -> Result<Value, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;

    let total_elements = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
                                      FROM MEDIA_PAGE \
                                      WHERE FILE_HASH = ?",
    )
    .bind(page_hash)
    .fetch_one(&pool)
    .await?
    .get::<i64, _>("COUNT") as u64;

    let size = size.max(1);
    let offset = page.saturating_mul(size);

    let content = sqlx::query(
        "SELECT mp.BOOK_ID, b.URL, mp.NUMBER, mp.FILE_NAME, mp.FILE_SIZE, mp.MEDIA_TYPE \
         FROM MEDIA_PAGE mp \
         LEFT \
         JOIN BOOK b ON b.ID = mp.BOOK_ID \
         WHERE mp.FILE_HASH = ? \
         ORDER BY mp.BOOK_ID ASC, mp.NUMBER ASC \
         LIMIT ? \
         OFFSET ?",
    )
    .bind(page_hash)
    .bind((size.min(i64::MAX as u64)) as i64)
    .bind((offset.min(i64::MAX as u64)) as i64)
    .fetch_all(&pool)
    .await?
    .into_iter()
    .map(|row| {
        let page_number = row.get::<i64, _>("NUMBER") + 1;
        json!({
            "bookId": row.get::<String, _>("BOOK_ID"),
            "url": row.get::<String, _>("URL"),
            "pageNumber": page_number,
            "fileName": row.get::<String, _>("FILE_NAME"),
            "fileSize": row.get::<Option<i64>, _>("FILE_SIZE").unwrap_or_default(),
            "mediaType": row.get::<String, _>("MEDIA_TYPE"),
        })
    })
    .collect::<Vec<_>>();

    let total_pages = if total_elements == 0 {
        0
    } else {
        total_elements.div_ceil(size)
    };
    let number_of_elements = content.len() as u64;
    let first = page == 0;
    let last = total_pages == 0 || page + 1 >= total_pages;

    Ok(json!({
        "content": content,
        "pageable": {
            "pageNumber": page,
            "pageSize": size,
            "sort": {
                "empty": false,
                "sorted": true,
                "unsorted": false,
            },
            "offset": offset,
            "paged": true,
            "unpaged": false,
        },
        "last": last,
        "totalElements": total_elements,
        "totalPages": total_pages,
        "first": first,
        "size": size,
        "number": page,
        "sort": {
            "empty": false,
            "sorted": true,
            "unsorted": false,
        },
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0,
    }))
}

async fn persisted_page_hash_thumbnail(
    database_file: &Path,
    page_hash: &str,
) -> Result<Option<Vec<u8>>, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let thumbnail = sqlx::query(
        "SELECT THUMBNAIL \
                                 FROM PAGE_HASH_THUMBNAIL \
                                 WHERE HASH = ?",
    )
    .bind(page_hash)
    .fetch_optional(&pool)
    .await?
    .map(|row| row.get::<Vec<u8>, _>("THUMBNAIL"));
    Ok(thumbnail)
}

struct PersistedUnknownPageHashThumbnail {
    bytes: Vec<u8>,
    media_type: String,
}

async fn persisted_unknown_page_hash_thumbnail(
    database_file: &Path,
    page_hash: &str,
) -> Result<Option<PersistedUnknownPageHashThumbnail>, sqlx::Error> {
    if let Some(thumbnail) = persisted_page_hash_thumbnail(database_file, page_hash).await? {
        return Ok(Some(PersistedUnknownPageHashThumbnail {
            bytes: thumbnail,
            media_type: "image/jpeg".to_string(),
        }));
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT b.URL, mp.MEDIA_TYPE \
         FROM MEDIA_PAGE mp \
         INNER \
         JOIN BOOK b ON b.ID = mp.BOOK_ID \
         WHERE mp.FILE_HASH = ? \
         ORDER BY mp.BOOK_ID ASC, mp.NUMBER ASC \
         LIMIT 1",
    )
    .bind(page_hash)
    .fetch_optional(&pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };
    let media_type = row
        .get::<Option<String>, _>("MEDIA_TYPE")
        .unwrap_or_else(|| "image/jpeg".to_string());
    if !media_type.starts_with("image/") {
        return Ok(None);
    }

    let file_path = row.get::<String, _>("URL");
    let bytes = match fs::read(&file_path) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(None),
    };

    Ok(Some(PersistedUnknownPageHashThumbnail {
        bytes,
        media_type,
    }))
}

async fn persisted_upsert_page_hash(
    database_file: &Path,
    page_hash: &str,
    size: Option<i64>,
    action: &str,
) -> Result<(), sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    sqlx::query(
        "INSERT INTO PAGE_HASH (HASH, SIZE, ACTION, DELETE_COUNT, CREATED_DATE, \
           LAST_MODIFIED_DATE) \
         VALUES (?, ?, ?, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) \
         ON CONFLICT(HASH) DO UPDATE \
         SET SIZE = excluded.SIZE, ACTION = excluded.ACTION, \
             LAST_MODIFIED_DATE = CURRENT_TIMESTAMP",
    )
    .bind(page_hash)
    .bind(size)
    .bind(action)
    .execute(&pool)
    .await?;
    Ok(())
}

async fn persisted_delete_all_page_hash_matches(
    database_file: &Path,
    page_hash: &str,
) -> Result<u64, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let mut tx = pool.begin().await?;

    let deleted = sqlx::query(
        "DELETE \
                               FROM MEDIA_PAGE \
                               WHERE FILE_HASH = ?",
    )
    .bind(page_hash)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if deleted > 0 {
        sqlx::query(
            "UPDATE PAGE_HASH \
             SET DELETE_COUNT = DELETE_COUNT + ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
             WHERE HASH = ?",
        )
        .bind(deleted as i64)
        .bind(page_hash)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(deleted)
}

async fn persisted_delete_page_hash_match(
    database_file: &Path,
    page_hash: &str,
    book_id: &str,
    page_number: u64,
) -> Result<u64, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let mut tx = pool.begin().await?;

    let zero_based_page_number = page_number.saturating_sub(1) as i64;
    let deleted = sqlx::query(
        "DELETE \
                     FROM MEDIA_PAGE \
                     WHERE FILE_HASH = ? \
                     AND BOOK_ID = ? \
                     AND NUMBER = ?",
    )
    .bind(page_hash)
    .bind(book_id)
    .bind(zero_based_page_number)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if deleted > 0 {
        sqlx::query(
            "UPDATE PAGE_HASH \
             SET DELETE_COUNT = DELETE_COUNT + ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
             WHERE HASH = ?",
        )
        .bind(deleted as i64)
        .bind(page_hash)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(deleted)
}

fn sqlite_datetime_to_utc(value: &str) -> String {
    if value.ends_with('Z') || value.contains('T') {
        value.to_string()
    } else if let Some((date, time)) = value.split_once(' ') {
        format!("{date}T{time}Z")
    } else {
        value.to_string()
    }
}

fn generate_claimed_user_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    format!("rust-claim-{nanos:x}")
}

fn parse_bool(value: Option<&Option<String>>, default: bool) -> bool {
    value
        .and_then(|value| value.as_deref())
        .map(|value| value.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(default)
}

fn normalize_requested_path(requested_path: &str, runtime_config_dir: Option<&PathBuf>) -> PathBuf {
    let raw = PathBuf::from(requested_path);
    let candidate = if raw.is_absolute() {
        raw
    } else if let Some(config_dir) = runtime_config_dir {
        config_dir.join(raw)
    } else {
        raw
    };

    candidate.canonicalize().unwrap_or(candidate)
}

fn list_directory_entries(path: &Path, directories_only: bool) -> Vec<Value> {
    let mut entries = fs::read_dir(path)
        .ok()
        .into_iter()
        .flat_map(|items| items.filter_map(Result::ok))
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            let is_directory = file_type.is_dir();
            if directories_only != is_directory {
                return None;
            }

            let entry_path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let entry_type = if is_directory { "directory" } else { "file" };

            Some(json!({
                "name": name,
                "path": entry_path.to_string_lossy().to_string(),
                "type": entry_type,
            }))
        })
        .collect::<Vec<_>>();

    entries.sort_by(|left, right| {
        left["name"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["name"].as_str().unwrap_or_default())
    });
    entries
}

fn list_transient_book_entries(path: &Path) -> Vec<Value> {
    let mut entries = Vec::new();
    collect_transient_book_entries(path, &mut entries);
    entries.sort_by(|left, right| {
        left["path"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["path"].as_str().unwrap_or_default())
    });
    entries
}

fn collect_transient_book_entries(path: &Path, entries: &mut Vec<Value>) {
    let Ok(directory_entries) = fs::read_dir(path) else {
        return;
    };

    for entry in directory_entries.filter_map(Result::ok) {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        let entry_path = entry.path();
        if file_type.is_dir() {
            collect_transient_book_entries(&entry_path, entries);
            continue;
        }

        if !is_recognized_transient_book_file(&entry_path) {
            continue;
        }

        let Some(name) = entry_path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .filter(|stem| !stem.is_empty())
        else {
            continue;
        };

        entries.push(json!({
            "name": name,
            "path": entry_path.to_string_lossy().to_string(),
        }));
    }
}

fn is_recognized_transient_book_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some(extension)
            if extension.eq_ignore_ascii_case("cbz")
                || extension.eq_ignore_ascii_case("cbr")
                || extension.eq_ignore_ascii_case("zip")
                || extension.eq_ignore_ascii_case("rar")
                || extension.eq_ignore_ascii_case("pdf")
                || extension.eq_ignore_ascii_case("epub")
                || extension.eq_ignore_ascii_case("jpg")
                || extension.eq_ignore_ascii_case("jpeg")
                || extension.eq_ignore_ascii_case("png")
                || extension.eq_ignore_ascii_case("gif")
                || extension.eq_ignore_ascii_case("webp")
                || extension.eq_ignore_ascii_case("avif")
    )
}

fn list_font_families(fonts_directory: &Path) -> Vec<String> {
    let mut families = fs::read_dir(fonts_directory)
        .ok()
        .into_iter()
        .flat_map(|items| items.filter_map(Result::ok))
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_dir() {
                return None;
            }
            Some(entry.file_name().to_string_lossy().to_string())
        })
        .collect::<Vec<_>>();
    families.sort();
    families
}

fn font_extension(file_name: &str) -> Option<&str> {
    file_name
        .rsplit_once('.')
        .map(|(_, extension)| extension)
        .map(|extension| extension.to_ascii_lowercase())
        .filter(|extension| matches!(extension.as_str(), "woff" | "woff2" | "ttf" | "otf"))
        .map(|extension| {
            if extension == "woff2" {
                "woff2"
            } else if extension == "woff" {
                "woff"
            } else if extension == "ttf" {
                "ttf"
            } else {
                "otf"
            }
        })
}

fn font_media_type(file_name: &str) -> Option<&'static str> {
    match font_extension(file_name) {
        Some("woff") => Some("font/woff"),
        Some("woff2") => Some("font/woff2"),
        Some("ttf") => Some("font/ttf"),
        Some("otf") => Some("font/otf"),
        _ => None,
    }
}

fn font_format(file_name: &str) -> Option<&'static str> {
    match font_extension(file_name) {
        Some("ttf") => Some("truetype"),
        Some("otf") => Some("opentype"),
        Some("woff") => Some("woff"),
        Some("woff2") => Some("woff2"),
        _ => None,
    }
}

fn parse_u64(value: Option<&Option<String>>) -> Option<u64> {
    value
        .and_then(|value| value.as_deref())
        .and_then(|value| value.trim().parse::<u64>().ok())
}

fn parse_u16(value: Option<&Option<String>>) -> Option<u16> {
    value
        .and_then(|value| value.as_deref())
        .and_then(|value| value.trim().parse::<u16>().ok())
}

fn parse_string(value: Option<&Option<String>>) -> Option<String> {
    value
        .and_then(|value| value.as_ref())
        .map(|value| value.to_string())
}

fn parse_non_blank_string(value: Option<&Option<String>>) -> Option<String> {
    value
        .and_then(|value| value.as_ref())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_thumbnail_size(value: Option<&Option<String>>) -> Option<&'static str> {
    match value.and_then(|value| value.as_deref()) {
        Some("DEFAULT") => Some("DEFAULT"),
        Some("MEDIUM") => Some("MEDIUM"),
        Some("LARGE") => Some("LARGE"),
        Some("XLARGE") => Some("XLARGE"),
        _ => None,
    }
}

fn generate_remember_me_key() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    let raw = format!("{nanos:032x}{sequence:016x}");
    raw.chars().take(32).collect()
}

fn settings_json(runtime: &crate::config::RuntimeConfig, settings: &OperationalSettings) -> Value {
    json!({
        "deleteEmptyCollections": settings.delete_empty_collections,
        "deleteEmptyReadLists": settings.delete_empty_read_lists,
        "rememberMeDurationDays": settings.remember_me_duration_days,
        "thumbnailSize": settings.thumbnail_size,
        "taskPoolSize": settings.task_pool_size,
        "serverPort": multi_source_number(
            None,
            settings.server_port.map(u64::from),
            effective_server_port(runtime, settings).map(u64::from),
        ),
        "serverContextPath": multi_source_string(
            runtime.server_context_path.as_deref(),
            settings.server_context_path.as_deref(),
            Some(effective_server_context_path(runtime, settings)),
        ),
        "koboProxy": settings.kobo_proxy,
        "koboPort": settings.kobo_port,
        "kepubifyPath": multi_source_string(
            runtime.kepubify_path.as_ref().and_then(|path| path.to_str()),
            settings.kepubify_path.as_deref(),
            effective_kepubify_path(runtime, settings),
        ),
    })
}

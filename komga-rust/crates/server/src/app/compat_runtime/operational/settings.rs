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
use reqwest::Client as AsyncHttpClient;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::app::placeholder_auth::{require_admin, require_auth, resolved_auth_user, user_id};
use crate::task_queue::TaskQueueRecord;

use super::super::{OperationalSettings, OperationalState, TransientBookRecord, now_epoch_seconds};
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

pub(in crate::app::compat_runtime) async fn get_history(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
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
    if let Some(response) = require_auth(&headers) {
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

pub(in crate::app::compat_runtime) async fn get_page_hash_thumbnail(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    AxumPath(page_hash): AxumPath<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
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

        let record = TransientBookRecord {
            id: id.clone(),
            name: name.to_string(),
            path: path.to_string(),
            file_last_modified_epoch_seconds: to_unix_seconds(file_metadata.modified().ok()),
            size_bytes: file_metadata.len(),
            status,
            media_type,
        };
        store.records.insert(id, record.clone());
        payload.push(transient_book_payload(&record));
    }

    store.persist();
    payload.sort_by(|left, right| {
        left["path"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["path"].as_str().unwrap_or_default())
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

    if let Err(_) =
        persisted_ensure_transient_book(state.runtime.database_file.as_path(), &record).await
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    {
        let mut task_queue = state
            .task_queue
            .lock()
            .expect("task queue state lock should not be poisoned");
        task_queue.enqueue(TaskQueueRecord::new(
            format!("ANALYZE_BOOK:{}", record.id),
            100,
            Some(record.id.clone()),
        ));
        if task_queue.process_available(&state.runtime).is_err() {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    let media =
        match persisted_media_for_book(state.runtime.database_file.as_path(), &record.id).await {
            Ok(Some(media)) => media,
            Ok(None) => ("UNPROCESSED".to_string(), String::new()),
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };

    let mut store = state
        .transient_books
        .lock()
        .expect("transient books state lock should not be poisoned");
    let Some(entry) = store.records.get_mut(&transient_book_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    entry.status = media.0;
    entry.media_type = media.1;
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
    let Some(mut record) = maybe_record else {
        return StatusCode::NOT_FOUND.into_response();
    };

    if let Ok(Some((status, media_type))) =
        persisted_media_for_book(state.runtime.database_file.as_path(), &record.id).await
    {
        record.status = status;
        record.media_type = media_type;

        let mut store = state
            .transient_books
            .lock()
            .expect("transient books state lock should not be poisoned");
        store.records.insert(transient_book_id, record.clone());
        store.persist();
    }

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
    if page_number != 1 {
        return StatusCode::BAD_REQUEST.into_response();
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
    format!("transient-{:x}", digest)[..26].to_string()
}

fn transient_book_payload(record: &TransientBookRecord) -> Value {
    json!({
        "id": record.id,
        "name": record.name,
        "path": record.path,
        "fileLastModified": record.file_last_modified_epoch_seconds,
        "sizeBytes": record.size_bytes,
        "status": record.status,
        "mediaType": record.media_type,
        "pages": [{ "number": 1 }],
        "files": [PathBuf::from(record.path.as_str()).file_name().and_then(|value| value.to_str()).unwrap_or_default()],
        "comment": "",
        "number": Value::Null,
        "seriesId": Value::Null,
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

fn to_unix_seconds(time: Option<SystemTime>) -> i64 {
    time.and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_secs() as i64)
        .unwrap_or_default()
}

async fn persisted_user_count_from_db_path(database_file: &Path) -> Result<i64, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let count = sqlx::query("SELECT COUNT(*) AS COUNT FROM USER")
        .fetch_one(&pool)
        .await?
        .get::<i64, _>("COUNT");
    pool.close().await;
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
        "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES, AGE_RESTRICTION, AGE_RESTRICTION_ALLOW_ONLY) VALUES (?, ?, ?, ?, ?, ?)",
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

    sqlx::query("INSERT INTO USER_ROLE (USER_ID, ROLE) VALUES (?, ?)")
        .bind(&created_user_id)
        .bind("ADMIN")
        .execute(&mut *tx)
        .await
        .map_err(|_| PersistedClaimError::Storage)?;

    tx.commit()
        .await
        .map_err(|_| PersistedClaimError::Storage)?;
    pool.close().await;

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
    sqlx::query("DELETE FROM SYNC_POINT WHERE USER_ID = ? AND API_KEY_ID = ?")
        .bind(user_id)
        .bind(key_id)
        .execute(&pool)
        .await?;
    pool.close().await;
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
            "INSERT OR IGNORE INTO ANNOUNCEMENTS_READ (USER_ID, ANNOUNCEMENT_ID) VALUES (?, ?)",
        )
        .bind(user_id)
        .bind(announcement_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    pool.close().await;
    Ok(())
}

async fn persisted_announcement_read_ids(
    database_file: &Path,
    user_id: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT ANNOUNCEMENT_ID FROM ANNOUNCEMENTS_READ WHERE USER_ID = ? ORDER BY ANNOUNCEMENT_ID ASC",
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await?;
    pool.close().await;

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
            "SELECT KEY, VALUE, ALLOW_UNAUTHORIZED FROM CLIENT_SETTINGS_GLOBAL WHERE ALLOW_UNAUTHORIZED = 1 ORDER BY KEY ASC",
        )
        .fetch_all(&pool)
        .await?
    } else {
        sqlx::query(
            "SELECT KEY, VALUE, ALLOW_UNAUTHORIZED FROM CLIENT_SETTINGS_GLOBAL ORDER BY KEY ASC",
        )
        .fetch_all(&pool)
        .await?
    };
    pool.close().await;

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
        "SELECT KEY, VALUE FROM CLIENT_SETTINGS_USER WHERE USER_ID = ? ORDER BY KEY ASC",
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await?;
    pool.close().await;

    let mut map = serde_json::Map::new();
    for row in rows {
        let key = row.get::<String, _>("KEY");
        let value = row.get::<String, _>("VALUE");
        map.insert(key, json!({ "value": value }));
    }
    Ok(Value::Object(map))
}

async fn persisted_ensure_transient_book(
    database_file: &Path,
    record: &TransientBookRecord,
) -> Result<(), sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let mut tx = pool.begin().await?;

    sqlx::query(
        "INSERT OR IGNORE INTO LIBRARY (ID, NAME, ROOT, SCAN_STARTUP, EMPTY_TRASH_AFTER_SCAN, ONESHOTS_DIRECTORY) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("__transient_runtime_library__")
    .bind("Transient Runtime")
    .bind("/")
    .bind(false)
    .bind(false)
    .bind(None::<String>)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT OR IGNORE INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, oneshot) VALUES (?, ?, ?, ?, ?, 0)",
    )
    .bind("__transient_runtime_series__")
    .bind(record.file_last_modified_epoch_seconds)
    .bind("Transient Runtime")
    .bind("__transient_runtime_series__")
    .bind("__transient_runtime_library__")
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT OR IGNORE INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, LIBRARY_ID, oneshot) VALUES (?, ?, ?, ?, ?, ?, ?, 0)",
    )
    .bind(record.id.as_str())
    .bind(record.file_last_modified_epoch_seconds)
    .bind(record.name.as_str())
    .bind(record.path.as_str())
    .bind("__transient_runtime_series__")
    .bind(record.size_bytes as i64)
    .bind("__transient_runtime_library__")
    .execute(&mut *tx)
    .await?;

    let file_name = PathBuf::from(record.path.as_str())
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();
    sqlx::query(
        "INSERT INTO MEDIA_FILE (FILE_NAME, BOOK_ID, FILE_SIZE) SELECT ?, ?, ? WHERE NOT EXISTS (SELECT 1 FROM MEDIA_FILE WHERE FILE_NAME = ? AND BOOK_ID = ?)",
    )
    .bind(file_name.as_str())
    .bind(record.id.as_str())
    .bind(record.size_bytes as i64)
    .bind(file_name.as_str())
    .bind(record.id.as_str())
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    pool.close().await;
    Ok(())
}

async fn persisted_media_for_book(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<(String, String)>, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query("SELECT STATUS, MEDIA_TYPE FROM MEDIA WHERE BOOK_ID = ?")
        .bind(book_id)
        .fetch_optional(&pool)
        .await?;
    pool.close().await;

    Ok(row.map(|row| {
        (
            row.get::<String, _>("STATUS"),
            row.get::<Option<String>, _>("MEDIA_TYPE")
                .unwrap_or_default(),
        )
    }))
}

async fn persisted_history_page(
    database_file: &Path,
    page: u64,
    size: u64,
) -> Result<Value, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;

    let total_elements = sqlx::query("SELECT COUNT(*) AS COUNT FROM HISTORICAL_EVENT")
        .fetch_one(&pool)
        .await?
        .get::<i64, _>("COUNT") as u64;

    let size = size.max(1);
    let offset = page.saturating_mul(size);

    let events = sqlx::query(
        "SELECT ID, TYPE, BOOK_ID, SERIES_ID, TIMESTAMP FROM HISTORICAL_EVENT ORDER BY TIMESTAMP DESC, ID DESC LIMIT ? OFFSET ?",
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
            "SELECT ID, \"KEY\" AS EVENT_KEY, VALUE FROM HISTORICAL_EVENT_PROPERTIES WHERE ID IN ({placeholders})",
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

    pool.close().await;

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

    let total_elements = sqlx::query("SELECT COUNT(*) AS COUNT FROM PAGE_HASH")
        .fetch_one(&pool)
        .await?
        .get::<i64, _>("COUNT") as u64;

    let size = size.max(1);
    let offset = page.saturating_mul(size);

    let content = sqlx::query(
        "SELECT HASH, SIZE, ACTION, DELETE_COUNT FROM PAGE_HASH ORDER BY LAST_MODIFIED_DATE DESC, HASH DESC LIMIT ? OFFSET ?",
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

    pool.close().await;

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
    let thumbnail = sqlx::query("SELECT THUMBNAIL FROM PAGE_HASH_THUMBNAIL WHERE HASH = ?")
        .bind(page_hash)
        .fetch_optional(&pool)
        .await?
        .map(|row| row.get::<Vec<u8>, _>("THUMBNAIL"));
    pool.close().await;
    Ok(thumbnail)
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
                || extension.eq_ignore_ascii_case("cb7")
                || extension.eq_ignore_ascii_case("cbt")
                || extension.eq_ignore_ascii_case("zip")
                || extension.eq_ignore_ascii_case("rar")
                || extension.eq_ignore_ascii_case("pdf")
                || extension.eq_ignore_ascii_case("epub")
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

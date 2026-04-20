use axum::Json;
use axum::body::Bytes;
use axum::extract::Extension;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

use crate::http::identity_access::auth::require_admin;
use crate::http::state::HttpAppState;

#[derive(Default, Deserialize)]
#[serde(default)]
struct DirectoryRequestDto {
    path: String,
    #[serde(rename = "showFiles")]
    show_files: bool,
}

pub(crate) async fn post_filesystem(
    Extension(app): Extension<HttpAppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let request = if body.is_empty() {
        DirectoryRequestDto::default()
    } else {
        match serde_json::from_slice::<DirectoryRequestDto>(&body) {
            Ok(value) => value,
            Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        }
    };

    if request.path.is_empty() {
        return Json(json!({
            "directories": root_directory_entries(),
            "files": [],
        }))
        .into_response();
    }

    let requested_path = PathBuf::from(&request.path);
    if !requested_path.is_absolute() {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let directory = listing_directory(&requested_path);
    if !directory.is_dir() || std::fs::read_dir(&directory).is_err() {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let directories = app
        .services
        .operational_settings
        .list_directory_entries(directory.clone(), true);
    let files = if request.show_files {
        app.services
            .operational_settings
            .list_directory_entries(directory.clone(), false)
    } else {
        Vec::new()
    };

    Json(directory_listing_payload(
        Some(parent_value(&requested_path)),
        directories,
        files,
    ))
    .into_response()
}

fn listing_directory(requested_path: &Path) -> PathBuf {
    if requested_path.is_dir() {
        requested_path.to_path_buf()
    } else {
        requested_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| requested_path.to_path_buf())
    }
}

fn parent_value(requested_path: &Path) -> String {
    requested_path
        .parent()
        .map(|parent| parent.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn directory_listing_payload(
    parent: Option<String>,
    directories: Vec<Value>,
    files: Vec<Value>,
) -> Value {
    let mut payload = Map::new();
    if let Some(parent) = parent {
        payload.insert("parent".to_string(), Value::String(parent));
    }
    payload.insert("directories".to_string(), Value::Array(directories));
    payload.insert("files".to_string(), Value::Array(files));
    Value::Object(payload)
}

fn root_directory_entries() -> Vec<Value> {
    current_root_directories()
        .into_iter()
        .map(|root| {
            json!({
                "type": "directory",
                "name": root,
                "path": root,
            })
        })
        .collect()
}

#[cfg(windows)]
fn current_root_directories() -> Vec<String> {
    ('A'..='Z')
        .map(|drive| format!("{drive}:\\"))
        .filter(|root| Path::new(root).exists())
        .collect()
}

#[cfg(not(windows))]
fn current_root_directories() -> Vec<String> {
    vec![std::path::MAIN_SEPARATOR.to_string()]
}

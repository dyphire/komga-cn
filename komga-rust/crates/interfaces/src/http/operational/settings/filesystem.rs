use axum::Json;
use axum::body::Bytes;
use axum::extract::Extension;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::http::identity_access::auth::require_auth;
use crate::operational_settings_access::filesystem as filesystem_access;

use super::super::super::OperationalState;
use super::normalize_requested_path;

pub(crate) async fn post_filesystem(
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
    let directories = filesystem_access::list_directory_entries(&resolved_path, true);
    let files = if show_files {
        filesystem_access::list_directory_entries(&resolved_path, false)
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

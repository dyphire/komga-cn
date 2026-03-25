use axum::extract::Extension;
use axum::extract::Path as AxumPath;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use std::path::{Path, PathBuf};

use super::super::OperationalState;

pub(in crate::app::compat_runtime) async fn webui_entrypoint(
    Extension(state): Extension<OperationalState>,
) -> Response {
    serve_webui_asset(&state, "")
}

pub(in crate::app::compat_runtime) async fn webui_asset(
    AxumPath(webui_path): AxumPath<String>,
    Extension(state): Extension<OperationalState>,
) -> Response {
    if is_runtime_owned_prefix(webui_path.as_str()) {
        return StatusCode::NOT_FOUND.into_response();
    }
    serve_webui_asset(&state, webui_path.as_str())
}

fn serve_webui_asset(state: &OperationalState, webui_path: &str) -> Response {
    let Some(root) = state.webui_assets_root.as_ref() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CONTENT_TYPE, "application/json")],
            axum::Json(json!({
                "message": "webui assets layout was not resolved at startup",
            })),
        )
            .into_response();
    };

    let Some(asset_file) = resolve_asset_file(root.as_path(), webui_path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(bytes) = std::fs::read(asset_file.as_path()) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    (
        [(header::CONTENT_TYPE, content_type_for(asset_file.as_path()))],
        bytes,
    )
        .into_response()
}

fn resolve_asset_file(root: &Path, webui_path: &str) -> Option<PathBuf> {
    let normalized = webui_path.trim_matches('/');
    if normalized
        .split('/')
        .any(|segment| segment == ".." || segment.contains('\\'))
    {
        return None;
    }

    if normalized.is_empty() {
        return Some(root.join("index.html"));
    }

    let candidate = root.join(normalized);
    if is_index_fallback_candidate(normalized) {
        if candidate.is_file() {
            return Some(candidate);
        }
        return Some(root.join("index.html"));
    }

    if candidate.is_file() {
        Some(candidate)
    } else {
        resolve_nested_route_asset_file(root, normalized)
    }
}

fn resolve_nested_route_asset_file(root: &Path, webui_path: &str) -> Option<PathBuf> {
    let segments = webui_path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();

    if segments.len() < 2 {
        return None;
    }

    for index in 1..segments.len() {
        if !is_root_asset_candidate(segments[index], segments.len() - index) {
            continue;
        }

        let candidate = root.join(segments[index..].join("/"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

fn is_index_fallback_candidate(path: &str) -> bool {
    Path::new(path).extension().is_none()
}

fn is_root_asset_candidate(first_segment: &str, remaining_segments: usize) -> bool {
    match first_segment {
        "js" | "css" | "fonts" | "img" => remaining_segments > 1,
        _ => remaining_segments == 1 && Path::new(first_segment).extension().is_some(),
    }
}

fn is_runtime_owned_prefix(path: &str) -> bool {
    let normalized = path.trim_matches('/');
    normalized == "api"
        || normalized.starts_with("api/")
        || normalized == "opds"
        || normalized.starts_with("opds/")
        || normalized == "kobo"
        || normalized.starts_with("kobo/")
        || normalized == "koreader"
        || normalized.starts_with("koreader/")
        || normalized == "sse"
        || normalized.starts_with("sse/")
        || normalized == "health"
        || normalized.starts_with("health/")
        || normalized == "metrics"
        || normalized.starts_with("metrics/")
        || normalized == "actuator"
        || normalized.starts_with("actuator/")
        || normalized == "oauth2"
        || normalized.starts_with("oauth2/")
        || normalized == "login"
        || normalized.starts_with("login/oauth2/")
}

fn content_type_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        _ => "application/octet-stream",
    }
}

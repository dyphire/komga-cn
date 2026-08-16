use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

pub(super) fn parse_group_concat_values(raw: &str) -> Vec<String> {
    const SEPARATOR: char = '\u{1e}';

    if raw.trim().is_empty() {
        return vec![];
    }

    raw.split(SEPARATOR)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

pub(super) fn internal_error_response(error: impl std::fmt::Display + std::fmt::Debug) -> Response {
    tracing::error!(?error, "internal discovery detail error");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": format!("{error:#}") })),
    )
        .into_response()
}

pub(super) fn format_size_bytes(size_bytes: u64) -> String {
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

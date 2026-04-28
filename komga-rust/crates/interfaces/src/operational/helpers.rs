use axum::Json;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use super::super::RuntimeState;

pub(super) fn multi_source_number(
    configuration: Option<u64>,
    database: Option<u64>,
    effective: Option<u64>,
) -> Value {
    json!({
        "configurationSource": configuration,
        "databaseSource": database,
        "effectiveValue": effective,
    })
}

pub(super) fn multi_source_string(
    configuration: Option<&str>,
    database: Option<&str>,
    effective: Option<String>,
) -> Value {
    json!({
        "configurationSource": configuration.unwrap_or_default(),
        "databaseSource": database.unwrap_or_default(),
        "effectiveValue": effective.unwrap_or_default(),
    })
}

pub(super) fn effective_server_port(runtime: &RuntimeState) -> Option<u16> {
    Some(runtime.bind_address.port())
}

pub(super) fn effective_server_context_path(runtime: &RuntimeState) -> String {
    runtime.server_context_path.clone().unwrap_or_default()
}

pub(super) fn invalid_settings_payload(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        [(header::CONTENT_TYPE, "application/json")],
        Json(json!({ "message": message })),
    )
        .into_response()
}

pub(super) fn is_valid_context_path(value: &str) -> bool {
    if value.is_empty() || !value.starts_with('/') || value.ends_with('/') {
        return false;
    }

    let Some(last) = value.chars().last() else {
        return false;
    };
    if !last.is_ascii_alphanumeric() {
        return false;
    }

    value
        .chars()
        .all(|ch| ch == '/' || ch == '-' || ch == '_' || ch.is_ascii_alphanumeric())
}

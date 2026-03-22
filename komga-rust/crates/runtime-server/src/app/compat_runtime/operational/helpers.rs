use axum::Json;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use super::super::OperationalSettings;

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
        "configurationSource": configuration,
        "databaseSource": database,
        "effectiveValue": effective,
    })
}

pub(super) fn effective_server_port(
    runtime: &crate::config::RuntimeConfig,
    settings: &OperationalSettings,
) -> Option<u16> {
    settings
        .server_port
        .or_else(|| Some(runtime.bind_address.port()))
}

pub(super) fn effective_server_context_path(
    runtime: &crate::config::RuntimeConfig,
    settings: &OperationalSettings,
) -> String {
    settings
        .server_context_path
        .clone()
        .or_else(|| runtime.server_context_path.clone())
        .unwrap_or_default()
}

pub(super) fn effective_kepubify_path(
    runtime: &crate::config::RuntimeConfig,
    settings: &OperationalSettings,
) -> Option<String> {
    settings.kepubify_path.clone().or_else(|| {
        runtime
            .kepubify_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
    })
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

pub(super) fn env_or_default(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

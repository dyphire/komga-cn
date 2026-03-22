use axum::Json;
use axum::body::Bytes;
use axum::extract::Extension;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::app::placeholder_auth::{require_admin, require_auth};

use super::super::{OperationalSettings, OperationalState};
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

    let settings = state
        .settings
        .lock()
        .expect("settings state lock should not be poisoned")
        .clone();

    Json(settings_json(&state.runtime, &settings)).into_response()
}

pub(in crate::app::compat_runtime) async fn get_claim_status() -> Response {
    Json(json!({ "isClaimed": true })).into_response()
}

pub(in crate::app::compat_runtime) async fn get_client_settings_global(
    Extension(state): Extension<OperationalState>,
) -> Response {
    Json(state.client_settings.global.clone()).into_response()
}

pub(in crate::app::compat_runtime) async fn get_client_settings_user(
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    Json(json!({})).into_response()
}

pub(in crate::app::compat_runtime) async fn get_oauth2_providers() -> Response {
    Json(json!([])).into_response()
}

pub(in crate::app::compat_runtime) async fn delete_tasks(headers: HeaderMap) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    Json(json!(0)).into_response()
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

    let mut settings = state
        .settings
        .lock()
        .expect("settings state lock should not be poisoned");

    if let Some(value) = payload.get("deleteEmptyCollections") {
        let Some(value) = value.as_bool() else {
            return invalid_settings_payload("deleteEmptyCollections must be a boolean");
        };
        settings.delete_empty_collections = value;
    }

    if let Some(value) = payload.get("deleteEmptyReadLists") {
        let Some(value) = value.as_bool() else {
            return invalid_settings_payload("deleteEmptyReadLists must be a boolean");
        };
        settings.delete_empty_read_lists = value;
    }

    if let Some(value) = payload.get("rememberMeDurationDays") {
        let Some(value) = value.as_u64() else {
            return invalid_settings_payload("rememberMeDurationDays must be a positive integer");
        };
        if value == 0 {
            return invalid_settings_payload("rememberMeDurationDays must be greater than 0");
        }
        settings.remember_me_duration_days = value;
    }

    if let Some(value) = payload.get("thumbnailSize") {
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
    }

    if let Some(value) = payload.get("taskPoolSize") {
        let Some(value) = value.as_u64() else {
            return invalid_settings_payload("taskPoolSize must be a positive integer");
        };
        if value == 0 {
            return invalid_settings_payload("taskPoolSize must be greater than 0");
        }
        settings.task_pool_size = value;
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
    }

    if let Some(value) = payload.get("koboProxy") {
        let Some(value) = value.as_bool() else {
            return invalid_settings_payload("koboProxy must be a boolean");
        };
        settings.kobo_proxy = value;
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
    }

    axum::http::StatusCode::NO_CONTENT.into_response()
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

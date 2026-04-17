use axum::Json;
use axum::body::Bytes;
use axum::extract::Extension;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::http::identity_access::auth::{require_admin, sync_remember_me_runtime_settings};
use crate::http::operational::helpers::{
    effective_server_context_path, effective_server_port, invalid_settings_payload,
    is_valid_context_path, multi_source_number, multi_source_string,
};
use crate::http::state::OperationalSettings;
use crate::operational_settings_access::server_settings as server_settings_access;

use super::super::super::OperationalState;

pub(crate) async fn get_server_settings(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let settings = match load_operational_settings(&state).await {
        Ok(settings) => settings,
        Err(response) => return response,
    };

    if let Err(error) = (state.apply_task_pool_size)(settings.task_pool_size as usize) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "message": format!("failed to process queued tasks: {error}") })),
        )
            .into_response();
    }

    Json(settings_json(&state.runtime, &settings)).into_response()
}

pub(crate) async fn update_server_settings(
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

    let mut settings = match load_operational_settings(&state).await {
        Ok(settings) => settings,
        Err(response) => return response,
    };
    let mut persistence_changes: Vec<(String, Option<String>)> = Vec::new();
    let mut task_pool_size_change: Option<u64> = None;

    if let Some(value) = payload.get("deleteEmptyCollections")
        && !value.is_null()
    {
        let Some(value) = value.as_bool() else {
            return invalid_settings_payload("deleteEmptyCollections must be a boolean");
        };
        settings.delete_empty_collections = value;
        persistence_changes.push((
            "DELETE_EMPTY_COLLECTIONS".to_string(),
            Some(value.to_string()),
        ));
    }

    if let Some(value) = payload.get("deleteEmptyReadLists")
        && !value.is_null()
    {
        let Some(value) = value.as_bool() else {
            return invalid_settings_payload("deleteEmptyReadLists must be a boolean");
        };
        settings.delete_empty_read_lists = value;
        persistence_changes.push((
            "DELETE_EMPTY_READLISTS".to_string(),
            Some(value.to_string()),
        ));
    }

    if let Some(value) = payload.get("rememberMeDurationDays")
        && !value.is_null()
    {
        let Some(value) = value.as_u64() else {
            return invalid_settings_payload("rememberMeDurationDays must be a positive integer");
        };
        if value == 0 {
            return invalid_settings_payload("rememberMeDurationDays must be greater than 0");
        }
        settings.remember_me_duration_days = value;
        persistence_changes.push(("REMEMBER_ME_DURATION".to_string(), Some(value.to_string())));
    }

    if let Some(value) = payload.get("renewRememberMeKey")
        && !value.is_null()
    {
        let Some(value) = value.as_bool() else {
            return invalid_settings_payload("renewRememberMeKey must be a boolean");
        };
        if value {
            settings.remember_me_key = server_settings_access::generate_remember_me_key();
            persistence_changes.push((
                "REMEMBER_ME_KEY".to_string(),
                Some(settings.remember_me_key.clone()),
            ));
        }
    }

    if let Some(value) = payload.get("thumbnailSize")
        && !value.is_null()
    {
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

    if let Some(value) = payload.get("taskPoolSize")
        && !value.is_null()
    {
        let Some(value) = value.as_u64() else {
            return invalid_settings_payload("taskPoolSize must be a positive integer");
        };
        if value == 0 {
            return invalid_settings_payload("taskPoolSize must be greater than 0");
        }
        settings.task_pool_size = value;
        task_pool_size_change = Some(value);
        persistence_changes.push(("TASK_POOL_SIZE".to_string(), Some(value.to_string())));
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

    if let Some(value) = payload.get("koboProxy")
        && !value.is_null()
    {
        let Some(value) = value.as_bool() else {
            return invalid_settings_payload("koboProxy must be a boolean");
        };
        settings.kobo_proxy = value;
        persistence_changes.push(("KOBO_PROXY".to_string(), Some(value.to_string())));
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

    if let Err(error) = server_settings_access::apply_server_settings_changes(
        state.settings_store.as_ref(),
        &persistence_changes,
    )
    .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "message": format!("failed to persist settings: {error}") })),
        )
            .into_response();
    }

    sync_remember_me_runtime_settings(
        state.remember_me_runtime_key.as_str(),
        settings.remember_me_key.as_str(),
        settings.remember_me_duration_days,
    );

    if let Some(value) = task_pool_size_change
        && let Err(error) = (state.apply_task_pool_size)(value as usize)
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "message": format!("failed to process queued tasks: {error}") })),
        )
            .into_response();
    }

    axum::http::StatusCode::NO_CONTENT.into_response()
}

fn operational_settings_from_persisted(
    settings: server_settings_access::PersistedServerSettings,
) -> OperationalSettings {
    let mut operational = OperationalSettings::from_runtime();
    operational.delete_empty_collections = settings.delete_empty_collections;
    operational.delete_empty_read_lists = settings.delete_empty_read_lists;
    operational.remember_me_key = settings.remember_me_key;
    operational.remember_me_duration_days = settings.remember_me_duration_days;
    operational.thumbnail_size = settings.thumbnail_size;
    operational.task_pool_size = settings.task_pool_size;
    operational.server_port = settings.server_port;
    operational.server_context_path = settings.server_context_path;
    operational.kobo_proxy = settings.kobo_proxy;
    operational.kobo_port = settings.kobo_port;
    operational
}

async fn load_operational_settings(
    state: &OperationalState,
) -> Result<OperationalSettings, Response> {
    server_settings_access::load_server_settings(state.settings_store.as_ref())
        .await
        .map(|settings| {
            sync_remember_me_runtime_settings(
                state.remember_me_runtime_key.as_str(),
                settings.remember_me_key.as_str(),
                settings.remember_me_duration_days,
            );
            operational_settings_from_persisted(settings)
        })
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "message": format!("failed to load settings: {error}") })),
            )
                .into_response()
        })
}

fn settings_json(runtime: &crate::http::RuntimeState, settings: &OperationalSettings) -> Value {
    json!({
        "deleteEmptyCollections": settings.delete_empty_collections,
        "deleteEmptyReadLists": settings.delete_empty_read_lists,
        "rememberMeDurationDays": settings.remember_me_duration_days,
        "thumbnailSize": settings.thumbnail_size,
        "taskPoolSize": settings.task_pool_size,
        "serverPort": multi_source_number(
            Some(u64::from(runtime.bind_address.port())),
            settings.server_port.map(u64::from),
            effective_server_port(runtime, settings).map(u64::from),
        ),
        "serverContextPath": multi_source_string(
            runtime.server_context_path.as_deref(),
            settings.server_context_path.as_deref(),
            Some(effective_server_context_path(runtime, settings)),
        ),
        "kepubifyPath": multi_source_string(None, None, None),
        "koboProxy": settings.kobo_proxy,
        "koboPort": settings.kobo_port,
    })
}

#[cfg(test)]
mod tests;

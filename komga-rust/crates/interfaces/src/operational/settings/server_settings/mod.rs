use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use komga_application::operational::{
    PersistedServerSettings, ServerSettingPatch, ServerSettingsLoadError,
    ServerSettingsUpdateCommand, ServerSettingsUpdateError, ThumbnailSize,
};
use serde_json::{Value, json};

use crate::identity_access::auth::Admin;
use crate::operational::helpers::{
    effective_server_context_path, effective_server_port, invalid_settings_payload,
    multi_source_number, multi_source_string,
};
use crate::state::{RuntimeState, ServerSettingsState};

pub(crate) async fn get_server_settings(
    State(app): State<ServerSettingsState>,
    Admin(_admin): Admin,
) -> Response {
    let settings = match app.server_settings.load().await {
        Ok(settings) => settings,
        Err(ServerSettingsLoadError::Load(error)) => return settings_load_error_response(error),
    };

    Json(settings_json(&app.runtime, &settings)).into_response()
}

pub(crate) async fn update_server_settings(
    State(app): State<ServerSettingsState>,
    Admin(_admin): Admin,
    body: Bytes,
) -> Response {
    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return invalid_settings_payload("invalid settings payload");
    };

    if !payload.is_object() {
        return invalid_settings_payload("invalid settings payload");
    }

    let command = match settings_update_command(&payload) {
        Ok(command) => command,
        Err(message) => return invalid_settings_payload(&message.to_string()),
    };

    match app.server_settings.update(command).await {
        Ok(()) => axum::http::StatusCode::NO_CONTENT.into_response(),
        Err(ServerSettingsUpdateError::InvalidPayload(message)) => {
            invalid_settings_payload(&message)
        }
        Err(ServerSettingsUpdateError::Load(error)) => settings_load_error_response(error),
        Err(ServerSettingsUpdateError::Persist(error)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "message": format!("failed to persist settings: {error:#}") })),
        )
            .into_response(),
        Err(ServerSettingsUpdateError::ApplyTaskPool(error)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "message": format!("failed to process queued tasks: {error:#}") })),
        )
            .into_response(),
    }
}

fn settings_load_error_response(error: impl std::fmt::Display + std::fmt::Debug) -> Response {
    tracing::error!(?error, "server settings load error");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "message": format!("failed to load settings: {error:#}") })),
    )
        .into_response()
}

fn settings_update_command(payload: &Value) -> anyhow::Result<ServerSettingsUpdateCommand> {
    let mut command = ServerSettingsUpdateCommand::default();

    if let Some(value) = payload.get("deleteEmptyCollections")
        && !value.is_null()
    {
        command.delete_empty_collections = Some(
            value
                .as_bool()
                .ok_or_else(|| anyhow::anyhow!("deleteEmptyCollections must be a boolean"))?,
        );
    }

    if let Some(value) = payload.get("deleteEmptyReadLists")
        && !value.is_null()
    {
        command.delete_empty_read_lists = Some(
            value
                .as_bool()
                .ok_or_else(|| anyhow::anyhow!("deleteEmptyReadLists must be a boolean"))?,
        );
    }

    if let Some(value) = payload.get("rememberMeDurationDays")
        && !value.is_null()
    {
        command.remember_me_duration_days =
            Some(value.as_u64().ok_or_else(|| {
                anyhow::anyhow!("rememberMeDurationDays must be a positive integer")
            })?);
    }

    if let Some(value) = payload.get("renewRememberMeKey")
        && !value.is_null()
    {
        command.renew_remember_me_key = Some(
            value
                .as_bool()
                .ok_or_else(|| anyhow::anyhow!("renewRememberMeKey must be a boolean"))?,
        );
    }

    if let Some(value) = payload.get("thumbnailSize")
        && !value.is_null()
    {
        let value = value
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("thumbnailSize must be a string"))?;
        command.thumbnail_size = Some(
            ThumbnailSize::parse(value)
                .ok_or_else(|| anyhow::anyhow!("thumbnailSize is invalid"))?,
        );
    }

    if let Some(value) = payload.get("taskPoolSize")
        && !value.is_null()
    {
        command.task_pool_size = Some(
            value
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("taskPoolSize must be a positive integer"))?,
        );
    }

    command.server_port = optional_integer_patch(
        payload,
        "serverPort",
        "serverPort must be an integer between 1 and 65535",
    )?;
    command.server_context_path = optional_string_patch(
        payload,
        "serverContextPath",
        "serverContextPath must be a string or null",
    )?;

    if let Some(value) = payload.get("koboProxy")
        && !value.is_null()
    {
        command.kobo_proxy = Some(
            value
                .as_bool()
                .ok_or_else(|| anyhow::anyhow!("koboProxy must be a boolean"))?,
        );
    }

    command.kobo_port = optional_integer_patch(
        payload,
        "koboPort",
        "koboPort must be an integer between 1 and 65535",
    )?;

    Ok(command)
}

fn optional_integer_patch(
    payload: &Value,
    field: &str,
    type_error: &str,
) -> anyhow::Result<ServerSettingPatch<u64>> {
    match payload.get(field) {
        Some(Value::Null) => Ok(ServerSettingPatch::Clear),
        Some(value) => value
            .as_u64()
            .map(ServerSettingPatch::Set)
            .ok_or_else(|| anyhow::anyhow!("{}", type_error)),
        None => Ok(ServerSettingPatch::Unchanged),
    }
}

fn optional_string_patch(
    payload: &Value,
    field: &str,
    type_error: &str,
) -> anyhow::Result<ServerSettingPatch<String>> {
    match payload.get(field) {
        Some(Value::Null) => Ok(ServerSettingPatch::Clear),
        Some(value) => value
            .as_str()
            .map(|value| ServerSettingPatch::Set(value.to_string()))
            .ok_or_else(|| anyhow::anyhow!("{}", type_error)),
        None => Ok(ServerSettingPatch::Unchanged),
    }
}

fn settings_json(runtime: &RuntimeState, settings: &PersistedServerSettings) -> Value {
    json!({
        "deleteEmptyCollections": settings.delete_empty_collections,
        "deleteEmptyReadLists": settings.delete_empty_read_lists,
        "rememberMeDurationDays": settings.remember_me_duration_days,
        "thumbnailSize": settings.thumbnail_size.as_str(),
        "taskPoolSize": settings.task_pool_size,
        "serverPort": multi_source_number(
            Some(u64::from(runtime.configuration_bind_address.port())),
            settings.server_port.map(u64::from),
            effective_server_port(runtime).map(u64::from),
        ),
        "serverContextPath": multi_source_string(
            runtime.configuration_server_context_path.as_deref(),
            settings.server_context_path.as_deref(),
            Some(effective_server_context_path(runtime)),
        ),
        "kepubifyPath": multi_source_string(None, None, None),
        "koboProxy": settings.kobo_proxy,
        "koboPort": settings.kobo_port,
    })
}

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::Value;

use crate::contracts::client_settings::{client_settings_global_dto, client_settings_user_dto};
use crate::identity_access::auth::{Admin, Authenticated, resolved_auth_user};
use crate::state::OperationalApiState;
use komga_application::identity_access::user_id;
use komga_application::operational::{
    ClientGlobalSetting, ClientGlobalSettings, ClientUserSetting, ClientUserSettings,
};

pub(crate) async fn get_client_settings_global(
    State(app): State<OperationalApiState>,
    headers: HeaderMap,
) -> Response {
    let include_unauthorized_only = match resolved_auth_user(&app.identity, &headers) {
        Ok(user) => user.is_none(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let settings = match app
        .client_settings
        .load_client_settings_global(include_unauthorized_only)
        .await
    {
        Ok(settings) => settings,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    Json(client_settings_global_dto(&settings)).into_response()
}

pub(crate) async fn get_client_settings_user(
    State(app): State<OperationalApiState>,
    Authenticated(current_user): Authenticated,
) -> Response {
    let settings = match app
        .client_settings
        .load_client_settings_user(user_id(&current_user))
        .await
    {
        Ok(settings) => settings,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    Json(client_settings_user_dto(&settings)).into_response()
}

pub(crate) async fn patch_client_settings_global(
    State(app): State<OperationalApiState>,
    _: Admin,
    body: Bytes,
) -> Response {
    let settings = match parse_client_settings_global_payload(&body) {
        Ok(settings) => settings,
        Err(status) => return status.into_response(),
    };

    match app
        .client_settings
        .upsert_client_settings_global(&settings)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn patch_client_settings_user(
    State(app): State<OperationalApiState>,
    Authenticated(current_user): Authenticated,
    body: Bytes,
) -> Response {
    let settings = match parse_client_settings_user_payload(&body) {
        Ok(settings) => settings,
        Err(status) => return status.into_response(),
    };

    match app
        .client_settings
        .upsert_client_settings_user(user_id(&current_user), &settings)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn delete_client_settings_global(
    State(app): State<OperationalApiState>,
    _: Admin,
    body: Bytes,
) -> Response {
    let keys = match parse_client_settings_delete_keys(&body) {
        Ok(keys) => keys,
        Err(status) => return status.into_response(),
    };

    match app
        .client_settings
        .delete_client_settings_global(&keys)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn delete_client_settings_user(
    State(app): State<OperationalApiState>,
    Authenticated(current_user): Authenticated,
    body: Bytes,
) -> Response {
    let keys = match parse_client_settings_delete_keys(&body) {
        Ok(keys) => keys,
        Err(status) => return status.into_response(),
    };

    match app
        .client_settings
        .delete_client_settings_user(user_id(&current_user), &keys)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn parse_client_settings_global_payload(body: &[u8]) -> Result<ClientGlobalSettings, StatusCode> {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let Some(object) = value.as_object() else {
        return Err(StatusCode::BAD_REQUEST);
    };

    let mut settings = BTreeMap::new();
    for (key, item) in object {
        if !is_valid_client_settings_key(key) {
            return Err(StatusCode::BAD_REQUEST);
        }
        let Some(item) = item.as_object() else {
            return Err(StatusCode::BAD_REQUEST);
        };
        let Some(value) = item.get("value").and_then(Value::as_str) else {
            return Err(StatusCode::BAD_REQUEST);
        };
        if value.trim().is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        let Some(allow_unauthorized) = item.get("allowUnauthorized").and_then(Value::as_bool)
        else {
            return Err(StatusCode::BAD_REQUEST);
        };
        settings.insert(
            key.to_string(),
            ClientGlobalSetting {
                value: value.to_string(),
                allow_unauthorized,
            },
        );
    }

    Ok(settings)
}

fn parse_client_settings_user_payload(body: &[u8]) -> Result<ClientUserSettings, StatusCode> {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let Some(object) = value.as_object() else {
        return Err(StatusCode::BAD_REQUEST);
    };

    let mut settings = BTreeMap::new();
    for (key, item) in object {
        if !is_valid_client_settings_key(key) {
            return Err(StatusCode::BAD_REQUEST);
        }
        let Some(item) = item.as_object() else {
            return Err(StatusCode::BAD_REQUEST);
        };
        let Some(value) = item.get("value").and_then(Value::as_str) else {
            return Err(StatusCode::BAD_REQUEST);
        };
        if value.trim().is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        settings.insert(
            key.to_string(),
            ClientUserSetting {
                value: value.to_string(),
            },
        );
    }

    Ok(settings)
}

fn parse_client_settings_delete_keys(body: &[u8]) -> Result<Vec<String>, StatusCode> {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let Some(items) = value.as_array() else {
        return Err(StatusCode::BAD_REQUEST);
    };

    let mut keys = Vec::new();
    for item in items {
        let Some(key) = item.as_str().filter(|value| !value.is_empty()) else {
            return Err(StatusCode::BAD_REQUEST);
        };
        if !is_valid_client_settings_key(key) {
            return Err(StatusCode::BAD_REQUEST);
        }
        keys.push(key.to_string());
    }

    Ok(keys)
}

fn is_valid_client_settings_key(key: &str) -> bool {
    let mut segments = key.split('.');
    let Some(first) = segments.next() else {
        return false;
    };
    if !is_valid_client_settings_first_segment(first) {
        return false;
    }
    segments.all(is_valid_client_settings_following_segment)
}

fn is_valid_client_settings_first_segment(segment: &str) -> bool {
    if segment.is_empty() {
        return false;
    }
    let mut chars = segment.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    let Some(last) = segment.chars().last() else {
        return false;
    };
    if !last.is_ascii_lowercase() && !last.is_ascii_digit() {
        return false;
    }

    segment
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
}

fn is_valid_client_settings_following_segment(segment: &str) -> bool {
    if segment.is_empty() {
        return false;
    }
    let Some(first) = segment.chars().next() else {
        return false;
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    let Some(last) = segment.chars().last() else {
        return false;
    };
    if !last.is_ascii_lowercase() && !last.is_ascii_digit() {
        return false;
    }

    segment
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
}

#[cfg(test)]
mod tests {
    use super::{
        ClientGlobalSetting, parse_client_settings_delete_keys,
        parse_client_settings_global_payload, parse_client_settings_user_payload,
    };
    use std::collections::BTreeMap;

    #[test]
    fn global_payload_preserves_non_blank_whitespace() {
        let payload = br#"{"appearance.mode":{"value":"  dark  ","allowUnauthorized":true}}"#;

        let settings = parse_client_settings_global_payload(payload)
            .expect("payload with surrounding whitespace should remain valid");

        assert_eq!(
            settings,
            BTreeMap::from([(
                "appearance.mode".to_string(),
                ClientGlobalSetting {
                    value: "  dark  ".to_string(),
                    allow_unauthorized: true,
                },
            )])
        );
    }

    #[test]
    fn user_payload_rejects_blank_only_values() {
        let payload = br#"{"reader.zoom":{"value":"   "}}"#;

        let response = parse_client_settings_user_payload(payload)
            .expect_err("blank-only values should be rejected");

        assert_eq!(response, axum::http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn payload_accepts_numeric_following_segments_in_keys() {
        let payload = br#"{"reader.1panel":{"value":"spread","allowUnauthorized":false}}"#;

        let settings = parse_client_settings_global_payload(payload)
            .expect("keys matching Kotlin regex should be accepted");

        assert_eq!(
            settings,
            BTreeMap::from([(
                "reader.1panel".to_string(),
                ClientGlobalSetting {
                    value: "spread".to_string(),
                    allow_unauthorized: false,
                },
            )])
        );
    }

    #[test]
    fn delete_keys_reject_whitespace_padded_values() {
        let response = parse_client_settings_delete_keys(br#"[" reader.zoom "]"#)
            .expect_err("whitespace-padded keys should fail raw validation");

        assert_eq!(response, axum::http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn delete_keys_reject_following_segments_starting_with_underscore_or_dash() {
        for invalid_payload in [
            br#"["reader._zoom"]"#.as_slice(),
            br#"["reader.-zoom"]"#.as_slice(),
        ] {
            let response = parse_client_settings_delete_keys(invalid_payload)
                .expect_err("following segments must start with lowercase letter or digit");

            assert_eq!(response, axum::http::StatusCode::BAD_REQUEST);
        }
    }
}

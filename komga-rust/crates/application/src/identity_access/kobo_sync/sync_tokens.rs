use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use time::OffsetDateTime;

use super::models::{KoboLibrarySyncPayload, KoboLibrarySyncResponse};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KomgaSyncTokenPayload {
    #[serde(default = "default_sync_token_version")]
    pub version: i32,
    #[serde(default, rename = "rawKoboSyncToken", alias = "raw_kobo_sync_token")]
    pub raw_kobo_sync_token: String,
    #[serde(
        default,
        rename = "ongoingSyncPointId",
        alias = "ongoing_sync_point_id"
    )]
    pub ongoing_sync_point_id: Option<String>,
    #[serde(
        default,
        rename = "lastSuccessfulSyncPointId",
        alias = "last_successful_sync_point_id"
    )]
    pub last_successful_sync_point_id: Option<String>,
}

pub fn is_kobo_store_sync_token_candidate(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && trimmed.contains('.')
}

pub fn decode_or_passthrough_sync_token(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(normalized) = trimmed.strip_prefix("KOMGA.") {
        return STANDARD
            .decode(normalized)
            .ok()
            .or_else(|| STANDARD_NO_PAD.decode(normalized).ok())
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
    }

    if !trimmed.contains('.') {
        let decoded = STANDARD
            .decode(trimmed)
            .ok()
            .or_else(|| STANDARD_NO_PAD.decode(trimmed).ok())
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        return decoded.and_then(|decoded| extract_calibre_web_raw_sync_token(&decoded));
    }

    Some(trimmed.to_string())
}

pub fn build_kobo_library_sync_payload(
    response: KoboLibrarySyncResponse,
) -> KoboLibrarySyncPayload {
    KoboLibrarySyncPayload {
        events: response.events,
        encoded_sync_token: format!(
            "KOMGA.{}",
            STANDARD_NO_PAD.encode(response.sync_token_payload)
        ),
        should_continue: response.should_continue,
    }
}

pub fn now_sync_marker() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "2000-01-01T00:00:00Z".to_string())
}

pub fn parse_komga_sync_token_payload(value: &str) -> Option<KomgaSyncTokenPayload> {
    serde_json::from_str::<KomgaSyncTokenPayload>(value).ok()
}

pub fn build_komga_sync_token_payload(
    previous: Option<KomgaSyncTokenPayload>,
    incoming_raw_sync_token: Option<String>,
    sync_point_id: &str,
    should_continue: bool,
) -> String {
    let mut payload = previous.unwrap_or_default();
    if payload.version <= 0 {
        payload.version = default_sync_token_version();
    }
    if payload.raw_kobo_sync_token.is_empty()
        && let Some(raw) = incoming_raw_sync_token
    {
        payload.raw_kobo_sync_token = raw;
    }
    if should_continue {
        payload.ongoing_sync_point_id = Some(sync_point_id.to_string());
    } else {
        let finalized_sync_point = payload
            .ongoing_sync_point_id
            .clone()
            .unwrap_or_else(|| sync_point_id.to_string());
        payload.ongoing_sync_point_id = None;
        payload.last_successful_sync_point_id = Some(finalized_sync_point);
    }
    serde_json::to_string(&payload).unwrap_or_else(|_| {
        json!({
            "version": default_sync_token_version(),
            "rawKoboSyncToken": "",
            "ongoingSyncPointId": if should_continue { Value::String(sync_point_id.to_string()) } else { Value::Null },
            "lastSuccessfulSyncPointId": if should_continue { Value::Null } else { Value::String(sync_point_id.to_string()) },
        })
        .to_string()
    })
}

fn default_sync_token_version() -> i32 {
    1
}

fn extract_calibre_web_raw_sync_token(decoded_token: &str) -> Option<String> {
    serde_json::from_str::<Value>(decoded_token)
        .ok()
        .and_then(|value| value.get("data").cloned())
        .and_then(|value| {
            value
                .get("raw_kobo_store_token")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn kobo_protocol_payload_encodes_library_sync_token() {
        let payload = build_kobo_library_sync_payload(KoboLibrarySyncResponse {
            events: vec![json!({ "NewTag": {} })],
            sync_token_payload: r#"{"version":1,"rawKoboSyncToken":""}"#.to_string(),
            should_continue: true,
        });

        assert_eq!(
            payload.encoded_sync_token,
            format!(
                "KOMGA.{}",
                STANDARD_NO_PAD.encode(r#"{"version":1,"rawKoboSyncToken":""}"#)
            )
        );
        assert_eq!(payload.events, vec![json!({ "NewTag": {} })]);
        assert!(payload.should_continue);
    }

    #[test]
    fn decode_or_passthrough_sync_token_extracts_calibre_web_raw_token() {
        let calibre_payload = json!({
            "data": {
                "raw_kobo_store_token": "store.token.segment"
            }
        })
        .to_string();
        let encoded = STANDARD.encode(calibre_payload.as_bytes());

        let decoded = decode_or_passthrough_sync_token(encoded.as_str());
        assert_eq!(decoded, Some("store.token.segment".to_string()));
    }

    #[test]
    fn decode_or_passthrough_sync_token_keeps_komga_payload_json() {
        let payload = json!({
            "version": 1,
            "rawKoboSyncToken": "store.token.segment",
            "ongoingSyncPointId": "sync-1",
            "lastSuccessfulSyncPointId": null,
        })
        .to_string();
        let encoded = format!("KOMGA.{}", STANDARD_NO_PAD.encode(payload.as_bytes()));

        let decoded = decode_or_passthrough_sync_token(encoded.as_str());
        assert_eq!(decoded, Some(payload));
    }
}

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path, State};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_application::identity_access::{
    DeviceSyncPort, KoboReadingStateSnapshot, KoboReadingStateStatus, KoboReadingStateUpdate,
    now_sync_marker, user_id,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{
    ensure_kobo_book_access, proxied_missing_kobo_book_response,
    resolved_kobo_request_api_key_metadata,
};
use crate::access_log::RequestConnectionInfo;
use crate::identity_access::device_auth::auth_resolvers::required_kobo_user;
use crate::identity_access::device_auth::device_progress_service;
use crate::state::IdentityAccessState;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct KoboReadingStateUpdatePayload {
    #[serde(default)]
    reading_states: Vec<KoboReadingStateUpdateEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct KoboReadingStateUpdateEntry {
    last_modified: String,
    current_bookmark: KoboReadingStateBookmark,
    status_info: KoboReadingStateStatusInfo,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct KoboReadingStateBookmark {
    progress_percent: Option<f64>,
    content_source_progress_percent: Option<f64>,
    location: Option<KoboReadingStateLocation>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct KoboReadingStateLocation {
    value: Option<String>,
    #[serde(rename = "Type", default = "default_kobo_location_type")]
    location_type: String,
    source: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct KoboReadingStateStatusInfo {
    status: String,
}

fn default_kobo_location_type() -> String {
    "KoboSpan".to_string()
}

async fn persisted_book_exists(
    device_sync: &dyn DeviceSyncPort,
    book_id: &str,
) -> Result<bool, Response> {
    device_sync
        .persisted_book_exists(book_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

pub(crate) async fn kobo_library_book_state(
    State(app): State<IdentityAccessState>,
    Path((auth_token, book_id)): Path<(String, String)>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let current_user = match required_kobo_user(
        &app.identity,
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
    )
    .await
    {
        Ok(current_user) => current_user,
        Err(status) => return status.into_response(),
    };

    let device_sync = app.identity.device_sync();
    let book_exists = match persisted_book_exists(device_sync, &book_id).await {
        Ok(exists) => exists,
        Err(response) => return response,
    };
    if !book_exists {
        let proxy_path = format!("/v1/library/{book_id}/state");
        match proxied_missing_kobo_book_response(
            app.server_settings.as_ref(),
            app.identity.kobo_proxy(),
            &Method::GET,
            proxy_path.as_str(),
            uri.query(),
            &headers,
            &Bytes::new(),
        )
        .await
        {
            Ok(Some(response)) => return response,
            Ok(None) => {}
            Err(status) => return status.into_response(),
        }

        return StatusCode::NOT_FOUND.into_response();
    }
    if let Err(status) = ensure_kobo_book_access(&app, &current_user, &book_id).await {
        return status.into_response();
    }

    let user_id_value = user_id(&current_user);
    let created_timestamp = match device_sync.load_book_created_timestamp(&book_id).await {
        Ok(Some(timestamp)) => timestamp,
        Ok(None) => now_sync_marker(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let progress_service = device_progress_service(
        app.identity.device_sync(),
        app.device_progress_reader.as_ref(),
        app.epub_navigation_content.as_ref(),
        app.progress.as_ref(),
    );
    let reading_state = match progress_service
        .kobo_reading_state(&book_id, user_id_value, created_timestamp.as_str())
        .await
    {
        Ok(reading_state) => reading_state,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(json!([kobo_reading_state_payload(reading_state)])).into_response()
}

fn kobo_reading_state_payload(reading_state: KoboReadingStateSnapshot) -> Value {
    let mut current_bookmark = Map::new();
    current_bookmark.insert(
        "LastModified".to_string(),
        Value::String(reading_state.last_modified.clone()),
    );
    if let Some(progress_percent) = reading_state.total_progress_percent {
        current_bookmark.insert("ProgressPercent".to_string(), json!(progress_percent));
    }
    if let Some(content_source_progress_percent) = reading_state.content_source_progress_percent {
        current_bookmark.insert(
            "ContentSourceProgressPercent".to_string(),
            json!(content_source_progress_percent),
        );
    }
    if let Some(location) = reading_state.location {
        let mut location_payload = Map::new();
        location_payload.insert("Source".to_string(), Value::String(location.source));
        location_payload.insert("Type".to_string(), Value::String("KoboSpan".to_string()));
        if let Some(kobo_span) = location.kobo_span {
            location_payload.insert("Value".to_string(), Value::String(kobo_span));
        }
        current_bookmark.insert("Location".to_string(), Value::Object(location_payload));
    }

    json!({
        "Created": reading_state.created,
        "CurrentBookmark": Value::Object(current_bookmark),
        "EntitlementId": reading_state.book_id,
        "LastModified": reading_state.last_modified,
        "PriorityTimestamp": reading_state.last_modified,
        "Statistics": {
            "LastModified": reading_state.last_modified,
        },
        "StatusInfo": {
            "LastModified": reading_state.last_modified,
            "Status": reading_state.status.as_str(),
            "TimesStartedReading": reading_state.times_started_reading,
        },
    })
}

pub(crate) async fn kobo_library_book_state_update(
    State(app): State<IdentityAccessState>,
    Path((auth_token, book_id)): Path<(String, String)>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    let current_user = match required_kobo_user(
        &app.identity,
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
    )
    .await
    {
        Ok(current_user) => current_user,
        Err(status) => return status.into_response(),
    };

    let payload = match serde_json::from_slice::<KoboReadingStateUpdatePayload>(&body) {
        Ok(payload) => payload,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "invalid Kobo state payload" })),
            )
                .into_response();
        }
    };

    let device_sync = app.identity.device_sync();
    let book_exists = match persisted_book_exists(device_sync, &book_id).await {
        Ok(exists) => exists,
        Err(response) => return response,
    };
    if !book_exists {
        let proxy_path = format!("/v1/library/{book_id}/state");
        match proxied_missing_kobo_book_response(
            app.server_settings.as_ref(),
            app.identity.kobo_proxy(),
            &Method::PUT,
            proxy_path.as_str(),
            uri.query(),
            &headers,
            &body,
        )
        .await
        {
            Ok(Some(response)) => return response,
            Ok(None) => {}
            Err(status) => return status.into_response(),
        }

        return StatusCode::NOT_FOUND.into_response();
    }
    if let Err(status) = ensure_kobo_book_access(&app, &current_user, &book_id).await {
        return status.into_response();
    }

    let Some(state) = payload.reading_states.first() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "ReadingStates must contain one element" })),
        )
            .into_response();
    };
    let Some(location) = state.current_bookmark.location.as_ref() else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if state
        .current_bookmark
        .content_source_progress_percent
        .is_none()
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let user_id_value = user_id(&current_user);
    let api_key_metadata = resolved_kobo_request_api_key_metadata(
        &app.identity,
        &current_user,
        auth_token.as_str(),
        &headers,
    )
    .await;
    let api_key_metadata = match api_key_metadata {
        Ok(metadata) => metadata,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let device_id = api_key_metadata
        .as_ref()
        .map(|metadata| metadata.id.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let device_name = api_key_metadata
        .as_ref()
        .map(|metadata| metadata.comment.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let progress_service = device_progress_service(
        app.identity.device_sync(),
        app.device_progress_reader.as_ref(),
        app.epub_navigation_content.as_ref(),
        app.progress.as_ref(),
    );
    let persist_result = progress_service
        .update_kobo_reading_state(
            &book_id,
            user_id_value,
            KoboReadingStateUpdate {
                last_modified: state.last_modified.clone(),
                status: kobo_reading_state_status(&state.status_info.status),
                progress_percent: state.current_bookmark.progress_percent,
                content_source_progress_percent: state
                    .current_bookmark
                    .content_source_progress_percent,
                location_source: location.source.clone(),
                kobo_span: kobo_span_location_value(location),
                device_id,
                device_name,
            },
        )
        .await;

    let update_result = if persist_result.is_ok() {
        "Success"
    } else {
        "Failure"
    };

    Json(json!({
        "RequestResult": update_result,
        "UpdateResults": [
            {
                "EntitlementId": book_id,
                "CurrentBookmarkResult": {"Result": update_result},
                "StatisticsResult": {"Result": if persist_result.is_ok() { "Ignored" } else { "Failure" }},
                "StatusInfoResult": {"Result": update_result},
            }
        ],
    }))
    .into_response()
}

fn kobo_reading_state_status(value: &str) -> KoboReadingStateStatus {
    if value.eq_ignore_ascii_case("Finished") {
        KoboReadingStateStatus::Finished
    } else {
        KoboReadingStateStatus::Reading
    }
}

fn kobo_span_location_value(location: &KoboReadingStateLocation) -> Option<String> {
    if location.location_type.eq_ignore_ascii_case("KoboSpan") {
        location.value.clone()
    } else {
        None
    }
}

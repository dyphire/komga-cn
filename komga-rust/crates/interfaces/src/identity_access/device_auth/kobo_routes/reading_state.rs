use super::*;
use komga_application::identity_access::KoboReadingStateUpdate;
use serde::Deserialize;

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

pub async fn kobo_library_book_state(
    State(app): State<IdentityAccessState>,
    Path((auth_token, book_id)): Path<(String, String)>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    uri: axum::http::Uri,
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

    if !persisted_book_exists(&app, &book_id).await.unwrap_or(false) {
        let proxy_path = format!("/v1/library/{book_id}/state");
        if let Some(response) = proxied_missing_kobo_book_response(
            &app,
            &axum::http::Method::GET,
            proxy_path.as_str(),
            uri.query(),
            &headers,
            &Bytes::new(),
        )
        .await
        {
            return response;
        }

        return StatusCode::NOT_FOUND.into_response();
    }

    let user_id_value = user_id(&current_user);
    let created_timestamp = load_book_created_timestamp(&app, &book_id)
        .await
        .unwrap_or(None)
        .unwrap_or_else(now_sync_marker);

    let payload = match device_progress_service(&app)
        .kobo_reading_state(&book_id, user_id_value, created_timestamp.as_str())
        .await
    {
        Ok(payload) => payload,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(json!([payload])).into_response()
}

pub async fn kobo_library_book_state_update(
    State(app): State<IdentityAccessState>,
    Path((auth_token, book_id)): Path<(String, String)>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    uri: axum::http::Uri,
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

    if !persisted_book_exists(&app, &book_id).await.unwrap_or(false) {
        let proxy_path = format!("/v1/library/{book_id}/state");
        if let Some(response) = proxied_missing_kobo_book_response(
            &app,
            &axum::http::Method::PUT,
            proxy_path.as_str(),
            uri.query(),
            &headers,
            &body,
        )
        .await
        {
            return response;
        }

        return StatusCode::NOT_FOUND.into_response();
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
    let (device_id, device_name) = resolved_kobo_request_api_key_metadata(
        &app.identity,
        &current_user,
        auth_token.as_str(),
        &headers,
    )
    .await
    .unwrap_or_else(|| ("unknown".to_string(), "unknown".to_string()));
    let persist_result = device_progress_service(&app)
        .update_kobo_reading_state(
            &book_id,
            user_id_value,
            KoboReadingStateUpdate {
                last_modified: state.last_modified.clone(),
                status: state.status_info.status.clone(),
                progress_percent: state.current_bookmark.progress_percent,
                content_source_progress_percent: state
                    .current_bookmark
                    .content_source_progress_percent,
                location_source: location.source.clone(),
                location_type: location.location_type.clone(),
                location_value: location.value.clone(),
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

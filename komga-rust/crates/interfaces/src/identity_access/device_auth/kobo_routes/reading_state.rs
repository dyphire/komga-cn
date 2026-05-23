use super::*;

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

    let progress = match load_read_progress(&app, &book_id, user_id_value).await {
        Ok(progress) => progress,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let payload = match progress {
        Some(record) => kobo_reading_state_payload(
            &book_id,
            &record,
            parse_locator_payload(record.locator.as_deref()),
        ),
        None => kobo_empty_reading_state_payload(&book_id, created_timestamp.as_str()),
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
    let Some(content_source_progress_percent) =
        state.current_bookmark.content_source_progress_percent
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let content_source_progress = content_source_progress_percent / 100.0;
    let total_progress = state
        .current_bookmark
        .progress_percent
        .map(|value| value / 100.0);
    let completed = state.status_info.status.eq_ignore_ascii_case("Finished");
    let progress_last_modified = state.last_modified.clone();
    let kobo_span = if location.location_type.eq_ignore_ascii_case("kobospan") {
        location.value.clone()
    } else {
        None
    };
    let user_id_value = user_id(&current_user);

    let locator = if completed {
        match load_book_last_epub_position_locator(&app, &book_id).await {
            Ok(Some(locator)) => locator,
            _ => {
                return kobo_state_update_failure(book_id.as_str());
            }
        }
    } else {
        let request_locator = json!({
            "href": location.source,
            "type": "application/xhtml+xml",
            "koboSpan": kobo_span,
            "locations": {
                "progression": content_source_progress,
                "totalProgression": total_progress,
            },
        });

        match normalize_book_epub_locator(app.reader.as_ref(), &book_id, &request_locator).await {
            Ok(locator) => locator,
            Err(_) => return kobo_state_update_failure(book_id.as_str()),
        }
    };

    let stale_progression = match progression_is_older_than_existing(
        app.reader.as_ref(),
        &book_id,
        user_id_value,
        progress_last_modified.as_str(),
    )
    .await
    {
        Ok(stale) => stale,
        Err(_) => return kobo_state_update_failure(book_id.as_str()),
    };
    if stale_progression {
        return kobo_state_update_failure(book_id.as_str());
    }

    let (device_id, device_name) = resolved_kobo_request_api_key_metadata(
        &app.identity,
        &current_user,
        auth_token.as_str(),
        &headers,
    )
    .await
    .unwrap_or_else(|| ("unknown".to_string(), "unknown".to_string()));

    let locator_progression = locator
        .get("locations")
        .and_then(|value| value.get("progression"))
        .and_then(Value::as_f64)
        .unwrap_or(if completed {
            1.0
        } else {
            content_source_progress
        });

    let persist_result = app
        .progress
        .persist_book_progression(
            &book_id,
            user_id_value,
            locator_progression,
            false,
            Some(progress_last_modified.clone()),
            Some(device_id.to_string()),
            Some(device_name.to_string()),
            Some(locator),
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

fn kobo_state_update_failure(book_id: &str) -> Response {
    Json(json!({
        "RequestResult": "Failure",
        "UpdateResults": [
            {
                "EntitlementId": book_id,
                "CurrentBookmarkResult": {"Result": "Failure"},
                "StatisticsResult": {"Result": "Failure"},
                "StatusInfoResult": {"Result": "Failure"},
            }
        ],
    }))
    .into_response()
}

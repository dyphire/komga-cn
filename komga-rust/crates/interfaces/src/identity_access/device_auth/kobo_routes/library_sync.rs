use super::*;

pub async fn kobo_library_sync(
    State(app): State<IdentityAccessState>,
    Path(auth_token): Path<String>,
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
    let current_api_key_id = resolved_kobo_request_api_key_metadata(
        &app.identity,
        &current_user,
        auth_token.as_str(),
        &headers,
    )
    .await
    .map(|(id, _)| id);
    let sync_token_raw = kobo_sync_token_from_request(&headers, &uri);
    let base_url = kobo_request_base_url(&app, &headers).await;
    let forwarded_headers = headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_string(), value.to_string()))
        })
        .collect::<Vec<_>>();
    let store_sync_enabled = load_kobo_proxy_enabled(app.server_settings.as_ref()).await;
    let sync_response = match app
        .identity
        .device_sync()
        .load_kobo_library_sync(KoboLibrarySyncRequest {
            user: current_user,
            current_api_key_id,
            sync_token_raw,
            store_sync_enabled,
            forwarded_headers,
            query: uri.query().map(str::to_string),
            base_url,
            auth_token,
            limit: KOBO_SYNC_ITEM_LIMIT,
        })
        .await
    {
        Ok(response) => response,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let sync_payload = build_kobo_library_sync_payload(sync_response);

    let mut response = (
        StatusCode::OK,
        [(
            HeaderName::from_static("x-kobo-synctoken"),
            HeaderValue::from_str(sync_payload.encoded_sync_token.as_str())
                .unwrap_or_else(|_| HeaderValue::from_static("")),
        )],
        Json(Value::Array(sync_payload.events)),
    )
        .into_response();
    if sync_payload.should_continue {
        response.headers_mut().insert(
            HeaderName::from_static("x-kobo-sync"),
            HeaderValue::from_static("continue"),
        );
    }
    response
}

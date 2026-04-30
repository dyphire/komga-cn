use super::kobo_routes::proxy_kobo_catch_all_request;
use super::*;
use axum::body::to_bytes;
use axum::extract::State;
use std::sync::Arc;

pub async fn kobo_ping(
    State(app): State<Arc<HttpAppState>>,
    Path(auth_token): Path<String>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
) -> Response {
    match kobo_path_user_status(
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
        app.auth_db.db.database_file(),
    )
    .await
    {
        Ok(_) => {}
        Err(status) => return status.into_response(),
    }

    "pong".into_response()
}
pub async fn kobo_initialization(
    State(app): State<Arc<HttpAppState>>,
    Path(auth_token): Path<String>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
) -> Response {
    let state = &app.operational;
    if let Err(status) = required_kobo_user(
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
        state.runtime.database_file.as_path(),
    )
    .await
    {
        return status.into_response();
    }

    let mut resources = match initialization_resources(&app, &headers).await {
        Ok(resources) => resources,
        Err(status) => return status.into_response(),
    };
    apply_initialization_overrides(
        &mut resources,
        auth_token.as_str(),
        kobo_request_base_url(&app, &headers).await.as_str(),
    );

    let mut response = (StatusCode::OK, Json(json!({ "Resources": resources }))).into_response();
    response.headers_mut().insert(
        HeaderName::from_static("x-kobo-apitoken"),
        HeaderValue::from_static("e30="),
    );
    response
}

async fn initialization_resources(
    app: &HttpAppState,
    headers: &HeaderMap,
) -> Result<Value, StatusCode> {
    if load_kobo_proxy_enabled(app.services.server_settings.as_ref()).await {
        match proxied_initialization_resources(headers).await {
            Ok(Some(resources)) => return Ok(resources),
            Err(status) if status == StatusCode::UNAUTHORIZED => return Err(status),
            Ok(None) | Err(_) => {}
        }
    }

    serde_json::from_str(include_str!("kobo_initialization_resources.json"))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn proxied_initialization_resources(
    headers: &HeaderMap,
) -> Result<Option<Value>, StatusCode> {
    let response = match proxy_kobo_catch_all_request(
        &axum::http::Method::GET,
        "/v1/initialization",
        None,
        headers,
        &Bytes::new(),
    )
    .await
    {
        Ok(response) => response,
        Err(status) => {
            return if status == StatusCode::UNAUTHORIZED {
                Err(status)
            } else {
                Ok(None)
            };
        }
    };

    let status = response.status();
    if status == StatusCode::UNAUTHORIZED {
        return Err(status);
    }
    if !status.is_success() {
        return Ok(None);
    }

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if body.is_empty() {
        return Ok(None);
    }

    let payload =
        serde_json::from_slice::<Value>(&body).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(payload.get("Resources").cloned())
}

fn apply_initialization_overrides(resources: &mut Value, auth_token: &str, context_base_url: &str) {
    let Some(object) = resources.as_object_mut() else {
        return;
    };

    object.insert(
        "image_host".to_string(),
        Value::String(context_base_url.to_string()),
    );
    object.insert(
        "image_url_template".to_string(),
        Value::String(format!(
            "{context_base_url}/kobo/{auth_token}/v1/books/{{ImageId}}/thumbnail/{{Width}}/{{Height}}/false/image.jpg"
        )),
    );
    object.insert(
        "image_url_quality_template".to_string(),
        Value::String(format!(
            "{context_base_url}/kobo/{auth_token}/v1/books/{{ImageId}}/thumbnail/{{Width}}/{{Height}}/{{Quality}}/{{IsGreyscale}}/image.jpg"
        )),
    );
}

pub async fn kobo_auth_device(
    State(app): State<Arc<HttpAppState>>,
    Path(auth_token): Path<String>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    uri: axum::http::Uri,
    body: Bytes,
) -> Response {
    let state = &app.operational;
    if let Err(status) = required_kobo_user(
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
        state.runtime.database_file.as_path(),
    )
    .await
    {
        return status.into_response();
    }

    let user_key = match validated_kobo_auth_device_user_key(&headers, &body) {
        Ok(user_key) => user_key,
        Err(status) => return status.into_response(),
    };

    if load_kobo_proxy_enabled(app.services.server_settings.as_ref()).await
        && let Ok(response) = proxy_kobo_catch_all_request(
            &axum::http::Method::POST,
            "/v1/auth/device",
            uri.query(),
            &headers,
            &body,
        )
        .await
        && response.status().is_success()
    {
        return response;
    }

    let (access_token, refresh_token, tracking_id) = generated_kobo_token_triplet();

    Json(KoboDeviceAuthResponse {
        access_token,
        refresh_token,
        token_type: "Bearer",
        tracking_id,
        user_key,
    })
    .into_response()
}

fn validated_kobo_auth_device_user_key(
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<String, StatusCode> {
    if body.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_ascii_lowercase());
    let is_json = content_type
        .as_deref()
        .is_some_and(|value| value.starts_with("application/json") || value.contains("+json"));
    if !is_json {
        return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    let payload = serde_json::from_slice::<Value>(body).map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(match payload.get("UserKey") {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Bool(value)) => value.to_string(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::Null | Value::Array(_) | Value::Object(_)) | None => String::new(),
    })
}

async fn kobo_path_user_status(
    auth_token: &str,
    headers: &HeaderMap,
    remote_addr: Option<SocketAddr>,
    database_file: &FsPath,
) -> Result<AuthUser, StatusCode> {
    if !valid_kobo_path_token(auth_token) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    match persisted_api_key_user_by_token(auth_token, database_file).await {
        Some(AuthOutcome::Valid(user)) => {
            let _ = record_successful_api_key_authentication_by_token(
                headers,
                remote_addr,
                database_file,
                &user,
                auth_token,
            )
            .await;
            if user.roles.iter().any(|role| role == "KOBO_SYNC") {
                Ok(*user)
            } else {
                Err(StatusCode::FORBIDDEN)
            }
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

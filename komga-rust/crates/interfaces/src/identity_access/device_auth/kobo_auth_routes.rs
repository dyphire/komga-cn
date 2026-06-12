use super::kobo_routes::execute_kobo_proxy_request;
use axum::Json;
use axum::body::{Bytes, to_bytes};
use axum::extract::{Extension, Path, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use komga_application::identity_access::{
    AuthOutcome, AuthUser, AuthUserRole, generate_kobo_device_tokens, user_has_role,
};
use serde_json::{Value, json};
use std::net::SocketAddr;

use crate::access_log::RequestConnectionInfo;
use crate::identity_access::auth::persisted_api_key_user_by_token;
use crate::identity_access::device_auth::auth_resolvers::{
    required_kobo_user, valid_kobo_path_token,
};
use crate::identity_access::device_auth::helpers::record_successful_api_key_authentication_by_token;
use crate::identity_access::device_auth::{kobo_request_base_url, load_kobo_proxy_enabled};
use crate::state::{IdentityAccessState, IdentityState};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct KoboDeviceAuthResponse {
    access_token: String,
    refresh_token: String,
    token_type: &'static str,
    tracking_id: String,
    user_key: String,
}

pub(crate) async fn kobo_ping(
    State(app): State<IdentityAccessState>,
    Path(auth_token): Path<String>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
) -> Response {
    match kobo_path_user_status(
        &app.identity,
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
    )
    .await
    {
        Ok(_) => {}
        Err(status) => return status.into_response(),
    }

    "pong".into_response()
}
pub(crate) async fn kobo_initialization(
    State(app): State<IdentityAccessState>,
    Path(auth_token): Path<String>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
) -> Response {
    if let Err(status) = required_kobo_user(
        &app.identity,
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
    )
    .await
    {
        return status.into_response();
    }

    let mut resources = match initialization_resources(&app, &headers).await {
        Ok(resources) => resources,
        Err(status) => return status.into_response(),
    };
    let base_url = match kobo_request_base_url(
        app.server_settings.as_ref(),
        &app.operational.runtime,
        &headers,
    )
    .await
    {
        Ok(base_url) => base_url,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    apply_initialization_overrides(&mut resources, auth_token.as_str(), base_url.as_str());

    let mut response = (StatusCode::OK, Json(json!({ "Resources": resources }))).into_response();
    response.headers_mut().insert(
        HeaderName::from_static("x-kobo-apitoken"),
        HeaderValue::from_static("e30="),
    );
    response
}

async fn initialization_resources(
    app: &IdentityAccessState,
    headers: &HeaderMap,
) -> Result<Value, StatusCode> {
    if load_kobo_proxy_enabled(app.server_settings.as_ref())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        match proxied_initialization_resources(app, headers).await {
            Ok(Some(resources)) => return Ok(resources),
            Ok(None) => {}
            Err(status) => return Err(status),
        }
    }

    serde_json::from_str(include_str!("kobo_initialization_resources.json"))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn proxied_initialization_resources(
    app: &IdentityAccessState,
    headers: &HeaderMap,
) -> Result<Option<Value>, StatusCode> {
    let response = match execute_kobo_proxy_request(
        app.identity.kobo_proxy(),
        &Method::GET,
        "/v1/initialization",
        None,
        headers,
        &Bytes::new(),
    )
    .await
    {
        Ok(response) => response,
        Err(status) => return Err(status),
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

pub(crate) async fn kobo_auth_device(
    State(app): State<IdentityAccessState>,
    Path(auth_token): Path<String>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    if let Err(status) = required_kobo_user(
        &app.identity,
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
    )
    .await
    {
        return status.into_response();
    }

    let user_key = match validated_kobo_auth_device_user_key(&headers, &body) {
        Ok(user_key) => user_key,
        Err(status) => return status.into_response(),
    };

    let proxy_enabled = match load_kobo_proxy_enabled(app.server_settings.as_ref()).await {
        Ok(enabled) => enabled,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    if proxy_enabled {
        match execute_kobo_proxy_request(
            app.identity.kobo_proxy(),
            &Method::POST,
            "/v1/auth/device",
            uri.query(),
            &headers,
            &body,
        )
        .await
        {
            Ok(response) if response.status().is_success() => return response,
            Ok(_) => {}
            Err(status) => return status.into_response(),
        }
    }

    let tokens = generate_kobo_device_tokens();

    Json(KoboDeviceAuthResponse {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        token_type: "Bearer",
        tracking_id: tokens.tracking_id,
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
    identity: &IdentityState,
    auth_token: &str,
    headers: &HeaderMap,
    remote_addr: Option<SocketAddr>,
) -> Result<AuthUser, StatusCode> {
    if !valid_kobo_path_token(auth_token) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    match persisted_api_key_user_by_token(identity, auth_token).await {
        Ok(AuthOutcome::Valid(user)) => {
            let _ = record_successful_api_key_authentication_by_token(
                identity,
                headers,
                remote_addr,
                &user,
                auth_token,
            )
            .await;
            if user_has_role(&user, AuthUserRole::KoboSync) {
                Ok(*user)
            } else {
                Err(StatusCode::FORBIDDEN)
            }
        }
        Ok(AuthOutcome::Invalid | AuthOutcome::Missing) => Err(StatusCode::UNAUTHORIZED),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path, State};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use super::execute_kobo_proxy_request;
use crate::access_log::RequestConnectionInfo;
use crate::identity_access::device_auth::auth_resolvers::required_kobo_user;
use crate::identity_access::device_auth::load_kobo_proxy_enabled;
use crate::state::IdentityAccessState;

#[derive(Serialize)]
struct EmptyKoboResponse {}

pub(crate) async fn kobo_catch_all(
    State(app): State<IdentityAccessState>,
    Path((auth_token, path)): Path<(String, String)>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    method: Method,
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

    let proxy_enabled = match load_kobo_proxy_enabled(app.server_settings.as_ref()).await {
        Ok(enabled) => enabled,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !proxy_enabled {
        return Json(EmptyKoboResponse {}).into_response();
    }

    match execute_kobo_proxy_request(
        app.identity.kobo_proxy(),
        &method,
        &path,
        uri.query(),
        &headers,
        &body,
    )
    .await
    {
        Ok(response) => response,
        Err(status) => status.into_response(),
    }
}

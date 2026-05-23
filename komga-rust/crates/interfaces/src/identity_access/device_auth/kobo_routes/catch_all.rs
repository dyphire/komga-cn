use super::*;

pub(in crate::identity_access::device_auth) async fn proxy_kobo_catch_all_request(
    method: &axum::http::Method,
    path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<Response, StatusCode> {
    proxy::execute_kobo_proxy_request(method, path, query, headers, body).await
}

pub async fn kobo_catch_all(
    State(app): State<IdentityAccessState>,
    Path((auth_token, path)): Path<(String, String)>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    method: axum::http::Method,
    uri: axum::http::Uri,
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

    if !load_kobo_proxy_enabled(app.server_settings.as_ref()).await {
        return Json(json!({})).into_response();
    }

    match proxy_kobo_catch_all_request(&method, &path, uri.query(), &headers, &body).await {
        Ok(response) => response,
        Err(status) => status.into_response(),
    }
}

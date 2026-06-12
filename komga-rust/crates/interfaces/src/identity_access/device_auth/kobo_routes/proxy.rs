use axum::Json;
use axum::body::Bytes;
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use komga_application::identity_access::{
    KoboProxyHeader, KoboProxyPort, KoboProxyRequestBodyError, KoboProxyResponse,
    build_kobo_proxy_request,
};
use komga_application::operational::ServerSettingsPort;

use crate::identity_access::device_auth::load_kobo_proxy_enabled;

pub(super) async fn proxied_missing_kobo_book_response(
    server_settings: &dyn ServerSettingsPort,
    kobo_proxy: &dyn KoboProxyPort,
    method: &Method,
    proxy_path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<Option<Response>, StatusCode> {
    if !load_kobo_proxy_enabled(server_settings)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        return Ok(None);
    }

    Ok(Some(
        match execute_kobo_proxy_request(kobo_proxy, method, proxy_path, query, headers, body).await
        {
            Ok(response) => response,
            Err(status) => status.into_response(),
        },
    ))
}

pub(in crate::identity_access::device_auth) async fn execute_kobo_proxy_request(
    kobo_proxy: &dyn KoboProxyPort,
    method: &Method,
    path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<Response, StatusCode> {
    let headers = kobo_proxy_headers(headers);
    let request = build_kobo_proxy_request(method.as_str(), path, query, &headers, body.as_ref())
        .map_err(kobo_proxy_request_error_status)?;
    let response = kobo_proxy
        .proxy_kobo_request(request)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(kobo_proxy_response(response))
}

fn kobo_proxy_headers(headers: &HeaderMap) -> Vec<KoboProxyHeader> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| KoboProxyHeader::new(name.as_str(), value))
        })
        .collect()
}

fn kobo_proxy_request_error_status(error: KoboProxyRequestBodyError) -> StatusCode {
    match error {
        KoboProxyRequestBodyError::UnsupportedMediaType => StatusCode::UNSUPPORTED_MEDIA_TYPE,
        KoboProxyRequestBodyError::InvalidBody => StatusCode::BAD_REQUEST,
    }
}

fn kobo_proxy_response(response: KoboProxyResponse) -> Response {
    let status = StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut proxied = match response.body {
        Some(body) if !status.is_client_error() && !status.is_server_error() => {
            let mut response = Json(body).into_response();
            *response.status_mut() = status;
            response
        }
        _ => status.into_response(),
    };

    if !status.is_client_error() && !status.is_server_error() {
        for header in response.headers {
            let Ok(header_name) = HeaderName::from_bytes(header.name.as_bytes()) else {
                continue;
            };
            let Ok(header_value) = HeaderValue::from_str(&header.value) else {
                continue;
            };
            if header_name
                .as_str()
                .to_ascii_lowercase()
                .starts_with("x-kobo-")
            {
                proxied.headers_mut().append(header_name, header_value);
            }
        }
    }

    proxied
}

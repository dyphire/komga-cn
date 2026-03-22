use axum::extract::Request;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::super::{DEV_CORS_ALLOW_HEADERS, DEV_CORS_ALLOW_METHODS, DEV_FRONTEND_ORIGIN};

pub(in crate::app::compat_runtime) async fn dev_cors_middleware(
    req: Request,
    next: Next,
) -> Response {
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    if is_dev_frontend_origin(origin.as_deref())
        && req.method() == axum::http::Method::OPTIONS
        && req.headers().contains_key("Access-Control-Request-Method")
    {
        let mut response = StatusCode::NO_CONTENT.into_response();
        apply_dev_cors_headers(response.headers_mut(), origin.as_deref());
        return response;
    }

    let mut response = next.run(req).await;
    apply_dev_cors_headers(response.headers_mut(), origin.as_deref());
    response
}

fn is_dev_frontend_origin(origin: Option<&str>) -> bool {
    origin == Some(DEV_FRONTEND_ORIGIN)
}

fn apply_dev_cors_headers(headers: &mut HeaderMap, origin: Option<&str>) {
    if !is_dev_frontend_origin(origin) {
        return;
    }

    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static(DEV_FRONTEND_ORIGIN),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
        HeaderValue::from_static("true"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static(DEV_CORS_ALLOW_METHODS),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static(DEV_CORS_ALLOW_HEADERS),
    );
    headers.append(header::VARY, HeaderValue::from_static("Origin"));
    headers.append(
        header::VARY,
        HeaderValue::from_static("Access-Control-Request-Method"),
    );
    headers.append(
        header::VARY,
        HeaderValue::from_static("Access-Control-Request-Headers"),
    );
}

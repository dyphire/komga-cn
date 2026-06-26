use axum::http::{HeaderName, HeaderValue, Method, header};
use tower_http::cors::{CorsLayer, Vary};

pub(crate) const DEV_FRONTEND_ORIGIN: &str = "http://127.0.0.1:8081";

pub(crate) fn dev_cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin([HeaderValue::from_static(DEV_FRONTEND_ORIGIN)])
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            HeaderName::from_static("x-auth-token"),
            HeaderName::from_static("x-api-key"),
            HeaderName::from_static("x-komga-email"),
            HeaderName::from_static("x-komga-password"),
            HeaderName::from_static("x-requested-with"),
        ])
        .vary(Vary::list([
            header::ORIGIN,
            header::ACCESS_CONTROL_REQUEST_METHOD,
            header::ACCESS_CONTROL_REQUEST_HEADERS,
        ]))
}

use axum::Json;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::http::request_urls::{opds_auth_json, request_base_url};

pub(crate) async fn opds_auth(headers: HeaderMap) -> Response {
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds-authentication+json"),
        )],
        Json(opds_auth_json(&headers)),
    )
        .into_response()
}

pub(super) fn opds_catalog_unauthorized_response(headers: &HeaderMap) -> Response {
    let base_url = request_base_url(headers);
    let auth_href = format!("{base_url}/opds/v2/auth");
    let link = format!(
        "<{}>; rel=\"http://opds-spec.org/auth/document\"; type=\"application/opds-authentication+json\"",
        auth_href,
    );

    (
        StatusCode::UNAUTHORIZED,
        [
            (
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static("Basic realm=\"Realm\""),
            ),
            (header::LINK, HeaderValue::from_str(&link).unwrap()),
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/opds-authentication+json;charset=UTF-8"),
            ),
        ],
        Json(opds_auth_json(headers)),
    )
        .into_response()
}

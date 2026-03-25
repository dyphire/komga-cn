use axum::http::{header, HeaderMap};
use serde_json::{json, Value};

pub(super) fn opds_auth_json(headers: &HeaderMap) -> Value {
    let host = request_host(headers);

    json!({
        "authentication": [
            {
                "type": "http://opds-spec.org/auth/basic",
                "labels": {
                    "login": "Email",
                    "password": "Password"
                }
            }
        ],
        "title": "Komga",
        "id": absolute_url(&host, "/opds/v2/auth"),
        "description": "Enter your email and password to authenticate.",
        "links": [
            {
                "rel": "help",
                "href": "https://komga.org"
            },
            {
                "rel": "logo",
                "href": absolute_url(&host, "/android-chrome-512x512.png")
            }
        ]
    })
}

pub(super) fn request_host(headers: &HeaderMap) -> String {
    headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("localhost")
        .to_string()
}

fn absolute_url(host: &str, path: &str) -> String {
    format!("http://{host}{path}")
}

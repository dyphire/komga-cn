use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use reqwest::header::{AUTHORIZATION, COOKIE};
use serde_json::Value;

use crate::app::placeholder_auth::{PlaceholderUser, user_is_admin, user_shared_all_libraries};

pub(super) async fn fetch_json(user: PlaceholderUser, path: &str, request_label: &str) -> Result<Value, String> {
    let base_url = java_live_base_url();
    let request_url = format!("{}{}", base_url, path);
    let client = reqwest::Client::builder()
        .build()
        .map_err(|error| format!("java live {request_label} client build failed: {error}"))?;

    let request = client.get(request_url);
    let response = send_bootstrapped_get(&client, user, request, request_label, &base_url).await?;

    if !response.status().is_success() {
        return Err(format!(
            "java live {request_label} returned HTTP {}",
            response.status().as_u16()
        ));
    }

    response
        .json::<Value>()
        .await
        .map_err(|error| format!("java live {request_label} JSON decode failed: {error}"))
}

pub(super) async fn fetch_text_response(
    user: PlaceholderUser,
    path: &str,
    request_label: &str,
) -> Result<Response, String> {
    let base_url = java_live_base_url();
    let request_url = format!("{}{}", base_url, path);
    let client = reqwest::Client::builder()
        .build()
        .map_err(|error| format!("java live {request_label} client build failed: {error}"))?;

    let request = client.get(request_url);
    let response = send_bootstrapped_get(&client, user, request, request_label, &base_url).await?;

    let status = StatusCode::from_u16(response.status().as_u16())
        .map_err(|error| format!("java live {request_label} invalid status code: {error}"))?;
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = response
        .text()
        .await
        .map_err(|error| format!("java live {request_label} body decode failed: {error}"))?;
    let empty_body = body.is_empty();

    let mut proxied = (status, body).into_response();
    if let Some(content_type) = content_type {
        proxied.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_str(&content_type)
                .unwrap_or_else(|_| HeaderValue::from_static("application/json")),
        );
    } else if empty_body {
        proxied.headers_mut().remove(header::CONTENT_TYPE);
    }

    Ok(proxied)
}

async fn send_bootstrapped_get(
    client: &reqwest::Client,
    user: PlaceholderUser,
    request: reqwest::RequestBuilder,
    request_label: &str,
    base_url: &str,
) -> Result<reqwest::Response, String> {
    let bootstrap_url = format!("{base_url}/api/v2/users/me");
    let bootstrap = client
        .get(bootstrap_url)
        .header(AUTHORIZATION, java_live_basic_auth_header(user))
        .header("X-Auth-Token", "")
        .send()
        .await
        .map_err(|error| format!("java live {request_label} bootstrap failed: {error}"))?;

    if !bootstrap.status().is_success() {
        return Err(format!(
            "java live {request_label} bootstrap returned HTTP {}",
            bootstrap.status().as_u16()
        ));
    }

    let bootstrap_headers = bootstrap.headers();
    let response = match extract_java_live_session_cookie(bootstrap_headers) {
        Some(cookie) => request.header(COOKIE, cookie),
        None => {
            let token = extract_java_live_session_token(bootstrap_headers).ok_or_else(|| {
                format!(
                    "java live {request_label} bootstrap missing KOMGA-SESSION cookie and X-Auth-Token"
                )
            })?;
            request.header("X-Auth-Token", token)
        }
    }
    .send()
    .await
    .map_err(|error| format!("java live {request_label} fetch failed: {error}"))?;

    Ok(response)
}

fn java_live_base_url() -> String {
    std::env::var("KOMGA_RUST_JAVA_LIVE_BASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:8080".to_string())
        .trim_end_matches('/')
        .to_string()
}

fn java_live_basic_auth_header(user: PlaceholderUser) -> &'static str {
    if user_is_admin(user) {
        "Basic YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4="
    } else if user_shared_all_libraries(user) {
        "Basic dXNlckBleGFtcGxlLm9yZzp1c2Vy"
    } else {
        "Basic bGltaXRlZEBleGFtcGxlLm9yZzpsaW1pdGVk"
    }
}

fn extract_java_live_session_cookie(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers.get_all(header::SET_COOKIE).iter().find_map(|value| {
        value.to_str().ok().and_then(|cookie| {
            cookie
                .split(';')
                .map(str::trim)
                .find(|part| part.starts_with("KOMGA-SESSION="))
                .map(str::to_string)
        })
    })
}

fn extract_java_live_session_token(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get("X-Auth-Token")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

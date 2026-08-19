use crate::contracts::opds::{
    OpdsAuthenticationDto, OpdsAuthenticationLabelsDto, OpdsAuthenticationLinkDto,
    OpdsAuthenticationMethodDto,
};
use axum::http::{HeaderMap, header};

pub(crate) fn opds_auth_json(headers: &HeaderMap) -> OpdsAuthenticationDto {
    let auth_url = app_absolute_url(headers, "/opds/v2/auth");
    let logo_url = app_absolute_url(headers, "/android-chrome-512x512.png");

    OpdsAuthenticationDto {
        authentication: vec![OpdsAuthenticationMethodDto {
            authentication_type: "http://opds-spec.org/auth/basic".to_string(),
            labels: OpdsAuthenticationLabelsDto {
                login: "Email".to_string(),
                password: "Password".to_string(),
            },
        }],
        title: "Komga".to_string(),
        id: auth_url,
        description: "Enter your email and password to authenticate.".to_string(),
        links: vec![
            OpdsAuthenticationLinkDto {
                rel: "help".to_string(),
                href: "https://komga.org".to_string(),
            },
            OpdsAuthenticationLinkDto {
                rel: "logo".to_string(),
                href: logo_url,
            },
        ],
    }
}

pub(crate) fn app_absolute_url(headers: &HeaderMap, path: &str) -> String {
    let base_url = request_base_url(headers);
    let prefix = request_context_path(headers);
    format!("{base_url}{prefix}{path}")
}

pub(crate) fn request_base_url(headers: &HeaderMap) -> String {
    request_base_url_with_port(headers, None)
}

pub(crate) fn request_base_url_with_port(
    headers: &HeaderMap,
    fallback_port: Option<u16>,
) -> String {
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("http");
    let host = request_host_with_port(headers, fallback_port);
    format!("{scheme}://{host}")
}

pub(crate) fn request_context_path(headers: &HeaderMap) -> String {
    let prefix = headers
        .get("x-forwarded-prefix")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("");

    if prefix.is_empty() || prefix == "/" {
        return String::new();
    }

    let normalized = if prefix.starts_with('/') {
        prefix.to_string()
    } else {
        format!("/{prefix}")
    };
    normalized.trim_end_matches('/').to_string()
}

fn request_host_with_port(headers: &HeaderMap, fallback_port: Option<u16>) -> String {
    let forwarded_host = headers
        .get("x-forwarded-host")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if let Some(host) = forwarded_host {
        return host;
    }

    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("localhost")
        .to_string();
    if host.contains(':') || fallback_port.is_none() {
        return host;
    }

    format!("{host}:{}", fallback_port.expect("fallback port checked"))
}

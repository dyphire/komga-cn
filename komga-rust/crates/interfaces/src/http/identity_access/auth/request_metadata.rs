use std::net::{IpAddr, SocketAddr};

use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Request, header};

use crate::runtime_identity_access::AuthenticationActivityWriteInput;

#[derive(Clone, Debug, Default)]
pub struct AuthenticationActivityRequestMetadata {
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

pub fn authentication_activity_request_metadata<B>(
    request: &Request<B>,
) -> AuthenticationActivityRequestMetadata {
    authentication_activity_headers_metadata_with_remote_addr(
        request.headers(),
        connect_info_ip(request),
    )
}

pub fn authentication_activity_headers_metadata_with_remote_addr(
    headers: &HeaderMap,
    remote_addr: Option<SocketAddr>,
) -> AuthenticationActivityRequestMetadata {
    AuthenticationActivityRequestMetadata {
        ip: forwarded_client_ip(headers).or_else(|| remote_addr.map(|addr| addr.ip().to_string())),
        user_agent: normalized_header_value(headers, header::USER_AGENT.as_str()),
    }
}

pub fn authentication_activity_write_input(
    metadata: &AuthenticationActivityRequestMetadata,
    source: &str,
    api_key_id: Option<&str>,
    api_key_comment: Option<&str>,
) -> AuthenticationActivityWriteInput {
    AuthenticationActivityWriteInput {
        source: source.to_string(),
        api_key_id: api_key_id.map(ToString::to_string),
        api_key_comment: api_key_comment.map(ToString::to_string),
        ip: metadata.ip.clone(),
        user_agent: metadata.user_agent.clone(),
    }
}

fn forwarded_client_ip(headers: &HeaderMap) -> Option<String> {
    normalized_header_value(headers, "forwarded")
        .and_then(|value| parse_forwarded_header_for(&value))
        .or_else(|| {
            normalized_header_value(headers, "x-forwarded-for")
                .and_then(|value| parse_x_forwarded_for(&value))
        })
        .or_else(|| normalized_header_value(headers, "x-real-ip"))
        .or_else(|| normalized_header_value(headers, "cf-connecting-ip"))
}

fn connect_info_ip<B>(request: &Request<B>) -> Option<SocketAddr> {
    request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|connect_info| connect_info.0)
}

fn parse_forwarded_header_for(value: &str) -> Option<String> {
    value.split(',').find_map(|entry| {
        entry.split(';').find_map(|segment| {
            let (name, candidate) = segment.split_once('=')?;
            if !name.trim().eq_ignore_ascii_case("for") {
                return None;
            }
            normalize_forwarded_ip_candidate(candidate)
        })
    })
}

fn parse_x_forwarded_for(value: &str) -> Option<String> {
    value.split(',').find_map(normalize_forwarded_ip_candidate)
}

fn normalize_forwarded_ip_candidate(value: &str) -> Option<String> {
    let value = value.trim().trim_matches('"');
    if value.is_empty() || value.eq_ignore_ascii_case("unknown") || value.starts_with('_') {
        return None;
    }

    if let Some(host) = bracketed_forwarded_host(value) {
        return Some(host);
    }
    if let Ok(socket_addr) = value.parse::<SocketAddr>() {
        return Some(socket_addr.ip().to_string());
    }
    if let Ok(ip_addr) = value.parse::<IpAddr>() {
        return Some(ip_addr.to_string());
    }
    if let Some((host, _port)) = value.rsplit_once(':')
        && let Ok(ip_addr) = host.parse::<IpAddr>()
    {
        return Some(ip_addr.to_string());
    }

    Some(value.to_string())
}

fn bracketed_forwarded_host(value: &str) -> Option<String> {
    let host = value.strip_prefix('[')?.split_once(']')?.0;
    if let Ok(ip_addr) = host.parse::<IpAddr>() {
        Some(ip_addr.to_string())
    } else {
        Some(host.to_string())
    }
}

fn normalized_header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn request_metadata_prefers_forwarded_ip_and_normalizes_ipv6() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "forwarded",
            HeaderValue::from_static("for=\"[2001:db8:cafe::17]:4711\";proto=https"),
        );
        headers.insert(
            header::USER_AGENT,
            HeaderValue::from_static(" reader-device "),
        );

        let metadata = authentication_activity_headers_metadata_with_remote_addr(
            &headers,
            Some(
                "198.51.100.77:43123"
                    .parse()
                    .expect("socket address should parse"),
            ),
        );

        assert_eq!(metadata.ip.as_deref(), Some("2001:db8:cafe::17"));
        assert_eq!(metadata.user_agent.as_deref(), Some("reader-device"));
    }

    #[test]
    fn request_metadata_falls_back_to_connect_info_when_proxy_headers_are_not_usable() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("unknown, _hidden"),
        );

        let metadata = authentication_activity_headers_metadata_with_remote_addr(
            &headers,
            Some(
                "203.0.113.55:61888"
                    .parse()
                    .expect("socket address should parse"),
            ),
        );

        assert_eq!(metadata.ip.as_deref(), Some("203.0.113.55"));
        assert_eq!(metadata.user_agent, None);
    }

    #[test]
    fn authentication_activity_write_input_preserves_metadata_and_api_key_fields() {
        let input = authentication_activity_write_input(
            &AuthenticationActivityRequestMetadata {
                ip: Some("198.51.100.24".to_string()),
                user_agent: Some("koreader".to_string()),
            },
            "API_KEY",
            Some("api-key-1"),
            Some("KOReader"),
        );

        assert_eq!(input.source, "API_KEY");
        assert_eq!(input.api_key_id.as_deref(), Some("api-key-1"));
        assert_eq!(input.api_key_comment.as_deref(), Some("KOReader"));
        assert_eq!(input.ip.as_deref(), Some("198.51.100.24"));
        assert_eq!(input.user_agent.as_deref(), Some("koreader"));
    }
}

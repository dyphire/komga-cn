use async_trait::async_trait;
use serde_json::Value;

use super::{KoboStoreSyncMergeResult, KoboStoreSyncPort, decode_or_passthrough_sync_token};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KoboProxyHeader {
    pub name: String,
    pub value: String,
}

impl KoboProxyHeader {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }

    fn name_eq(&self, expected: &str) -> bool {
        self.name.eq_ignore_ascii_case(expected)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KoboProxyRequest {
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub headers: Vec<KoboProxyHeader>,
    pub body: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KoboProxyResponse {
    pub status: u16,
    pub headers: Vec<KoboProxyHeader>,
    pub body: Option<Value>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KoboProxyRequestBodyError {
    UnsupportedMediaType,
    InvalidBody,
}

#[async_trait]
pub trait KoboProxyPort: Send + Sync {
    async fn proxy_kobo_request(
        &self,
        request: KoboProxyRequest,
    ) -> Result<KoboProxyResponse, String>;
}

pub fn build_kobo_proxy_request(
    method: &str,
    path: &str,
    query: Option<&str>,
    headers: &[KoboProxyHeader],
    body: &[u8],
) -> Result<KoboProxyRequest, KoboProxyRequestBodyError> {
    let body = kobo_proxy_request_body(headers, body)?;
    Ok(KoboProxyRequest {
        method: method.to_string(),
        path: path.to_string(),
        query: normalized_query(query),
        headers: kobo_proxy_forwarded_headers(headers),
        body,
    })
}

#[async_trait]
impl<T> KoboStoreSyncPort for T
where
    T: KoboProxyPort + Send + Sync + ?Sized,
{
    async fn sync_store_library(
        &self,
        forwarded_headers: &[KoboProxyHeader],
        query: Option<&str>,
        raw_sync_token: &str,
    ) -> Result<KoboStoreSyncMergeResult, String> {
        let response = self
            .proxy_kobo_request(kobo_store_sync_proxy_request(
                forwarded_headers,
                query,
                raw_sync_token,
            ))
            .await?;
        if !(200..=299).contains(&response.status) {
            return Err("kobo store sync proxy failed".to_string());
        }

        let events = match response.body {
            Some(body) => body
                .as_array()
                .cloned()
                .ok_or_else(|| "kobo store sync proxy body must be an array".to_string())?,
            None => Vec::new(),
        };
        let should_continue = response
            .headers
            .iter()
            .find_map(|header| header.name_eq("x-kobo-sync").then_some(header.value.trim()))
            .is_some_and(|value| value.eq_ignore_ascii_case("continue"));
        let raw_sync_token = response
            .headers
            .iter()
            .find_map(|header| {
                header
                    .name_eq("x-kobo-synctoken")
                    .then_some(header.value.as_str())
            })
            .and_then(decode_or_passthrough_sync_token);

        Ok(KoboStoreSyncMergeResult {
            events,
            raw_sync_token,
            should_continue,
        })
    }
}

fn kobo_store_sync_proxy_request(
    forwarded_headers: &[KoboProxyHeader],
    query: Option<&str>,
    raw_sync_token: &str,
) -> KoboProxyRequest {
    let mut headers = kobo_proxy_forwarded_headers(forwarded_headers);
    headers.push(KoboProxyHeader::new("x-kobo-synctoken", raw_sync_token));
    KoboProxyRequest {
        method: "GET".to_string(),
        path: "/v1/library/sync".to_string(),
        query: normalized_query(query),
        headers,
        body: None,
    }
}

fn kobo_proxy_forwarded_headers(headers: &[KoboProxyHeader]) -> Vec<KoboProxyHeader> {
    headers
        .iter()
        .filter(|header| should_forward_kobo_proxy_header(&header.name))
        .cloned()
        .collect()
}

fn should_forward_kobo_proxy_header(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower == "x-kobo-synctoken" {
        return false;
    }

    matches!(
        lower.as_str(),
        "authorization" | "user-agent" | "accept" | "accept-language" | "content-type"
    ) || lower.starts_with("x-kobo-")
}

fn kobo_proxy_request_body(
    headers: &[KoboProxyHeader],
    body: &[u8],
) -> Result<Option<Vec<u8>>, KoboProxyRequestBodyError> {
    if body.is_empty() {
        return Ok(None);
    }

    let content_type = headers.iter().find_map(|header| {
        header
            .name_eq("content-type")
            .then(|| header.value.to_ascii_lowercase())
    });
    let is_json = content_type
        .as_deref()
        .is_some_and(|value| value.starts_with("application/json") || value.contains("+json"));
    let is_xml = content_type.as_deref().is_some_and(|value| {
        value.starts_with("application/xml")
            || value.starts_with("text/xml")
            || value.contains("+xml")
    });

    if is_xml {
        validate_kobo_xml_request_body(body)?;
        return Ok(Some(body.to_vec()));
    }

    if !is_json {
        return Err(KoboProxyRequestBodyError::UnsupportedMediaType);
    }

    serde_json::from_slice::<Value>(body).map_err(|_| KoboProxyRequestBodyError::InvalidBody)?;
    Ok(Some(body.to_vec()))
}

fn validate_kobo_xml_request_body(body: &[u8]) -> Result<(), KoboProxyRequestBodyError> {
    let mut reader = quick_xml::Reader::from_reader(body);
    let mut buffer = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(quick_xml::events::Event::Eof) => return Ok(()),
            Ok(_) => buffer.clear(),
            Err(_) => return Err(KoboProxyRequestBodyError::InvalidBody),
        }
    }
}

fn normalized_query(query: Option<&str>) -> Option<String> {
    query
        .map(str::trim)
        .filter(|query| !query.is_empty())
        .map(str::to_string)
}

use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::json;

use super::{
    KoboProxyHeader, KoboProxyPort, KoboProxyRequest, KoboProxyRequestBodyError, KoboProxyResponse,
    KoboStoreSyncPort, build_kobo_proxy_request,
};

fn header(name: &str, value: &str) -> KoboProxyHeader {
    KoboProxyHeader::new(name, value)
}

#[test]
fn kobo_proxy_request_preparation_filters_headers_and_keeps_validated_body() {
    let body = br#"{"ok":true}"#;

    let request = build_kobo_proxy_request(
        "POST",
        "/v1/auth/device",
        Some("foo=bar"),
        &[
            header("authorization", "Bearer store"),
            header("user-agent", "Kobo"),
            header("accept", "application/json"),
            header("content-type", "application/json"),
            header("x-kobo-device-id", "device-1"),
            header("x-kobo-synctoken", "komga.sync.token"),
            header("x-auth-token", "komga-session"),
            header("cookie", "SESSION=secret"),
            header("host", "localhost"),
        ],
        body,
    )
    .expect("json proxy body should validate");

    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/v1/auth/device");
    assert_eq!(request.query.as_deref(), Some("foo=bar"));
    assert_eq!(request.body.as_deref(), Some(body.as_slice()));
    assert_eq!(
        request.headers,
        vec![
            header("authorization", "Bearer store"),
            header("user-agent", "Kobo"),
            header("accept", "application/json"),
            header("content-type", "application/json"),
            header("x-kobo-device-id", "device-1"),
        ]
    );
}

#[test]
fn kobo_proxy_request_preparation_accepts_xml_and_empty_bodies() {
    let xml_request = build_kobo_proxy_request(
        "POST",
        "/v1/auth/device",
        None,
        &[header("content-type", "application/xml")],
        b"<request/>",
    )
    .expect("xml proxy body should validate");
    assert_eq!(xml_request.body.as_deref(), Some(b"<request/>".as_slice()));

    let empty_request = build_kobo_proxy_request(
        "POST",
        "/v1/auth/device",
        None,
        &[header("content-type", "text/plain")],
        b"",
    )
    .expect("empty proxy body should validate");
    assert_eq!(empty_request.body, None);
}

#[test]
fn kobo_proxy_request_preparation_rejects_unsupported_or_invalid_bodies() {
    assert_eq!(
        build_kobo_proxy_request(
            "POST",
            "/v1/auth/device",
            None,
            &[header("content-type", "text/plain")],
            b"hello",
        ),
        Err(KoboProxyRequestBodyError::UnsupportedMediaType)
    );
    assert_eq!(
        build_kobo_proxy_request(
            "POST",
            "/v1/auth/device",
            None,
            &[header("content-type", "application/json")],
            b"{broken",
        ),
        Err(KoboProxyRequestBodyError::InvalidBody)
    );
    assert_eq!(
        build_kobo_proxy_request(
            "POST",
            "/v1/auth/device",
            None,
            &[header("content-type", "application/xml")],
            b"<request><",
        ),
        Err(KoboProxyRequestBodyError::InvalidBody)
    );
}

#[tokio::test]
async fn kobo_store_sync_uses_proxy_port_with_raw_store_sync_token() {
    let proxy = RecordingKoboProxy {
        requests: Mutex::new(Vec::new()),
    };

    let result = proxy
        .sync_store_library(
            &[
                header("accept", "application/json"),
                header("x-kobo-device-id", "device-1"),
                header("x-kobo-synctoken", "komga.sync.token"),
                header("x-auth-token", "komga-session"),
                header("cookie", "SESSION=secret"),
            ],
            Some("filter=all"),
            "raw.store.token",
        )
        .await
        .expect("store sync proxy response should merge");

    assert_eq!(result.events, vec![json!({ "StoreOnly": true })]);
    assert_eq!(result.raw_sync_token.as_deref(), Some("store.next.token"));
    assert!(result.should_continue);

    let requests = proxy
        .requests
        .lock()
        .expect("proxy request lock should not be poisoned");
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.method, "GET");
    assert_eq!(request.path, "/v1/library/sync");
    assert_eq!(request.query.as_deref(), Some("filter=all"));
    assert_eq!(request.body, None);
    assert_eq!(
        request.headers,
        vec![
            header("accept", "application/json"),
            header("x-kobo-device-id", "device-1"),
            header("x-kobo-synctoken", "raw.store.token"),
        ]
    );
}

#[tokio::test]
async fn kobo_store_sync_rejects_successful_non_array_proxy_body() {
    let proxy = NonArrayStoreProxy;

    let error = proxy
        .sync_store_library(&[], None, "raw.store.token")
        .await
        .expect_err("successful store sync body must be an event array");

    assert!(error.contains("store sync proxy body"));
}

struct RecordingKoboProxy {
    requests: Mutex<Vec<KoboProxyRequest>>,
}

#[async_trait]
impl KoboProxyPort for RecordingKoboProxy {
    async fn proxy_kobo_request(
        &self,
        request: KoboProxyRequest,
    ) -> Result<KoboProxyResponse, String> {
        self.requests
            .lock()
            .expect("proxy request lock should not be poisoned")
            .push(request);
        Ok(KoboProxyResponse {
            status: 200,
            headers: vec![
                header("x-kobo-sync", "continue"),
                header("x-kobo-synctoken", "store.next.token"),
            ],
            body: Some(json!([{ "StoreOnly": true }])),
        })
    }
}

struct NonArrayStoreProxy;

#[async_trait]
impl KoboProxyPort for NonArrayStoreProxy {
    async fn proxy_kobo_request(
        &self,
        _request: KoboProxyRequest,
    ) -> Result<KoboProxyResponse, String> {
        Ok(KoboProxyResponse {
            status: 200,
            headers: Vec::new(),
            body: Some(json!({ "unexpected": true })),
        })
    }
}

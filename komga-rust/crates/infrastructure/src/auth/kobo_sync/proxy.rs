use komga_application::identity_access::{
    KoboStoreSyncMergeResult, decode_or_passthrough_sync_token,
};
use reqwest::header::{HeaderName, HeaderValue};
use serde_json::Value;

pub(super) async fn proxy_kobo_store_library_sync(
    forwarded_headers: &[(String, String)],
    query: Option<&str>,
    raw_sync_token: &str,
) -> Result<KoboStoreSyncMergeResult, ()> {
    let mut target = String::from("https://storeapi.kobo.com/v1/library/sync");
    if let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) {
        target.push('?');
        target.push_str(query);
    }

    let client = reqwest::Client::builder().build().map_err(|_| ())?;
    let mut request = client.get(target);
    for (name, value) in forwarded_headers {
        let lower = name.to_ascii_lowercase();
        if lower == "host" || lower == "content-length" || lower == "x-kobo-synctoken" {
            continue;
        }
        let Ok(header_name) = HeaderName::from_bytes(name.as_bytes()) else {
            continue;
        };
        let Ok(header_value) = HeaderValue::from_str(value) else {
            continue;
        };
        request = request.header(header_name, header_value);
    }
    request = request.header("x-kobo-synctoken", raw_sync_token);

    let response = request.send().await.map_err(|_| ())?;
    if !response.status().is_success() {
        return Err(());
    }

    let headers = response.headers().clone();
    let body = response.json::<Value>().await.map_err(|_| ())?;
    let events = body.as_array().cloned().unwrap_or_default();
    let should_continue = headers
        .get("x-kobo-sync")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .is_some_and(|value| value.eq_ignore_ascii_case("continue"));
    let raw_sync_token = headers
        .get("x-kobo-synctoken")
        .and_then(|value| value.to_str().ok())
        .and_then(decode_or_passthrough_sync_token);

    Ok(KoboStoreSyncMergeResult {
        events,
        raw_sync_token,
        should_continue,
    })
}

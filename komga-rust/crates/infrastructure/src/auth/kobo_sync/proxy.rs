use komga_application::identity_access::{KoboProxyHeader, KoboProxyRequest, KoboProxyResponse};
use reqwest::header::{HeaderName, HeaderValue};
use serde_json::Value;

pub(super) async fn execute_kobo_proxy_request(
    base_url: &str,
    request: KoboProxyRequest,
) -> anyhow::Result<KoboProxyResponse> {
    let mut target = format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        request.path.trim_start_matches('/')
    );
    if let Some(query) = request.query.as_deref() {
        target.push('?');
        target.push_str(query);
    }

    let client = reqwest::Client::builder()
        .build()
        .map_err(anyhow::Error::from)?;
    let request_method =
        reqwest::Method::from_bytes(request.method.as_bytes()).map_err(anyhow::Error::from)?;
    let mut builder = client.request(request_method, target);

    for header in request.headers {
        let Ok(header_name) = HeaderName::from_bytes(header.name.as_bytes()) else {
            continue;
        };
        let Ok(header_value) = HeaderValue::from_str(&header.value) else {
            continue;
        };
        builder = builder.header(header_name, header_value);
    }

    if let Some(body) = request.body {
        builder = builder.body(body);
    }

    let response = builder.send().await.map_err(anyhow::Error::from)?;
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .filter(|(name, _)| name.as_str().to_ascii_lowercase().starts_with("x-kobo-"))
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| KoboProxyHeader::new(name.as_str(), value))
        })
        .collect::<Vec<_>>();
    let response_bytes = response.bytes().await.map_err(anyhow::Error::from)?;
    if response_bytes.is_empty() || !(200..=299).contains(&status) {
        return Ok(KoboProxyResponse {
            status,
            headers,
            body: None,
        });
    }

    let body = serde_json::from_slice::<Value>(&response_bytes).map_err(anyhow::Error::from)?;
    Ok(KoboProxyResponse {
        status,
        headers,
        body: Some(body),
    })
}

use super::*;

pub(super) async fn proxied_missing_kobo_book_response(
    app: &HttpAppState,
    method: &axum::http::Method,
    proxy_path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: &Bytes,
) -> Option<Response> {
    if !load_kobo_proxy_enabled(app.services.server_settings.as_ref()).await {
        return None;
    }

    Some(
        match execute_kobo_proxy_request(method, proxy_path, query, headers, body).await {
            Ok(response) => response,
            Err(status) => status.into_response(),
        },
    )
}

pub(super) async fn execute_kobo_proxy_request(
    method: &axum::http::Method,
    path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<Response, StatusCode> {
    let base_url = std::env::var("KOMGA_RUST_KOBO_PROXY_URL")
        .unwrap_or_else(|_| "https://storeapi.kobo.com".to_string());
    let mut target = format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    );
    if let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) {
        target.push('?');
        target.push_str(query);
    }

    let client = Client::builder()
        .build()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let request_method = reqwest::Method::from_bytes(method.as_str().as_bytes())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut request = client.request(request_method, target);

    for (name, value) in headers {
        let header_name = name.as_str();
        let lower = header_name.to_ascii_lowercase();
        let should_forward = matches!(
            lower.as_str(),
            "authorization" | "user-agent" | "accept" | "accept-language" | "content-type"
        ) || lower.starts_with("x-kobo-");
        if !should_forward || lower == "x-kobo-synctoken" {
            continue;
        }
        let Ok(value) = value.to_str() else {
            continue;
        };
        request = request.header(header_name, value);
    }

    if let Some(prepared_body) = prepare_kobo_proxy_request_body(headers, body)? {
        request = request.body(prepared_body);
    }

    let response = request
        .send()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let status = StatusCode::from_u16(response.status().as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let response_headers = response.headers().clone();
    let response_bytes = response
        .bytes()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if status.is_client_error() || status.is_server_error() {
        return Ok(status.into_response());
    }
    if response_bytes.is_empty() {
        let mut proxied = status.into_response();
        for (name, value) in &response_headers {
            if name.as_str().to_ascii_lowercase().starts_with("x-kobo-") {
                proxied.headers_mut().append(name.clone(), value.clone());
            }
        }
        return Ok(proxied);
    }

    let (mut proxied, include_kobo_headers) = match serde_json::from_slice::<Value>(&response_bytes)
    {
        Ok(response_body) => {
            let mut response = Json(response_body).into_response();
            *response.status_mut() = status;
            (response, true)
        }
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    if include_kobo_headers {
        for (name, value) in &response_headers {
            if name.as_str().to_ascii_lowercase().starts_with("x-kobo-") {
                proxied.headers_mut().append(name.clone(), value.clone());
            }
        }
    }
    Ok(proxied)
}

fn prepare_kobo_proxy_request_body(
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<Option<Vec<u8>>, StatusCode> {
    if body.is_empty() {
        return Ok(None);
    }

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_ascii_lowercase());
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
        return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    serde_json::from_slice::<Value>(body).map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Some(body.to_vec()))
}

fn validate_kobo_xml_request_body(body: &Bytes) -> Result<(), StatusCode> {
    let mut reader = quick_xml::Reader::from_reader(body.as_ref());
    let mut buffer = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(quick_xml::events::Event::Eof) => return Ok(()),
            Ok(_) => buffer.clear(),
            Err(_) => return Err(StatusCode::BAD_REQUEST),
        }
    }
}

use axum::http::{HeaderMap, HeaderName, HeaderValue, header};
use axum::response::Response;

use crate::cache::{
    asset_etag, asset_not_modified_response, asset_ok_response, if_modified_since_matches,
    if_none_match_matches,
};

pub(crate) struct MediaAssetResponse {
    content_type: String,
    bytes: Vec<u8>,
    etag: Option<String>,
    last_modified: Option<String>,
    content_disposition: Option<String>,
    headers: Vec<(HeaderName, HeaderValue)>,
}

impl MediaAssetResponse {
    pub(crate) fn new(content_type: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            content_type: content_type.into(),
            bytes,
            etag: None,
            last_modified: None,
            content_disposition: None,
            headers: Vec::new(),
        }
    }

    pub(crate) fn with_etag(mut self) -> Self {
        self.etag = Some(asset_etag(self.bytes.as_slice()));
        self
    }

    pub(crate) fn with_last_modified(mut self, last_modified: Option<String>) -> Self {
        self.last_modified = last_modified;
        self
    }

    pub(crate) fn with_content_disposition(mut self, content_disposition: Option<String>) -> Self {
        self.content_disposition = content_disposition;
        self
    }

    pub(crate) fn with_header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.headers.push((name, value));
        self
    }

    pub(crate) fn into_response(self, request_headers: Option<&HeaderMap>) -> Response {
        let Self {
            content_type,
            bytes,
            etag,
            last_modified,
            content_disposition,
            headers,
        } = self;

        if let Some(request_headers) = request_headers {
            if let Some(etag) = etag.as_deref()
                && if_none_match_matches(request_headers, etag)
            {
                return not_modified_response(Some(etag), last_modified.as_deref(), headers);
            }
            if let Some(last_modified) = last_modified.as_deref()
                && if_modified_since_matches(request_headers, last_modified)
            {
                return not_modified_response(etag.as_deref(), Some(last_modified), headers);
            }
        }

        let mut response = asset_ok_response(
            content_type.as_str(),
            bytes,
            etag.as_deref(),
            last_modified.as_deref(),
        );
        if let Some(content_disposition) = content_disposition {
            response.headers_mut().insert(
                header::CONTENT_DISPOSITION,
                HeaderValue::from_str(&content_disposition)
                    .expect("asset content disposition should be valid"),
            );
        }
        insert_headers(&mut response, headers);
        response
    }
}

fn not_modified_response(
    etag: Option<&str>,
    last_modified: Option<&str>,
    headers: Vec<(HeaderName, HeaderValue)>,
) -> Response {
    let mut response = asset_not_modified_response(etag, last_modified);
    insert_headers(&mut response, headers);
    response
}

fn insert_headers(response: &mut Response, headers: Vec<(HeaderName, HeaderValue)>) {
    for (name, value) in headers {
        response.headers_mut().insert(name, value);
    }
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue, StatusCode, header};

    use super::*;

    #[test]
    fn conditional_not_modified_response_keeps_extra_headers() {
        let last_modified = "Wed, 21 Oct 2015 07:28:00 GMT";
        let mut request_headers = HeaderMap::new();
        request_headers.insert(
            header::IF_MODIFIED_SINCE,
            HeaderValue::from_static(last_modified),
        );

        let response = MediaAssetResponse::new("text/css", b"body {}".to_vec())
            .with_last_modified(Some(last_modified.to_string()))
            .with_header(
                header::CONTENT_SECURITY_POLICY,
                HeaderValue::from_static("script-src 'none'; object-src 'none';"),
            )
            .into_response(Some(&request_headers));

        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(
            response
                .headers()
                .get(header::LAST_MODIFIED)
                .and_then(|value| value.to_str().ok()),
            Some(last_modified)
        );
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_SECURITY_POLICY)
                .and_then(|value| value.to_str().ok()),
            Some("script-src 'none'; object-src 'none';")
        );
    }

    #[test]
    fn conditional_not_modified_response_omits_content_disposition() {
        let last_modified = "Wed, 21 Oct 2015 07:28:00 GMT";
        let mut request_headers = HeaderMap::new();
        request_headers.insert(
            header::IF_MODIFIED_SINCE,
            HeaderValue::from_static(last_modified),
        );

        let response = MediaAssetResponse::new("text/plain", b"hello".to_vec())
            .with_last_modified(Some(last_modified.to_string()))
            .with_content_disposition(Some("inline; filename=\"hello.txt\"".to_string()))
            .into_response(Some(&request_headers));

        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
        assert!(!response.headers().contains_key(header::CONTENT_DISPOSITION));
    }
}

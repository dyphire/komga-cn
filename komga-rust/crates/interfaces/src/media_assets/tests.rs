use std::time::UNIX_EPOCH;

use axum::http::{HeaderMap, HeaderValue, header};

use crate::cache::{format_http_date, if_modified_since_matches};

#[test]
fn if_modified_since_uses_http_date_ordering() {
    let resource_time = UNIX_EPOCH + std::time::Duration::from_secs(10);
    let expected_last_modified =
        format_http_date(resource_time).expect("resource date should format as HTTP date");

    let mut newer_headers = HeaderMap::new();
    newer_headers.insert(
        header::IF_MODIFIED_SINCE,
        HeaderValue::from_str(
            format_http_date(UNIX_EPOCH + std::time::Duration::from_secs(20))
                .expect("newer header date should format as HTTP date")
                .as_str(),
        )
        .expect("if-modified-since header should be valid"),
    );
    assert!(if_modified_since_matches(
        &newer_headers,
        expected_last_modified.as_str(),
    ));

    let mut older_headers = HeaderMap::new();
    older_headers.insert(
        header::IF_MODIFIED_SINCE,
        HeaderValue::from_str(
            format_http_date(UNIX_EPOCH + std::time::Duration::from_secs(5))
                .expect("older header date should format as HTTP date")
                .as_str(),
        )
        .expect("if-modified-since header should be valid"),
    );
    assert!(!if_modified_since_matches(
        &older_headers,
        expected_last_modified.as_str(),
    ));
}

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::Value;
use serde_json::json;

use crate::helpers::query_values;
use komga_domain::discovery::DiscoveryError;

pub(in crate::discovery) fn requested_query_values(query: &str, key: &str) -> Option<Vec<String>> {
    let values = query_values(query, key)
        .into_iter()
        .filter(|value| !value.is_empty())
        .map(decode_query_component)
        .collect::<Vec<_>>();
    (!values.is_empty()).then_some(values)
}

pub(in crate::discovery) fn decode_query_component(value: &str) -> String {
    let mut decoded = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0usize;

    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let first = (bytes[index + 1] as char).to_digit(16);
                let second = (bytes[index + 2] as char).to_digit(16);

                if let (Some(first), Some(second)) = (first, second) {
                    decoded.push((first * 16 + second) as u8);
                    index += 3;
                } else {
                    decoded.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }

    String::from_utf8_lossy(&decoded).into_owned()
}

pub(in crate::discovery) fn internal_error_response(
    error: impl std::fmt::Display + std::fmt::Debug,
) -> Response {
    tracing::error!(?error, "internal persisted discovery error");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": format!("{error:#}") })),
    )
        .into_response()
}

pub(in crate::discovery) fn discovery_error_response(error: DiscoveryError) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": format!("{error:?}") })),
    )
        .into_response()
}

pub(in crate::discovery) fn filter_rows<T>(
    rows: Vec<T>,
    mut predicate: impl FnMut(&T) -> bool,
) -> Vec<T> {
    rows.into_iter().filter(|row| predicate(row)).collect()
}

#[derive(Clone, Copy)]
pub(in crate::discovery) struct PagePayloadMetadata {
    pub(in crate::discovery) page: usize,
    pub(in crate::discovery) size: usize,
    pub(in crate::discovery) total_elements: usize,
    pub(in crate::discovery) total_pages: usize,
    pub(in crate::discovery) paged: bool,
    pub(in crate::discovery) sorted: bool,
    pub(in crate::discovery) offset: usize,
}

pub(in crate::discovery) fn page_payload(
    content: Vec<Value>,
    metadata: PagePayloadMetadata,
) -> Value {
    let number_of_elements = content.len();
    let first = metadata.page == 0;
    let last = metadata.total_pages == 0 || metadata.page + 1 >= metadata.total_pages;
    let sort = json!({
        "empty": !metadata.sorted,
        "sorted": metadata.sorted,
        "unsorted": !metadata.sorted,
    });

    json!({
        "content": content,
        "pageable": {
            "pageNumber": metadata.page,
            "pageSize": metadata.size,
            "sort": sort.clone(),
            "offset": metadata.offset,
            "paged": metadata.paged,
            "unpaged": !metadata.paged,
        },
        "last": last,
        "totalElements": metadata.total_elements,
        "totalPages": metadata.total_pages,
        "first": first,
        "size": metadata.size,
        "number": metadata.page,
        "sort": sort,
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn filter_rows_preserves_input_order() {
        let rows = vec!["book-2", "book-1", "book-3"];

        let filtered = super::filter_rows(rows, |row| *row != "book-1");

        assert_eq!(filtered, vec!["book-2", "book-3"]);
    }

    #[test]
    fn page_payload_builds_expected_metadata() {
        let payload = super::page_payload(
            vec![json!({ "id": "book-1" })],
            super::PagePayloadMetadata {
                page: 2,
                size: 20,
                total_elements: 41,
                total_pages: 3,
                paged: true,
                sorted: true,
                offset: 40,
            },
        );

        assert_eq!(payload.get("number"), Some(&json!(2)));
        assert_eq!(payload.pointer("/pageable/offset"), Some(&json!(40)));
        assert_eq!(payload.get("totalPages"), Some(&json!(3)));
        assert_eq!(payload.get("numberOfElements"), Some(&json!(1)));
    }
}

use axum::http::StatusCode;
use axum::response::Response;

use crate::helpers::{query_values, spring_error_response};
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
    spring_error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("{error:#}"))
}

pub(in crate::discovery) fn discovery_error_response(error: DiscoveryError) -> Response {
    spring_error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("{error:?}"))
}

pub(in crate::discovery) fn filter_rows<T>(
    rows: Vec<T>,
    mut predicate: impl FnMut(&T) -> bool,
) -> Vec<T> {
    rows.into_iter().filter(|row| predicate(row)).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn filter_rows_preserves_input_order() {
        let rows = vec!["book-2", "book-1", "book-3"];

        let filtered = super::filter_rows(rows, |row| *row != "book-1");

        assert_eq!(filtered, vec!["book-2", "book-3"]);
    }
}

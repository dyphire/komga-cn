use super::*;

#[derive(Clone, Copy)]
pub enum TextMatchMode {
    Exact,
    Contains,
    StartsWith,
    EndsWith,
}

pub struct RuntimeListRequest {
    pub sorts: Vec<String>,
    pub page: usize,
    pub size: usize,
    pub unpaged: bool,
}

pub fn requested_query_values(query: &str, key: &str) -> Option<Vec<String>> {
    let values = query_values(query, key)
        .into_iter()
        .filter(|value| !value.is_empty())
        .map(decode_query_component)
        .collect::<Vec<_>>();
    (!values.is_empty()).then_some(values)
}

pub fn runtime_list_request(query: &str) -> RuntimeListRequest {
    RuntimeListRequest {
        sorts: query_values(query, "sort")
            .into_iter()
            .map(decode_query_component)
            .collect(),
        page: query_value(query, "page")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0),
        size: query_value(query, "size")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(20)
            .max(1),
        unpaged: query_bool(query, "unpaged"),
    }
}

pub fn first_group_key(title: &str) -> String {
    title
        .trim()
        .chars()
        .next()
        .map(|ch| ch.to_lowercase().collect::<String>())
        .unwrap_or_else(|| "#".to_string())
}

pub fn decode_query_component(value: &str) -> String {
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

pub fn internal_error_response(error: String) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": error })),
    )
        .into_response()
}

pub fn filter_rows<T>(rows: Vec<T>, mut predicate: impl FnMut(&T) -> bool) -> Vec<T> {
    rows.into_iter().filter(|row| predicate(row)).collect()
}

/// `expected` is assumed to already be normalized by the request parser.
/// We only normalize the persisted-side value here so callers can share one
/// matching path across exact/contains/prefix/suffix filters without repeating
/// transport parsing rules at every site.
pub fn normalized_text_matches(value: &str, expected: &[String], mode: TextMatchMode) -> bool {
    let normalized = value.to_ascii_lowercase();
    match mode {
        TextMatchMode::Exact => expected.contains(&normalized),
        TextMatchMode::Contains => expected
            .iter()
            .any(|candidate| normalized.contains(candidate)),
        TextMatchMode::StartsWith => expected
            .iter()
            .any(|candidate| normalized.starts_with(candidate)),
        TextMatchMode::EndsWith => expected
            .iter()
            .any(|candidate| normalized.ends_with(candidate)),
    }
}

pub fn any_normalized_text_matches<'a>(
    values: impl IntoIterator<Item = &'a str>,
    expected: &[String],
    mode: TextMatchMode,
) -> bool {
    values
        .into_iter()
        .any(|value| normalized_text_matches(value, expected, mode))
}

pub fn any_ignore_ascii_case<'a>(
    values: impl IntoIterator<Item = &'a str>,
    expected: &[String],
) -> bool {
    values.into_iter().any(|value| {
        expected
            .iter()
            .any(|candidate| value.eq_ignore_ascii_case(candidate))
    })
}

pub fn matches_optional_value<T>(
    value: Option<T>,
    missing_result: bool,
    predicate: impl FnOnce(T) -> bool,
) -> bool {
    match value {
        Some(value) => predicate(value),
        None => missing_result,
    }
}

#[derive(Clone, Copy)]
pub struct PagePayloadMetadata {
    pub page: usize,
    pub size: usize,
    pub total_elements: usize,
    pub total_pages: usize,
    pub paged: bool,
    pub sorted: bool,
    pub offset: usize,
}

pub fn page_payload(content: Vec<Value>, metadata: PagePayloadMetadata) -> Value {
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

pub fn normalize_unpaged_page_size<T>(mut page: PageEnvelope<T>, size: usize) -> PageEnvelope<T> {
    page.page = 0;
    page.size = size;
    page.total_pages = if page.total_elements == 0 {
        0
    } else {
        ((page.total_elements - 1) / size) + 1
    };
    page
}

pub fn invalid_runtime_series_list_response(error: DiscoveryError) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "error": format!("invalid runtime series request: {error:?}"),
        })),
    )
        .into_response()
}

pub fn invalid_runtime_books_list_response(error: DiscoveryError) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "error": format!("invalid runtime books request: {error:?}"),
        })),
    )
        .into_response()
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
    fn normalized_text_matches_supports_all_modes() {
        assert!(super::normalized_text_matches(
            "Alpha",
            &["alpha".to_string()],
            super::TextMatchMode::Exact,
        ));
        assert!(super::normalized_text_matches(
            "Alpha Beta",
            &["ha be".to_string()],
            super::TextMatchMode::Contains,
        ));
        assert!(super::normalized_text_matches(
            "Alpha",
            &["alp".to_string()],
            super::TextMatchMode::StartsWith,
        ));
        assert!(super::normalized_text_matches(
            "Alpha",
            &["pha".to_string()],
            super::TextMatchMode::EndsWith,
        ));
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

    #[test]
    fn runtime_list_request_decodes_sort_and_defaults_size() {
        let request = super::runtime_list_request("sort=title%2Casc&page=3&unpaged=true");

        assert_eq!(request.sorts, vec!["title,asc"]);
        assert_eq!(request.page, 3);
        assert_eq!(request.size, 20);
        assert!(request.unpaged);
    }
}

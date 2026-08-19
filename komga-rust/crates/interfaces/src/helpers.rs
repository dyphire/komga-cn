use crate::contracts::common::{PageDto, SpringInternalErrorDto, ValidationErrorDto, ViolationDto};
use crate::contracts::discovery::BookDto;
use crate::discovery_auth::context::{DetailAccessDenial, DiscoveryQueryContext};
use axum::Json;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use komga_application::discovery::BookReadModel;
use komga_domain::common_ids::{LibraryId, UserId};
use komga_domain::discovery::{DiscoveryQueryContext as DomainDiscoveryQueryContext, PageEnvelope};
use reqwest::Url;

pub(crate) fn spring_error_response(
    status: StatusCode,
    error: impl std::fmt::Display + std::fmt::Debug,
) -> Response {
    (
        status,
        Json(SpringInternalErrorDto {
            error: format!("{error:#}"),
        }),
    )
        .into_response()
}

pub(crate) fn internal_error_response(error: impl std::fmt::Display + std::fmt::Debug) -> Response {
    tracing::error!(?error, "internal server error");
    spring_error_response(StatusCode::INTERNAL_SERVER_ERROR, error)
}

pub(crate) fn books_page_payload(
    page: PageEnvelope<BookReadModel>,
    is_admin: bool,
    paged: bool,
    sorted: bool,
) -> anyhow::Result<PageDto<BookDto>> {
    let page_number = page.page;
    let page_size = page.size;
    let total_pages = page.total_pages;
    books_page_payload_with_shape(
        page,
        page_number,
        page_size,
        total_pages,
        is_admin,
        paged,
        sorted,
    )
}

pub(crate) fn books_page_payload_with_shape(
    page: PageEnvelope<BookReadModel>,
    page_number: usize,
    page_size: usize,
    total_pages: usize,
    is_admin: bool,
    paged: bool,
    sorted: bool,
) -> anyhow::Result<PageDto<BookDto>> {
    let content = page
        .content
        .iter()
        .map(|book| BookDto::from_read_model(book, is_admin))
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(PageDto::from_parts(
        content,
        page_number,
        page_size,
        page.total_elements,
        total_pages,
        paged,
        sorted,
    ))
}

pub(crate) fn api_file_path(value: &str) -> String {
    decode_file_url_path(value).unwrap_or_else(|| value.to_string())
}

pub(crate) fn restricted_book_url(url: &str, is_admin: bool) -> String {
    if is_admin {
        return url.to_string();
    }

    url.rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or_default()
        .to_string()
}

fn decode_file_url_path(value: &str) -> Option<String> {
    if let Ok(parsed) = Url::parse(value) {
        if parsed.scheme() == "file" {
            // API payloads must expose decoded URL paths instead of OS-native file paths,
            // so contracts stay stable across platforms.
            return percent_decode_path(parsed.path());
        }

        return None;
    }

    value.strip_prefix("file:").and_then(percent_decode_path)
}

fn percent_decode_path(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0usize;

    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hi = *bytes.get(index + 1)?;
            let lo = *bytes.get(index + 2)?;
            decoded.push((hex_value(hi)? << 4) | hex_value(lo)?);
            index += 3;
            continue;
        }

        decoded.push(bytes[index]);
        index += 1;
    }

    String::from_utf8(decoded).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let name = parts.next().unwrap_or_default();
        if name != key {
            return None;
        }
        Some(parts.next().unwrap_or_default())
    })
}

pub(crate) fn query_values<'a>(query: &'a str, key: &str) -> Vec<&'a str> {
    query
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let name = parts.next().unwrap_or_default();
            if name != key {
                return None;
            }
            Some(parts.next().unwrap_or_default())
        })
        .collect()
}

pub(crate) fn query_bool(query: &str, key: &str) -> bool {
    query_value(query, key)
        .map(|value| value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

pub(crate) fn to_domain_query_context(
    context: DiscoveryQueryContext,
) -> DomainDiscoveryQueryContext {
    DomainDiscoveryQueryContext {
        user_id: context.user_id.map(UserId::from),
        is_admin: context.is_admin,
        authorized_library_ids: context
            .authorized_library_ids
            .map(|ids| ids.into_iter().map(LibraryId::from).collect()),
        restrictions: context.restrictions,
    }
}

pub(crate) fn detail_access_denial_response(denial: DetailAccessDenial) -> Response {
    match denial {
        DetailAccessDenial::Unauthorized => StatusCode::UNAUTHORIZED.into_response(),
        DetailAccessDenial::Forbidden => StatusCode::FORBIDDEN.into_response(),
        DetailAccessDenial::NotFound => StatusCode::NOT_FOUND.into_response(),
        DetailAccessDenial::StorageFailure => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) fn validation_error_response(violations: Vec<ViolationDto>) -> Response {
    (
        StatusCode::BAD_REQUEST,
        [(header::CONTENT_TYPE, "application/json")],
        Json(ValidationErrorDto { violations }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::api_file_path;

    #[test]
    fn api_file_path_decodes_file_url_paths() {
        assert_eq!(
            api_file_path("file:/data/Library%20Root"),
            "/data/Library Root"
        );
    }

    #[test]
    fn api_file_path_preserves_non_file_or_invalid_values() {
        assert_eq!(api_file_path("/data/Library Root"), "/data/Library Root");
        assert_eq!(
            api_file_path("https://example.com/library/root"),
            "https://example.com/library/root"
        );
        assert_eq!(api_file_path("file:/tmp/%ZZ"), "file:/tmp/%ZZ");
    }
}

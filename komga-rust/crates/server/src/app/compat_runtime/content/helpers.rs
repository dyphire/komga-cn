use axum::http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use komga_domain::discovery::{
    AgeRestrictionKind as DomainAgeRestrictionKind, BookReadModel, DiscoveryError,
    DiscoveryQueryContext as DomainDiscoveryQueryContext, NonNativeRequestShape, PageEnvelope,
    QueryRestrictions as DomainQueryRestrictions,
};
use serde_json::{json, Value};

use crate::app::discovery_auth::{
    AgeRestrictionKind, DetailAccessDenial, DiscoveryQueryContext, QueryRestrictions,
};
use crate::app::CompatProfile;

use super::super::{
    ReadProgress, ReadProgressState, SEARCH_OWNERSHIP_HEADER, SHADOW_JAVA_WRITER_MARKER,
};

const NATIVE_OWNERSHIP_MARKER: &str = "native-rust-owned";

#[derive(Clone, Copy)]
pub(in crate::app::compat_runtime) enum DiscoveryShape {
    SeriesList,
    BooksList,
    BooksLatest,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(in crate::app::compat_runtime) enum DiscoveryOwnershipRoute {
    NativeOwned,
    LegacyCompat,
}

pub(in crate::app::compat_runtime) fn discovery_ownership_route(
    profile: CompatProfile,
    headers: &HeaderMap,
    _shape: DiscoveryShape,
) -> DiscoveryOwnershipRoute {
    if profile == CompatProfile::SnapshotAligned && has_native_ownership_marker(headers) {
        DiscoveryOwnershipRoute::NativeOwned
    } else {
        DiscoveryOwnershipRoute::LegacyCompat
    }
}

fn has_native_ownership_marker(headers: &HeaderMap) -> bool {
    headers
        .get(SEARCH_OWNERSHIP_HEADER)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == NATIVE_OWNERSHIP_MARKER)
}

pub(super) fn books_page_payload(
    page: PageEnvelope<BookReadModel>,
    is_admin: bool,
    paged: bool,
) -> Value {
    let content = page
        .content
        .iter()
        .map(|book| book_payload(book, is_admin))
        .collect::<Vec<_>>();
    let number_of_elements = content.len();
    let first = page.page == 0;
    let last = page.total_pages == 0 || page.page + 1 >= page.total_pages;
    let offset = if paged {
        page.page.saturating_mul(page.size)
    } else {
        0
    };

    json!({
        "content": content,
        "pageable": {
            "pageNumber": page.page,
            "pageSize": page.size,
            "sort": {
                "empty": false,
                "sorted": true,
                "unsorted": false
            },
            "offset": offset,
            "paged": paged,
            "unpaged": !paged
        },
        "last": last,
        "totalElements": page.total_elements,
        "totalPages": page.total_pages,
        "first": first,
        "size": page.size,
        "number": page.page,
        "sort": {
            "empty": false,
            "sorted": true,
            "unsorted": false
        },
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0
    })
}

fn book_payload(book: &BookReadModel, is_admin: bool) -> Value {
    let url = restricted_book_url(&book.url, is_admin);

    json!({
        "id": book.id,
        "seriesId": book.series_id,
        "seriesTitle": book.series_title,
        "libraryId": book.library_id,
        "name": book.title,
        "url": url,
        "number": 1,
        "created": book.created,
        "lastModified": book.last_modified,
        "fileLastModified": book.file_last_modified,
        "sizeBytes": book.size_bytes,
        "size": "0 B",
        "media": {
            "status": book.media_status,
            "mediaType": book.media_type,
            "pagesCount": book.media_pages_count,
            "comment": "",
            "epubDivinaCompatible": false,
            "epubIsKepub": false,
            "mediaProfile": ""
        },
        "metadata": {
            "title": book.title,
            "titleLock": false,
            "summary": "",
            "summaryLock": false,
            "number": "1",
            "numberLock": false,
            "numberSort": 1.0,
            "numberSortLock": false,
            "releaseDate": book.metadata_release_date,
            "releaseDateLock": false,
            "authors": [],
            "authorsLock": false,
            "tags": [],
            "tagsLock": false,
            "isbn": "",
            "isbnLock": false,
            "links": [],
            "linksLock": false,
            "created": book.created,
            "lastModified": book.last_modified
        },
        "readProgress": Value::Null,
        "deleted": book.deleted,
        "fileHash": "",
        "oneshot": book.oneshot
    })
}

pub(super) fn restricted_book_url(url: &str, is_admin: bool) -> String {
    if is_admin {
        return url.to_string();
    }

    url.rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or_default()
        .to_string()
}

pub(super) fn extract_full_text_search(payload: &Value) -> Option<String> {
    payload
        .get("fullTextSearch")
        .and_then(|value| value.as_str())
        .map(str::to_owned)
}

pub(super) fn wants_shadow_marker(headers: &HeaderMap, payload: Option<&Value>) -> bool {
    let ownership = payload
        .and_then(|payload| payload.get("ownership"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_ascii_lowercase());

    let requested_shadow_marker = headers
        .get(SEARCH_OWNERSHIP_HEADER)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == SHADOW_JAVA_WRITER_MARKER);

    let is_shadow_ownership = ownership.as_deref().is_some_and(|value| value == "shadow");

    is_shadow_ownership || requested_shadow_marker
}

pub(super) fn mark_non_native(response: &mut Response) {
    response.headers_mut().insert(
        HeaderName::from_static(SEARCH_OWNERSHIP_HEADER),
        HeaderValue::from_static(SHADOW_JAVA_WRITER_MARKER),
    );
}

pub(super) fn non_native_response(mut response: Response) -> Response {
    mark_non_native(&mut response);
    response
}

pub(in crate::app::compat_runtime) fn mark_native(response: &mut Response) {
    response.headers_mut().insert(
        HeaderName::from_static(SEARCH_OWNERSHIP_HEADER),
        HeaderValue::from_static(NATIVE_OWNERSHIP_MARKER),
    );
}

pub(super) fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let name = parts.next().unwrap_or_default();
        if name != key {
            return None;
        }
        Some(parts.next().unwrap_or_default())
    })
}

pub(super) fn query_values<'a>(query: &'a str, key: &str) -> Vec<&'a str> {
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

pub(super) fn query_bool(query: &str, key: &str) -> bool {
    query_value(query, key)
        .map(|value| value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

pub(super) fn query_has_key(query: &str, key: &str) -> bool {
    query
        .split('&')
        .any(|pair| pair.split('=').next().unwrap_or_default() == key)
}

pub(super) fn parse_search_regex(value: &str) -> Option<(String, String)> {
    let mut parts = value.splitn(2, ',');
    let pattern = parts.next()?.trim();
    let field = parts.next()?.trim().to_ascii_lowercase();
    if pattern.is_empty() || (field != "title" && field != "title_sort") {
        return None;
    }
    Some((pattern.to_string(), field))
}

pub(super) fn matches_search_pattern(candidate: &str, pattern: &str) -> bool {
    let text = candidate.to_ascii_lowercase();
    let mut expected = pattern.to_ascii_lowercase();

    let anchored_start = expected.starts_with('^');
    let anchored_end = expected.ends_with('$');
    if anchored_start {
        expected.remove(0);
    }
    if anchored_end {
        expected.pop();
    }

    if anchored_start && anchored_end {
        text == expected
    } else if anchored_start {
        text.starts_with(&expected)
    } else if anchored_end {
        text.ends_with(&expected)
    } else {
        text.contains(&expected)
    }
}

pub(super) fn apply_non_native_diagnostics(payload: &mut Value, error: &DiscoveryError) {
    let (reason, shape) = match error {
        DiscoveryError::NonNativeRequestShape(details) => {
            ("unsupported-request-shape", non_native_shape_label(details))
        }
        DiscoveryError::InvalidRequest(message) => ("invalid-request", message.clone()),
        DiscoveryError::Persistence(message) => ("persistence-error", message.clone()),
    };

    payload["_compat"] = json!({
        "discoveryOwnership": "non-native",
        "reason": reason,
        "shape": shape,
    });
}

fn non_native_shape_label(shape: &NonNativeRequestShape) -> String {
    match shape {
        NonNativeRequestShape::UnsupportedSeriesSort(value) => {
            format!("UnsupportedSeriesSort({value})")
        }
        NonNativeRequestShape::UnsupportedSeriesFilter(value) => {
            format!("UnsupportedSeriesFilter({value})")
        }
        NonNativeRequestShape::UnsupportedBookSort(value) => {
            format!("UnsupportedBookSort({value})")
        }
        NonNativeRequestShape::UnsupportedBookFilter(value) => {
            format!("UnsupportedBookFilter({value})")
        }
    }
}

pub(super) fn to_domain_query_context(
    context: DiscoveryQueryContext,
) -> DomainDiscoveryQueryContext {
    DomainDiscoveryQueryContext {
        user_id: context.user_id,
        is_admin: context.is_admin,
        authorized_library_ids: context.authorized_library_ids,
        restrictions: context.restrictions.map(to_domain_restrictions),
    }
}

fn to_domain_restrictions(restrictions: QueryRestrictions) -> DomainQueryRestrictions {
    DomainQueryRestrictions {
        age: restrictions.age,
        age_restriction: restrictions
            .age_restriction
            .map(to_domain_age_restriction_kind),
        labels_allow: restrictions.labels_allow,
        labels_exclude: restrictions.labels_exclude,
    }
}

fn to_domain_age_restriction_kind(kind: AgeRestrictionKind) -> DomainAgeRestrictionKind {
    match kind {
        AgeRestrictionKind::AllowOnly => DomainAgeRestrictionKind::AllowOnly,
        AgeRestrictionKind::Exclude => DomainAgeRestrictionKind::Exclude,
    }
}

pub(super) fn detail_access_denial_response(denial: DetailAccessDenial) -> Response {
    match denial {
        DetailAccessDenial::Unauthorized => StatusCode::UNAUTHORIZED.into_response(),
        DetailAccessDenial::Forbidden => StatusCode::FORBIDDEN.into_response(),
        DetailAccessDenial::NotFound => StatusCode::NOT_FOUND.into_response(),
    }
}

pub(super) fn invalid_read_progress_payload() -> Response {
    (
        StatusCode::BAD_REQUEST,
        [(header::CONTENT_TYPE, "application/json")],
        Json(json!({
            "error": "invalid read progress payload",
        })),
    )
        .into_response()
}

pub(super) fn invalid_progression_payload() -> Response {
    (
        StatusCode::BAD_REQUEST,
        [(header::CONTENT_TYPE, "application/json")],
        Json(json!({
            "error": "invalid progression payload",
        })),
    )
        .into_response()
}

pub(super) fn method_not_allowed_json_response(path: &str) -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        [(header::CONTENT_TYPE, "application/json")],
        Json(json!({
            "error": "Method Not Allowed",
            "message": "Method 'GET' is not supported.",
            "path": path,
            "status": 405,
            "timestamp": "1970-01-01T00:00:00.000+00:00",
            "trace": "org.springframework.web.HttpRequestMethodNotSupportedException: Request method 'GET' is not supported",
        })),
    )
        .into_response()
}

pub(super) fn set_read_progress(
    state: &ReadProgressState,
    token: String,
    book_id: String,
    _page: u64,
    _completed: bool,
) {
    let mut all_progress = state
        .progress_by_token
        .lock()
        .expect("read-progress state lock should not be poisoned");

    let user_progress = all_progress.entry(token).or_default();
    user_progress.insert(book_id, ReadProgress);
}

use axum::http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use komga_application::discovery::BookReadModel;
use komga_domain::common_ids::{LibraryId, UserId};
use komga_domain::discovery::{
    AgeRestrictionKind as DomainAgeRestrictionKind, DiscoveryError,
    DiscoveryQueryContext as DomainDiscoveryQueryContext, PageEnvelope,
    QueryRestrictions as DomainQueryRestrictions, UnsupportedDiscoverySemantics,
};
use serde_json::{json, Value};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::http::discovery_auth::{
    AgeRestrictionKind, DetailAccessDenial, DiscoveryQueryContext, QueryRestrictions,
};
use crate::http::state::RuntimeProfile;

use super::super::{
    ReadProgress, ReadProgressState, PERSISTED_OWNERSHIP_MARKER, SEARCH_OWNERSHIP_HEADER,
};

const RUNTIME_OWNERSHIP_MARKER: &str = "runtime-rust-owned";

#[derive(Clone, Copy)]
pub(crate) enum DiscoveryShape {
    SeriesList,
    BooksList,
    BooksLatest,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum DiscoveryOwnershipRoute {
    RuntimeOwned,
    PersistedOwned,
}

pub(crate) fn discovery_ownership_route(
    profile: RuntimeProfile,
    headers: &HeaderMap,
    _shape: DiscoveryShape,
) -> DiscoveryOwnershipRoute {
    if profile == RuntimeProfile::SnapshotAligned && has_runtime_ownership_marker(headers) {
        DiscoveryOwnershipRoute::RuntimeOwned
    } else {
        DiscoveryOwnershipRoute::PersistedOwned
    }
}

fn has_runtime_ownership_marker(headers: &HeaderMap) -> bool {
    headers
        .get(SEARCH_OWNERSHIP_HEADER)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == RUNTIME_OWNERSHIP_MARKER)
}

pub(crate) fn books_page_payload(
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
    let url = restricted_book_url(&book.name, is_admin);

    json!({
        "id": book.id,
        "seriesId": book.series_id,
        "seriesTitle": Value::Null,
        "libraryId": Value::Null,
        "name": book.name,
        "url": url,
        "number": 1,
        "created": Value::Null,
        "lastModified": Value::Null,
        "fileLastModified": Value::Null,
        "sizeBytes": 0,
        "size": "0 B",
        "media": {
            "status": Value::Null,
            "mediaType": Value::Null,
            "pagesCount": 0,
            "comment": "",
            "epubDivinaCompatible": false,
            "epubIsKepub": false,
            "mediaProfile": ""
        },
        "metadata": {
            "title": book.name,
            "titleLock": false,
            "summary": "",
            "summaryLock": false,
            "number": "1",
            "numberLock": false,
            "numberSort": 1.0,
            "numberSortLock": false,
            "releaseDate": Value::Null,
            "releaseDateLock": false,
            "authors": [],
            "authorsLock": false,
            "tags": [],
            "tagsLock": false,
            "isbn": "",
            "isbnLock": false,
            "links": [],
            "linksLock": false,
            "created": Value::Null,
            "lastModified": Value::Null
        },
        "readProgress": Value::Null,
        "deleted": false,
        "fileHash": "",
        "oneshot": false
    })
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

pub(crate) fn extract_full_text_search(payload: &Value) -> Option<String> {
    payload
        .get("fullTextSearch")
        .and_then(|value| value.as_str())
        .map(str::to_owned)
}

pub(crate) fn wants_persisted_marker(headers: &HeaderMap, payload: Option<&Value>) -> bool {
    let ownership = payload
        .and_then(|payload| payload.get("ownership"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_ascii_lowercase());

    let requested_persisted_marker = headers
        .get(SEARCH_OWNERSHIP_HEADER)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == PERSISTED_OWNERSHIP_MARKER);

    let is_persisted_ownership = ownership
        .as_deref()
        .is_some_and(|value| value == PERSISTED_OWNERSHIP_MARKER);

    is_persisted_ownership || requested_persisted_marker
}

pub(crate) fn mark_persisted_owned(response: &mut Response) {
    response.headers_mut().insert(
        HeaderName::from_static(SEARCH_OWNERSHIP_HEADER),
        HeaderValue::from_static(PERSISTED_OWNERSHIP_MARKER),
    );
}

pub fn mark_runtime_owned(response: &mut Response) {
    response.headers_mut().insert(
        HeaderName::from_static(SEARCH_OWNERSHIP_HEADER),
        HeaderValue::from_static(RUNTIME_OWNERSHIP_MARKER),
    );
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

pub(crate) fn parse_search_regex(value: &str) -> Option<(String, String)> {
    let mut parts = value.splitn(2, ',');
    let pattern = parts.next()?.trim();
    let field = parts.next()?.trim().to_ascii_lowercase();
    if pattern.is_empty() || (field != "title" && field != "title_sort") {
        return None;
    }
    Some((pattern.to_string(), field))
}

pub(crate) fn matches_search_pattern(candidate: &str, pattern: &str) -> bool {
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

pub(crate) fn apply_persisted_diagnostics(payload: &mut Value, error: &DiscoveryError) {
    let (reason, shape) = match error {
        DiscoveryError::UnsupportedSemantics(details) => {
            ("unsupported-semantics", unsupported_shape_label(details))
        }
        DiscoveryError::InvalidSemantics(message) => ("invalid-semantics", message.clone()),
        DiscoveryError::Persistence(message) => ("persistence-error", message.clone()),
    };

    payload["_diagnostics"] = json!({
        "discoveryOwnership": "persisted",
        "reason": reason,
        "shape": shape,
    });
}

fn unsupported_shape_label(shape: &UnsupportedDiscoverySemantics) -> String {
    match shape {
        UnsupportedDiscoverySemantics::UnsupportedSeriesSort(value) => {
            format!("UnsupportedSeriesSort({value})")
        }
        UnsupportedDiscoverySemantics::UnsupportedBookSort(value) => {
            format!("UnsupportedBookSort({value})")
        }
    }
}

pub fn to_domain_query_context(context: DiscoveryQueryContext) -> DomainDiscoveryQueryContext {
    DomainDiscoveryQueryContext {
        user_id: context.user_id.map(UserId::from),
        is_admin: context.is_admin,
        authorized_library_ids: context
            .authorized_library_ids
            .map(|ids| ids.into_iter().map(LibraryId::from).collect()),
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

pub(crate) fn detail_access_denial_response(denial: DetailAccessDenial) -> Response {
    match denial {
        DetailAccessDenial::Unauthorized => StatusCode::UNAUTHORIZED.into_response(),
        DetailAccessDenial::Forbidden => StatusCode::FORBIDDEN.into_response(),
        DetailAccessDenial::NotFound => StatusCode::NOT_FOUND.into_response(),
    }
}

pub(crate) fn invalid_read_progress_payload() -> Response {
    (
        StatusCode::BAD_REQUEST,
        [(header::CONTENT_TYPE, "application/json")],
        Json(json!({
            "error": "invalid read progress payload",
        })),
    )
        .into_response()
}

pub(crate) fn invalid_progression_payload() -> Response {
    (
        StatusCode::BAD_REQUEST,
        [(header::CONTENT_TYPE, "application/json")],
        Json(json!({
            "error": "invalid progression payload",
        })),
    )
        .into_response()
}

pub(crate) fn method_not_allowed_json_response(path: &str) -> Response {
    let timestamp = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "2000-01-01T00:00:00Z".to_string());
    (
        StatusCode::METHOD_NOT_ALLOWED,
        [(header::CONTENT_TYPE, "application/json")],
        Json(json!({
            "error": "Method Not Allowed",
            "message": "Request method is not supported.",
            "path": path,
            "status": 405,
            "timestamp": timestamp,
            "trace": "org.springframework.web.HttpRequestMethodNotSupportedException: Request method is not supported",
        })),
    )
        .into_response()
}

pub(crate) fn set_read_progress(
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

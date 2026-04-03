use crate::http::discovery_auth::{
    AgeRestrictionKind, DetailAccessDenial, DiscoveryQueryContext, QueryRestrictions,
};
use axum::Json;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use komga_application::discovery::BookReadModel;
use komga_domain::common_ids::{LibraryId, UserId};
use komga_domain::discovery::{
    AgeRestrictionKind as DomainAgeRestrictionKind,
    DiscoveryQueryContext as DomainDiscoveryQueryContext, PageEnvelope,
    QueryRestrictions as DomainQueryRestrictions,
};
use serde_json::{Value, json};

use super::super::{
    PERSISTED_OWNERSHIP_MARKER, ReadProgress, ReadProgressState, SEARCH_OWNERSHIP_HEADER,
};

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

pub(crate) fn contains_legacy_search_input(payload: &Value) -> bool {
    payload.get("regexSearch").is_some()
        || payload.get("searchRegex").is_some()
        || payload.get("search_regex").is_some()
}

pub(crate) fn contains_legacy_search_query(query: &str) -> bool {
    query_value(query, "regexSearch").is_some()
        || query_value(query, "searchRegex").is_some()
        || query_value(query, "search_regex").is_some()
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
        HeaderValue::from_static("runtime-rust-owned"),
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

pub(crate) fn set_read_progress(state: &ReadProgressState, token: String, book_id: String) {
    let mut all_progress = state
        .progress_by_token
        .lock()
        .expect("read-progress state lock should not be poisoned");

    let user_progress = all_progress.entry(token).or_default();
    user_progress.insert(book_id, ReadProgress);
}

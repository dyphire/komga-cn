use crate::http::discovery_auth::context::{
    DetailAccessDenial, DiscoveryQueryContext, QueryRestrictions,
};
use crate::http::discovery_auth::principal::AgeRestrictionKind;
use axum::Json;
use axum::http::{HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use komga_application::discovery::BookReadModel;
use komga_domain::common_ids::{LibraryId, UserId};
use komga_domain::discovery::{
    AgeRestrictionKind as DomainAgeRestrictionKind,
    DiscoveryQueryContext as DomainDiscoveryQueryContext, PageEnvelope,
    QueryRestrictions as DomainQueryRestrictions,
};
use reqwest::Url;
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::http::state::{ReadProgress, ReadProgressState};
use crate::SEARCH_OWNERSHIP_HEADER;

pub(crate) fn books_page_payload(
    page: PageEnvelope<BookReadModel>,
    is_admin: bool,
    paged: bool,
    sorted: bool,
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
                "empty": !sorted,
                "sorted": sorted,
                "unsorted": !sorted
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
            "empty": !sorted,
            "sorted": sorted,
            "unsorted": !sorted
        },
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0
    })
}

fn book_payload(book: &BookReadModel, is_admin: bool) -> Value {
    let url = restricted_book_url(&book.url, is_admin);
    let media_profile = media_profile_for_media_type(&book.media_type);

    json!({
        "id": book.id,
        "seriesId": book.series_id,
        "seriesTitle": book.series_title,
        "libraryId": book.library_id,
        "name": book.name,
        "url": url,
        "number": book.number,
        "created": normalized_date_time(&book.created),
        "lastModified": normalized_date_time(&book.last_modified),
        "fileLastModified": normalized_file_last_modified(&book.file_last_modified),
        "sizeBytes": book.size_bytes,
        "size": format_size_bytes(book.size_bytes),
        "media": {
            "status": book.media_status,
            "mediaType": book.media_type,
            "pagesCount": book.media_pages_count,
            "comment": book.media_comment,
            "epubDivinaCompatible": book.media_epub_divina_compatible,
            "epubIsKepub": book.media_epub_is_kepub,
            "mediaProfile": media_profile
        },
        "metadata": {
            "title": book.metadata_title,
            "titleLock": book.metadata_title_lock,
            "summary": book.metadata_summary,
            "summaryLock": book.metadata_summary_lock,
            "number": book.metadata_number,
            "numberLock": book.metadata_number_lock,
            "numberSort": book.metadata_number_sort,
            "numberSortLock": book.metadata_number_sort_lock,
            "releaseDate": book.metadata_release_date,
            "releaseDateLock": book.metadata_release_date_lock,
            "authors": book.metadata_authors.iter().map(|author| json!({ "name": author.name, "role": author.role })).collect::<Vec<_>>(),
            "authorsLock": book.metadata_authors_lock,
            "tags": book.metadata_tags,
            "tagsLock": book.metadata_tags_lock,
            "isbn": book.metadata_isbn,
            "isbnLock": book.metadata_isbn_lock,
            "links": book.metadata_links.iter().map(|link| json!({ "label": link.label, "url": link.url })).collect::<Vec<_>>(),
            "linksLock": book.metadata_links_lock,
            "created": normalized_date_time(&book.metadata_created),
            "lastModified": normalized_date_time(&book.metadata_last_modified)
        },
        "readProgress": book.read_progress.as_ref().map_or(Value::Null, |progress| json!({
            "page": progress.page,
            "completed": progress.completed,
            "readDate": normalized_optional_read_progress_date(progress.read_date.as_deref(), &progress.last_modified, &progress.created),
            "created": normalized_date_time(&progress.created),
            "lastModified": normalized_date_time(&progress.last_modified),
            "deviceId": progress.device_id,
            "deviceName": progress.device_name,
        })),
        "deleted": book.deleted,
        "fileHash": book.file_hash,
        "oneshot": book.oneshot
    })
}

pub(crate) fn normalized_file_last_modified(value: &str) -> String {
    if let Ok(epoch_seconds) = value.parse::<i64>()
        && let Ok(datetime) = OffsetDateTime::from_unix_timestamp(epoch_seconds)
        && let Ok(formatted) = datetime.format(&Rfc3339)
    {
        return formatted;
    }

    normalized_date_time(value)
}

pub(crate) fn normalized_date_time(value: &str) -> String {
    if let Ok(datetime) = OffsetDateTime::parse(value, &Rfc3339)
        && let Ok(formatted) = datetime.format(&Rfc3339)
    {
        return formatted;
    }

    if !value.is_empty() && !value.contains('T') && value.contains(' ') {
        let replaced = value.replacen(' ', "T", 1);
        return format!("{replaced}Z");
    }

    if !value.is_empty() && value.contains('T') && !value.ends_with('Z') && !value.contains('+') {
        return format!("{value}Z");
    }

    value.to_string()
}

pub(crate) fn normalized_optional_read_progress_date(
    read_date: Option<&str>,
    last_modified: &str,
    created: &str,
) -> String {
    let chosen = read_date
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            if !last_modified.trim().is_empty() {
                last_modified
            } else {
                created
            }
        });
    normalized_date_time(chosen)
}

pub(crate) fn api_file_path(value: &str) -> String {
    decode_file_url_path(value).unwrap_or_else(|| value.to_string())
}

fn format_size_bytes(size_bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];

    if size_bytes < 1024 {
        return format!("{size_bytes} B");
    }

    let mut size = size_bytes as f64;
    let mut unit_index = 0usize;
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if (size - size.round()).abs() < 0.05 {
        format!("{} {}", size.round() as u64, UNITS[unit_index])
    } else {
        format!("{size:.1} {}", UNITS[unit_index])
    }
}

fn media_profile_for_media_type(media_type: &str) -> &'static str {
    match media_type {
        "application/zip"
        | "application/x-rar-compressed"
        | "application/x-rar-compressed; version=4"
        | "application/x-rar-compressed; version=5" => "DIVINA",
        "application/epub+zip" => "EPUB",
        "application/pdf" => "PDF",
        _ => "",
    }
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

pub(crate) fn extract_full_text_search(payload: &Value) -> Option<String> {
    payload
        .get("fullTextSearch")
        .and_then(|value| value.as_str())
        .map(str::to_owned)
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

pub(crate) fn validation_error_response(violations: Vec<Value>) -> Response {
    (
        StatusCode::BAD_REQUEST,
        [(header::CONTENT_TYPE, "application/json")],
        Json(json!({
            "violations": violations,
        })),
    )
        .into_response()
}

pub(crate) fn read_progress_validation_error_response(violations: Vec<Value>) -> Response {
    validation_error_response(violations)
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

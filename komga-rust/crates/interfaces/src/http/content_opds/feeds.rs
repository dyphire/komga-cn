use axum::Json;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::http::identity_access::auth::require_auth;
use crate::http::request_urls::app_absolute_url;

use super::types::{PersistedBookFeedItem, PersistedSeries};

pub(super) fn opds_v1_navigation_feed_response(
    headers: &HeaderMap,
    feed_id: &str,
    title: &str,
    self_path: &str,
    entries: Vec<(String, String, String)>,
    pagination: Option<(usize, bool)>,
) -> Response {
    let self_href = app_absolute_url(headers, self_path);
    let start_href = app_absolute_url(headers, "/opds/v1.2/catalog");
    let now = opds_now_timestamp();

    let mut body = String::new();
    body.push_str("<feed xmlns=\"http://www.w3.org/2005/Atom\">");
    body.push_str(format!("<id>{}</id>", xml_escape(feed_id)).as_str());
    body.push_str(format!("<title>{}</title>", xml_escape(title)).as_str());
    body.push_str(format!("<updated>{}</updated><author><name>Komga</name><uri>https://github.com/gotson/komga</uri></author>", xml_escape(&now)).as_str());
    body.push_str(format!("<link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"self\" href=\"{}\"/>", xml_escape(&self_href)).as_str());
    body.push_str(format!("<link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"start\" href=\"{}\"/>", xml_escape(&start_href)).as_str());
    if let Some((page, has_next)) = pagination {
        if page > 0 {
            let previous_href = app_absolute_url(
                headers,
                format!("{self_path}?page={}", page.saturating_sub(1)).as_str(),
            );
            body.push_str(format!("<link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"previous\" href=\"{}\"/>", xml_escape(previous_href.as_str())).as_str());
        }
        if has_next {
            let next_href =
                app_absolute_url(headers, format!("{self_path}?page={}", page + 1).as_str());
            body.push_str(format!("<link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"next\" href=\"{}\"/>", xml_escape(next_href.as_str())).as_str());
        }
    }
    for (entry_id, entry_title, href_path) in entries {
        let href = app_absolute_url(headers, href_path.as_str());
        body.push_str(
            format!(
                "<entry><title>{}</title><updated>{}</updated><id>{}</id><content></content><link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"subsection\" href=\"{}\"/></entry>",
                xml_escape(&entry_title),
                xml_escape(&now),
                xml_escape(&entry_id),
                xml_escape(&href),
            )
            .as_str(),
        );
    }
    body.push_str("</feed>");

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/atom+xml"),
        )],
        body,
    )
        .into_response()
}

pub(super) fn opds_v1_library_series_feed_response(
    headers: &HeaderMap,
    feed_id: &str,
    title: &str,
    self_path: &str,
    series_entries: Vec<PersistedSeries>,
    feed_updated: Option<&str>,
    page: usize,
    has_next: bool,
) -> Response {
    let self_href = app_absolute_url(headers, self_path);
    let start_href = app_absolute_url(headers, "/opds/v1.2/catalog");
    let now = opds_now_timestamp();
    let feed_updated = feed_updated
        .filter(|value| !value.is_empty())
        .unwrap_or(now.as_str())
        .to_string();

    let mut body = String::new();
    body.push_str("<feed xmlns=\"http://www.w3.org/2005/Atom\">");
    body.push_str(format!("<id>{}</id>", xml_escape(feed_id)).as_str());
    body.push_str(format!("<title>{}</title>", xml_escape(title)).as_str());
    body.push_str(format!("<updated>{}</updated><author><name>Komga</name><uri>https://github.com/gotson/komga</uri></author>", xml_escape(feed_updated.as_str())).as_str());
    body.push_str(format!("<link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"self\" href=\"{}\"/>", xml_escape(&self_href)).as_str());
    body.push_str(format!("<link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"start\" href=\"{}\"/>", xml_escape(&start_href)).as_str());
    if page > 0 {
        let previous_href = app_absolute_url(
            headers,
            format!(
                "/opds/v1.2/libraries/{feed_id}?page={}",
                page.saturating_sub(1)
            )
            .as_str(),
        );
        body.push_str(format!("<link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"previous\" href=\"{}\"/>", xml_escape(previous_href.as_str())).as_str());
    }
    if has_next {
        let next_href = app_absolute_url(
            headers,
            format!("/opds/v1.2/libraries/{feed_id}?page={}", page + 1).as_str(),
        );
        body.push_str(format!("<link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"next\" href=\"{}\"/>", xml_escape(next_href.as_str())).as_str());
    }
    for entry in series_entries {
        let entry_updated = if entry.last_modified.is_empty() {
            now.clone()
        } else {
            entry.last_modified.clone()
        };
        let href = app_absolute_url(headers, format!("/opds/v1.2/series/{}", entry.id).as_str());
        body.push_str(
            format!(
                "<entry><title>{}</title><updated>{}</updated><id>{}</id><content></content><link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"subsection\" href=\"{}\"/></entry>",
                xml_escape(&entry.title),
                xml_escape(&entry_updated),
                xml_escape(&entry.id),
                xml_escape(&href),
            )
            .as_str(),
        );
    }
    body.push_str("</feed>");

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/atom+xml"),
        )],
        body,
    )
        .into_response()
}

pub(super) fn opds_v1_acquisition_feed_response(
    headers: &HeaderMap,
    feed_id: &str,
    title: &str,
    self_path: &str,
    books: Vec<PersistedBookFeedItem>,
    feed_updated: Option<&str>,
    pagination: Option<(usize, bool)>,
) -> Response {
    let self_href = app_absolute_url(headers, self_path);
    let start_href = app_absolute_url(headers, "/opds/v1.2/catalog");
    let now = opds_now_timestamp();
    let feed_updated = feed_updated
        .filter(|value| !value.is_empty())
        .unwrap_or(now.as_str())
        .to_string();

    let mut body = String::new();
    body.push_str("<feed xmlns=\"http://www.w3.org/2005/Atom\">");
    body.push_str(format!("<id>{}</id>", xml_escape(feed_id)).as_str());
    body.push_str(format!("<title>{}</title>", xml_escape(title)).as_str());
    body.push_str(format!("<updated>{}</updated><author><name>Komga</name><uri>https://github.com/gotson/komga</uri></author>", xml_escape(feed_updated.as_str())).as_str());
    body.push_str(format!("<link type=\"application/atom+xml;profile=opds-catalog;kind=acquisition\" rel=\"self\" href=\"{}\"/>", xml_escape(&self_href)).as_str());
    body.push_str(format!("<link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"start\" href=\"{}\"/>", xml_escape(&start_href)).as_str());
    if let Some((page, has_next)) = pagination {
        if page > 0 {
            let previous_href = app_absolute_url(
                headers,
                format!("{self_path}?page={}", page.saturating_sub(1)).as_str(),
            );
            body.push_str(format!("<link type=\"application/atom+xml;profile=opds-catalog;kind=acquisition\" rel=\"previous\" href=\"{}\"/>", xml_escape(previous_href.as_str())).as_str());
        }
        if has_next {
            let next_href =
                app_absolute_url(headers, format!("{self_path}?page={}", page + 1).as_str());
            body.push_str(format!("<link type=\"application/atom+xml;profile=opds-catalog;kind=acquisition\" rel=\"next\" href=\"{}\"/>", xml_escape(next_href.as_str())).as_str());
        }
    }

    for book in books {
        let entry_updated = if book.last_modified.is_empty() {
            now.clone()
        } else {
            book.last_modified.clone()
        };
        let book_href = app_absolute_url(
            headers,
            format!(
                "/opds/v1.2/books/{}/file/{}",
                book.id,
                query_escape(book.file_name.as_str()),
            )
            .as_str(),
        );
        let thumb_href = app_absolute_url(
            headers,
            format!("/opds/v1.2/books/{}/thumbnail/small", book.id).as_str(),
        );
        let image_href = app_absolute_url(
            headers,
            format!("/opds/v1.2/books/{}/thumbnail", book.id).as_str(),
        );
        body.push_str(
            format!(
                "<entry><title>{}</title><updated>{}</updated><id>{}</id><content></content><link type=\"{}\" rel=\"http://opds-spec.org/acquisition\" href=\"{}\"/><link type=\"image/jpeg\" rel=\"http://opds-spec.org/image/thumbnail\" href=\"{}\"/><link type=\"image/jpeg\" rel=\"http://opds-spec.org/image\" href=\"{}\"/></entry>",
                xml_escape(&book.title),
                xml_escape(&entry_updated),
                xml_escape(&book.id),
                xml_escape(&book.media_type),
                xml_escape(&book_href),
                xml_escape(&thumb_href),
                xml_escape(&image_href),
            )
            .as_str(),
        );
    }

    body.push_str("</feed>");

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/atom+xml"),
        )],
        body,
    )
        .into_response()
}

pub(super) fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub(super) fn query_escape(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            b' ' => "%20".to_string(),
            _ => format!("%{:02X}", byte),
        })
        .collect::<String>()
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

pub(super) fn query_values(query: &str, key: &str) -> Vec<String> {
    query
        .split('&')
        .filter_map(|segment| {
            let mut parts = segment.splitn(2, '=');
            let name = parts.next().unwrap_or_default();
            if name != key {
                return None;
            }
            let value = parts.next().unwrap_or_default();
            if value.is_empty() {
                None
            } else {
                Some(percent_decode(value))
            }
        })
        .collect()
}

pub(super) fn parse_page_size(query: &str) -> (usize, usize) {
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .clamp(1, 100);
    (page, size)
}

pub(super) fn paginate_vec<T>(items: Vec<T>, page: usize, size: usize) -> (Vec<T>, bool) {
    let start = page.saturating_mul(size);
    let end = start.saturating_add(size);
    if start >= items.len() {
        return (Vec::new(), false);
    }
    let has_next = end < items.len();
    let page_items = items.into_iter().skip(start).take(size).collect::<Vec<_>>();
    (page_items, has_next)
}

pub(super) fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hi = bytes[index + 1] as char;
            let lo = bytes[index + 2] as char;
            let parsed = hi
                .to_digit(16)
                .and_then(|hi| lo.to_digit(16).map(|lo| ((hi << 4) | lo) as u8));
            if let Some(byte) = parsed {
                decoded.push(byte);
                index += 3;
                continue;
            }
        }
        if bytes[index] == b'+' {
            decoded.push(b' ');
        } else {
            decoded.push(bytes[index]);
        }
        index += 1;
    }
    String::from_utf8(decoded).unwrap_or_else(|_| value.to_string())
}

pub(super) fn opds_now_timestamp() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "2000-01-01T00:00:00Z".to_string())
}

pub(super) fn redirect_to_opds_v2(headers: HeaderMap, target_path: &str) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let location = app_absolute_url(&headers, target_path);
    (
        StatusCode::FOUND,
        [(
            header::LOCATION,
            HeaderValue::from_str(&location)
                .unwrap_or_else(|_| HeaderValue::from_static("/opds/v2/catalog")),
        )],
    )
        .into_response()
}

pub(super) fn opds_subsection_navigation_link(
    headers: &HeaderMap,
    title: &str,
    path: &str,
) -> Value {
    json!({
        "title": title,
        "rel": "subsection",
        "href": app_absolute_url(headers, path),
        "type": "application/opds+json",
    })
}

pub(super) fn opds_navigation_link(headers: &HeaderMap, title: &str, path: &str) -> Value {
    json!({
        "title": title,
        "href": app_absolute_url(headers, path),
        "type": "application/opds+json",
    })
}

pub(super) fn opds_navigation_response(
    headers: &HeaderMap,
    title: &str,
    self_href: &str,
    navigation: Vec<Value>,
    _modified: Option<&str>,
) -> Response {
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(json!({
            "metadata": {
                "title": title,
            },
            "links": [
                {
                    "rel": "self",
                    "href": self_href,
                    "type": "application/opds+json",
                },
                {
                    "rel": "start",
                    "href": app_absolute_url(headers, "/opds/v2/catalog"),
                    "type": "application/opds+json",
                },
                {
                    "rel": "search",
                    "href": app_absolute_url(headers, "/opds/v2/search{?query}"),
                    "type": "application/opds+json",
                    "templated": true,
                }
            ],
            "navigation": navigation,
        })),
    )
        .into_response()
}

pub(super) fn opds_publications_response(
    headers: &HeaderMap,
    title: &str,
    self_href: &str,
    publications: Vec<Value>,
) -> Response {
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(json!({
            "metadata": {
                "title": title,
            },
            "links": [
                {
                    "rel": "self",
                    "href": self_href,
                    "type": "application/opds+json",
                },
                {
                    "rel": "start",
                    "href": app_absolute_url(headers, "/opds/v2/catalog"),
                    "type": "application/opds+json",
                },
                {
                    "rel": "search",
                    "href": app_absolute_url(headers, "/opds/v2/search{?query}"),
                    "type": "application/opds+json",
                    "templated": true,
                }
            ],
            "publications": publications,
        })),
    )
        .into_response()
}

pub(super) fn opds_publication_for_book(
    headers: &HeaderMap,
    book_id: &str,
    title: &str,
    media_type: &str,
) -> Value {
    json!({
        "metadata": {
            "title": title,
        },
        "links": [
            {
                "rel": "self",
                "href": app_absolute_url(headers, format!("/opds/v2/books/{book_id}/manifest").as_str()),
                "type": "application/opds-publication+json",
            },
            {
                "rel": "http://opds-spec.org/acquisition",
                "href": app_absolute_url(headers, format!("/opds/v2/books/{book_id}/file").as_str()),
                "type": media_type,
            },
            {
                "rel": "http://opds-spec.org/image/thumbnail",
                "href": app_absolute_url(headers, format!("/opds/v2/books/{book_id}/thumbnail/small").as_str()),
                "type": "image/jpeg",
            }
        ],
    })
}

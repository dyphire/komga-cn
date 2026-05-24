#![allow(clippy::too_many_arguments)]

use axum::Json;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use time::format_description::well_known::Rfc3339;
use time::{OffsetDateTime, UtcOffset};

use crate::request_urls::app_absolute_url;
use crate::state::{OpdsBookAuthorEntry, OpdsBookFeedEntry};

use super::types::PersistedSeries;
use super::xml_renderer::{
    OpdsV1AcquisitionFeedDocument, OpdsV1AcquisitionFeedEntry as OpdsV1XmlAcquisitionFeedEntry,
    OpdsV1NavigationFeedDocument, OpdsV1NavigationFeedEntry as OpdsV1XmlNavigationFeedEntry,
    render_opds_v1_acquisition_feed, render_opds_v1_navigation_feed,
};
pub(super) use super::xml_renderer::{OpdsV1XmlLink, xml_escape};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct OpdsV1NavigationEntry {
    pub id: String,
    pub title: String,
    pub content: String,
    pub href_path: String,
    pub updated: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct OpdsV1AcquisitionEntry {
    pub id: String,
    pub title: String,
    pub updated: Option<String>,
    pub content: String,
    pub authors: Vec<String>,
    pub acquisition_media_type: String,
    pub acquisition_href_path: String,
    pub thumbnail_href_path: String,
    pub image_href_path: String,
    pub extra_links: Vec<OpdsV1XmlLink>,
}

pub(super) fn opds_v1_navigation_feed_response(
    headers: &HeaderMap,
    feed_id: &str,
    title: &str,
    self_path: &str,
    entries: Vec<OpdsV1NavigationEntry>,
    pagination: Option<(usize, bool)>,
) -> Response {
    opds_v1_navigation_feed_response_with_feed_updated(
        headers, feed_id, title, self_path, entries, None, pagination,
    )
}

pub(super) fn opds_v1_navigation_feed_response_with_feed_updated(
    headers: &HeaderMap,
    feed_id: &str,
    title: &str,
    self_path: &str,
    entries: Vec<OpdsV1NavigationEntry>,
    feed_updated: Option<&str>,
    pagination: Option<(usize, bool)>,
) -> Response {
    opds_v1_navigation_feed_response_with_extra_links(
        headers,
        feed_id,
        title,
        self_path,
        entries,
        feed_updated,
        pagination,
        Vec::new(),
    )
}

pub(super) fn opds_v1_navigation_feed_response_with_extra_links(
    headers: &HeaderMap,
    feed_id: &str,
    title: &str,
    self_path: &str,
    entries: Vec<OpdsV1NavigationEntry>,
    feed_updated: Option<&str>,
    pagination: Option<(usize, bool)>,
    extra_links: Vec<OpdsV1XmlLink>,
) -> Response {
    let self_href = app_absolute_url(headers, self_path);
    let start_href = app_absolute_url(headers, "/opds/v1.2/catalog");
    let now = opds_now_timestamp();
    let feed_updated = feed_updated
        .filter(|value| !value.is_empty())
        .map(normalize_opds_updated)
        .unwrap_or_else(|| now.clone());
    let (previous_href, next_href) = navigation_paging_hrefs(headers, self_path, pagination);

    atom_xml_response(render_opds_v1_navigation_feed(
        OpdsV1NavigationFeedDocument {
            id: feed_id.to_string(),
            title: title.to_string(),
            updated: feed_updated,
            self_href,
            start_href,
            previous_href,
            next_href,
            extra_links,
            entries: entries
                .into_iter()
                .map(|entry| OpdsV1XmlNavigationFeedEntry {
                    id: entry.id,
                    title: entry.title,
                    updated: entry
                        .updated
                        .as_deref()
                        .filter(|value| !value.is_empty())
                        .map(normalize_opds_updated)
                        .unwrap_or_else(|| now.clone()),
                    content: entry.content,
                    href: app_absolute_url(headers, entry.href_path.as_str()),
                })
                .collect(),
        },
    ))
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
    let previous_href = (page > 0).then(|| {
        app_absolute_url(
            headers,
            format!(
                "/opds/v1.2/libraries/{feed_id}?page={}",
                page.saturating_sub(1)
            )
            .as_str(),
        )
    });
    let next_href = has_next.then(|| {
        app_absolute_url(
            headers,
            format!("/opds/v1.2/libraries/{feed_id}?page={}", page + 1).as_str(),
        )
    });

    atom_xml_response(render_opds_v1_navigation_feed(
        OpdsV1NavigationFeedDocument {
            id: feed_id.to_string(),
            title: title.to_string(),
            updated: feed_updated,
            self_href,
            start_href,
            previous_href,
            next_href,
            extra_links: Vec::new(),
            entries: series_entries
                .into_iter()
                .map(|entry| OpdsV1XmlNavigationFeedEntry {
                    updated: if entry.last_modified.is_empty() {
                        now.clone()
                    } else {
                        entry.last_modified.clone()
                    },
                    href: app_absolute_url(
                        headers,
                        format!("/opds/v1.2/series/{}", entry.id).as_str(),
                    ),
                    id: entry.id,
                    title: entry.title,
                    content: String::new(),
                })
                .collect(),
        },
    ))
}

pub(super) fn opds_v1_acquisition_feed_response_with_entries(
    headers: &HeaderMap,
    feed_id: &str,
    title: &str,
    self_path: &str,
    entries: Vec<OpdsV1AcquisitionEntry>,
    feed_updated: Option<&str>,
    pagination: Option<(usize, bool)>,
) -> Response {
    let self_href = app_absolute_url(headers, self_path);
    let start_href = app_absolute_url(headers, "/opds/v1.2/catalog");
    let now = opds_now_timestamp();
    let feed_updated = feed_updated
        .filter(|value| !value.is_empty())
        .map(normalize_opds_updated)
        .unwrap_or(now.clone());
    let (previous_href, next_href) = navigation_paging_hrefs(headers, self_path, pagination);

    atom_xml_response(render_opds_v1_acquisition_feed(
        OpdsV1AcquisitionFeedDocument {
            id: feed_id.to_string(),
            title: title.to_string(),
            updated: feed_updated,
            self_href,
            start_href,
            previous_href,
            next_href,
            entries: entries
                .into_iter()
                .map(|entry| OpdsV1XmlAcquisitionFeedEntry {
                    id: entry.id,
                    title: entry.title,
                    updated: entry
                        .updated
                        .as_deref()
                        .filter(|value| !value.is_empty())
                        .map(normalize_opds_updated)
                        .unwrap_or_else(|| now.clone()),
                    content: entry.content,
                    authors: entry.authors,
                    acquisition_media_type: entry.acquisition_media_type,
                    acquisition_href: app_absolute_url(
                        headers,
                        entry.acquisition_href_path.as_str(),
                    ),
                    thumbnail_href: app_absolute_url(headers, entry.thumbnail_href_path.as_str()),
                    image_href: app_absolute_url(headers, entry.image_href_path.as_str()),
                    extra_links: entry.extra_links,
                })
                .collect(),
        },
    ))
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

fn atom_xml_response(body: String) -> Response {
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

fn navigation_paging_hrefs(
    headers: &HeaderMap,
    self_path: &str,
    pagination: Option<(usize, bool)>,
) -> (Option<String>, Option<String>) {
    let Some((page, has_next)) = pagination else {
        return (None, None);
    };

    let previous_href = (page > 0).then(|| {
        app_absolute_url(
            headers,
            page_link_path(self_path, page.saturating_sub(1)).as_str(),
        )
    });
    let next_href =
        has_next.then(|| app_absolute_url(headers, page_link_path(self_path, page + 1).as_str()));

    (previous_href, next_href)
}

fn page_link_path(self_path: &str, page: usize) -> String {
    if self_path.contains('?') {
        format!("{self_path}&page={page}")
    } else {
        format!("{self_path}?page={page}")
    }
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
        .max(1);
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
    let now_utc = OffsetDateTime::now_utc();
    let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    format_opds_timestamp(now_utc, offset)
}

pub(super) fn normalize_opds_updated(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return opds_now_timestamp();
    }
    if OffsetDateTime::parse(trimmed, &Rfc3339).is_ok() {
        return trimmed.to_string();
    }
    if let Some((date, time)) = trimmed.split_once(' ') {
        return format!("{date}T{time}Z");
    }
    if trimmed.contains('T') {
        return format!("{trimmed}Z");
    }
    trimmed.to_string()
}

fn format_opds_timestamp(now_utc: OffsetDateTime, offset: UtcOffset) -> String {
    now_utc
        .to_offset(offset)
        .format(&Rfc3339)
        .unwrap_or_else(|_| "2000-01-01T00:00:00Z".to_string())
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

pub(super) fn opds_navigation_response_with_paging(
    headers: &HeaderMap,
    title: &str,
    self_path: &str,
    modified: Option<&str>,
    navigation: Vec<Value>,
    page: usize,
    size: usize,
    total: usize,
) -> Response {
    let self_href = app_absolute_url(headers, self_path);
    let modified = modified
        .filter(|value| !value.is_empty())
        .map(normalize_opds_updated)
        .unwrap_or_else(opds_now_timestamp);

    let mut links = vec![
        json!({
            "rel": "self",
            "href": self_href,
        }),
        json!({
            "title": "Home",
            "rel": "start",
            "href": app_absolute_url(headers, "/opds/v2/catalog"),
            "type": "application/opds+json",
        }),
        json!({
            "title": "Search",
            "rel": "search",
            "href": app_absolute_url(headers, "/opds/v2/search{?query}"),
            "type": "application/opds+json",
            "templated": true,
        }),
    ];

    if page > 0 {
        links.push(json!({
            "rel": "previous",
            "href": app_absolute_url(headers, page_link_path(self_path, page.saturating_sub(1)).as_str()),
        }));
    }
    if page.saturating_add(1).saturating_mul(size) < total {
        links.push(json!({
            "rel": "next",
            "href": app_absolute_url(headers, page_link_path(self_path, page + 1).as_str()),
        }));
    }

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(json!({
            "metadata": {
                "title": title,
                "modified": modified,
                "itemsPerPage": size,
                "currentPage": page + 1,
                "numberOfItems": total,
            },
            "links": links,
            "navigation": navigation,
        })),
    )
        .into_response()
}

pub(super) fn opds_publications_response_with_paging(
    headers: &HeaderMap,
    title: &str,
    self_path: &str,
    modified: Option<&str>,
    publications: Vec<Value>,
    page: usize,
    size: usize,
    total: usize,
) -> Response {
    let self_href = app_absolute_url(headers, self_path);
    let modified = modified
        .filter(|value| !value.is_empty())
        .map(normalize_opds_updated)
        .unwrap_or_else(opds_now_timestamp);

    let mut links = vec![
        json!({
            "rel": "self",
            "href": self_href,
        }),
        json!({
            "title": "Home",
            "rel": "start",
            "href": app_absolute_url(headers, "/opds/v2/catalog"),
            "type": "application/opds+json",
        }),
        json!({
            "title": "Search",
            "rel": "search",
            "href": app_absolute_url(headers, "/opds/v2/search{?query}"),
            "type": "application/opds+json",
            "templated": true,
        }),
    ];

    if page > 0 {
        links.push(json!({
            "rel": "previous",
            "href": app_absolute_url(headers, page_link_path(self_path, page.saturating_sub(1)).as_str()),
        }));
    }
    if page.saturating_add(1).saturating_mul(size) < total {
        links.push(json!({
            "rel": "next",
            "href": app_absolute_url(headers, page_link_path(self_path, page + 1).as_str()),
        }));
    }

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(json!({
            "metadata": {
                "title": title,
                "modified": modified,
                "itemsPerPage": size,
                "currentPage": page + 1,
                "numberOfItems": total,
            },
            "links": links,
            "publications": publications,
        })),
    )
        .into_response()
}

pub(super) fn opds_publication_for_feed_entry(
    headers: &HeaderMap,
    book: &OpdsBookFeedEntry,
) -> Value {
    let auth_href = app_absolute_url(headers, "/opds/v2/auth");
    let manifest_href = app_absolute_url(
        headers,
        format!("/opds/v2/books/{}/manifest", book.id).as_str(),
    );
    let file_href = app_absolute_url(headers, format!("/opds/v2/books/{}/file", book.id).as_str());
    let progression_href = app_absolute_url(
        headers,
        format!("/opds/v2/books/{}/progression", book.id).as_str(),
    );
    let thumbnail_href = app_absolute_url(
        headers,
        format!("/opds/v2/books/{}/thumbnail", book.id).as_str(),
    );

    let mut metadata = serde_json::Map::new();
    metadata.insert("title".to_string(), Value::String(book.title.clone()));
    if let Some(isbn) = book.isbn.as_ref().filter(|value| !value.is_empty()) {
        metadata.insert(
            "identifier".to_string(),
            Value::String(format!("urn:isbn:{isbn}")),
        );
    }
    if !book.summary.is_empty() {
        metadata.insert(
            "description".to_string(),
            Value::String(book.summary.clone()),
        );
    }
    if book.page_count > 0 {
        metadata.insert(
            "numberOfPages".to_string(),
            Value::Number(book.page_count.into()),
        );
    }
    if let Some(release_date) = book.release_date.as_ref().filter(|value| !value.is_empty()) {
        metadata.insert("published".to_string(), Value::String(release_date.clone()));
    }
    if !book.last_modified.is_empty() {
        metadata.insert(
            "modified".to_string(),
            Value::String(normalize_opds_updated(&book.last_modified)),
        );
    }
    if !book.tags.is_empty() {
        metadata.insert(
            "subject".to_string(),
            Value::Array(
                book.tags
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect::<Vec<_>>(),
            ),
        );
    }
    extend_metadata_with_role_authors(&mut metadata, &book.authors);
    if !book.series_id.is_empty() && !book.series_title.is_empty() {
        let mut series_entry = serde_json::Map::new();
        series_entry.insert("name".to_string(), Value::String(book.series_title.clone()));
        if let Some(number) = serde_json::Number::from_f64(book.number_sort) {
            series_entry.insert("position".to_string(), Value::Number(number));
        }
        series_entry.insert(
            "links".to_string(),
            Value::Array(vec![json!({
                "href": app_absolute_url(headers, format!("/opds/v2/series/{}", book.series_id).as_str()),
                "type": "application/opds+json",
            })]),
        );
        metadata.insert(
            "belongsTo".to_string(),
            Value::Object(serde_json::Map::from_iter([(
                "series".to_string(),
                Value::Array(vec![Value::Object(series_entry)]),
            )])),
        );
    }

    let mut links = vec![
        json!({
            "rel": "self",
            "href": manifest_href,
            "type": publication_manifest_type(book.media_type.as_str()),
            "properties": {
                "authenticate": {
                    "href": auth_href.as_str(),
                    "type": "application/opds-authentication+json",
                },
            },
        }),
        json!({
            "rel": "http://opds-spec.org/acquisition",
            "href": file_href,
            "type": book.media_type,
            "properties": {
                "authenticate": {
                    "href": auth_href.as_str(),
                    "type": "application/opds-authentication+json",
                },
            },
        }),
        json!({
            "rel": "http://www.cantook.com/api/progression",
            "href": progression_href,
            "type": "application/vnd.readium.progression+json",
            "properties": {
                "authenticate": {
                    "href": auth_href.as_str(),
                    "type": "application/opds-authentication+json",
                },
            },
        }),
    ];

    if book.media_type == "application/pdf"
        || (book.media_type == "application/epub+zip" && book.epub_divina_compatible)
    {
        links.push(json!({
            "href": app_absolute_url(headers, format!("/opds/v2/books/{}/manifest/divina", book.id).as_str()),
            "type": "application/divina+json",
            "properties": {
                "authenticate": {
                    "href": auth_href.as_str(),
                    "type": "application/opds-authentication+json",
                },
            },
        }));
    }

    json!({
        "@context": "https://readium.org/webpub-manifest/context.jsonld",
        "metadata": Value::Object(metadata),
        "links": links,
        "images": [
            {
                "href": thumbnail_href,
                "type": "image/jpeg",
                "properties": {
                    "authenticate": {
                        "href": auth_href.as_str(),
                        "type": "application/opds-authentication+json",
                    },
                },
            }
        ],
    })
}

fn publication_manifest_type(media_type: &str) -> &'static str {
    match media_type {
        media if media.starts_with("image/") => "application/divina+json",
        "application/vnd.comicbook+zip" | "application/vnd.comicbook-rar" | "application/zip" => {
            "application/divina+json"
        }
        _ => "application/webpub+json",
    }
}

fn extend_metadata_with_role_authors(
    metadata: &mut serde_json::Map<String, Value>,
    authors: &[OpdsBookAuthorEntry],
) {
    let mut author = Vec::new();
    let mut translator = Vec::new();
    let mut editor = Vec::new();
    let mut artist = Vec::new();
    let mut illustrator = Vec::new();
    let mut letterer = Vec::new();
    let mut penciler = Vec::new();
    let mut colorist = Vec::new();
    let mut inker = Vec::new();
    let mut contributor = Vec::new();

    for entry in authors {
        let target = match entry.role.as_str() {
            "author" => &mut author,
            "translator" => &mut translator,
            "editor" => &mut editor,
            "artist" => &mut artist,
            "illustrator" => &mut illustrator,
            "letterer" => &mut letterer,
            "penciler" | "penciller" => &mut penciler,
            "colorist" => &mut colorist,
            "inker" => &mut inker,
            _ => &mut contributor,
        };
        target.push(Value::String(entry.name.clone()));
    }

    for (key, values) in [
        ("author", author),
        ("translator", translator),
        ("editor", editor),
        ("artist", artist),
        ("illustrator", illustrator),
        ("letterer", letterer),
        ("penciler", penciler),
        ("colorist", colorist),
        ("inker", inker),
        ("contributor", contributor),
    ] {
        if !values.is_empty() {
            metadata.insert(key.to_string(), Value::Array(values));
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use axum::http::HeaderMap;
    use time::{Month, OffsetDateTime, UtcOffset};

    use super::{OpdsV1NavigationEntry, format_opds_timestamp, opds_v1_navigation_feed_response};

    #[tokio::test]
    async fn navigation_feed_uses_entry_specific_updated_timestamp() {
        let response = opds_v1_navigation_feed_response(
            &HeaderMap::new(),
            "feed",
            "Feed",
            "/opds/v1.2/feed",
            vec![OpdsV1NavigationEntry {
                id: "entry-1".to_string(),
                title: "Entry".to_string(),
                content: "".to_string(),
                href_path: "/opds/v1.2/entry-1".to_string(),
                updated: Some("2024-01-02T03:04:05Z".to_string()),
            }],
            None,
        );

        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        let body = String::from_utf8(bytes.to_vec()).expect("feed body should be utf-8");

        assert!(body.contains("<updated>2024-01-02T03:04:05Z</updated>"));
    }

    #[test]
    fn parse_page_size_does_not_cap_large_requested_size() {
        assert_eq!(super::parse_page_size("page=2&size=250"), (2, 250));
    }

    #[test]
    fn opds_now_timestamp_uses_local_offset_format() {
        let base = OffsetDateTime::from_unix_timestamp(0)
            .expect("unix epoch should be valid")
            .replace_date(time::Date::from_calendar_date(2024, Month::March, 3).expect("date"))
            .replace_time(time::Time::from_hms(0, 0, 0).expect("time"));
        let utc = base.to_offset(UtcOffset::UTC);
        let formatted = format_opds_timestamp(utc, UtcOffset::from_hms(9, 0, 0).expect("offset"));
        assert_eq!(formatted, "2024-03-03T09:00:00+09:00");
    }
}

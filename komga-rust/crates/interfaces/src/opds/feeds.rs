use axum::Json;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use time::format_description::well_known::Rfc3339;
use time::{OffsetDateTime, UtcOffset};

use komga_application::opds::{OpdsBookAuthorEntry, OpdsBookFeedEntry};

use crate::contracts::opds::{
    OpdsV2AuthenticationLinkDto, OpdsV2BelongsToDto, OpdsV2FeedMetadataDto, OpdsV2LinkDto,
    OpdsV2LinkPropertiesDto, OpdsV2NavigationFeedDto, OpdsV2PublicationDto,
    OpdsV2PublicationFeedDto, OpdsV2PublicationMetadataDto, OpdsV2PublicationSeriesDto,
    OpdsV2UpdatedDto,
};
use crate::request_urls::app_absolute_url;

use super::types::PersistedSeries;
use super::xml_renderer::{
    OpdsV1AcquisitionFeedDocument, OpdsV1AcquisitionFeedEntry as OpdsV1XmlAcquisitionFeedEntry,
    OpdsV1NavigationFeedDocument, OpdsV1NavigationFeedEntry as OpdsV1XmlNavigationFeedEntry,
    render_opds_v1_acquisition_feed, render_opds_v1_navigation_feed,
};
pub(super) use super::xml_renderer::{OpdsV1XmlLink, xml_escape};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct OpdsV1NavigationEntry {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) content: String,
    pub(super) href_path: String,
    pub(super) updated: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct OpdsV1AcquisitionEntry {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) updated: Option<String>,
    pub(super) content: String,
    pub(super) authors: Vec<String>,
    pub(super) acquisition_media_type: String,
    pub(super) acquisition_href_path: String,
    pub(super) thumbnail_href_path: String,
    pub(super) image_href_path: String,
    pub(super) extra_links: Vec<OpdsV1XmlLink>,
}

pub(super) struct OpdsV1FeedHeader<'a> {
    pub(super) headers: &'a HeaderMap,
    pub(super) feed_id: &'a str,
    pub(super) title: &'a str,
    pub(super) self_path: &'a str,
    pub(super) feed_updated: Option<&'a str>,
    pub(super) pagination: Option<OpdsPageNavigation>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct OpdsPageRequest {
    pub(super) page: usize,
    pub(super) size: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct OpdsPageNavigation {
    pub(super) page: usize,
    pub(super) has_next: bool,
}

pub(super) struct OpdsPagedItems<T> {
    pub(super) items: Vec<T>,
    pub(super) has_next: bool,
}

pub(super) fn opds_v1_navigation_feed_response(
    feed: OpdsV1FeedHeader<'_>,
    entries: Vec<OpdsV1NavigationEntry>,
) -> Response {
    opds_v1_navigation_feed_response_with_extra_links(feed, entries, Vec::new())
}

pub(super) fn opds_v1_navigation_feed_response_with_extra_links(
    feed: OpdsV1FeedHeader<'_>,
    entries: Vec<OpdsV1NavigationEntry>,
    extra_links: Vec<OpdsV1XmlLink>,
) -> Response {
    let self_href = app_absolute_url(feed.headers, feed.self_path);
    let start_href = app_absolute_url(feed.headers, "/opds/v1.2/catalog");
    let now = opds_now_timestamp();
    let feed_updated = feed
        .feed_updated
        .filter(|value| !value.is_empty())
        .map(normalize_opds_updated)
        .unwrap_or_else(|| now.clone());
    let paging_hrefs = navigation_paging_hrefs(feed.headers, feed.self_path, feed.pagination);

    atom_xml_response(render_opds_v1_navigation_feed(
        OpdsV1NavigationFeedDocument {
            id: feed.feed_id.to_string(),
            title: feed.title.to_string(),
            updated: feed_updated,
            self_href,
            start_href,
            previous_href: paging_hrefs.previous,
            next_href: paging_hrefs.next,
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
                    href: app_absolute_url(feed.headers, entry.href_path.as_str()),
                })
                .collect(),
        },
    ))
}

pub(super) fn opds_v1_library_series_feed_response(
    feed: OpdsV1FeedHeader<'_>,
    series_entries: Vec<PersistedSeries>,
) -> Response {
    let self_href = app_absolute_url(feed.headers, feed.self_path);
    let start_href = app_absolute_url(feed.headers, "/opds/v1.2/catalog");
    let now = opds_now_timestamp();
    let feed_updated = feed
        .feed_updated
        .filter(|value| !value.is_empty())
        .unwrap_or(now.as_str())
        .to_string();
    let pagination = feed.pagination.unwrap_or(OpdsPageNavigation {
        page: 0,
        has_next: false,
    });
    let previous_href = (pagination.page > 0).then(|| {
        app_absolute_url(
            feed.headers,
            format!(
                "/opds/v1.2/libraries/{}?page={}",
                feed.feed_id,
                pagination.page.saturating_sub(1)
            )
            .as_str(),
        )
    });
    let next_href = pagination.has_next.then(|| {
        app_absolute_url(
            feed.headers,
            format!(
                "/opds/v1.2/libraries/{}?page={}",
                feed.feed_id,
                pagination.page + 1
            )
            .as_str(),
        )
    });

    atom_xml_response(render_opds_v1_navigation_feed(
        OpdsV1NavigationFeedDocument {
            id: feed.feed_id.to_string(),
            title: feed.title.to_string(),
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
                        feed.headers,
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
    pagination: Option<OpdsPageNavigation>,
) -> Response {
    let self_href = app_absolute_url(headers, self_path);
    let start_href = app_absolute_url(headers, "/opds/v1.2/catalog");
    let now = opds_now_timestamp();
    let feed_updated = feed_updated
        .filter(|value| !value.is_empty())
        .map(normalize_opds_updated)
        .unwrap_or(now.clone());
    let paging_hrefs = navigation_paging_hrefs(headers, self_path, pagination);

    atom_xml_response(render_opds_v1_acquisition_feed(
        OpdsV1AcquisitionFeedDocument {
            id: feed_id.to_string(),
            title: title.to_string(),
            updated: feed_updated,
            self_href,
            start_href,
            previous_href: paging_hrefs.previous,
            next_href: paging_hrefs.next,
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
    pagination: Option<OpdsPageNavigation>,
) -> OpdsPagingHrefs {
    let Some(pagination) = pagination else {
        return OpdsPagingHrefs {
            previous: None,
            next: None,
        };
    };

    let previous = (pagination.page > 0).then(|| {
        app_absolute_url(
            headers,
            page_link_path(self_path, pagination.page.saturating_sub(1)).as_str(),
        )
    });
    let next = pagination.has_next.then(|| {
        app_absolute_url(
            headers,
            page_link_path(self_path, pagination.page + 1).as_str(),
        )
    });

    OpdsPagingHrefs { previous, next }
}

struct OpdsPagingHrefs {
    previous: Option<String>,
    next: Option<String>,
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

pub(super) fn parse_page_size(query: &str) -> OpdsPageRequest {
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    OpdsPageRequest { page, size }
}

pub(super) fn paginate_vec<T>(items: Vec<T>, page_request: OpdsPageRequest) -> OpdsPagedItems<T> {
    let start = page_request.page.saturating_mul(page_request.size);
    let end = start.saturating_add(page_request.size);
    if start >= items.len() {
        return OpdsPagedItems {
            items: Vec::new(),
            has_next: false,
        };
    }
    let has_next = end < items.len();
    let page_items = items
        .into_iter()
        .skip(start)
        .take(page_request.size)
        .collect::<Vec<_>>();
    OpdsPagedItems {
        items: page_items,
        has_next,
    }
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

pub(super) fn opds_v2_updated(value: Option<&str>) -> OpdsV2UpdatedDto {
    value
        .filter(|value| !value.trim().is_empty())
        .map(OpdsV2UpdatedDto::from_storage)
        .unwrap_or_else(OpdsV2UpdatedDto::now)
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
) -> OpdsV2LinkDto {
    OpdsV2LinkDto {
        title: Some(title.to_string()),
        rel: Some("subsection".to_string()),
        href: app_absolute_url(headers, path),
        media_type: Some("application/opds+json".to_string()),
        templated: None,
        properties: None,
    }
}

pub(super) fn opds_navigation_link(headers: &HeaderMap, title: &str, path: &str) -> OpdsV2LinkDto {
    OpdsV2LinkDto {
        title: Some(title.to_string()),
        rel: None,
        href: app_absolute_url(headers, path),
        media_type: Some("application/opds+json".to_string()),
        templated: None,
        properties: None,
    }
}

pub(super) struct OpdsV2PagedFeed<'a> {
    pub(super) headers: &'a HeaderMap,
    pub(super) title: &'a str,
    pub(super) self_path: &'a str,
    pub(super) modified: Option<&'a str>,
    pub(super) page: usize,
    pub(super) size: usize,
    pub(super) total: usize,
}

pub(super) fn opds_navigation_response_with_paging(
    feed: OpdsV2PagedFeed<'_>,
    navigation: Vec<OpdsV2LinkDto>,
) -> Response {
    let metadata = opds_v2_feed_metadata(&feed);
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(OpdsV2NavigationFeedDto {
            metadata,
            links: opds_v2_feed_links(&feed),
            navigation,
        }),
    )
        .into_response()
}

pub(super) fn opds_publications_response_with_paging(
    feed: OpdsV2PagedFeed<'_>,
    publications: Vec<OpdsV2PublicationDto>,
) -> Response {
    let metadata = opds_v2_feed_metadata(&feed);
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(OpdsV2PublicationFeedDto {
            metadata,
            links: opds_v2_feed_links(&feed),
            publications,
        }),
    )
        .into_response()
}

fn opds_v2_feed_metadata(feed: &OpdsV2PagedFeed<'_>) -> OpdsV2FeedMetadataDto {
    OpdsV2FeedMetadataDto {
        title: feed.title.to_string(),
        modified: opds_v2_updated(feed.modified),
        description: None,
        items_per_page: feed.size,
        current_page: feed.page + 1,
        number_of_items: feed.total,
    }
}

fn opds_v2_feed_links(feed: &OpdsV2PagedFeed<'_>) -> Vec<OpdsV2LinkDto> {
    let mut links = vec![
        OpdsV2LinkDto {
            title: None,
            rel: Some("self".to_string()),
            href: app_absolute_url(feed.headers, feed.self_path),
            media_type: None,
            templated: None,
            properties: None,
        },
        OpdsV2LinkDto {
            title: Some("Home".to_string()),
            rel: Some("start".to_string()),
            href: app_absolute_url(feed.headers, "/opds/v2/catalog"),
            media_type: Some("application/opds+json".to_string()),
            templated: None,
            properties: None,
        },
        OpdsV2LinkDto {
            title: Some("Search".to_string()),
            rel: Some("search".to_string()),
            href: app_absolute_url(feed.headers, "/opds/v2/search{?query}"),
            media_type: Some("application/opds+json".to_string()),
            templated: Some(true),
            properties: None,
        },
    ];

    if feed.page > 0 {
        links.push(OpdsV2LinkDto {
            title: None,
            rel: Some("previous".to_string()),
            href: app_absolute_url(
                feed.headers,
                page_link_path(feed.self_path, feed.page.saturating_sub(1)).as_str(),
            ),
            media_type: None,
            templated: None,
            properties: None,
        });
    }
    if feed.page.saturating_add(1).saturating_mul(feed.size) < feed.total {
        links.push(OpdsV2LinkDto {
            title: None,
            rel: Some("next".to_string()),
            href: app_absolute_url(
                feed.headers,
                page_link_path(feed.self_path, feed.page + 1).as_str(),
            ),
            media_type: None,
            templated: None,
            properties: None,
        });
    }

    links
}

pub(super) fn opds_publication_for_feed_entry(
    headers: &HeaderMap,
    book: &OpdsBookFeedEntry,
) -> OpdsV2PublicationDto {
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

    let mut roles = RoleContributors::default();
    for author in &book.authors {
        roles.push(author);
    }
    let belongs_to =
        (!book.series_id.is_empty() && !book.series_title.is_empty()).then(|| OpdsV2BelongsToDto {
            series: vec![OpdsV2PublicationSeriesDto {
                name: book.series_title.clone(),
                position: book.number_sort.is_finite().then_some(book.number_sort),
                links: Some(vec![OpdsV2LinkDto {
                    title: None,
                    rel: None,
                    href: app_absolute_url(
                        headers,
                        format!("/opds/v2/series/{}", book.series_id).as_str(),
                    ),
                    media_type: Some("application/opds+json".to_string()),
                    templated: None,
                    properties: None,
                }]),
            }],
        });

    let metadata = OpdsV2PublicationMetadataDto {
        title: book.title.clone(),
        identifier: book
            .isbn
            .as_ref()
            .filter(|value| !value.is_empty())
            .map(|isbn| format!("urn:isbn:{isbn}")),
        description: (!book.summary.is_empty()).then(|| book.summary.clone()),
        number_of_pages: (book.page_count > 0).then_some(book.page_count as u64),
        published: book
            .release_date
            .as_ref()
            .filter(|value| !value.is_empty())
            .cloned(),
        modified: (!book.last_modified.is_empty())
            .then(|| OpdsV2UpdatedDto::from_storage(&book.last_modified)),
        subject: (!book.tags.is_empty()).then(|| book.tags.clone()),
        author: roles.author,
        translator: roles.translator,
        editor: roles.editor,
        artist: roles.artist,
        illustrator: roles.illustrator,
        letterer: roles.letterer,
        penciler: roles.penciler,
        colorist: roles.colorist,
        inker: roles.inker,
        contributor: roles.contributor,
        belongs_to,
    };

    let properties = || {
        Some(OpdsV2LinkPropertiesDto {
            authenticate: OpdsV2AuthenticationLinkDto {
                href: auth_href.clone(),
                media_type: "application/opds-authentication+json".to_string(),
            },
        })
    };
    let mut links = vec![
        OpdsV2LinkDto {
            title: None,
            rel: Some("self".to_string()),
            href: manifest_href,
            media_type: Some(publication_manifest_type(book.media_type.as_str()).to_string()),
            templated: None,
            properties: properties(),
        },
        OpdsV2LinkDto {
            title: None,
            rel: Some("http://opds-spec.org/acquisition".to_string()),
            href: file_href,
            media_type: Some(book.media_type.clone()),
            templated: None,
            properties: properties(),
        },
        OpdsV2LinkDto {
            title: None,
            rel: Some("http://www.cantook.com/api/progression".to_string()),
            href: progression_href,
            media_type: Some("application/vnd.readium.progression+json".to_string()),
            templated: None,
            properties: properties(),
        },
    ];

    if book.media_type == "application/pdf"
        || (book.media_type == "application/epub+zip" && book.epub_divina_compatible)
    {
        links.push(OpdsV2LinkDto {
            title: None,
            rel: None,
            href: app_absolute_url(
                headers,
                format!("/opds/v2/books/{}/manifest/divina", book.id).as_str(),
            ),
            media_type: Some("application/divina+json".to_string()),
            templated: None,
            properties: properties(),
        });
    }

    OpdsV2PublicationDto {
        context: "https://readium.org/webpub-manifest/context.jsonld".to_string(),
        metadata,
        links,
        images: vec![OpdsV2LinkDto {
            title: None,
            rel: None,
            href: thumbnail_href,
            media_type: Some("image/jpeg".to_string()),
            templated: None,
            properties: properties(),
        }],
    }
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

#[derive(Default)]
struct RoleContributors {
    author: Option<Vec<String>>,
    translator: Option<Vec<String>>,
    editor: Option<Vec<String>>,
    artist: Option<Vec<String>>,
    illustrator: Option<Vec<String>>,
    letterer: Option<Vec<String>>,
    penciler: Option<Vec<String>>,
    colorist: Option<Vec<String>>,
    inker: Option<Vec<String>>,
    contributor: Option<Vec<String>>,
}

impl RoleContributors {
    fn push(&mut self, entry: &OpdsBookAuthorEntry) {
        let target = match entry.role.as_str() {
            "author" => &mut self.author,
            "translator" => &mut self.translator,
            "editor" => &mut self.editor,
            "artist" => &mut self.artist,
            "illustrator" => &mut self.illustrator,
            "letterer" => &mut self.letterer,
            "penciler" | "penciller" => &mut self.penciler,
            "colorist" => &mut self.colorist,
            "inker" => &mut self.inker,
            _ => &mut self.contributor,
        };
        target.get_or_insert_with(Vec::new).push(entry.name.clone());
    }
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use axum::http::HeaderMap;
    use time::{Month, OffsetDateTime, UtcOffset};

    use super::{
        OpdsV1FeedHeader, OpdsV1NavigationEntry, format_opds_timestamp,
        opds_v1_navigation_feed_response,
    };

    #[tokio::test]
    async fn navigation_feed_uses_entry_specific_updated_timestamp() {
        let headers = HeaderMap::new();
        let response = opds_v1_navigation_feed_response(
            OpdsV1FeedHeader {
                headers: &headers,
                feed_id: "feed",
                title: "Feed",
                self_path: "/opds/v1.2/feed",
                feed_updated: None,
                pagination: None,
            },
            vec![OpdsV1NavigationEntry {
                id: "entry-1".to_string(),
                title: "Entry".to_string(),
                content: "".to_string(),
                href_path: "/opds/v1.2/entry-1".to_string(),
                updated: Some("2024-01-02T03:04:05Z".to_string()),
            }],
        );

        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        let body = String::from_utf8(bytes.to_vec()).expect("feed body should be utf-8");

        assert!(body.contains("<updated>2024-01-02T03:04:05Z</updated>"));
    }

    #[test]
    fn parse_page_size_does_not_cap_large_requested_size() {
        assert_eq!(
            super::parse_page_size("page=2&size=250"),
            super::OpdsPageRequest { page: 2, size: 250 }
        );
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

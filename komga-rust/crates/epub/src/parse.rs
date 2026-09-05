use std::collections::HashMap;
use std::fmt;

use quick_xml::Reader;
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};

const DEFAULT_MANIFEST_MEDIA_TYPE: &str = "application/octet-stream";
const DEFAULT_SPINE_MEDIA_TYPE: &str = "application/xhtml+xml";
const PACKAGE_DOCUMENT: &str = "EPUB package document";
const CONTAINER_DOCUMENT: &str = "EPUB container document";

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EpubManifestItem {
    pub id: String,
    pub href: String,
    pub media_type: String,
    pub properties: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EpubSpineItem {
    pub href: String,
    pub media_type: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EpubParseError {
    message: String,
}

impl EpubParseError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for EpubParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for EpubParseError {}

pub fn parse_epub_manifest_items(
    package_document: &[u8],
    rootfile_path: &str,
) -> Result<HashMap<String, EpubManifestItem>, EpubParseError> {
    parse_manifest_items(package_document, rootfile_path, DEFAULT_MANIFEST_MEDIA_TYPE)
}

pub fn parse_epub_spine_items(
    package_document: &[u8],
    rootfile_path: &str,
) -> Result<Vec<EpubSpineItem>, EpubParseError> {
    let manifest = parse_manifest_items(package_document, rootfile_path, DEFAULT_SPINE_MEDIA_TYPE)?;
    Ok(parse_epub_spine_itemrefs(package_document)?
        .into_iter()
        .filter_map(|idref| manifest.get(&idref))
        .map(|item| EpubSpineItem {
            href: item.href.clone(),
            media_type: item.media_type.clone(),
        })
        .collect())
}

pub fn parse_epub_spine_itemrefs(package_document: &[u8]) -> Result<Vec<String>, EpubParseError> {
    let mut reader = reader_for(package_document);
    let mut buffer = Vec::new();
    let mut spine_ids = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) | Ok(Event::Empty(event))
                if xml_name_matches(event.name().as_ref(), "itemref") =>
            {
                if let Some(idref) = attribute_value(&event, "idref", PACKAGE_DOCUMENT)? {
                    spine_ids.push(idref);
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(xml_error(PACKAGE_DOCUMENT, error)),
            _ => {}
        }
        buffer.clear();
    }

    Ok(spine_ids)
}

fn parse_manifest_items(
    package_document: &[u8],
    rootfile_path: &str,
    default_media_type: &str,
) -> Result<HashMap<String, EpubManifestItem>, EpubParseError> {
    let mut reader = reader_for(package_document);
    let mut buffer = Vec::new();
    let mut manifest = HashMap::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) | Ok(Event::Empty(event))
                if xml_name_matches(event.name().as_ref(), "item") =>
            {
                let id = attribute_value(&event, "id", PACKAGE_DOCUMENT)?;
                let href = attribute_value(&event, "href", PACKAGE_DOCUMENT)?;
                let Some(id) = id else {
                    buffer.clear();
                    continue;
                };
                let Some(href) = href else {
                    buffer.clear();
                    continue;
                };

                manifest.insert(
                    id.clone(),
                    EpubManifestItem {
                        id,
                        href: normalize_epub_resource_href(rootfile_path, &href),
                        media_type: attribute_value(&event, "media-type", PACKAGE_DOCUMENT)?
                            .unwrap_or_else(|| default_media_type.to_string()),
                        properties: attribute_value(&event, "properties", PACKAGE_DOCUMENT)?
                            .unwrap_or_default(),
                    },
                );
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(xml_error(PACKAGE_DOCUMENT, error)),
            _ => {}
        }
        buffer.clear();
    }

    Ok(manifest)
}

pub fn parse_epub_metadata_cover_id(
    package_document: &[u8],
) -> Result<Option<String>, EpubParseError> {
    let mut reader = reader_for(package_document);
    let mut buffer = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) | Ok(Event::Empty(event))
                if xml_name_matches(event.name().as_ref(), "meta") =>
            {
                let name = attribute_value(&event, "name", PACKAGE_DOCUMENT)?;
                if name
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case("cover"))
                {
                    return Ok(attribute_value(&event, "content", PACKAGE_DOCUMENT)?
                        .filter(|value| !value.trim().is_empty()));
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(xml_error(PACKAGE_DOCUMENT, error)),
            _ => {}
        }
        buffer.clear();
    }

    Ok(None)
}

pub fn parse_epub_guide_cover_href(
    package_document: &[u8],
) -> Result<Option<String>, EpubParseError> {
    let mut reader = reader_for(package_document);
    let mut buffer = Vec::new();
    let mut in_guide = false;

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) if xml_name_matches(event.name().as_ref(), "guide") => {
                in_guide = true;
            }
            Ok(Event::End(event)) if xml_name_matches(event.name().as_ref(), "guide") => {
                in_guide = false;
            }
            Ok(Event::Start(event)) | Ok(Event::Empty(event))
                if in_guide
                    && xml_name_matches(event.name().as_ref(), "reference")
                    && attribute_value(&event, "type", PACKAGE_DOCUMENT)?
                        .as_deref()
                        .is_some_and(|value| value.eq_ignore_ascii_case("cover")) =>
            {
                return attribute_value(&event, "href", PACKAGE_DOCUMENT);
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(xml_error(PACKAGE_DOCUMENT, error)),
            _ => {}
        }
        buffer.clear();
    }

    Ok(None)
}

pub fn parse_epub_rootfile_path(container_xml: &[u8]) -> Result<Option<String>, EpubParseError> {
    let mut reader = reader_for(container_xml);
    let mut buffer = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) | Ok(Event::Empty(event))
                if xml_name_matches(event.name().as_ref(), "rootfile") =>
            {
                if let Some(path) = attribute_value(&event, "full-path", CONTAINER_DOCUMENT)? {
                    return Ok(Some(normalize_epub_zip_path(&path)));
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(xml_error(CONTAINER_DOCUMENT, error)),
            _ => {}
        }
        buffer.clear();
    }

    Ok(None)
}

pub fn parse_epub_fixed_layout(package_document: &[u8]) -> Result<bool, EpubParseError> {
    let mut reader = reader_for(package_document);
    let mut buffer = Vec::new();
    let mut awaiting_rendition_layout_text = false;

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) if xml_name_matches(event.name().as_ref(), "meta") => {
                if is_fixed_layout_meta(&event)? {
                    return Ok(true);
                }
                awaiting_rendition_layout_text =
                    attribute_value(&event, "property", PACKAGE_DOCUMENT)?
                        .as_deref()
                        .is_some_and(|value| value.eq_ignore_ascii_case("rendition:layout"));
            }
            Ok(Event::Empty(event)) if xml_name_matches(event.name().as_ref(), "meta") => {
                if is_fixed_layout_meta(&event)? {
                    return Ok(true);
                }
            }
            Ok(Event::Text(text)) if awaiting_rendition_layout_text => {
                let value = text.as_ref();
                if value.trim().eq_ignore_ascii_case("pre-paginated") {
                    return Ok(true);
                }
            }
            Ok(Event::End(event)) if xml_name_matches(event.name().as_ref(), "meta") => {
                awaiting_rendition_layout_text = false;
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(xml_error(PACKAGE_DOCUMENT, error)),
            _ => {}
        }
        buffer.clear();
    }

    Ok(false)
}

fn is_fixed_layout_meta(event: &BytesStart<'_>) -> Result<bool, EpubParseError> {
    let property = attribute_value(event, "property", PACKAGE_DOCUMENT)?;
    let name = attribute_value(event, "name", PACKAGE_DOCUMENT)?;
    let content = attribute_value(event, "content", PACKAGE_DOCUMENT)?;

    Ok(property.as_deref().is_some_and(|value| {
        value.eq_ignore_ascii_case("rendition:layout")
            && content
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case("pre-paginated"))
    }) || name.as_deref().is_some_and(|value| {
        value.eq_ignore_ascii_case("fixed-layout")
            && content
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case("true"))
    }))
}

pub fn normalize_epub_resource_href(rootfile_path: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or_default();
    let href = percent_decode(href);
    if href.starts_with('/') {
        return normalize_epub_zip_path(&href);
    }

    let base = rootfile_path
        .trim_start_matches('/')
        .rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or_default();
    let joined = if base.is_empty() {
        href.to_string()
    } else {
        format!("{base}/{href}")
    };
    normalize_epub_zip_path(&joined)
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = (bytes[index + 1] as char).to_digit(16);
            let low = (bytes[index + 2] as char).to_digit(16);
            if let (Some(high), Some(low)) = (high, low) {
                decoded.push(((high << 4) | low) as u8);
                index += 3;
                continue;
            }
        }

        decoded.push(bytes[index]);
        index += 1;
    }

    String::from_utf8(decoded).unwrap_or_else(|_| value.to_string())
}

pub fn normalize_epub_zip_path(path: &str) -> String {
    let normalized_path = path.replace('\\', "/");
    let mut normalized_segments = Vec::new();

    for segment in normalized_path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                normalized_segments.pop();
            }
            _ => normalized_segments.push(segment),
        }
    }

    if normalized_segments.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", normalized_segments.join("/"))
    }
}

fn reader_for(document: &[u8]) -> Reader<&[u8]> {
    let mut reader = Reader::from_reader(document);
    reader.config_mut().trim_text(true);
    reader
}

fn attribute_value(
    event: &BytesStart<'_>,
    expected_name: &str,
    document_name: &str,
) -> Result<Option<String>, EpubParseError> {
    for attribute in event.attributes() {
        let attribute = attribute.map_err(|error| {
            EpubParseError::new(format!(
                "failed to parse {document_name} attribute: {error}"
            ))
        })?;
        if xml_name_matches(attribute.key.as_ref(), expected_name) {
            let value = attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .map(|value| value.into_owned())
                .map_err(|error| {
                    EpubParseError::new(format!(
                        "failed to parse {document_name} attribute value: {error}"
                    ))
                })?;
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn xml_error(document_name: &str, error: impl fmt::Display) -> EpubParseError {
    EpubParseError::new(format!("failed to parse {document_name}: {error}"))
}

fn xml_name_matches(actual: &str, expected: &str) -> bool {
    actual == expected || actual.ends_with(expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_manifest_rootfile_and_spine() {
        let container = br#"<container><rootfiles><rootfile full-path="OPS/content.opf"/></rootfiles></container>"#;
        let rootfile = parse_epub_rootfile_path(container)
            .expect("container should parse")
            .expect("rootfile should exist");
        assert_eq!(rootfile, "/OPS/content.opf");

        let package = br#"<package><manifest><item id="chapter" href="text/../chapter.xhtml" media-type="application/xhtml+xml" properties="nav"/></manifest><spine><itemref idref="chapter"/></spine></package>"#;
        let manifest =
            parse_epub_manifest_items(package, &rootfile).expect("manifest should parse");
        assert_eq!(manifest["chapter"].href, "/OPS/chapter.xhtml");
        assert_eq!(manifest["chapter"].properties, "nav");

        let spine = parse_epub_spine_items(package, &rootfile).expect("spine should parse");
        assert_eq!(spine[0].href, "/OPS/chapter.xhtml");
        assert_eq!(spine[0].media_type, "application/xhtml+xml");
    }

    #[test]
    fn parses_legacy_cover_metadata() {
        let package =
            br#"<package><metadata><meta name="cover" content="cover-image"/></metadata></package>"#;
        assert_eq!(
            parse_epub_metadata_cover_id(package).expect("metadata should parse"),
            Some("cover-image".to_string())
        );
    }

    #[test]
    fn parses_guide_cover_href() {
        let package =
            br#"<package><guide><reference type="cover" href="cover.xhtml"/></guide></package>"#;
        assert_eq!(
            parse_epub_guide_cover_href(package).expect("guide should parse"),
            Some("cover.xhtml".to_string())
        );
    }

    #[test]
    fn parses_guide_cover_href_with_other_references() {
        let package = br#"<package><guide><reference type="text" href="toc.xhtml"/><reference type="cover" href="images/cover.jpg"/><reference type="copyright" href="copy.xhtml"/></guide></package>"#;
        assert_eq!(
            parse_epub_guide_cover_href(package).expect("guide should parse"),
            Some("images/cover.jpg".to_string())
        );
    }

    #[test]
    fn parses_guide_cover_href_returns_none_when_absent() {
        let package =
            br#"<package><guide><reference type="text" href="toc.xhtml"/></guide></package>"#;
        assert_eq!(
            parse_epub_guide_cover_href(package).expect("guide should parse"),
            None
        );
    }

    #[test]
    fn detects_fixed_layout_variants() {
        let by_property =
            br#"<package><metadata><meta property="rendition:layout">pre-paginated</meta></metadata></package>"#;
        assert!(parse_epub_fixed_layout(by_property).expect("package should parse"));

        let by_name =
            br#"<package><metadata><meta name="fixed-layout" content="true"/></metadata></package>"#;
        assert!(parse_epub_fixed_layout(by_name).expect("package should parse"));

        let flowing =
            br#"<package><metadata><meta property="rendition:layout">reflowable</meta></metadata></package>"#;
        assert!(!parse_epub_fixed_layout(flowing).expect("package should parse"));
    }

    #[test]
    fn normalizes_resource_hrefs_and_zip_paths() {
        assert_eq!(
            normalize_epub_resource_href("/OPS/sub/content.opf", "../chapter.xhtml#part-1"),
            "/OPS/chapter.xhtml"
        );
        assert_eq!(
            normalize_epub_resource_href("/OPS/content.opf", "./text/../chapter.xhtml"),
            "/OPS/chapter.xhtml"
        );
        assert_eq!(
            normalize_epub_zip_path("OPS\\text\\chapter.xhtml"),
            "/OPS/text/chapter.xhtml"
        );
    }

    #[test]
    fn percent_decodes_resource_hrefs() {
        assert_eq!(
            normalize_epub_resource_href("/OPS/content.opf", "images/cover%20final.jpg"),
            "/OPS/images/cover final.jpg"
        );
        assert_eq!(
            normalize_epub_resource_href("/OPS/content.opf", "chapter%2Bappendix.xhtml"),
            "/OPS/chapter+appendix.xhtml"
        );
        assert_eq!(
            normalize_epub_resource_href("/OPS/content.opf", "caf%C3%A9.png"),
            "/OPS/café.png"
        );
        assert_eq!(
            normalize_epub_resource_href("/OPS/content.opf", "images/cover%23final.jpg"),
            "/OPS/images/cover#final.jpg"
        );
    }
}

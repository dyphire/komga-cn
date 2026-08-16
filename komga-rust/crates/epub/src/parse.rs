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
                if xml_name_matches(event.name().as_ref(), b"itemref") =>
            {
                if let Some(idref) = attribute_value(&event, b"idref", PACKAGE_DOCUMENT)? {
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
                if xml_name_matches(event.name().as_ref(), b"item") =>
            {
                let id = attribute_value(&event, b"id", PACKAGE_DOCUMENT)?;
                let href = attribute_value(&event, b"href", PACKAGE_DOCUMENT)?;
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
                        media_type: attribute_value(&event, b"media-type", PACKAGE_DOCUMENT)?
                            .unwrap_or_else(|| default_media_type.to_string()),
                        properties: attribute_value(&event, b"properties", PACKAGE_DOCUMENT)?
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
                if xml_name_matches(event.name().as_ref(), b"meta") =>
            {
                let name = attribute_value(&event, b"name", PACKAGE_DOCUMENT)?;
                if name
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case("cover"))
                {
                    return Ok(attribute_value(&event, b"content", PACKAGE_DOCUMENT)?
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

pub fn parse_epub_rootfile_path(container_xml: &[u8]) -> Result<Option<String>, EpubParseError> {
    let mut reader = reader_for(container_xml);
    let mut buffer = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) | Ok(Event::Empty(event))
                if xml_name_matches(event.name().as_ref(), b"rootfile") =>
            {
                if let Some(path) = attribute_value(&event, b"full-path", CONTAINER_DOCUMENT)? {
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
            Ok(Event::Start(event)) if xml_name_matches(event.name().as_ref(), b"meta") => {
                if is_fixed_layout_meta(&event)? {
                    return Ok(true);
                }
                awaiting_rendition_layout_text =
                    attribute_value(&event, b"property", PACKAGE_DOCUMENT)?
                        .as_deref()
                        .is_some_and(|value| value.eq_ignore_ascii_case("rendition:layout"));
            }
            Ok(Event::Empty(event)) if xml_name_matches(event.name().as_ref(), b"meta") => {
                if is_fixed_layout_meta(&event)? {
                    return Ok(true);
                }
            }
            Ok(Event::Text(text)) if awaiting_rendition_layout_text => {
                let value = String::from_utf8_lossy(text.as_ref());
                if value.trim().eq_ignore_ascii_case("pre-paginated") {
                    return Ok(true);
                }
            }
            Ok(Event::End(event)) if xml_name_matches(event.name().as_ref(), b"meta") => {
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
    let property = attribute_value(event, b"property", PACKAGE_DOCUMENT)?;
    let name = attribute_value(event, b"name", PACKAGE_DOCUMENT)?;
    let content = attribute_value(event, b"content", PACKAGE_DOCUMENT)?;

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
    if href.starts_with('/') {
        return normalize_epub_zip_path(href);
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
    expected_name: &[u8],
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

fn xml_name_matches(actual: &[u8], expected: &[u8]) -> bool {
    actual == expected || actual.ends_with(expected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read};
    use std::path::{Path, PathBuf};
    use zip::ZipArchive;

    fn repository_resource(relative_path: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../komga/src/test/resources")
            .join(relative_path)
    }

    fn read_zip_entry(bytes: &[u8], entry_name: &str) -> Option<Vec<u8>> {
        let mut archive = ZipArchive::new(Cursor::new(bytes)).ok()?;
        let mut entry = archive.by_name(entry_name).ok()?;
        let mut entry_bytes = Vec::new();
        entry.read_to_end(&mut entry_bytes).ok()?;
        Some(entry_bytes)
    }

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
    fn parses_repository_epub_samples() {
        for relative_path in [
            "archives/epub3.epub",
            "epub/The Incomplete Theft - Ralph Burke.epub",
        ] {
            let path = repository_resource(relative_path);
            assert!(
                path.is_file(),
                "repository EPUB sample is missing: {}",
                path.display()
            );
            let bytes = std::fs::read(&path).expect("repository EPUB sample should be readable");
            let container = read_zip_entry(&bytes, "META-INF/container.xml")
                .expect("EPUB sample should contain container.xml");
            let rootfile = parse_epub_rootfile_path(&container)
                .expect("EPUB container should parse")
                .expect("EPUB sample should declare a rootfile");
            let package = read_zip_entry(&bytes, rootfile.trim_start_matches('/'))
                .expect("EPUB sample should contain its package document");
            let manifest =
                parse_epub_manifest_items(&package, &rootfile).expect("manifest should parse");
            let spine = parse_epub_spine_items(&package, &rootfile).expect("spine should parse");

            assert!(!manifest.is_empty(), "EPUB manifest should not be empty");
            assert!(!spine.is_empty(), "EPUB spine should not be empty");
            assert!(
                !parse_epub_fixed_layout(&package).expect("fixed-layout metadata should parse")
            );
        }
    }

    #[test]
    fn rejects_repository_zip_disguised_as_epub() {
        let path = repository_resource("archives/zip-as-epub.epub");
        assert!(
            path.is_file(),
            "repository sample is missing: {}",
            path.display()
        );
        let bytes = std::fs::read(path).expect("repository sample should be readable");
        assert!(
            read_zip_entry(&bytes, "META-INF/container.xml").is_none(),
            "ZIP sample must not be accepted as an EPUB container"
        );
    }
}

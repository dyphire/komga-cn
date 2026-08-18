use std::collections::HashMap;
use std::fmt;
use std::io::{Read, Seek};

use quick_xml::Reader;
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};
use regex::Regex;
use zip::ZipArchive;

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

pub fn parse_epub_guide_cover_href(
    package_document: &[u8],
) -> Result<Option<String>, EpubParseError> {
    let mut reader = reader_for(package_document);
    let mut buffer = Vec::new();
    let mut in_guide = false;

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) if xml_name_matches(event.name().as_ref(), b"guide") => {
                in_guide = true;
            }
            Ok(Event::End(event)) if xml_name_matches(event.name().as_ref(), b"guide") => {
                in_guide = false;
            }
            Ok(Event::Start(event)) | Ok(Event::Empty(event))
                if in_guide
                    && xml_name_matches(event.name().as_ref(), b"reference")
                    && attribute_value(&event, b"type", PACKAGE_DOCUMENT)?
                        .as_deref()
                        .is_some_and(|value| value.eq_ignore_ascii_case("cover")) =>
            {
                return attribute_value(&event, b"href", PACKAGE_DOCUMENT);
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

pub fn parse_epub_fixed_layout_with_heuristic<R: Read + Seek>(
    package_document: &[u8],
    manifest: &HashMap<String, EpubManifestItem>,
    archive: &mut ZipArchive<R>,
    rootfile_path: &str,
) -> Result<bool, EpubParseError> {
    if parse_epub_fixed_layout(package_document)? {
        return Ok(true);
    }

    const IMAGE_MEDIA_TYPES: &[&str] = &[
        "image/jpeg",
        "image/png",
        "image/webp",
        "image/gif",
        "image/avif",
        "image/jxl",
    ];

    let image_count = manifest
        .values()
        .filter(|item| IMAGE_MEDIA_TYPES.contains(&item.media_type.as_str()))
        .count();
    let total_count = manifest.len();

    if total_count == 0 {
        return Ok(false);
    }

    let image_ratio = image_count as f64 / total_count as f64;
    if image_ratio < 0.40 {
        return Ok(false);
    }

    let spine_ids = parse_epub_spine_itemrefs(package_document)?;
    let spine_items: Vec<&EpubManifestItem> =
        spine_ids.iter().filter_map(|id| manifest.get(id)).collect();

    let sample_indices = {
        let len = spine_items.len();

        if len <= 15 {
            (0..len).collect::<Vec<_>>()
        } else {
            let mut indices = Vec::with_capacity(15);

            indices.extend(0..5);

            let middle_start = (len - 5) / 2;
            indices.extend(middle_start..middle_start + 5);

            indices.extend(len - 5..len);

            indices
        }
    };

    let mut sampled_pages = 0;
    let mut has_text_content = false;

    let tag_regex = Regex::new(r"<[^>]*>").expect("valid HTML tag regex");

    for index in sample_indices {
        if has_text_content {
            break;
        }

        let item = &spine_items[index];
        let href = normalize_epub_resource_href(rootfile_path, &item.href);

        let entry_name = href.trim_start_matches('/');
        if let Ok(mut zip_entry) = archive.by_name(entry_name) {
            let mut content = String::new();
            if zip_entry.read_to_string(&mut content).is_ok() {
                let text = tag_regex.replace_all(&content, "").to_string();
                if text.chars().any(|c| !c.is_whitespace()) {
                    has_text_content = true;
                }
                sampled_pages += 1;
            }
        }
    }

    if has_text_content {
        return Ok(false);
    }

    Ok(sampled_pages > 0)
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
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipArchive, ZipWriter};

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

    fn assert_heuristic(package: &[u8], entries: &[(&str, &[u8])], expected: bool) {
        let manifest = parse_epub_manifest_items(package, "/OPS/content.opf")
            .expect("heuristic manifest should parse");
        let mut archive = build_heuristic_archive(package, entries);
        let actual = parse_epub_fixed_layout_with_heuristic(
            package,
            &manifest,
            &mut archive,
            "OPS/content.opf",
        )
        .expect("heuristic package should parse");
        assert_eq!(actual, expected);
    }

    fn build_heuristic_archive(
        package: &[u8],
        entries: &[(&str, &[u8])],
    ) -> ZipArchive<Cursor<Vec<u8>>> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        writer
            .start_file("OPS/content.opf", stored)
            .expect("package entry should be created");
        writer
            .write_all(package)
            .expect("package entry should be written");
        for &(file_name, bytes) in entries {
            writer
                .start_file(file_name, stored)
                .expect("heuristic entry should be created");
            writer
                .write_all(bytes)
                .expect("heuristic entry should be written");
        }
        ZipArchive::new(writer.finish().expect("heuristic archive should finish"))
            .expect("heuristic archive should open")
    }

    #[test]
    fn heuristic_detects_image_only_comic_as_fixed_layout() {
        let package = br#"<?xml version="1.0"?><package><manifest><item id="img1" href="img1.jpg" media-type="image/jpeg"/><item id="img2" href="img2.jpg" media-type="image/jpeg"/><item id="img3" href="img3.jpg" media-type="image/jpeg"/><item id="page" href="page.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="page"/></spine></package>"#;
        assert_heuristic(
            package,
            &[(
                "OPS/page.xhtml",
                br#"<?xml version="1.0"?><html><body><img src="img1.jpg"/></body></html>"#,
            )],
            true,
        );
    }

    #[test]
    fn heuristic_does_not_flag_text_based_book_as_fixed_layout() {
        let package = br#"<?xml version="1.0"?><package><manifest><item id="img1" href="img1.jpg" media-type="image/jpeg"/><item id="img2" href="img2.jpg" media-type="image/jpeg"/><item id="txt1" href="chapter1.xhtml" media-type="application/xhtml+xml"/><item id="txt2" href="chapter2.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="txt1"/><itemref idref="txt2"/></spine></package>"#;
        assert_heuristic(
            package,
            &[
                (
                    "OPS/chapter1.xhtml",
                    br#"<?xml version="1.0"?><html><body><p>Hello world</p></body></html>"#,
                ),
                (
                    "OPS/chapter2.xhtml",
                    br#"<?xml version="1.0"?><html><body><p>More text here</p></body></html>"#,
                ),
            ],
            false,
        );
    }

    #[test]
    fn heuristic_returns_false_when_no_spine_page_is_readable() {
        let package = br#"<?xml version="1.0"?><package><manifest><item id="img1" href="img1.jpg" media-type="image/jpeg"/><item id="img2" href="img2.jpg" media-type="image/jpeg"/><item id="page" href="page.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="page"/></spine></package>"#;
        assert_heuristic(package, &[], false);
    }

    #[test]
    fn heuristic_does_not_flag_low_image_ratio_as_fixed_layout() {
        let package = br#"<?xml version="1.0"?><package><manifest><item id="img1" href="img1.jpg" media-type="image/jpeg"/><item id="txt1" href="chapter1.xhtml" media-type="application/xhtml+xml"/><item id="txt2" href="chapter2.xhtml" media-type="application/xhtml+xml"/><item id="txt3" href="chapter3.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="txt1"/><itemref idref="txt2"/><itemref idref="txt3"/></spine></package>"#;
        assert_heuristic(package, &[], false);
    }
}

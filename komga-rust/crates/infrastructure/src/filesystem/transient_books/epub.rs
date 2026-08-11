use std::collections::HashMap;
use std::io::Read;

use quick_xml::Reader as XmlReader;
use quick_xml::XmlVersion;
use quick_xml::events::Event as XmlEvent;
use zip::ZipArchive;

use komga_domain::discovery::MediaStatus;

use super::{
    EPUB_DIVINA_LETTER_COUNT_THRESHOLD, TransientBookAnalysis, TransientBookPage,
    TransientEpubManifestItem,
};
use crate::filesystem::media_analysis::{ImageDimensions, image_dimensions_from_bytes_u32};

pub(super) fn analyze_transient_epub(path: &str) -> Result<TransientBookAnalysis, &'static str> {
    let file = std::fs::File::open(path).map_err(|_| "ERR_1032")?;
    let mut archive = ZipArchive::new(file).map_err(|_| "ERR_1032")?;
    let container_xml = read_zip_entry_bytes_normalized(&mut archive, "META-INF/container.xml")
        .ok_or("ERR_1032")?;
    let rootfile_path = parse_transient_epub_rootfile_path(&container_xml).ok_or("ERR_1032")?;
    let package_document =
        read_zip_entry_bytes_normalized(&mut archive, &rootfile_path).ok_or("ERR_1032")?;
    let manifest = parse_transient_epub_manifest_items(&package_document, &rootfile_path)
        .map_err(|_| "ERR_1032")?;
    let spine =
        parse_transient_epub_spine_items(&package_document, &manifest).map_err(|_| "ERR_1032")?;
    let page_count = compute_transient_epub_page_count(&mut archive, &spine);
    let pages = extract_transient_epub_divina_pages(&mut archive, &manifest, &spine)
        .map_err(|_| "ERR_1032")?;
    let mut files = manifest
        .values()
        .map(|item| item.href.clone())
        .collect::<Vec<_>>();
    files.sort();

    Ok(TransientBookAnalysis {
        status: MediaStatus::Ready,
        media_type: "application/epub+zip".to_string(),
        page_count: if pages.is_empty() {
            page_count
        } else {
            pages.len() as u32
        },
        pages,
        files,
        comment: String::new(),
        number: None,
        series_id: None,
    })
}

// PLACEHOLDER_REMAINING_FUNCTIONS

fn compute_transient_epub_page_count<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    spine: &[TransientEpubManifestItem],
) -> u32 {
    let spine_paths = spine
        .iter()
        .map(|item| item.href.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut page_count = 0_u64;

    for index in 0..archive.len() {
        let Ok(entry) = archive.by_index(index) else {
            continue;
        };
        let Ok(entry_name) = entry.name() else {
            continue;
        };
        let normalized_name = normalize_transient_epub_zip_path(entry_name.as_ref());
        if !spine_paths.contains(normalized_name.as_str()) {
            continue;
        }
        page_count = page_count.saturating_add(entry.compressed_size().div_ceil(1024));
    }

    page_count.min(u32::MAX as u64) as u32
}

fn extract_transient_epub_divina_pages<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    manifest: &HashMap<String, TransientEpubManifestItem>,
    spine: &[TransientEpubManifestItem],
) -> Result<Vec<TransientBookPage>, String> {
    let mut pages = Vec::new();

    for item in spine {
        let page = if item.media_type.starts_with("image/") {
            let bytes = read_zip_entry_bytes_normalized(archive, &item.href)
                .ok_or_else(|| format!("missing epub resource {}", item.href))?;
            let dimensions = transient_epub_image_dimensions(&bytes, &item.media_type, &item.href)?;
            TransientBookPage {
                number: (pages.len() as u32) + 1,
                file_name: item.href.clone(),
                media_type: item.media_type.clone(),
                width: dimensions.map(|dimensions| dimensions.width),
                height: dimensions.map(|dimensions| dimensions.height),
                size_bytes: Some(bytes.len() as u64),
            }
        } else if is_transient_epub_html_media_type(&item.media_type) {
            let resource_bytes = read_zip_entry_bytes_normalized(archive, &item.href)
                .ok_or_else(|| format!("missing epub resource {}", item.href))?;
            let Some(image_href) =
                parse_transient_epub_divina_image_href(&resource_bytes, &item.href)?
            else {
                return Ok(Vec::new());
            };
            // PLACEHOLDER_DIVINA_CONT
            let Some(image_item) = manifest.values().find(|entry| entry.href == image_href) else {
                return Ok(Vec::new());
            };
            if !image_item.media_type.starts_with("image/") {
                return Ok(Vec::new());
            }
            let image_bytes = read_zip_entry_bytes_normalized(archive, &image_href)
                .ok_or_else(|| format!("missing epub image resource {image_href}"))?;
            let dimensions =
                transient_epub_image_dimensions(&image_bytes, &image_item.media_type, &image_href)?;
            TransientBookPage {
                number: (pages.len() as u32) + 1,
                file_name: image_href,
                media_type: image_item.media_type.clone(),
                width: dimensions.map(|dimensions| dimensions.width),
                height: dimensions.map(|dimensions| dimensions.height),
                size_bytes: Some(image_bytes.len() as u64),
            }
        } else {
            return Ok(Vec::new());
        };

        pages.push(page);
    }

    Ok(pages)
}

fn transient_epub_image_dimensions(
    bytes: &[u8],
    media_type: &str,
    resource_path: &str,
) -> Result<Option<ImageDimensions>, String> {
    if !is_supported_transient_epub_raster_image_media_type(media_type) {
        return Ok(None);
    }

    image_dimensions_from_bytes_u32(bytes)
        .map(Some)
        .ok_or_else(|| format!("failed to decode transient EPUB image dimensions {resource_path}"))
}

pub(super) fn read_zip_entry_bytes_normalized<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
) -> Option<Vec<u8>> {
    let normalized = normalize_transient_epub_zip_path(path);
    let mut entry = archive.by_name(normalized.as_str()).ok()?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

pub(super) fn parse_transient_epub_rootfile_path(container_xml: &[u8]) -> Option<String> {
    let mut reader = XmlReader::from_reader(container_xml);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer).ok()? {
            XmlEvent::Start(event) | XmlEvent::Empty(event) => {
                if !transient_xml_name_matches(event.name().as_ref(), b"rootfile") {
                    buffer.clear();
                    continue;
                }
                let Some(path) = transient_xml_attribute_value(&event, b"full-path") else {
                    buffer.clear();
                    continue;
                };
                return Some(normalize_transient_epub_zip_path(&path));
            }
            XmlEvent::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    None
}

// PLACEHOLDER_MANIFEST_SPINE

pub(super) fn parse_transient_epub_manifest_items(
    package_document: &[u8],
    rootfile_path: &str,
) -> Result<HashMap<String, TransientEpubManifestItem>, String> {
    let mut reader = XmlReader::from_reader(package_document);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut manifest = HashMap::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(XmlEvent::Start(event)) | Ok(XmlEvent::Empty(event)) => {
                if !transient_xml_name_matches(event.name().as_ref(), b"item") {
                    buffer.clear();
                    continue;
                }
                let mut id = None::<String>;
                let mut href = None::<String>;
                let mut media_type = None::<String>;
                for attribute in event.attributes() {
                    let attribute = attribute.map_err(|error| {
                        format!(
                            "failed to parse transient EPUB package document attribute: {error}"
                        )
                    })?;
                    if transient_xml_name_matches(attribute.key.as_ref(), b"id") {
                        id = Some(transient_xml_attribute_value_result(
                            attribute,
                            "transient EPUB package document",
                        )?);
                    } else if transient_xml_name_matches(attribute.key.as_ref(), b"href") {
                        href = Some(transient_xml_attribute_value_result(
                            attribute,
                            "transient EPUB package document",
                        )?);
                    } else if transient_xml_name_matches(attribute.key.as_ref(), b"media-type") {
                        media_type = Some(transient_xml_attribute_value_result(
                            attribute,
                            "transient EPUB package document",
                        )?);
                    }
                }
                let Some(id) = id else {
                    buffer.clear();
                    continue;
                };
                let Some(href) = href else {
                    buffer.clear();
                    continue;
                };
                let media_type =
                    media_type.unwrap_or_else(|| "application/octet-stream".to_string());
                manifest.insert(
                    id,
                    TransientEpubManifestItem {
                        href: normalize_transient_epub_resource_href(rootfile_path, &href),
                        media_type,
                    },
                );
            }
            Ok(XmlEvent::Eof) => break,
            Err(error) => {
                return Err(format!(
                    "failed to parse transient EPUB package document: {error}"
                ));
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok(manifest)
}

pub(super) fn parse_transient_epub_spine_items(
    package_document: &[u8],
    manifest: &HashMap<String, TransientEpubManifestItem>,
) -> Result<Vec<TransientEpubManifestItem>, String> {
    let mut reader = XmlReader::from_reader(package_document);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut spine = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(XmlEvent::Start(event)) | Ok(XmlEvent::Empty(event)) => {
                if !transient_xml_name_matches(event.name().as_ref(), b"itemref") {
                    buffer.clear();
                    continue;
                }
                let mut idref = None::<String>;
                for attribute in event.attributes() {
                    let attribute = attribute.map_err(|error| {
                        format!(
                            "failed to parse transient EPUB package document attribute: {error}"
                        )
                    })?;
                    if transient_xml_name_matches(attribute.key.as_ref(), b"idref") {
                        idref = Some(transient_xml_attribute_value_result(
                            attribute,
                            "transient EPUB package document",
                        )?);
                    }
                }
                let Some(idref) = idref else {
                    buffer.clear();
                    continue;
                };
                if let Some(item) = manifest.get(&idref) {
                    spine.push(item.clone());
                }
            }
            Ok(XmlEvent::Eof) => break,
            Err(error) => {
                return Err(format!(
                    "failed to parse transient EPUB package document: {error}"
                ));
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok(spine)
}

// PLACEHOLDER_DIVINA_IMAGE_HREF

pub(super) fn parse_transient_epub_divina_image_href(
    resource_bytes: &[u8],
    page_href: &str,
) -> Result<Option<String>, String> {
    let mut reader = XmlReader::from_reader(resource_bytes);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut inside_body = false;
    let mut text_len = 0usize;
    let mut image_sources = Vec::<String>::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(XmlEvent::Start(event)) => {
                if transient_xml_name_matches(event.name().as_ref(), b"body") {
                    inside_body = true;
                }
                collect_transient_epub_divina_image_source(
                    &event,
                    inside_body,
                    &mut image_sources,
                )?;
            }
            Ok(XmlEvent::Empty(event)) => {
                collect_transient_epub_divina_image_source(
                    &event,
                    inside_body,
                    &mut image_sources,
                )?;
            }
            Ok(XmlEvent::Text(text)) if inside_body => {
                text_len += String::from_utf8_lossy(text.as_ref())
                    .chars()
                    .filter(|character| !character.is_whitespace())
                    .count();
            }
            Ok(XmlEvent::CData(text)) if inside_body => {
                text_len += String::from_utf8_lossy(text.as_ref())
                    .chars()
                    .filter(|character| !character.is_whitespace())
                    .count();
            }
            Ok(XmlEvent::End(event))
                if transient_xml_name_matches(event.name().as_ref(), b"body") =>
            {
                inside_body = false;
            }
            Ok(XmlEvent::Eof) => break,
            Err(error) => {
                return Err(format!(
                    "failed to parse transient EPUB spine resource: {error}"
                ));
            }
            _ => {}
        }
        buffer.clear();
    }

    if text_len > EPUB_DIVINA_LETTER_COUNT_THRESHOLD {
        return Ok(None);
    }
    image_sources.sort();
    image_sources.dedup();
    if image_sources.len() > 1 {
        return Ok(None);
    }
    let Some(image_href) = image_sources.into_iter().next() else {
        return Ok(None);
    };

    Ok(Some(normalize_transient_epub_resource_href(
        page_href,
        &image_href,
    )))
}

fn collect_transient_epub_divina_image_source(
    event: &quick_xml::events::BytesStart<'_>,
    inside_body: bool,
    image_sources: &mut Vec<String>,
) -> Result<(), String> {
    if !inside_body {
        return Ok(());
    }

    let source = if transient_xml_name_matches(event.name().as_ref(), b"img") {
        transient_xml_attribute_value_checked(event, b"src", "transient EPUB spine resource")?
    } else if transient_xml_name_matches(event.name().as_ref(), b"image") {
        transient_xml_attribute_value_checked(event, b"href", "transient EPUB spine resource")?
    } else {
        None
    };
    if let Some(source) = source
        && !source.trim().is_empty()
    {
        image_sources.push(source);
    }
    Ok(())
}

fn is_transient_epub_html_media_type(media_type: &str) -> bool {
    matches!(
        media_type,
        "application/xhtml+xml" | "text/html" | "application/xml" | "text/xml"
    )
}

fn is_supported_transient_epub_raster_image_media_type(media_type: &str) -> bool {
    matches!(
        media_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "image/jpeg" | "image/jpg" | "image/png" | "image/gif" | "image/webp" | "image/avif"
    )
}

fn normalize_transient_epub_resource_href(rootfile_path: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or_default();
    if href.starts_with('/') {
        return normalize_transient_epub_zip_path(href);
    }
    let base = rootfile_path
        .rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or_default();
    let joined = if base.is_empty() {
        href.to_string()
    } else {
        format!("{base}/{href}")
    };
    normalize_transient_epub_zip_path(&joined)
}

pub(super) fn normalize_transient_epub_zip_path(path: &str) -> String {
    let normalized_path = path.replace('\\', "/");
    let mut normalized_segments = Vec::<&str>::new();
    for segment in normalized_path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                normalized_segments.pop();
            }
            _ => normalized_segments.push(segment),
        }
    }
    normalized_segments.join("/")
}

fn transient_xml_name_matches(actual: &[u8], expected: &[u8]) -> bool {
    actual == expected || actual.ends_with(expected)
}

fn transient_xml_attribute_value(
    event: &quick_xml::events::BytesStart<'_>,
    attribute_name: &[u8],
) -> Option<String> {
    event.attributes().flatten().find_map(|attribute| {
        transient_xml_name_matches(attribute.key.as_ref(), attribute_name).then(|| {
            attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .ok()
                .map(|value| value.into_owned())
        })?
    })
}

fn transient_xml_attribute_value_checked(
    event: &quick_xml::events::BytesStart<'_>,
    attribute_name: &[u8],
    document_name: &str,
) -> Result<Option<String>, String> {
    for attribute in event.attributes() {
        let attribute = attribute
            .map_err(|error| format!("failed to parse {document_name} attribute: {error}"))?;
        if transient_xml_name_matches(attribute.key.as_ref(), attribute_name) {
            return transient_xml_attribute_value_result(attribute, document_name).map(Some);
        }
    }
    Ok(None)
}

fn transient_xml_attribute_value_result(
    attribute: quick_xml::events::attributes::Attribute<'_>,
    document_name: &str,
) -> Result<String, String> {
    attribute
        .normalized_value(XmlVersion::Implicit1_0)
        .map(|value| value.into_owned())
        .map_err(|error| format!("failed to parse {document_name} attribute value: {error}"))
}

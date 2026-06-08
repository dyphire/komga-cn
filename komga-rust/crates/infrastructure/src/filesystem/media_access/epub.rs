use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Cursor, Read, Seek};
use std::path::Path;

use flate2::read::GzDecoder;
use komga_application::media_assets::{
    BookMediaRecord, EpubPositionsExtension, book_media_is_epub,
};
use quick_xml::Reader as XmlReader;
use quick_xml::XmlVersion;
use quick_xml::events::Event as XmlEvent;
use serde_json::{Value, json};
use zip::ZipArchive;

pub fn is_font_resource(resource_name: &str) -> bool {
    matches!(
        resource_name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .unwrap_or_default()
            .as_str(),
        "ttf" | "otf" | "woff" | "woff2"
    )
}

pub async fn read_epub_resource_bytes(epub_path: &Path, resource_name: &str) -> Option<Vec<u8>> {
    let path = epub_path.to_path_buf();
    let resource_name = resource_name.to_string();
    tokio::task::spawn_blocking(move || -> Option<Vec<u8>> {
        let file = File::open(&path).ok()?;
        let mut archive = ZipArchive::new(file).ok()?;
        read_zip_entry_bytes(&mut archive, &resource_name)
    })
    .await
    .ok()
    .flatten()
}

pub fn decode_epub_positions_extension(blob: &[u8]) -> Result<EpubPositionsExtension, String> {
    let mut decoder = GzDecoder::new(blob);
    let mut decoded = Vec::new();
    decoder
        .read_to_end(&mut decoded)
        .map_err(|error| format!("decode epub extension blob: {error}"))?;

    let extension = serde_json::from_slice::<Value>(&decoded)
        .map_err(|error| format!("parse epub extension blob json: {error}"))?;
    let positions = extension
        .get("positions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let is_fixed_layout = extension
        .get("isFixedLayout")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    Ok(EpubPositionsExtension {
        positions,
        is_fixed_layout,
    })
}

pub async fn load_epub_archive_positions(media: &BookMediaRecord) -> Option<Vec<Value>> {
    if !book_media_is_epub(media) {
        return None;
    }
    let path = media.file_path.clone();
    tokio::task::spawn_blocking(move || -> Option<Vec<Value>> {
        let file = File::open(&path).ok()?;
        let mut archive = ZipArchive::new(file).ok()?;
        let container_xml =
            read_zip_entry_bytes_normalized(&mut archive, "META-INF/container.xml")?;
        let rootfile_path = parse_epub_rootfile_path(&container_xml)?;
        let package_document = read_zip_entry_bytes_normalized(&mut archive, &rootfile_path)?;
        let spine_entries = parse_epub_spine_entries(&package_document, &rootfile_path);
        if spine_entries.is_empty() {
            return None;
        }
        let fixed_layout = parse_epub_fixed_layout(&package_document);
        let mut resources = spine_entries
            .into_iter()
            .map(|entry| {
                let bytes =
                    read_zip_entry_bytes_normalized(&mut archive, &entry.href).unwrap_or_default();
                let kobo_spans = if fixed_layout {
                    vec![]
                } else {
                    parse_epub_kobo_spans(&bytes)
                };
                (entry, bytes, kobo_spans)
            })
            .collect::<Vec<_>>();

        if !fixed_layout && resources.iter().all(|(_, _, spans)| spans.is_empty()) {
            let converted_kobo_spans = load_kepub_converted_spans(
                &path,
                resources
                    .iter()
                    .map(|(entry, _, _)| entry.clone())
                    .collect::<Vec<_>>()
                    .as_slice(),
            );
            for (entry, _, spans) in &mut resources {
                if let Some(converted) = converted_kobo_spans.get(&entry.href)
                    && !converted.is_empty()
                {
                    *spans = converted.clone();
                }
            }
        }

        let mut raw_positions = Vec::new();
        for (entry, bytes, spans) in resources {
            let position_count = if fixed_layout {
                1usize
            } else {
                ((bytes.len() as f64 / 1024.0).ceil() as usize).max(1)
            };

            for segment_index in 0..position_count {
                let progression = if fixed_layout {
                    0.0
                } else {
                    segment_index as f64 / position_count as f64
                };
                let kobo_span = if fixed_layout || position_count == 1 || segment_index == 0 {
                    Some("kobo.1.1".to_string())
                } else {
                    closest_kobo_span(&spans, progression)
                };
                raw_positions.push((
                    entry.href.clone(),
                    entry.media_type.clone(),
                    progression,
                    kobo_span,
                ));
            }
        }

        if raw_positions.is_empty() {
            return None;
        }

        let total_positions = raw_positions.len() as f64;
        Some(
            raw_positions
                .into_iter()
                .enumerate()
                .map(|(index, (href, media_type, progression, kobo_span))| {
                    let position = index + 1;
                    let mut locator = json!({
                        "href": href,
                        "type": media_type,
                        "locations": {
                            "position": position,
                            "progression": progression,
                            "totalProgression": position as f64 / total_positions,
                        },
                    });
                    if let Some(kobo_span) = kobo_span
                        && let Some(object) = locator.as_object_mut()
                    {
                        object.insert("koboSpan".to_string(), Value::String(kobo_span));
                    }
                    locator
                })
                .collect(),
        )
    })
    .await
    .ok()
    .flatten()
}

pub async fn load_epub_cover_bytes(media: &BookMediaRecord) -> Option<(Vec<u8>, String)> {
    if !book_media_is_epub(media) {
        return None;
    }
    let path = media.file_path.clone();
    tokio::task::spawn_blocking(move || -> Option<(Vec<u8>, String)> {
        let file = File::open(&path).ok()?;
        let mut archive = ZipArchive::new(file).ok()?;
        let container_xml =
            read_zip_entry_bytes_normalized(&mut archive, "META-INF/container.xml")?;
        let rootfile_path = parse_epub_rootfile_path(&container_xml)?;
        let package_document = read_zip_entry_bytes_normalized(&mut archive, &rootfile_path)?;
        let manifest = parse_epub_manifest_items(&package_document, &rootfile_path);
        let cover_item = manifest
            .values()
            .find(|item| {
                item.properties
                    .split_ascii_whitespace()
                    .any(|property| property.eq_ignore_ascii_case("cover-image"))
            })
            .cloned()
            .or_else(|| {
                parse_epub_metadata_cover_id(&package_document)
                    .and_then(|cover_id| manifest.get(&cover_id).cloned())
            })
            .or_else(|| {
                manifest
                    .values()
                    .find(|item| item.id == "cover-image")
                    .cloned()
            })?;

        let bytes = read_zip_entry_bytes_normalized(&mut archive, &cover_item.href)?;
        Some((bytes, cover_item.media_type))
    })
    .await
    .ok()
    .flatten()
}

pub async fn load_epub_package_document(media: &BookMediaRecord) -> Option<Vec<u8>> {
    if !book_media_is_epub(media) {
        return None;
    }
    let path = media.file_path.clone();
    tokio::task::spawn_blocking(move || -> Option<Vec<u8>> {
        let file = File::open(&path).ok()?;
        let mut archive = ZipArchive::new(file).ok()?;
        let container_xml =
            read_zip_entry_bytes_normalized(&mut archive, "META-INF/container.xml")?;
        let rootfile_path = parse_epub_rootfile_path(&container_xml)?;
        read_zip_entry_bytes_normalized(&mut archive, &rootfile_path)
    })
    .await
    .ok()
    .flatten()
}

fn read_zip_entry_bytes<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    entry_name: &str,
) -> Option<Vec<u8>> {
    let mut entry = archive.by_name(entry_name).ok()?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

fn read_zip_entry_bytes_normalized<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
) -> Option<Vec<u8>> {
    if let Some(bytes) = read_zip_entry_bytes(archive, path) {
        return Some(bytes);
    }

    let normalized = path.trim_start_matches('/');
    if normalized != path {
        return read_zip_entry_bytes(archive, normalized);
    }
    None
}

#[derive(Clone)]
struct EpubSpineEntry {
    href: String,
    media_type: String,
}

#[derive(Clone)]
struct EpubManifestItem {
    id: String,
    href: String,
    media_type: String,
    properties: String,
}

fn parse_epub_manifest_items(
    package_document: &[u8],
    rootfile_path: &str,
) -> HashMap<String, EpubManifestItem> {
    let mut reader = XmlReader::from_reader(package_document);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut manifest = HashMap::<String, EpubManifestItem>::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(XmlEvent::Start(event)) | Ok(XmlEvent::Empty(event)) => {
                if !xml_name_matches(event.name().as_ref(), b"item") {
                    buffer.clear();
                    continue;
                }
                let mut id = None::<String>;
                let mut href = None::<String>;
                let mut media_type = None::<String>;
                let mut properties = String::new();
                for attribute in event.attributes().flatten() {
                    if xml_name_matches(attribute.key.as_ref(), b"id") {
                        id = attribute
                            .normalized_value(XmlVersion::Implicit1_0)
                            .ok()
                            .map(|value| value.into_owned());
                    } else if xml_name_matches(attribute.key.as_ref(), b"href") {
                        href = attribute
                            .normalized_value(XmlVersion::Implicit1_0)
                            .ok()
                            .map(|value| value.into_owned());
                    } else if xml_name_matches(attribute.key.as_ref(), b"media-type") {
                        media_type = attribute
                            .normalized_value(XmlVersion::Implicit1_0)
                            .ok()
                            .map(|value| value.into_owned());
                    } else if xml_name_matches(attribute.key.as_ref(), b"properties") {
                        properties = attribute
                            .normalized_value(XmlVersion::Implicit1_0)
                            .ok()
                            .map(|value| value.into_owned())
                            .unwrap_or_default();
                    }
                }
                if let (Some(id), Some(href)) = (id, href) {
                    manifest.insert(
                        id.clone(),
                        EpubManifestItem {
                            id,
                            href: normalize_epub_resource_href(rootfile_path, &href),
                            media_type: media_type
                                .unwrap_or_else(|| "application/octet-stream".to_string()),
                            properties,
                        },
                    );
                }
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buffer.clear();
    }
    manifest
}

fn parse_epub_metadata_cover_id(package_document: &[u8]) -> Option<String> {
    let mut reader = XmlReader::from_reader(package_document);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(XmlEvent::Start(event)) | Ok(XmlEvent::Empty(event)) => {
                if !xml_name_matches(event.name().as_ref(), b"meta") {
                    buffer.clear();
                    continue;
                }
                let mut name = None::<String>;
                let mut content = None::<String>;
                for attribute in event.attributes().flatten() {
                    if xml_name_matches(attribute.key.as_ref(), b"name") {
                        name = attribute
                            .normalized_value(XmlVersion::Implicit1_0)
                            .ok()
                            .map(|value| value.into_owned());
                    } else if xml_name_matches(attribute.key.as_ref(), b"content") {
                        content = attribute
                            .normalized_value(XmlVersion::Implicit1_0)
                            .ok()
                            .map(|value| value.into_owned());
                    }
                }
                if name
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case("cover"))
                {
                    return content.filter(|value| !value.trim().is_empty());
                }
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buffer.clear();
    }
    None
}

fn load_kepub_converted_spans(
    epub_path: &Path,
    spine_entries: &[EpubSpineEntry],
) -> HashMap<String, Vec<(String, f64)>> {
    let converted_bytes = match komga_kepubify::convert_epub_file_to_bytes(epub_path) {
        Ok(bytes) => bytes,
        Err(_) => return HashMap::new(),
    };
    let mut archive = match ZipArchive::new(Cursor::new(converted_bytes)) {
        Ok(archive) => archive,
        Err(_) => return HashMap::new(),
    };
    let mut span_map = HashMap::new();
    for entry in spine_entries {
        let bytes = read_zip_entry_bytes_normalized(&mut archive, &entry.href).unwrap_or_default();
        let spans = parse_epub_kobo_spans(&bytes);
        if !spans.is_empty() {
            span_map.insert(entry.href.clone(), spans);
        }
    }
    span_map
}

fn parse_epub_rootfile_path(container_xml: &[u8]) -> Option<String> {
    let mut reader = XmlReader::from_reader(container_xml);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer).ok()? {
            XmlEvent::Start(event) | XmlEvent::Empty(event) => {
                if !xml_name_matches(event.name().as_ref(), b"rootfile") {
                    buffer.clear();
                    continue;
                }
                for attribute in event.attributes().flatten() {
                    if xml_name_matches(attribute.key.as_ref(), b"full-path") {
                        return attribute
                            .normalized_value(XmlVersion::Implicit1_0)
                            .ok()
                            .map(|value| normalize_epub_zip_path(value.as_ref()));
                    }
                }
            }
            XmlEvent::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    None
}

fn parse_epub_spine_entries(package_document: &[u8], rootfile_path: &str) -> Vec<EpubSpineEntry> {
    let mut reader = XmlReader::from_reader(package_document);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut manifest = HashMap::<String, EpubSpineEntry>::new();
    let mut spine = Vec::<String>::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(XmlEvent::Start(event)) | Ok(XmlEvent::Empty(event)) => {
                if xml_name_matches(event.name().as_ref(), b"item") {
                    let mut id = None::<String>;
                    let mut href = None::<String>;
                    let mut media_type = None::<String>;
                    for attribute in event.attributes().flatten() {
                        if xml_name_matches(attribute.key.as_ref(), b"id") {
                            id = attribute
                                .normalized_value(XmlVersion::Implicit1_0)
                                .ok()
                                .map(|value| value.into_owned());
                        } else if xml_name_matches(attribute.key.as_ref(), b"href") {
                            href = attribute
                                .normalized_value(XmlVersion::Implicit1_0)
                                .ok()
                                .map(|value| value.into_owned());
                        } else if xml_name_matches(attribute.key.as_ref(), b"media-type") {
                            media_type = attribute
                                .normalized_value(XmlVersion::Implicit1_0)
                                .ok()
                                .map(|value| value.into_owned());
                        }
                    }
                    if let (Some(id), Some(href)) = (id, href) {
                        manifest.insert(
                            id,
                            EpubSpineEntry {
                                href: normalize_epub_resource_href(rootfile_path, &href),
                                media_type: media_type
                                    .unwrap_or_else(|| "application/xhtml+xml".to_string()),
                            },
                        );
                    }
                } else if xml_name_matches(event.name().as_ref(), b"itemref") {
                    for attribute in event.attributes().flatten() {
                        if xml_name_matches(attribute.key.as_ref(), b"idref")
                            && let Some(idref) = attribute
                                .normalized_value(XmlVersion::Implicit1_0)
                                .ok()
                                .map(|value| value.into_owned())
                        {
                            spine.push(idref);
                        }
                    }
                }
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buffer.clear();
    }
    if spine.is_empty() {
        return vec![];
    }
    spine
        .into_iter()
        .filter_map(|idref| manifest.get(&idref).cloned())
        .collect()
}

pub fn parse_epub_fixed_layout(package_document: &[u8]) -> bool {
    let mut reader = XmlReader::from_reader(package_document);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut awaiting_rendition_layout_text = false;
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(XmlEvent::Start(event)) | Ok(XmlEvent::Empty(event)) => {
                if !xml_name_matches(event.name().as_ref(), b"meta") {
                    buffer.clear();
                    continue;
                }
                let mut property = None::<String>;
                let mut name = None::<String>;
                let mut content = None::<String>;
                for attribute in event.attributes().flatten() {
                    if xml_name_matches(attribute.key.as_ref(), b"property") {
                        property = attribute
                            .normalized_value(XmlVersion::Implicit1_0)
                            .ok()
                            .map(|value| value.into_owned());
                    } else if xml_name_matches(attribute.key.as_ref(), b"name") {
                        name = attribute
                            .normalized_value(XmlVersion::Implicit1_0)
                            .ok()
                            .map(|value| value.into_owned());
                    } else if xml_name_matches(attribute.key.as_ref(), b"content") {
                        content = attribute
                            .normalized_value(XmlVersion::Implicit1_0)
                            .ok()
                            .map(|value| value.into_owned());
                    }
                }
                if property
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case("rendition:layout"))
                {
                    if content
                        .as_deref()
                        .is_some_and(|value| value.eq_ignore_ascii_case("pre-paginated"))
                    {
                        return true;
                    }
                    awaiting_rendition_layout_text = true;
                }
                if name
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case("fixed-layout"))
                    && content
                        .as_deref()
                        .is_some_and(|value| value.eq_ignore_ascii_case("true"))
                {
                    return true;
                }
            }
            Ok(XmlEvent::Text(text)) if awaiting_rendition_layout_text => {
                let value = String::from_utf8_lossy(text.as_ref()).trim().to_string();
                if value.eq_ignore_ascii_case("pre-paginated") {
                    return true;
                }
            }
            Ok(XmlEvent::End(event)) if xml_name_matches(event.name().as_ref(), b"meta") => {
                awaiting_rendition_layout_text = false;
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buffer.clear();
    }
    false
}

pub fn parse_epub_kobo_spans(resource_bytes: &[u8]) -> Vec<(String, f64)> {
    let content = String::from_utf8_lossy(resource_bytes);
    if content.is_empty() {
        return vec![];
    }
    let mut spans = Vec::new();
    let mut seen = HashSet::new();
    let mut cursor = 0usize;
    let total_len = content.len().max(1) as f64;
    while let Some(relative_start) = content[cursor..].find("<span") {
        let span_start = cursor + relative_start;
        let Some(relative_end) = content[span_start..].find('>') else {
            break;
        };
        let span_end = span_start + relative_end;
        let tag = &content[span_start..=span_end];
        if !tag.to_ascii_lowercase().contains("kobospan") {
            cursor = span_end.saturating_add(1);
            if cursor >= content.len() {
                break;
            }
            continue;
        }
        let id = extract_html_attribute(tag, "id").unwrap_or_default();
        if id.starts_with("kobo.") && seen.insert(id.clone()) {
            let progression = (span_end as f64 / total_len).clamp(0.0, 1.0);
            spans.push((id, progression));
        }
        cursor = span_end.saturating_add(1);
        if cursor >= content.len() {
            break;
        }
    }
    spans
}

fn extract_html_attribute(tag: &str, attribute: &str) -> Option<String> {
    let double_quoted = format!("{attribute}=\"");
    if let Some(start) = tag.find(&double_quoted) {
        let value_start = start + double_quoted.len();
        let value_end = tag[value_start..].find('"')? + value_start;
        return Some(tag[value_start..value_end].to_string());
    }
    let single_quoted = format!("{attribute}='");
    if let Some(start) = tag.find(&single_quoted) {
        let value_start = start + single_quoted.len();
        let value_end = tag[value_start..].find('\'')? + value_start;
        return Some(tag[value_start..value_end].to_string());
    }
    None
}

fn closest_kobo_span(spans: &[(String, f64)], progression: f64) -> Option<String> {
    spans
        .iter()
        .min_by(|left, right| {
            let left_distance = (left.1 - progression).abs();
            let right_distance = (right.1 - progression).abs();
            left_distance
                .partial_cmp(&right_distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(id, _)| id.clone())
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
    normalize_epub_zip_path(joined.as_str())
}

fn normalize_epub_zip_path(path: &str) -> String {
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
    if normalized_segments.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", normalized_segments.join("/"))
    }
}

fn xml_name_matches(actual: &[u8], expected: &[u8]) -> bool {
    actual == expected || actual.ends_with(expected)
}

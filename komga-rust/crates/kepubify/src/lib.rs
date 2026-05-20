use std::collections::HashMap;
use std::fs;
use std::io::{Cursor, Read, Seek, Write};
use std::path::Path;

use quick_xml::Reader as XmlReader;
use quick_xml::XmlVersion;
use quick_xml::events::Event as XmlEvent;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

#[derive(Clone)]
struct EpubSpineEntry {
    href: String,
    media_type: String,
}

pub fn convert_epub_file_to_bytes(input_file: &Path) -> Result<Vec<u8>, String> {
    let file = fs::File::open(input_file).map_err(|error| format!("open epub file: {error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("open epub zip archive: {error}"))?;
    convert_epub_archive(&mut archive)
}

pub fn convert_epub_bytes(input: &[u8]) -> Result<Vec<u8>, String> {
    let source_cursor = Cursor::new(input);
    let mut archive = ZipArchive::new(source_cursor)
        .map_err(|error| format!("open epub zip archive: {error}"))?;
    convert_epub_archive(&mut archive)
}

fn convert_epub_archive<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Result<Vec<u8>, String> {
    let container_xml = load_zip_entry_by_normalized_name(archive, "/META-INF/container.xml")
        .ok_or_else(|| "epub archive missing META-INF/container.xml".to_string())?;

    let rootfile_path = parse_epub_rootfile_path(&container_xml)
        .ok_or_else(|| "epub container.xml missing rootfile path".to_string())?;

    let package_document = load_zip_entry_by_normalized_name(archive, rootfile_path.as_str())
        .ok_or_else(|| format!("epub package document not found: {rootfile_path}"))?;

    if parse_epub_fixed_layout(&package_document) {
        return write_zip_archive_with_replacements(archive, &HashMap::new());
    }

    let spine_entries = parse_epub_spine_entries(&package_document, &rootfile_path);
    if spine_entries.is_empty() {
        return write_zip_archive_with_replacements(archive, &HashMap::new());
    }

    let mut replacements = HashMap::<String, Vec<u8>>::new();
    let mut html_resources = 0usize;
    let mut already_converted_resources = 0usize;

    for (index, entry) in spine_entries.iter().enumerate() {
        if !media_type_is_html(entry.media_type.as_str()) {
            continue;
        }
        html_resources += 1;

        let Some(source_bytes) = load_zip_entry_by_normalized_name(archive, entry.href.as_str())
        else {
            continue;
        };

        match inject_kobo_span(source_bytes.as_slice(), index + 1) {
            InjectKoboSpanOutcome::Converted(converted) => {
                replacements.insert(entry.href.clone(), converted);
            }
            InjectKoboSpanOutcome::AlreadyConverted => {
                already_converted_resources += 1;
            }
            InjectKoboSpanOutcome::Unsupported => {}
        }
    }

    if html_resources > 0 && replacements.is_empty() && already_converted_resources == 0 {
        return Err(
            "epub conversion failed: no html spine resource could be converted".to_string(),
        );
    }

    write_zip_archive_with_replacements(archive, &replacements)
}

fn load_zip_entry_by_normalized_name<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    normalized_path: &str,
) -> Option<Vec<u8>> {
    for index in 0..archive.len() {
        let Ok(mut entry) = archive.by_index(index) else {
            continue;
        };
        let Ok(entry_name) = entry.name() else {
            continue;
        };
        if entry.is_dir() || normalize_zip_path(entry_name.as_ref()) != normalized_path {
            continue;
        }

        let mut bytes = Vec::new();
        if entry.read_to_end(&mut bytes).is_ok() {
            return Some(bytes);
        }
        return None;
    }

    None
}

fn write_zip_archive_with_replacements<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    replacements: &HashMap<String, Vec<u8>>,
) -> Result<Vec<u8>, String> {
    let writer_cursor = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(writer_cursor);

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("read epub zip entry #{index}: {error}"))?;

        let entry_name = entry
            .name()
            .map_err(|error| format!("read epub zip entry #{index} name: {error}"))?
            .into_owned();
        let normalized_name = normalize_zip_path(entry_name.as_str());

        let mut options = SimpleFileOptions::default().compression_method(entry.compression());
        if let Some(mode) = entry.unix_mode() {
            options = options.unix_permissions(mode);
        }

        if entry.is_dir() {
            writer
                .add_directory(entry_name.as_str(), options)
                .map_err(|error| format!("write epub directory entry {entry_name}: {error}"))?;
            continue;
        }

        writer
            .start_file(entry_name.as_str(), options)
            .map_err(|error| format!("write epub file entry {entry_name}: {error}"))?;

        if let Some(replacement) = replacements.get(&normalized_name) {
            writer
                .write_all(replacement)
                .map_err(|error| format!("write converted epub bytes for {entry_name}: {error}"))?;
            continue;
        }

        std::io::copy(&mut entry, &mut writer)
            .map_err(|error| format!("copy original epub bytes for {entry_name}: {error}"))?;
    }

    writer
        .finish()
        .map_err(|error| format!("finalize epub zip archive: {error}"))
        .map(|cursor| cursor.into_inner())
}

fn media_type_is_html(media_type: &str) -> bool {
    let normalized = media_type.trim().to_ascii_lowercase();
    matches!(normalized.as_str(), "application/xhtml+xml" | "text/html")
}

enum InjectKoboSpanOutcome {
    Converted(Vec<u8>),
    AlreadyConverted,
    Unsupported,
}

fn inject_kobo_span(resource_bytes: &[u8], chapter_index: usize) -> InjectKoboSpanOutcome {
    let content = match std::str::from_utf8(resource_bytes) {
        Ok(content) => content,
        Err(_) => {
            return InjectKoboSpanOutcome::Unsupported;
        }
    };

    if content.is_empty() {
        return InjectKoboSpanOutcome::Unsupported;
    }

    if contains_kobo_span(content) {
        return InjectKoboSpanOutcome::AlreadyConverted;
    }

    let lower = content.to_ascii_lowercase();

    let Some(body_start) = lower.find("<body") else {
        return InjectKoboSpanOutcome::Unsupported;
    };
    let Some(body_open_end_relative) = content[body_start..].find('>') else {
        return InjectKoboSpanOutcome::Unsupported;
    };
    let body_open_end = body_start + body_open_end_relative + 1;

    let body_close = lower[body_open_end..]
        .find("</body>")
        .map(|index| body_open_end + index)
        .unwrap_or(content.len());

    let before_body = &content[..body_open_end];
    let body_content = &content[body_open_end..body_close];
    let after_body = &content[body_close..];

    let (mut converted_body, inserted_count) = inject_spans_into_body(body_content, chapter_index);
    if inserted_count == 0 {
        let marker = kobo_span_marker(chapter_index, 1);
        let mut combined = String::with_capacity(converted_body.len() + marker.len());
        combined.push_str(marker.as_str());
        combined.push_str(converted_body.as_str());
        converted_body = combined;
    }

    let mut converted = String::with_capacity(content.len() + converted_body.len() / 10);
    converted.push_str(before_body);
    converted.push_str(converted_body.as_str());
    converted.push_str(after_body);
    InjectKoboSpanOutcome::Converted(converted.into_bytes())
}

fn contains_kobo_span(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    let mut cursor = 0usize;

    while let Some(relative_start) = lower[cursor..].find("<span") {
        let span_start = cursor + relative_start;
        let Some(relative_end) = content[span_start..].find('>') else {
            break;
        };
        let span_end = span_start + relative_end + 1;
        let tag = &content[span_start..span_end];
        if let Some(classes) = extract_html_attribute(tag, "class")
            && classes
                .split_ascii_whitespace()
                .any(|class_name| class_name.eq_ignore_ascii_case("koboSpan"))
        {
            return true;
        }

        cursor = span_end;
        if cursor >= content.len() {
            break;
        }
    }

    false
}

fn inject_spans_into_body(body: &str, chapter_index: usize) -> (String, usize) {
    let mut result = String::with_capacity(body.len() + 128);
    let mut cursor = 0usize;
    let mut span_index = 1usize;

    while let Some(relative_start) = body[cursor..].find('<') {
        let tag_start = cursor + relative_start;
        result.push_str(&body[cursor..tag_start]);

        let Some(relative_end) = body[tag_start..].find('>') else {
            result.push_str(&body[tag_start..]);
            return (result, span_index.saturating_sub(1));
        };

        let tag_end = tag_start + relative_end + 1;
        let tag = &body[tag_start..tag_end];
        result.push_str(tag);

        if html_tag_should_get_kobo_span(tag) {
            result.push_str(kobo_span_marker(chapter_index, span_index).as_str());
            span_index += 1;
        }

        cursor = tag_end;
        if cursor >= body.len() {
            break;
        }
    }

    result.push_str(&body[cursor..]);
    (result, span_index.saturating_sub(1))
}

fn html_tag_should_get_kobo_span(tag: &str) -> bool {
    let inner = tag
        .trim_start_matches('<')
        .trim_start()
        .trim_end_matches('>')
        .trim_start();
    if inner.starts_with('/') || inner.starts_with('!') || inner.starts_with('?') {
        return false;
    }

    let mut name_end = 0usize;
    for character in inner.chars() {
        if character.is_ascii_alphanumeric() {
            name_end += character.len_utf8();
        } else {
            break;
        }
    }
    if name_end == 0 {
        return false;
    }

    let tag_name = inner[..name_end].to_ascii_lowercase();
    matches!(
        tag_name.as_str(),
        "p" | "li"
            | "blockquote"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "img"
            | "table"
            | "tr"
            | "td"
            | "th"
    )
}

fn kobo_span_marker(chapter_index: usize, span_index: usize) -> String {
    format!("<span class=\"koboSpan\" id=\"kobo.{chapter_index}.{span_index}\"></span>")
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
                            .map(|value| normalize_zip_path(value.as_ref()));
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
                                .map(|value| value.to_string());
                        } else if xml_name_matches(attribute.key.as_ref(), b"href") {
                            href = attribute
                                .normalized_value(XmlVersion::Implicit1_0)
                                .ok()
                                .map(|value| value.to_string());
                        } else if xml_name_matches(attribute.key.as_ref(), b"media-type") {
                            media_type = attribute
                                .normalized_value(XmlVersion::Implicit1_0)
                                .ok()
                                .map(|value| value.to_string());
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
                                .map(|value| value.to_string())
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
        .collect::<Vec<_>>()
}

fn parse_epub_fixed_layout(package_document: &[u8]) -> bool {
    let mut reader = XmlReader::from_reader(package_document);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut awaiting_rendition_layout_text = false;

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(XmlEvent::Start(event)) => {
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
                            .map(|value| value.to_string());
                    } else if xml_name_matches(attribute.key.as_ref(), b"name") {
                        name = attribute
                            .normalized_value(XmlVersion::Implicit1_0)
                            .ok()
                            .map(|value| value.to_string());
                    } else if xml_name_matches(attribute.key.as_ref(), b"content") {
                        content = attribute
                            .normalized_value(XmlVersion::Implicit1_0)
                            .ok()
                            .map(|value| value.to_string());
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
            Ok(XmlEvent::Empty(event)) => {
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
                            .map(|value| value.to_string());
                    } else if xml_name_matches(attribute.key.as_ref(), b"name") {
                        name = attribute
                            .normalized_value(XmlVersion::Implicit1_0)
                            .ok()
                            .map(|value| value.to_string());
                    } else if xml_name_matches(attribute.key.as_ref(), b"content") {
                        content = attribute
                            .normalized_value(XmlVersion::Implicit1_0)
                            .ok()
                            .map(|value| value.to_string());
                    }
                }

                if property
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case("rendition:layout"))
                    && content
                        .as_deref()
                        .is_some_and(|value| value.eq_ignore_ascii_case("pre-paginated"))
                {
                    return true;
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

fn normalize_epub_resource_href(rootfile_path: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or_default();

    if href.starts_with('/') {
        return normalize_zip_path(href);
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

    normalize_zip_path(joined.as_str())
}

fn normalize_zip_path(path: &str) -> String {
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

#[cfg(test)]
fn read_zip_entry_bytes<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
) -> Option<Vec<u8>> {
    if let Ok(mut entry) = archive.by_name(path) {
        let mut bytes = Vec::new();
        if entry.read_to_end(&mut bytes).is_ok() {
            return Some(bytes);
        }
    }

    let normalized = path.trim_start_matches('/');
    if normalized != path
        && let Ok(mut entry) = archive.by_name(normalized)
    {
        let mut bytes = Vec::new();
        if entry.read_to_end(&mut bytes).is_ok() {
            return Some(bytes);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::CompressionMethod;

    fn build_epub_bytes(chapter: &str, opf_metadata: &str, media_type: &str) -> Vec<u8> {
        let writer = Cursor::new(Vec::new());
        let mut zip = ZipWriter::new(writer);

        let store = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip.start_file("mimetype", store)
            .expect("mimetype entry should be writable");
        zip.write_all(b"application/epub+zip")
            .expect("mimetype payload should be writable");

        let default = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        zip.add_directory("META-INF/", default)
            .expect("META-INF directory should be writable");
        zip.start_file("META-INF/container.xml", default)
            .expect("container.xml should be writable");
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
        )
        .expect("container.xml payload should be writable");

        zip.add_directory("OPS/", default)
            .expect("OPS directory should be writable");
        zip.start_file("OPS/content.opf", default)
            .expect("content.opf should be writable");
        let opf = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<package version="3.0" xmlns="http://www.idpf.org/2007/opf">
  <metadata>{opf_metadata}</metadata>
  <manifest>
    <item id="chapter-1" href="chapter-1.xhtml" media-type="{media_type}"/>
  </manifest>
  <spine>
    <itemref idref="chapter-1"/>
  </spine>
</package>"#,
        );
        zip.write_all(opf.as_bytes())
            .expect("content.opf payload should be writable");

        zip.start_file("OPS/chapter-1.xhtml", default)
            .expect("chapter-1.xhtml should be writable");
        zip.write_all(chapter.as_bytes())
            .expect("chapter payload should be writable");

        zip.finish()
            .expect("epub archive should finalize")
            .into_inner()
    }

    #[test]
    fn convert_epub_bytes_injects_kobospan_when_missing() {
        let source = build_epub_bytes(
            "<html><body><p>hello world</p></body></html>",
            "",
            "application/xhtml+xml",
        );

        let converted = convert_epub_bytes(&source).expect("conversion should succeed");
        let mut archive =
            ZipArchive::new(Cursor::new(converted)).expect("converted archive should open");
        let chapter = read_zip_entry_bytes(&mut archive, "/OPS/chapter-1.xhtml")
            .expect("converted chapter should be available");
        let chapter = String::from_utf8(chapter).expect("chapter should be utf8");

        assert!(chapter.contains("class=\"koboSpan\""));
        assert!(chapter.contains("id=\"kobo.1.1\""));
    }

    #[test]
    fn convert_epub_bytes_keeps_existing_kobospan() {
        let source = build_epub_bytes(
            "<html><body><span class=\"koboSpan\" id=\"kobo.1.1\"></span><p>already converted</p></body></html>",
            "",
            "application/xhtml+xml",
        );

        let converted = convert_epub_bytes(&source).expect("conversion should succeed");
        let mut archive =
            ZipArchive::new(Cursor::new(converted)).expect("converted archive should open");
        let chapter = read_zip_entry_bytes(&mut archive, "/OPS/chapter-1.xhtml")
            .expect("converted chapter should be available");
        let chapter = String::from_utf8(chapter).expect("chapter should be utf8");

        assert_eq!(chapter.matches("class=\"koboSpan\"").count(), 1);
    }

    #[test]
    fn convert_epub_bytes_skips_fixed_layout_publications() {
        let source = build_epub_bytes(
            "<html><body><p>fixed layout page</p></body></html>",
            "<meta property=\"rendition:layout\">pre-paginated</meta>",
            "application/xhtml+xml",
        );

        let converted = convert_epub_bytes(&source).expect("conversion should succeed");
        let mut archive =
            ZipArchive::new(Cursor::new(converted)).expect("converted archive should open");
        let chapter = read_zip_entry_bytes(&mut archive, "/OPS/chapter-1.xhtml")
            .expect("converted chapter should be available");
        let chapter = String::from_utf8(chapter).expect("chapter should be utf8");

        assert!(!chapter.contains("koboSpan"));
    }

    #[test]
    fn convert_epub_bytes_injects_multiple_spans_for_multiple_blocks() {
        let source = build_epub_bytes(
            "<html><body><p>first</p><p>second</p></body></html>",
            "",
            "application/xhtml+xml",
        );

        let converted = convert_epub_bytes(&source).expect("conversion should succeed");
        let mut archive =
            ZipArchive::new(Cursor::new(converted)).expect("converted archive should open");
        let chapter = read_zip_entry_bytes(&mut archive, "/OPS/chapter-1.xhtml")
            .expect("converted chapter should be available");
        let chapter = String::from_utf8(chapter).expect("chapter should be utf8");

        assert!(chapter.contains("id=\"kobo.1.1\""));
        assert!(chapter.contains("id=\"kobo.1.2\""));
    }

    #[test]
    fn convert_epub_bytes_skips_non_html_spine_resources() {
        let source = build_epub_bytes(
            "<html><body><p>xml resource</p></body></html>",
            "",
            "application/xml",
        );

        let converted = convert_epub_bytes(&source).expect("conversion should succeed");
        let mut archive =
            ZipArchive::new(Cursor::new(converted)).expect("converted archive should open");
        let chapter = read_zip_entry_bytes(&mut archive, "/OPS/chapter-1.xhtml")
            .expect("converted chapter should be available");
        let chapter = String::from_utf8(chapter).expect("chapter should be utf8");

        assert!(!chapter.contains("koboSpan"));
    }

    #[test]
    fn convert_epub_bytes_errors_when_html_spine_cannot_be_converted() {
        let source = build_epub_bytes(
            "<html><p>missing body</p></html>",
            "",
            "application/xhtml+xml",
        );

        let result = convert_epub_bytes(&source);
        assert!(result.is_err());
    }
}

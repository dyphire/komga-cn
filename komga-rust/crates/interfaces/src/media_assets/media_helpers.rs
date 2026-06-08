use super::*;
#[cfg(test)]
use std::collections::HashSet;

#[cfg(test)]
use quick_xml::Reader as XmlReader;
#[cfg(test)]
use quick_xml::XmlVersion;
#[cfg(test)]
use quick_xml::events::Event as XmlEvent;

pub(crate) fn content_type_from_filename(file_name: &str, default_mime_type: &str) -> String {
    let extension = file_name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();

    match extension.as_str() {
        "cbz" => "application/vnd.comicbook+zip".to_string(),
        "zip" => "application/zip".to_string(),
        "cbr" => "application/vnd.comicbook-rar".to_string(),
        "pdf" => "application/pdf".to_string(),
        "epub" => "application/epub+zip".to_string(),
        "jpg" | "jpeg" => "image/jpeg".to_string(),
        "png" => "image/png".to_string(),
        "gif" => "image/gif".to_string(),
        "webp" => "image/webp".to_string(),
        "avif" => "image/avif".to_string(),
        _ => default_mime_type.to_string(),
    }
}

pub(crate) fn book_media_supports_page_image(media: &PersistedBookMedia) -> bool {
    content_type_from_filename(&media.file_name, &media.media_type).starts_with("image/")
}

pub(crate) fn book_media_is_single_image(media: &PersistedBookMedia) -> bool {
    book_media_supports_page_image(media)
}

pub(crate) fn book_media_is_epub(media: &PersistedBookMedia) -> bool {
    content_type_from_filename(&media.file_name, &media.media_type) == "application/epub+zip"
}

pub(crate) fn book_media_is_pdf(media: &PersistedBookMedia) -> bool {
    content_type_from_filename(&media.file_name, &media.media_type) == "application/pdf"
}

#[cfg(test)]
pub(super) fn parse_epub_kobo_spans(resource_bytes: &[u8]) -> Vec<(String, f64)> {
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

#[cfg(test)]
pub(super) fn parse_epub_fixed_layout(package_document: &[u8]) -> bool {
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

#[cfg(test)]
pub(super) fn normalize_epub_resource_href(rootfile_path: &str, href: &str) -> String {
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
fn xml_name_matches(actual: &[u8], expected: &[u8]) -> bool {
    actual == expected || actual.ends_with(expected)
}

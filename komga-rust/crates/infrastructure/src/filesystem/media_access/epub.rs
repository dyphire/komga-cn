use std::collections::HashMap;
use std::fs::File;
use std::io::ErrorKind;
use std::io::{Read, Seek};
use std::path::Path;

use flate2::read::GzDecoder;
use komga_application::media_assets::{
    BookMediaRecord, EpubCoverImage, EpubNavigationExtension, EpubNavigationLink,
    EpubNavigationPosition, book_media_is_epub,
};
use quick_xml::Reader as XmlReader;
use quick_xml::XmlVersion;
use quick_xml::events::Event as XmlEvent;
use serde_json::Value;
use zip::ZipArchive;
use zip::result::ZipError;

pub(crate) async fn read_epub_resource_bytes(
    epub_path: &Path,
    resource_name: &str,
) -> Result<Option<Vec<u8>>, String> {
    let path = epub_path.to_path_buf();
    let resource_name = resource_name.to_string();
    let display_path = path.display().to_string();
    tokio::task::spawn_blocking(move || -> Result<Option<Vec<u8>>, String> {
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("open EPUB '{}': {error}", path.display())),
        };
        let mut archive = ZipArchive::new(file)
            .map_err(|error| format!("open EPUB archive '{}': {error}", path.display()))?;
        read_zip_entry_bytes_result(&mut archive, &resource_name, &path)
    })
    .await
    .map_err(|error| format!("join EPUB resource read for '{display_path}': {error}"))?
}

pub(crate) fn decode_epub_navigation_extension(
    blob: &[u8],
) -> Result<EpubNavigationExtension, String> {
    let extension = decode_epub_extension_json(blob)?;
    let positions = extension
        .get("positions")
        .and_then(Value::as_array)
        .map(|positions| {
            positions
                .iter()
                .cloned()
                .map(EpubNavigationPosition::from_raw)
                .collect()
        })
        .unwrap_or_default();
    let is_fixed_layout = extension
        .get("isFixedLayout")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    Ok(EpubNavigationExtension {
        positions,
        is_fixed_layout,
        toc: epub_navigation_links(&extension, "toc"),
        landmarks: epub_navigation_links(&extension, "landmarks"),
        page_list: epub_navigation_links(&extension, "pageList"),
    })
}

fn decode_epub_extension_json(blob: &[u8]) -> Result<Value, String> {
    let mut decoder = GzDecoder::new(blob);
    let mut decoded = Vec::new();
    decoder
        .read_to_end(&mut decoded)
        .map_err(|error| format!("decode epub extension blob: {error}"))?;

    serde_json::from_slice::<Value>(&decoded)
        .map_err(|error| format!("parse epub extension blob json: {error}"))
}

fn epub_navigation_links(extension: &Value, field_name: &str) -> Vec<EpubNavigationLink> {
    extension
        .get(field_name)
        .and_then(Value::as_array)
        .map(|links| links.iter().filter_map(epub_navigation_link).collect())
        .unwrap_or_default()
}

fn epub_navigation_link(value: &Value) -> Option<EpubNavigationLink> {
    let entry = value.as_object()?;
    Some(EpubNavigationLink {
        title: entry
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_string),
        href: entry
            .get("href")
            .and_then(Value::as_str)
            .map(str::to_string),
        children: entry
            .get("children")
            .and_then(Value::as_array)
            .map(|children| children.iter().filter_map(epub_navigation_link).collect())
            .unwrap_or_default(),
    })
}

pub(crate) async fn load_epub_cover_bytes(
    media: &BookMediaRecord,
) -> Result<Option<EpubCoverImage>, String> {
    if !book_media_is_epub(media) {
        return Ok(None);
    }
    let path = media.file_path.clone();
    let display_path = path.display().to_string();
    tokio::task::spawn_blocking(move || -> Result<Option<EpubCoverImage>, String> {
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("open EPUB '{}': {error}", path.display())),
        };
        let mut archive = ZipArchive::new(file)
            .map_err(|error| format!("open EPUB archive '{}': {error}", path.display()))?;
        let Some(container_xml) =
            read_zip_entry_bytes_normalized_result(&mut archive, "META-INF/container.xml", &path)?
        else {
            return Ok(None);
        };
        let Some(rootfile_path) = parse_epub_rootfile_path(&container_xml)? else {
            return Ok(None);
        };
        let Some(package_document) =
            read_zip_entry_bytes_normalized_result(&mut archive, &rootfile_path, &path)?
        else {
            return Ok(None);
        };
        let manifest = parse_epub_manifest_items(&package_document, &rootfile_path)?;
        let metadata_cover_item = parse_epub_metadata_cover_id(&package_document)?
            .and_then(|cover_id| manifest.get(&cover_id).cloned());
        let Some(cover_item) = manifest
            .values()
            .find(|item| {
                item.properties
                    .split_ascii_whitespace()
                    .any(|property| property.eq_ignore_ascii_case("cover-image"))
            })
            .cloned()
            .or(metadata_cover_item)
            .or_else(|| {
                manifest
                    .values()
                    .find(|item| item.id == "cover-image")
                    .cloned()
            })
        else {
            return Ok(None);
        };

        let Some(bytes) =
            read_zip_entry_bytes_normalized_result(&mut archive, &cover_item.href, &path)?
        else {
            return Ok(None);
        };
        Ok(Some(EpubCoverImage {
            bytes,
            media_type: cover_item.media_type,
        }))
    })
    .await
    .map_err(|error| format!("join EPUB cover read for '{display_path}': {error}"))?
}

pub(crate) async fn load_epub_package_document(
    media: &BookMediaRecord,
) -> Result<Option<Vec<u8>>, String> {
    if !book_media_is_epub(media) {
        return Ok(None);
    }
    let path = media.file_path.clone();
    let display_path = path.display().to_string();
    let result = tokio::task::spawn_blocking(move || -> Result<Option<Vec<u8>>, String> {
        let file = File::open(&path).map_err(|error| {
            format!(
                "failed to open EPUB package source '{}': {error}",
                path.display()
            )
        })?;
        let mut archive = ZipArchive::new(file).map_err(|error| {
            format!(
                "failed to open EPUB package archive '{}': {error}",
                path.display()
            )
        })?;
        let container_xml = read_zip_entry_bytes_normalized_required(
            &mut archive,
            "META-INF/container.xml",
            path.as_path(),
        )?;
        let rootfile_path = parse_epub_rootfile_path(&container_xml)?.ok_or_else(|| {
            format!(
                "failed to resolve EPUB package document rootfile in '{}'",
                path.display()
            )
        })?;
        let package_document =
            read_zip_entry_bytes_normalized_required(&mut archive, &rootfile_path, path.as_path())?;
        Ok(Some(package_document))
    })
    .await
    .map_err(|error| {
        format!("failed to join EPUB package document load for '{display_path}': {error}")
    })?;
    result
        .map_err(|error| format!("failed to load EPUB package document '{display_path}': {error}"))
}

fn read_zip_entry_bytes_normalized_result<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
    archive_path: &Path,
) -> Result<Option<Vec<u8>>, String> {
    if let Some(bytes) = read_zip_entry_bytes_result(archive, path, archive_path)? {
        return Ok(Some(bytes));
    }

    let normalized = path.trim_start_matches('/');
    if normalized != path {
        return read_zip_entry_bytes_result(archive, normalized, archive_path);
    }
    Ok(None)
}

fn read_zip_entry_bytes_result<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    entry_name: &str,
    archive_path: &Path,
) -> Result<Option<Vec<u8>>, String> {
    let mut entry = match archive.by_name(entry_name) {
        Ok(entry) => entry,
        Err(ZipError::FileNotFound) => return Ok(None),
        Err(error) => {
            return Err(format!(
                "failed to open EPUB archive entry '{entry_name}' from '{}': {error}",
                archive_path.display()
            ));
        }
    };
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).map_err(|error| {
        format!(
            "failed to read EPUB archive entry '{entry_name}' from '{}': {error}",
            archive_path.display()
        )
    })?;
    Ok(Some(bytes))
}

fn read_zip_entry_bytes_normalized_required<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
    archive_path: &Path,
) -> Result<Vec<u8>, String> {
    if let Some(bytes) = read_zip_entry_bytes_result(archive, path, archive_path)? {
        return Ok(bytes);
    }

    let normalized = path.trim_start_matches('/');
    if normalized != path
        && let Some(bytes) = read_zip_entry_bytes_result(archive, normalized, archive_path)?
    {
        return Ok(bytes);
    }

    Err(format!(
        "missing EPUB archive entry '{path}' in '{}'",
        archive_path.display()
    ))
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
) -> Result<HashMap<String, EpubManifestItem>, String> {
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
                for attribute in event.attributes() {
                    let attribute = attribute.map_err(|error| {
                        format!("failed to parse EPUB package document attribute: {error}")
                    })?;
                    if xml_name_matches(attribute.key.as_ref(), b"id") {
                        id = Some(epub_attribute_value(attribute, "EPUB package document")?);
                    } else if xml_name_matches(attribute.key.as_ref(), b"href") {
                        href = Some(epub_attribute_value(attribute, "EPUB package document")?);
                    } else if xml_name_matches(attribute.key.as_ref(), b"media-type") {
                        media_type =
                            Some(epub_attribute_value(attribute, "EPUB package document")?);
                    } else if xml_name_matches(attribute.key.as_ref(), b"properties") {
                        properties = epub_attribute_value(attribute, "EPUB package document")?;
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
            Err(error) => {
                return Err(format!("failed to parse EPUB package document: {error}"));
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok(manifest)
}

fn parse_epub_metadata_cover_id(package_document: &[u8]) -> Result<Option<String>, String> {
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
                for attribute in event.attributes() {
                    let attribute = attribute.map_err(|error| {
                        format!("failed to parse EPUB package document attribute: {error}")
                    })?;
                    if xml_name_matches(attribute.key.as_ref(), b"name") {
                        name = Some(epub_attribute_value(attribute, "EPUB package document")?);
                    } else if xml_name_matches(attribute.key.as_ref(), b"content") {
                        content = Some(epub_attribute_value(attribute, "EPUB package document")?);
                    }
                }
                if name
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case("cover"))
                {
                    return Ok(content.filter(|value| !value.trim().is_empty()));
                }
            }
            Ok(XmlEvent::Eof) => break,
            Err(error) => {
                return Err(format!("failed to parse EPUB package document: {error}"));
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok(None)
}

fn parse_epub_rootfile_path(container_xml: &[u8]) -> Result<Option<String>, String> {
    let mut reader = XmlReader::from_reader(container_xml);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(XmlEvent::Start(event)) | Ok(XmlEvent::Empty(event)) => {
                if !xml_name_matches(event.name().as_ref(), b"rootfile") {
                    buffer.clear();
                    continue;
                }
                for attribute in event.attributes() {
                    let attribute = attribute.map_err(|error| {
                        format!("failed to parse EPUB container document attribute: {error}")
                    })?;
                    if xml_name_matches(attribute.key.as_ref(), b"full-path") {
                        let path = epub_attribute_value(attribute, "EPUB container document")?;
                        return Ok(Some(normalize_epub_zip_path(path.as_str())));
                    }
                }
            }
            Ok(XmlEvent::Eof) => break,
            Err(error) => {
                return Err(format!("failed to parse EPUB container document: {error}"));
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok(None)
}

fn epub_attribute_value(
    attribute: quick_xml::events::attributes::Attribute<'_>,
    document_name: &str,
) -> Result<String, String> {
    attribute
        .normalized_value(XmlVersion::Implicit1_0)
        .map(|value| value.into_owned())
        .map_err(|error| format!("failed to parse {document_name} attribute value: {error}"))
}

pub(crate) fn parse_epub_fixed_layout(package_document: &[u8]) -> bool {
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

pub(crate) fn normalize_epub_resource_href(rootfile_path: &str, href: &str) -> String {
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use flate2::Compression;
    use flate2::write::GzEncoder;
    use komga_application::media_assets::BookMediaRecord;
    use serde_json::{Value, json};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    use super::{
        decode_epub_navigation_extension, load_epub_cover_bytes, normalize_epub_resource_href,
        parse_epub_fixed_layout,
    };

    fn unique_temp_path(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
    }

    fn build_test_zip_archive(entries: Vec<(String, Vec<u8>)>) -> Result<Vec<u8>, String> {
        let cursor = std::io::Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(cursor);

        for (file_name, bytes) in entries {
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            writer
                .start_file(file_name.as_str(), options)
                .map_err(|error| format!("start zip entry '{file_name}': {error}"))?;
            writer
                .write_all(&bytes)
                .map_err(|error| format!("write zip entry '{file_name}': {error}"))?;
        }

        writer
            .finish()
            .map(|cursor| cursor.into_inner())
            .map_err(|error| format!("finalize zip archive: {error}"))
    }

    fn epub_media(file_path: PathBuf) -> BookMediaRecord {
        BookMediaRecord {
            library_id: "lib".to_string(),
            file_name: "book.epub".to_string(),
            file_path,
            media_type: "application/epub+zip".to_string(),
            page_count: 0,
        }
    }

    fn basic_container_xml() -> &'static str {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#
    }

    #[tokio::test]
    async fn load_epub_cover_bytes_extracts_manifest_cover_image() {
        let file_path = unique_temp_path("komga-media-epub-cover");
        let package_document = r#"<?xml version="1.0" encoding="UTF-8"?>
<package version="3.0" xmlns="http://www.idpf.org/2007/opf">
  <manifest>
    <item id="cover" href="images/cover.png" media-type="image/png" properties="cover-image"/>
    <item id="chap-1" href="chapter-1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="chap-1"/>
  </spine>
</package>"#;
        let archive = build_test_zip_archive(vec![
            (
                "META-INF/container.xml".to_string(),
                basic_container_xml().as_bytes().to_vec(),
            ),
            (
                "OEBPS/content.opf".to_string(),
                package_document.as_bytes().to_vec(),
            ),
            (
                "OEBPS/images/cover.png".to_string(),
                b"cover-bytes".to_vec(),
            ),
        ])
        .expect("epub archive should be created");
        fs::write(&file_path, archive).expect("epub test file should be written");

        let cover = load_epub_cover_bytes(&epub_media(file_path.clone()))
            .await
            .expect("epub cover bytes should be readable")
            .expect("epub cover should exist");
        assert_eq!(cover.bytes, b"cover-bytes");
        assert_eq!(cover.media_type, "image/png");

        let _ = fs::remove_file(file_path);
    }

    #[tokio::test]
    async fn read_epub_resource_bytes_reports_invalid_archive_errors() {
        let file_path = unique_temp_path("komga-media-invalid-epub-resource");
        fs::write(&file_path, b"not a zip").expect("invalid epub test file should be written");

        let error = super::read_epub_resource_bytes(&file_path, "OPS/chapter.xhtml")
            .await
            .expect_err("invalid EPUB archive should be reported as an error");

        assert!(
            error.contains("open EPUB archive"),
            "unexpected error: {error}"
        );
        let _ = fs::remove_file(file_path);
    }

    #[tokio::test]
    async fn read_epub_resource_bytes_keeps_missing_entries_as_absent() {
        let file_path = unique_temp_path("komga-media-missing-epub-resource");
        let archive = build_test_zip_archive(vec![(
            "OPS/chapter-1.xhtml".to_string(),
            b"chapter".to_vec(),
        )])
        .expect("epub archive should be created");
        fs::write(&file_path, archive).expect("epub test file should be written");

        let bytes = super::read_epub_resource_bytes(&file_path, "OPS/missing.xhtml")
            .await
            .expect("missing EPUB entry should not fail");

        assert_eq!(bytes, None);
        let _ = fs::remove_file(file_path);
    }

    #[tokio::test]
    async fn load_epub_cover_bytes_reports_invalid_archive_errors() {
        let file_path = unique_temp_path("komga-media-invalid-epub-cover");
        fs::write(&file_path, b"not a zip").expect("invalid epub test file should be written");

        let error = load_epub_cover_bytes(&epub_media(file_path.clone()))
            .await
            .expect_err("invalid EPUB archive should be reported as an error");

        assert!(
            error.contains("open EPUB archive"),
            "unexpected error: {error}"
        );
        let _ = fs::remove_file(file_path);
    }

    #[tokio::test]
    async fn load_epub_cover_bytes_reports_malformed_container_attributes() {
        let file_path = unique_temp_path("komga-media-malformed-epub-container");
        let archive = build_test_zip_archive(vec![(
            "META-INF/container.xml".to_string(),
            br#"<container><rootfiles><rootfile full-path= /></rootfiles></container>"#.to_vec(),
        )])
        .expect("epub archive should be created");
        fs::write(&file_path, archive).expect("epub test file should be written");

        let error = load_epub_cover_bytes(&epub_media(file_path.clone()))
            .await
            .expect_err("malformed EPUB container should be reported as an error");

        assert!(
            error.contains("EPUB container document"),
            "unexpected error: {error}"
        );
        let _ = fs::remove_file(file_path);
    }

    #[tokio::test]
    async fn load_epub_cover_bytes_reports_malformed_package_attributes() {
        let file_path = unique_temp_path("komga-media-malformed-epub-package");
        let package_document = br#"<package><manifest><item id= href="images/cover.png" properties="cover-image"/></manifest></package>"#;
        let archive = build_test_zip_archive(vec![
            (
                "META-INF/container.xml".to_string(),
                basic_container_xml().as_bytes().to_vec(),
            ),
            ("OEBPS/content.opf".to_string(), package_document.to_vec()),
        ])
        .expect("epub archive should be created");
        fs::write(&file_path, archive).expect("epub test file should be written");

        let error = load_epub_cover_bytes(&epub_media(file_path.clone()))
            .await
            .expect_err("malformed EPUB package should be reported as an error");

        assert!(
            error.contains("EPUB package document"),
            "unexpected error: {error}"
        );
        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn parse_epub_fixed_layout_detects_property_and_name_variants() {
        let by_property = br#"<package><metadata><meta property="rendition:layout">pre-paginated</meta></metadata></package>"#;
        assert!(parse_epub_fixed_layout(by_property));

        let by_name =
            br#"<package><metadata><meta name="fixed-layout" content="true"/></metadata></package>"#;
        assert!(parse_epub_fixed_layout(by_name));

        let flowing = br#"<package><metadata><meta property="rendition:layout">reflowable</meta></metadata></package>"#;
        assert!(!parse_epub_fixed_layout(flowing));
    }

    #[test]
    fn normalize_epub_resource_href_collapses_parent_segments() {
        assert_eq!(
            normalize_epub_resource_href("/OPS/sub/content.opf", "../chapter.xhtml"),
            "/OPS/chapter.xhtml"
        );
        assert_eq!(
            normalize_epub_resource_href("/OPS/content.opf", "./text/../chapter.xhtml"),
            "/OPS/chapter.xhtml"
        );
    }

    #[test]
    fn decode_epub_navigation_extension_returns_typed_navigation_parts() {
        let payload = json!({
            "isFixedLayout": true,
            "toc": [
                {
                    "title": "Chapter 1",
                    "href": "/chap-1.xhtml",
                    "children": [{ "title": "Part 1", "href": "/chap-1.xhtml#part-1" }]
                }
            ],
            "landmarks": [{ "title": "Cover", "href": "/cover.xhtml" }],
            "pageList": [{ "title": "1", "href": "/chap-1.xhtml#page-1" }],
            "positions": [
                {
                    "href": "/chap-1.xhtml",
                    "type": "application/xhtml+xml",
                    "locations": { "position": 1, "progression": 0.1 }
                },
                {
                    "href": "/chap-2.xhtml",
                    "type": "application/xhtml+xml",
                    "locations": { "position": 2, "progression": 0.2 }
                }
            ]
        });
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(payload.to_string().as_bytes())
            .expect("gzip payload should be writable");
        let blob = encoder.finish().expect("gzip payload should finalize");

        let extension =
            decode_epub_navigation_extension(&blob).expect("epub positions should decode");
        assert_eq!(extension.positions.len(), 2);
        assert_eq!(
            extension.positions[0].raw().get("href"),
            Some(&Value::String("/chap-1.xhtml".to_string()))
        );
        assert!(extension.is_fixed_layout);
        assert_eq!(extension.toc.len(), 1);
        assert_eq!(extension.toc[0].title.as_deref(), Some("Chapter 1"));
        assert_eq!(
            extension.toc[0].children[0].href.as_deref(),
            Some("/chap-1.xhtml#part-1")
        );
        assert_eq!(extension.landmarks.len(), 1);
        assert_eq!(extension.page_list.len(), 1);
    }
}

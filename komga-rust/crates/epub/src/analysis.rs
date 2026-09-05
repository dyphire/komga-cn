use std::collections::{HashMap, HashSet};
use std::fmt;
use std::io::{Read, Seek, Write};
use std::path::Path;

use flate2::Compression;
use flate2::write::GzEncoder;
use quick_xml::Reader as XmlReader;
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};
use serde_json::{Value, json};
use zip::ZipArchive;

use crate::{
    EpubManifestItem, EpubNavigationLink, EpubParseError, normalize_epub_resource_href,
    normalize_epub_zip_path, parse_epub_fixed_layout, parse_epub_manifest_items,
    parse_epub_rootfile_path, parse_epub_spine_itemrefs,
};

const DIVINA_LETTER_COUNT_THRESHOLD: usize = 15;

#[derive(Clone, Debug)]
pub struct EpubAnalysisError {
    message: String,
}

impl EpubAnalysisError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for EpubAnalysisError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for EpubAnalysisError {}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EpubAnalysisPage {
    pub file_name: String,
    pub media_type: String,
    pub file_size: i64,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EpubAnalysisFile {
    pub file_name: String,
    pub media_type: String,
    pub sub_type: String,
    pub file_size: Option<i64>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EpubAnalysis {
    pub page_count: u64,
    pub divina_compatible: bool,
    pub is_kepub: bool,
    pub pages: Vec<EpubAnalysisPage>,
    pub files: Vec<String>,
    pub media_files: Vec<EpubAnalysisFile>,
    pub extension_blob: Vec<u8>,
    pub comment: Option<String>,
}

#[derive(Clone, Copy, Debug)]
struct ZipEntryMetadata {
    size: u64,
    compressed_size: u64,
}

pub fn analyze_epub_file(path: &Path) -> Result<EpubAnalysis, EpubAnalysisError> {
    let file = std::fs::File::open(path).map_err(|error| {
        EpubAnalysisError::new(format!("open EPUB '{}': {error}", path.display()))
    })?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| EpubAnalysisError::new(format!("open EPUB archive: {error}")))?;

    let container = read_entry(&mut archive, "META-INF/container.xml")?
        .ok_or_else(|| EpubAnalysisError::new("EPUB is missing META-INF/container.xml"))?;
    let rootfile_path = parse_rootfile(&container)?
        .ok_or_else(|| EpubAnalysisError::new("EPUB container has no rootfile"))?;
    let package = read_entry(&mut archive, &rootfile_path)?
        .ok_or_else(|| EpubAnalysisError::new("EPUB package document is missing"))?;
    let manifest = parse_manifest(&package, &rootfile_path)?;
    let spine_ids = parse_epub_spine_itemrefs(&package).map_err(parse_error("parse EPUB spine"))?;
    let spine = spine_ids
        .iter()
        .filter_map(|id| manifest.get(id).cloned())
        .collect::<Vec<_>>();
    let spine_id_set = spine_ids.into_iter().collect::<HashSet<_>>();
    let entries = collect_entries(&mut archive)?;

    let mut media_files = Vec::new();
    for item in &spine {
        add_media_file(&mut media_files, item, "EPUB_PAGE", &entries);
    }
    let mut assets = manifest
        .values()
        .filter(|item| !spine_id_set.contains(&item.id))
        .cloned()
        .collect::<Vec<_>>();
    assets.sort_by_key(|item| resource_name(&item.href));
    for item in &assets {
        add_media_file(&mut media_files, item, "EPUB_ASSET", &entries);
    }

    let mut errors = Vec::new();
    let is_kepub = is_kepub(&mut archive, &spine);
    let navigation = navigation(&mut archive, &package, &rootfile_path, &manifest);
    let toc = match navigation.toc {
        Ok(toc) => toc,
        Err(_) => {
            errors.push("ERR_1035");
            Vec::new()
        }
    };
    let landmarks = match navigation.landmarks {
        Ok(landmarks) => landmarks,
        Err(_) => {
            errors.push("ERR_1036");
            Vec::new()
        }
    };
    let page_list = match navigation.page_list {
        Ok(page_list) => page_list,
        Err(_) => {
            errors.push("ERR_1037");
            Vec::new()
        }
    };
    let pages = match divina_pages(&mut archive, &manifest, &spine, &entries) {
        Ok(pages) => pages,
        Err(_) => {
            errors.push("ERR_1038");
            Vec::new()
        }
    };
    let divina_compatible = !pages.is_empty();
    let is_fixed_layout = parse_epub_fixed_layout(&package)
        .map_err(parse_error("parse EPUB fixed-layout metadata"))?
        || divina_compatible;
    let page_count = if divina_compatible {
        pages.len() as u64
    } else {
        spine
            .iter()
            .filter_map(|item| entries.get(&resource_name(&item.href)))
            .map(|entry| entry.compressed_size.div_ceil(1024))
            .sum()
    };
    let positions = match positions(
        &mut archive,
        &spine,
        &media_files,
        &entries,
        is_fixed_layout,
        is_kepub,
    ) {
        Ok(positions) => positions,
        Err(_) => {
            errors.push("ERR_1039");
            Vec::new()
        }
    };
    let missing_resources = media_files
        .iter()
        .filter(|file| file.file_size.is_none())
        .map(|file| file.file_name.clone())
        .collect::<Vec<_>>();
    let mut comment_parts = errors
        .iter()
        .map(|error| (*error).to_string())
        .collect::<Vec<_>>();
    if !missing_resources.is_empty() {
        comment_parts.push(format!("ERR_1033 [{}]", missing_resources.join(", ")));
    }
    let comment = (!comment_parts.is_empty()).then(|| comment_parts.join(" "));
    let extension_blob =
        encode_extension(positions, is_fixed_layout, &toc, &landmarks, &page_list)?;
    let mut files = media_files
        .iter()
        .map(|file| file.file_name.clone())
        .collect::<Vec<_>>();
    files.sort();

    Ok(EpubAnalysis {
        page_count,
        divina_compatible,
        is_kepub,
        pages,
        files,
        media_files,
        extension_blob,
        comment,
    })
}

fn parse_rootfile(bytes: &[u8]) -> Result<Option<String>, EpubAnalysisError> {
    parse_epub_rootfile_path(bytes).map_err(parse_error("parse EPUB container"))
}

fn parse_manifest(
    bytes: &[u8],
    rootfile_path: &str,
) -> Result<HashMap<String, EpubManifestItem>, EpubAnalysisError> {
    parse_epub_manifest_items(bytes, rootfile_path).map_err(parse_error("parse EPUB manifest"))
}

fn parse_error(context: &'static str) -> impl FnOnce(EpubParseError) -> EpubAnalysisError {
    move |error| EpubAnalysisError::new(format!("{context}: {error}"))
}

fn collect_entries<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<HashMap<String, ZipEntryMetadata>, EpubAnalysisError> {
    let mut entries = HashMap::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| EpubAnalysisError::new(format!("read EPUB entry {index}: {error}")))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry
            .name()
            .map_err(|error| EpubAnalysisError::new(format!("read EPUB entry name: {error}")))?;
        let name = normalize_archive_path(&name);
        if name.is_empty() {
            continue;
        }
        entries.insert(
            name,
            ZipEntryMetadata {
                size: entry.size(),
                compressed_size: entry.compressed_size(),
            },
        );
    }
    Ok(entries)
}

fn add_media_file(
    media_files: &mut Vec<EpubAnalysisFile>,
    item: &EpubManifestItem,
    sub_type: &str,
    entries: &HashMap<String, ZipEntryMetadata>,
) {
    let file_name = resource_name(&item.href);
    let file_size = entries
        .get(&file_name)
        .map(|entry| entry.size.try_into().unwrap_or(i64::MAX));
    media_files.push(EpubAnalysisFile {
        file_name,
        media_type: item.media_type.clone(),
        sub_type: sub_type.to_string(),
        file_size,
    });
}

fn read_entry<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
) -> Result<Option<Vec<u8>>, EpubAnalysisError> {
    let path = normalize_archive_path(path);
    let mut entry = match archive.by_name(&path) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(error) => {
            return Err(EpubAnalysisError::new(format!(
                "read EPUB entry '{path}': {error}"
            )));
        }
    };
    let mut bytes = Vec::new();
    entry
        .read_to_end(&mut bytes)
        .map_err(|error| EpubAnalysisError::new(format!("read EPUB entry '{path}': {error}")))?;
    Ok(Some(bytes))
}

fn normalize_archive_path(path: &str) -> String {
    normalize_epub_zip_path(path)
        .trim_start_matches('/')
        .to_string()
}

fn resource_name(href: &str) -> String {
    normalize_archive_path(&percent_decode(href.split('#').next().unwrap_or_default()))
}

fn divina_pages<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    manifest: &HashMap<String, EpubManifestItem>,
    spine: &[EpubManifestItem],
    entries: &HashMap<String, ZipEntryMetadata>,
) -> Result<Vec<EpubAnalysisPage>, EpubAnalysisError> {
    let image_manifest: HashMap<String, &EpubManifestItem> = manifest
        .values()
        .filter(|item| item.media_type.starts_with("image/"))
        .map(|item| (resource_name(&item.href), item))
        .collect();

    let mut pages = Vec::with_capacity(spine.len());

    for item in spine {
        let page_name = resource_name(&item.href);
        let image_name = if item.media_type.starts_with("image/") {
            page_name
        } else if is_html(&item.media_type) {
            let Some(bytes) = read_entry(archive, &page_name)? else {
                return Ok(Vec::new());
            };
            let Some(source) = divina_image_source(&bytes, &page_name)? else {
                return Ok(Vec::new());
            };
            source
        } else {
            return Ok(Vec::new());
        };

        let Some(image_item) = image_manifest.get(&image_name) else {
            return Ok(Vec::new());
        };
        let Some(entry) = entries.get(&image_name) else {
            return Ok(Vec::new());
        };
        pages.push(EpubAnalysisPage {
            file_name: image_name,
            media_type: image_item.media_type.clone(),
            file_size: entry.size.try_into().unwrap_or(i64::MAX),
        });
    }
    if pages.len() == spine.len() {
        Ok(pages)
    } else {
        Ok(Vec::new())
    }
}

fn is_html(media_type: &str) -> bool {
    matches!(
        media_type.split(';').next().unwrap_or_default().trim(),
        "application/xhtml+xml" | "text/html" | "application/xml" | "text/xml"
    )
}

fn divina_image_source(bytes: &[u8], page_name: &str) -> Result<Option<String>, EpubAnalysisError> {
    let mut reader = XmlReader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut inside_body = false;
    let mut text_len = 0;
    let mut sources = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => {
                if xml_name_matches(event.name().as_ref(), "body") {
                    inside_body = true;
                }
                if inside_body {
                    collect_image_source(&event, &mut sources)?;
                }
            }
            Ok(Event::Empty(event)) if inside_body => collect_image_source(&event, &mut sources)?,
            Ok(Event::Text(text)) if inside_body => {
                text_len += text
                    .as_ref()
                    .chars()
                    .filter(|character| !character.is_whitespace())
                    .count();
            }
            Ok(Event::CData(text)) if inside_body => {
                text_len += text
                    .as_ref()
                    .chars()
                    .filter(|character| !character.is_whitespace())
                    .count();
            }
            Ok(Event::End(event)) if xml_name_matches(event.name().as_ref(), "body") => {
                inside_body = false;
            }
            Ok(Event::Eof) => break,
            Err(error) => {
                return Err(EpubAnalysisError::new(format!(
                    "parse EPUB spine resource: {error}"
                )));
            }
            _ => {}
        }
        buffer.clear();
    }

    if text_len > DIVINA_LETTER_COUNT_THRESHOLD {
        return Ok(None);
    }
    sources.sort();
    sources.dedup();
    if sources.len() != 1 {
        return Ok(None);
    }
    Ok(sources.pop().map(|source| resolve_href(page_name, &source)))
}

fn collect_image_source(
    event: &BytesStart<'_>,
    sources: &mut Vec<String>,
) -> Result<(), EpubAnalysisError> {
    let name = event.name();
    let attribute = if xml_name_matches(name.as_ref(), "img") {
        Some("src")
    } else if xml_name_matches(name.as_ref(), "image") {
        Some("href")
    } else {
        None
    };
    let Some(attribute) = attribute else {
        return Ok(());
    };
    for value in event.attributes() {
        let value = value.map_err(|error| {
            EpubAnalysisError::new(format!("parse EPUB image attribute: {error}"))
        })?;
        if !xml_name_matches(value.key.as_ref(), attribute) {
            continue;
        }
        let value = value
            .normalized_value(XmlVersion::Implicit1_0)
            .map(|value| value.into_owned())
            .map_err(|error| {
                EpubAnalysisError::new(format!("parse EPUB image attribute value: {error}"))
            })?;
        if !value.trim().is_empty() {
            sources.push(value);
        }
        break;
    }
    Ok(())
}

fn xml_name_matches(actual: &str, expected: &str) -> bool {
    actual == expected || actual.ends_with(expected)
}

fn is_kepub<R: Read + Seek>(archive: &mut ZipArchive<R>, spine: &[EpubManifestItem]) -> bool {
    for item in spine {
        if !is_html(&item.media_type) {
            continue;
        }
        let path = resource_name(&item.href);
        let bytes = match read_entry(archive, &path) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => continue,
            Err(_) => return false,
        };
        if String::from_utf8_lossy(&bytes).contains("koboSpan") {
            return true;
        }
    }
    false
}

#[derive(Clone, Debug, Default)]
struct XmlNode {
    name: String,
    attributes: Vec<(String, String)>,
    children: Vec<XmlNode>,
    text: String,
}

impl XmlNode {
    fn local_name(&self) -> &str {
        self.name.rsplit(':').next().unwrap_or(self.name.as_str())
    }

    fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name || key.rsplit(':').next() == Some(name))
            .map(|(_, value)| value.as_str())
    }

    fn attribute_ending_with(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key.rsplit(':').next() == Some(name))
            .map(|(_, value)| value.as_str())
    }

    fn text_content(&self) -> String {
        let mut text = self.text.clone();
        for child in &self.children {
            text.push_str(&child.text_content());
        }
        text
    }
}

fn parse_xml_document(bytes: &[u8]) -> Result<XmlNode, EpubAnalysisError> {
    let mut reader = XmlReader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut stack = vec![XmlNode::default()];

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => stack.push(xml_node_from_start(&event)?),
            Ok(Event::Empty(event)) => {
                let node = xml_node_from_start(&event)?;
                stack
                    .last_mut()
                    .expect("EPUB XML root should exist")
                    .children
                    .push(node);
            }
            Ok(Event::Text(text)) => stack
                .last_mut()
                .expect("EPUB XML root should exist")
                .text
                .push_str(text.as_ref()),
            Ok(Event::CData(text)) => stack
                .last_mut()
                .expect("EPUB XML root should exist")
                .text
                .push_str(text.as_ref()),
            Ok(Event::End(_)) if stack.len() > 1 => {
                let node = stack.pop().expect("EPUB XML node should exist");
                stack
                    .last_mut()
                    .expect("EPUB XML parent should exist")
                    .children
                    .push(node);
            }
            Ok(Event::Eof) => break,
            Err(error) => {
                return Err(EpubAnalysisError::new(format!(
                    "parse EPUB navigation XML: {error}"
                )));
            }
            _ => {}
        }
        buffer.clear();
    }

    if stack.len() != 1 {
        return Err(EpubAnalysisError::new(
            "EPUB navigation XML contains an unclosed element",
        ));
    }
    Ok(stack.pop().expect("EPUB XML root should exist"))
}

fn xml_node_from_start(event: &BytesStart<'_>) -> Result<XmlNode, EpubAnalysisError> {
    let mut attributes = Vec::new();
    for attribute in event.attributes() {
        let attribute = attribute.map_err(|error| {
            EpubAnalysisError::new(format!("parse EPUB navigation attribute: {error}"))
        })?;
        let key = attribute.key.as_ref().to_owned();
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map(|value| value.into_owned())
            .map_err(|error| {
                EpubAnalysisError::new(format!("parse EPUB navigation attribute value: {error}"))
            })?;
        attributes.push((key, value));
    }
    Ok(XmlNode {
        name: event.name().as_ref().to_owned(),
        attributes,
        children: Vec::new(),
        text: String::new(),
    })
}

fn direct_children<'a>(node: &'a XmlNode, name: &str) -> Vec<&'a XmlNode> {
    node.children
        .iter()
        .filter(|child| child.local_name().eq_ignore_ascii_case(name))
        .collect()
}

fn direct_child<'a>(node: &'a XmlNode, name: &str) -> Option<&'a XmlNode> {
    node.children
        .iter()
        .find(|child| child.local_name().eq_ignore_ascii_case(name))
}

fn find_node<'a>(node: &'a XmlNode, name: &str) -> Option<&'a XmlNode> {
    if node.local_name().eq_ignore_ascii_case(name) {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| find_node(child, name))
}

fn find_nav<'a>(node: &'a XmlNode, nav_type: &str) -> Option<&'a XmlNode> {
    if node.local_name().eq_ignore_ascii_case("nav")
        && node.attribute_ending_with("type").is_some_and(|value| {
            value
                .split_ascii_whitespace()
                .any(|item| item.eq_ignore_ascii_case(nav_type))
        })
    {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| find_nav(child, nav_type))
}

struct NavigationParts {
    toc: Result<Vec<EpubNavigationLink>, EpubAnalysisError>,
    landmarks: Result<Vec<EpubNavigationLink>, EpubAnalysisError>,
    page_list: Result<Vec<EpubNavigationLink>, EpubAnalysisError>,
}

fn navigation<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    package: &[u8],
    rootfile_path: &str,
    manifest: &HashMap<String, EpubManifestItem>,
) -> NavigationParts {
    let nav_path = manifest
        .values()
        .find(|item| {
            item.properties
                .split_ascii_whitespace()
                .any(|property| property.eq_ignore_ascii_case("nav"))
        })
        .map(|item| resource_name(&item.href));
    let nav_links = nav_path
        .as_deref()
        .map(|path| -> Result<_, EpubAnalysisError> {
            let bytes = read_entry(archive, path)?.unwrap_or_default();
            let document = parse_xml_document(&bytes)?;
            Ok((
                nav_links_for(&document, path, "toc"),
                nav_links_for(&document, path, "landmarks"),
                nav_links_for(&document, path, "page-list"),
            ))
        })
        .transpose();

    let ncx_path = manifest
        .values()
        .find(|item| {
            item.media_type
                .eq_ignore_ascii_case("application/x-dtbncx+xml")
        })
        .or_else(|| {
            manifest.values().find(|item| {
                matches!(
                    item.id.to_ascii_lowercase().as_str(),
                    "toc" | "ncx" | "ncxtoc"
                )
            })
        })
        .map(|item| resource_name(&item.href));
    let ncx_links = ncx_path
        .as_deref()
        .map(|path| -> Result<_, EpubAnalysisError> {
            let bytes = read_entry(archive, path)?.unwrap_or_default();
            let document = parse_xml_document(&bytes)?;
            Ok((
                ncx_links_for(&document, path, "navMap", "navPoint"),
                ncx_links_for(&document, path, "pageList", "pageTarget"),
            ))
        })
        .transpose();

    let toc = match nav_links.as_ref() {
        Err(error) => Err(error.clone()),
        Ok(Some(links)) if !links.0.is_empty() => Ok(links.0.clone()),
        Ok(_) => match ncx_links.as_ref() {
            Err(error) => Err(error.clone()),
            Ok(Some(links)) => Ok(links.0.clone()),
            Ok(None) => Ok(Vec::new()),
        },
    };
    let landmarks = match nav_links.as_ref() {
        Err(error) => Err(error.clone()),
        Ok(Some(links)) if !links.1.is_empty() => Ok(links.1.clone()),
        Ok(_) => Ok(guide_links(package, rootfile_path)),
    };
    let page_list = match nav_links.as_ref() {
        Err(error) => Err(error.clone()),
        Ok(Some(links)) if !links.2.is_empty() => Ok(links.2.clone()),
        Ok(_) => match ncx_links.as_ref() {
            Err(error) => Err(error.clone()),
            Ok(Some(links)) => Ok(links.1.clone()),
            Ok(None) => Ok(Vec::new()),
        },
    };

    NavigationParts {
        toc,
        landmarks,
        page_list,
    }
}

fn nav_links_for(document: &XmlNode, path: &str, nav_type: &str) -> Vec<EpubNavigationLink> {
    let Some(nav) = find_nav(document, nav_type) else {
        return Vec::new();
    };
    let Some(ordered_list) = direct_child(nav, "ol") else {
        return Vec::new();
    };
    direct_children(ordered_list, "li")
        .into_iter()
        .filter_map(|item| nav_link(item, path))
        .collect()
}

fn nav_link(node: &XmlNode, path: &str) -> Option<EpubNavigationLink> {
    let title_node = node
        .children
        .iter()
        .find(|child| matches!(child.local_name(), "a" | "span"))?;
    let title = title_node.text_content().trim().to_string();
    let href = direct_child(node, "a")
        .and_then(|anchor| anchor.attribute("href"))
        .map(|href| resolve_href(path, href));
    let children = direct_child(node, "ol")
        .map(|ordered_list| {
            direct_children(ordered_list, "li")
                .into_iter()
                .filter_map(|item| nav_link(item, path))
                .collect()
        })
        .unwrap_or_default();
    Some(EpubNavigationLink {
        title: Some(title),
        href,
        children,
    })
}

fn ncx_links_for(
    document: &XmlNode,
    path: &str,
    container_name: &str,
    item_name: &str,
) -> Vec<EpubNavigationLink> {
    let Some(container) = find_node(document, container_name) else {
        return Vec::new();
    };
    direct_children(container, item_name)
        .into_iter()
        .filter_map(|item| ncx_link(item, path, item_name))
        .collect()
}

fn ncx_link(node: &XmlNode, path: &str, item_name: &str) -> Option<EpubNavigationLink> {
    let title = direct_child(node, "navLabel")
        .and_then(|label| direct_child(label, "text"))
        .map(XmlNode::text_content)
        .map(|title| title.trim().to_string())?;
    let href = direct_child(node, "content")
        .and_then(|content| content.attribute("src"))
        .map(|href| resolve_href(path, href));
    let children = direct_children(node, item_name)
        .into_iter()
        .filter_map(|item| ncx_link(item, path, item_name))
        .collect();
    Some(EpubNavigationLink {
        title: Some(title),
        href,
        children,
    })
}

fn guide_links(package: &[u8], rootfile_path: &str) -> Vec<EpubNavigationLink> {
    let Ok(document) = parse_xml_document(package) else {
        return Vec::new();
    };
    let Some(guide) = find_node(&document, "guide") else {
        return Vec::new();
    };
    direct_children(guide, "reference")
        .into_iter()
        .map(|reference| EpubNavigationLink {
            title: Some(reference.attribute("title").unwrap_or_default().to_string()),
            href: reference
                .attribute("href")
                .map(|href| resolve_href(rootfile_path, href)),
            children: Vec::new(),
        })
        .collect()
}

fn positions<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    spine: &[EpubManifestItem],
    media_files: &[EpubAnalysisFile],
    entries: &HashMap<String, ZipEntryMetadata>,
    fixed_layout: bool,
    kepub: bool,
) -> Result<Vec<Value>, EpubAnalysisError> {
    let kobo_positions = if fixed_layout || !kepub {
        HashMap::new()
    } else {
        kobo_positions(archive, spine)?
    };
    let mut result = Vec::new();
    for item in spine {
        let file_name = resource_name(&item.href);
        let Some(file) = media_files
            .iter()
            .find(|file| file.sub_type == "EPUB_PAGE" && file.file_name == file_name)
        else {
            continue;
        };
        let Some(entry) = entries.get(&file_name) else {
            continue;
        };
        let position_count = if fixed_layout {
            1
        } else {
            entry.size.div_ceil(1024).max(1)
        };
        for index in 0..position_count {
            let progression = if fixed_layout {
                0.0
            } else {
                index as f64 / position_count as f64
            };
            let mut locator = json!({
                "href": file.file_name,
                "type": item.media_type,
                "locations": {
                    "progression": progression,
                    "position": result.len() + 1,
                },
            });
            let kobo_span = if fixed_layout || position_count == 1 || index == 0 {
                Some("kobo.1.1".to_string())
            } else {
                kobo_positions
                    .get(&file_name)
                    .and_then(|spans| {
                        spans.iter().min_by(|(_, left), (_, right)| {
                            (progression - *left)
                                .abs()
                                .partial_cmp(&(progression - *right).abs())
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                    })
                    .map(|(span, _)| span.clone())
            };
            if let Some(kobo_span) = kobo_span {
                locator["koboSpan"] = Value::String(kobo_span);
            }
            result.push(locator);
        }
    }

    let total = result.len() as f64;
    for (index, locator) in result.iter_mut().enumerate() {
        locator["locations"]["totalProgression"] = Value::from((index + 1) as f64 / total);
    }
    Ok(result)
}

fn kobo_positions<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    spine: &[EpubManifestItem],
) -> Result<HashMap<String, Vec<(String, f64)>>, EpubAnalysisError> {
    let mut positions = HashMap::new();
    for item in spine {
        if !is_html(&item.media_type) {
            continue;
        }
        let file_name = resource_name(&item.href);
        let Some(bytes) = read_entry(archive, &file_name)? else {
            continue;
        };
        let mut reader = XmlReader::from_reader(bytes.as_slice());
        reader.config_mut().trim_text(false);
        let mut buffer = Vec::new();
        let mut stack = Vec::<Option<String>>::new();
        let mut spans = Vec::new();
        loop {
            match reader.read_event_into(&mut buffer) {
                Ok(Event::Start(event)) => {
                    stack.push(kobo_span_id(&event)?);
                }
                Ok(Event::Empty(event)) => {
                    if let Some(id) = kobo_span_id(&event)? {
                        spans.push((
                            id,
                            reader.buffer_position() as f64 / bytes.len().max(1) as f64,
                        ));
                    }
                }
                Ok(Event::End(_)) => {
                    if let Some(Some(id)) = stack.pop() {
                        spans.push((
                            id,
                            reader.buffer_position() as f64 / bytes.len().max(1) as f64,
                        ));
                    }
                }
                Ok(Event::Eof) => break,
                Err(error) => {
                    return Err(EpubAnalysisError::new(format!(
                        "parse KEPUB resource '{file_name}': {error}"
                    )));
                }
                _ => {}
            }
            buffer.clear();
        }
        if !spans.is_empty() {
            positions.insert(file_name, spans);
        }
    }
    Ok(positions)
}

fn kobo_span_id(event: &BytesStart<'_>) -> Result<Option<String>, EpubAnalysisError> {
    let mut is_kobo_span = false;
    let mut id = None;
    for attribute in event.attributes() {
        let attribute = attribute.map_err(|error| {
            EpubAnalysisError::new(format!("parse KEPUB span attribute: {error}"))
        })?;
        let name = attribute.key.as_ref();
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map(|value| value.into_owned())
            .map_err(|error| {
                EpubAnalysisError::new(format!("parse KEPUB span attribute value: {error}"))
            })?;
        if name == "class" || name.ends_with(":class") {
            is_kobo_span = value
                .split_ascii_whitespace()
                .any(|class| class == "koboSpan");
        } else if name == "id" || name.ends_with(":id") {
            id = Some(value);
        }
    }
    if is_kobo_span {
        Ok(id.filter(|id| !id.is_empty()))
    } else {
        Ok(None)
    }
}

fn encode_extension(
    positions: Vec<Value>,
    fixed_layout: bool,
    toc: &[EpubNavigationLink],
    landmarks: &[EpubNavigationLink],
    page_list: &[EpubNavigationLink],
) -> Result<Vec<u8>, EpubAnalysisError> {
    let payload = json!({
        "positions": positions,
        "isFixedLayout": fixed_layout,
        "toc": links_json(toc),
        "landmarks": links_json(landmarks),
        "pageList": links_json(page_list),
    });
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&serde_json::to_vec(&payload).map_err(|error| {
            EpubAnalysisError::new(format!("encode EPUB extension JSON: {error}"))
        })?)
        .map_err(|error| EpubAnalysisError::new(format!("encode EPUB extension: {error}")))?;
    encoder
        .finish()
        .map_err(|error| EpubAnalysisError::new(format!("finish EPUB extension: {error}")))
}

fn links_json(links: &[EpubNavigationLink]) -> Vec<Value> {
    links
        .iter()
        .map(|link| {
            json!({
                "title": link.title,
                "href": link.href,
                "children": links_json(&link.children),
            })
        })
        .collect()
}

fn resolve_href(document_path: &str, href: &str) -> String {
    let href = percent_decode(href);
    let (path, fragment) = href
        .split_once('#')
        .map_or((href.as_str(), None), |(path, fragment)| {
            (path, Some(fragment))
        });
    let resolved = if path.is_empty() {
        normalize_archive_path(document_path)
    } else {
        normalize_epub_resource_href(&format!("/{}", document_path.trim_start_matches('/')), path)
            .trim_start_matches('/')
            .to_string()
    };
    fragment
        .filter(|fragment| !fragment.is_empty())
        .map(|fragment| format!("{resolved}#{fragment}"))
        .unwrap_or(resolved)
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
        decoded.push(if bytes[index] == b'+' {
            b' '
        } else {
            bytes[index]
        });
        index += 1;
    }
    String::from_utf8(decoded).unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::analyze_epub_file;
    use crate::decode_epub_navigation_extension;
    use zip::CompressionMethod;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    fn repository_resource(relative_path: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../komga/src/test/resources")
            .join(relative_path)
    }

    #[test]
    fn analyzes_reflowable_and_fixed_layout_repository_epubs() {
        let reflowable = analyze_epub_file(&repository_resource(
            "epub/The Incomplete Theft - Ralph Burke.epub",
        ))
        .expect("reflowable EPUB should analyze");
        assert_eq!(reflowable.page_count, 14);
        assert!(!reflowable.divina_compatible);
        assert!(reflowable.pages.is_empty());
        assert_eq!(
            reflowable
                .media_files
                .iter()
                .filter(|file| file.sub_type == "EPUB_PAGE")
                .count(),
            4
        );
        let reflowable_navigation = decode_epub_navigation_extension(&reflowable.extension_blob)
            .expect("reflowable extension should decode");
        assert_eq!(reflowable_navigation.positions.len(), 35);
        assert_eq!(
            reflowable_navigation.toc[0].title.as_deref(),
            Some("The Incomplete Theft")
        );
        assert_eq!(
            reflowable_navigation.landmarks[0].title.as_deref(),
            Some("Cover")
        );

        let fixed_layout = analyze_epub_file(&repository_resource("archives/epub3.epub"))
            .expect("fixed-layout EPUB should analyze");
        assert_eq!(fixed_layout.page_count, 2);
        assert!(fixed_layout.divina_compatible);
        assert_eq!(fixed_layout.pages.len(), 2);
        let fixed_layout_navigation =
            decode_epub_navigation_extension(&fixed_layout.extension_blob)
                .expect("fixed-layout extension should decode");
        assert!(fixed_layout_navigation.is_fixed_layout);
        assert_eq!(fixed_layout_navigation.positions.len(), 2);
        assert_eq!(
            fixed_layout_navigation.toc[0].title.as_deref(),
            Some("Page 1")
        );
    }

    #[test]
    fn image_ratio_without_complete_page_mapping_remains_reflowable() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should follow the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "komga-epub-incomplete-image-layout-{}-{unique}.epub",
            std::process::id()
        ));
        let file = File::create(&path).expect("temporary EPUB should be created");
        let mut zip = ZipWriter::new(file);
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, bytes) in [
            ("mimetype", b"application/epub+zip".as_slice()),
            (
                "META-INF/container.xml",
                br#"<container><rootfiles><rootfile full-path="content.opf"/></rootfiles></container>"#,
            ),
            (
                "content.opf",
                br#"<package><manifest><item id="page" href="page.xhtml" media-type="application/xhtml+xml"/><item id="image" href="image.jpg" media-type="image/jpeg"/></manifest><spine><itemref idref="page"/></spine></package>"#,
            ),
            ("page.xhtml", br#"<html><body/></html>"#),
            ("image.jpg", b"not-used".as_slice()),
        ] {
            zip.start_file(name, stored)
                .expect("temporary EPUB entry should be created");
            zip.write_all(bytes)
                .expect("temporary EPUB entry should be written");
        }
        zip.finish().expect("temporary EPUB should finish");

        let analysis = analyze_epub_file(&path).expect("temporary EPUB should analyze");
        std::fs::remove_file(&path).expect("temporary EPUB should be removed");
        let navigation = decode_epub_navigation_extension(&analysis.extension_blob)
            .expect("EPUB extension should decode");

        assert!(analysis.pages.is_empty());
        assert!(!navigation.is_fixed_layout);
    }

    #[test]
    fn preserves_missing_resources_and_partial_epub_errors() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should follow the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "komga-epub-partial-analysis-{}-{unique}.epub",
            std::process::id()
        ));
        let file = File::create(&path).expect("temporary EPUB should be created");
        let mut zip = ZipWriter::new(file);
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, bytes) in [
            ("mimetype", b"application/epub+zip".as_slice()),
            (
                "META-INF/container.xml",
                br#"<container><rootfiles><rootfile full-path="content.opf"/></rootfiles></container>"#,
            ),
            (
                "content.opf",
                br#"<package><manifest><item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/><item id="missing" href="missing.css" media-type="text/css"/><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/></manifest><spine><itemref idref="chapter"/></spine></package>"#,
            ),
            (
                "chapter.xhtml",
                br#"<html><body><span class="koboSpan" id="kobo.1.1""#,
            ),
            (
                "nav.xhtml",
                br#"<html><body><nav epub:type="toc"><ol>"#,
            ),
        ] {
            zip.start_file(name, stored)
                .expect("temporary EPUB entry should be created");
            zip.write_all(bytes)
                .expect("temporary EPUB entry should be written");
        }
        zip.finish().expect("temporary EPUB should finish");

        let analysis = analyze_epub_file(&path).expect("partial EPUB should remain analyzable");
        std::fs::remove_file(&path).expect("temporary EPUB should be removed");

        assert!(analysis.page_count > 0);
        assert_eq!(analysis.media_files.len(), 3);
        assert_eq!(analysis.media_files[0].file_name, "chapter.xhtml");
        assert_eq!(analysis.media_files[0].file_size, Some(48));
        assert_eq!(analysis.media_files[1].file_name, "missing.css");
        assert_eq!(analysis.media_files[1].file_size, None);
        assert_eq!(analysis.media_files[2].file_name, "nav.xhtml");
        assert_eq!(
            analysis.comment.as_deref(),
            Some("ERR_1035 ERR_1036 ERR_1037 ERR_1038 ERR_1039 ERR_1033 [missing.css]")
        );
    }
}

use std::fmt;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use flate2::Compression;
use flate2::write::GzEncoder;
use iepub::prelude::{MobiBook, MobiReader};
use serde_json::json;
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

mod navigation;
mod parse;

pub use navigation::{EpubNavigation, EpubNavigationLink, decode_epub_navigation_extension};
pub use parse::{
    EpubManifestItem, EpubParseError, EpubSpineItem, normalize_epub_resource_href,
    normalize_epub_zip_path, parse_epub_fixed_layout, parse_epub_manifest_items,
    parse_epub_metadata_cover_id, parse_epub_rootfile_path, parse_epub_spine_itemrefs,
    parse_epub_spine_items,
};

pub const MOBI_MEDIA_TYPE: &str = "application/x-mobipocket-ebook";
pub const EPUB_MEDIA_TYPE: &str = "application/epub+zip";
pub const ADAPTER_VERSION: &str = "mobi-epub-v1";

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum MobiUnsupportedReason {
    Drm,
    HufCompression,
    Kf8,
}

#[derive(Debug)]
pub enum MobiError {
    Unsupported(MobiUnsupportedReason),
    Invalid(String),
    Parse(String),
    Io(std::io::Error),
    Zip(zip::result::ZipError),
}

impl fmt::Display for MobiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(reason) => write!(f, "unsupported MOBI variant: {reason:?}"),
            Self::Invalid(message) => write!(f, "invalid MOBI: {message}"),
            Self::Parse(message) => write!(f, "failed to parse MOBI: {message}"),
            Self::Io(error) => write!(f, "MOBI I/O error: {error}"),
            Self::Zip(error) => write!(f, "derived EPUB ZIP error: {error}"),
        }
    }
}

impl std::error::Error for MobiError {}

impl From<std::io::Error> for MobiError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<zip::result::ZipError> for MobiError {
    fn from(value: zip::result::ZipError) -> Self {
        Self::Zip(value)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PublicationMetadata {
    pub title: String,
    pub identifier: String,
    pub creator: Option<String>,
    pub description: Option<String>,
    pub date: Option<String>,
    pub publisher: Option<String>,
    pub subject: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PublicationChapter {
    pub title: String,
    pub path: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PublicationResource {
    pub path: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NormalizedPublication {
    pub epub: Vec<u8>,
    pub metadata: PublicationMetadata,
    pub chapters: Vec<PublicationChapter>,
    pub resources: Vec<PublicationResource>,
}

impl NormalizedPublication {
    pub fn epub_extension_blob(&self) -> Result<Vec<u8>, MobiError> {
        let positions = self
            .chapters
            .iter()
            .enumerate()
            .map(|(index, chapter)| {
                json!({
                    "href": chapter.path,
                    "type": "application/xhtml+xml",
                    "locations": {
                        "position": index as i64 + 1,
                        "progression": 0.0,
                        "totalProgression": index as f64 / self.chapters.len().max(1) as f64,
                    }
                })
            })
            .collect::<Vec<_>>();
        let toc = self
            .chapters
            .iter()
            .map(|chapter| json!({"title": chapter.title, "href": chapter.path}))
            .collect::<Vec<_>>();
        let payload = serde_json::to_vec(&json!({
            "positions": positions,
            "isFixedLayout": false,
            "toc": toc,
            "landmarks": [],
            "pageList": [],
        }))
        .map_err(|error| MobiError::Invalid(format!("encode EPUB navigation: {error}")))?;
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&payload)?;
        Ok(encoder.finish()?)
    }
}

pub fn materialize_mobi_cache(
    source_path: &Path,
    cache_root: &Path,
    cache_key: &str,
) -> Result<PathBuf, MobiError> {
    if cache_key.is_empty()
        || cache_key.contains('/')
        || cache_key.contains('\\')
        || cache_key == "."
        || cache_key == ".."
    {
        return Err(MobiError::Invalid("invalid MOBI cache key".to_string()));
    }

    let source = std::fs::read(source_path)?;
    let source_metadata = std::fs::metadata(source_path)?;
    let source_modified = source_metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let source_hash = hex_digest(&source);
    let cache_dir = cache_root.join(cache_key);
    let output_path = cache_dir.join("publication.epub");
    let metadata_path = cache_dir.join("metadata.txt");
    let fingerprint = format!(
        "version={ADAPTER_VERSION}\nsize={}\nmodified={source_modified}\nsha256={source_hash}\n",
        source.len()
    );

    if output_path.is_file()
        && std::fs::read_to_string(&metadata_path).ok().as_deref() == Some(fingerprint.as_str())
    {
        return Ok(output_path);
    }

    let publication = normalize_mobi(&source)?;
    std::fs::create_dir_all(&cache_dir)?;
    let temporary_path = cache_dir.join(format!(
        ".publication-{}-{}.tmp",
        std::process::id(),
        source_modified
    ));
    std::fs::write(&temporary_path, &publication.epub)?;
    std::fs::rename(&temporary_path, &output_path)?;
    std::fs::write(&metadata_path, fingerprint)?;
    Ok(output_path)
}

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn normalize_mobi(bytes: &[u8]) -> Result<NormalizedPublication, MobiError> {
    if bytes.len() < 68 || bytes.get(60..68) != Some(b"BOOKMOBI") {
        return Err(MobiError::Invalid(
            "missing BOOKMOBI identifier".to_string(),
        ));
    }
    validate_mobi_variant(bytes)?;

    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut reader = MobiReader::new(Cursor::new(bytes))
            .map_err(|error| MobiError::Parse(error.to_string()))?;
        let book = reader
            .load()
            .map_err(|error| map_parser_error(error.to_string()))?;
        normalize_book(book)
    }))
    .map_err(|_| MobiError::Invalid("parser rejected malformed MOBI data".to_string()))?
}

fn validate_mobi_variant(bytes: &[u8]) -> Result<(), MobiError> {
    if bytes.len() < 82 {
        return Err(MobiError::Invalid("truncated PalmDB header".to_string()));
    }
    let record_offset = u32::from_be_bytes(bytes[78..82].try_into().unwrap()) as usize;
    if record_offset.checked_add(16).is_none() || record_offset + 16 > bytes.len() {
        return Err(MobiError::Invalid(
            "invalid first record offset".to_string(),
        ));
    }

    let compression =
        u16::from_be_bytes(bytes[record_offset..record_offset + 2].try_into().unwrap());
    if compression == 17480 {
        return Err(MobiError::Unsupported(
            MobiUnsupportedReason::HufCompression,
        ));
    }
    if !matches!(compression, 1 | 2) {
        return Err(MobiError::Unsupported(
            MobiUnsupportedReason::HufCompression,
        ));
    }
    let mobi_offset = record_offset + 16;
    if bytes.get(mobi_offset..mobi_offset + 4) != Some(b"MOBI") {
        return Err(MobiError::Invalid("missing MOBI header".to_string()));
    }
    let mobi_type =
        u32::from_be_bytes(bytes[mobi_offset + 8..mobi_offset + 12].try_into().unwrap());
    if mobi_type == 0x0000_00f8 {
        return Err(MobiError::Unsupported(MobiUnsupportedReason::Kf8));
    }
    if mobi_offset + 144 <= bytes.len() {
        let drm_offset = u32::from_be_bytes(
            bytes[mobi_offset + 128..mobi_offset + 132]
                .try_into()
                .unwrap(),
        );
        let drm_count = u32::from_be_bytes(
            bytes[mobi_offset + 132..mobi_offset + 136]
                .try_into()
                .unwrap(),
        );
        let drm_size = u32::from_be_bytes(
            bytes[mobi_offset + 136..mobi_offset + 140]
                .try_into()
                .unwrap(),
        );
        let drm_flags = u32::from_be_bytes(
            bytes[mobi_offset + 140..mobi_offset + 144]
                .try_into()
                .unwrap(),
        );
        let has_drm_offset = drm_offset != 0 && drm_offset != u32::MAX;
        let has_drm_count = drm_count != 0 && drm_count != u32::MAX;
        if has_drm_offset || has_drm_count || drm_size != 0 || drm_flags != 0 {
            return Err(MobiError::Unsupported(MobiUnsupportedReason::Drm));
        }
    }
    Ok(())
}

pub fn read_epub_resource_from_bytes(
    epub_bytes: &[u8],
    resource_name: &str,
) -> Result<Option<Vec<u8>>, MobiError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(epub_bytes))?;
    let candidates = [resource_name, resource_name.trim_start_matches('/')];
    for candidate in candidates {
        if candidate.is_empty() {
            continue;
        }
        match archive.by_name(candidate) {
            Ok(mut entry) => {
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes)?;
                return Ok(Some(bytes));
            }
            Err(zip::result::ZipError::FileNotFound) => {}
            Err(error) => return Err(MobiError::Zip(error)),
        }
    }
    Ok(None)
}

fn map_parser_error(message: String) -> MobiError {
    let lower = message.to_ascii_lowercase();
    if lower.contains("drm") || lower.contains("encryption") {
        MobiError::Unsupported(MobiUnsupportedReason::Drm)
    } else if lower.contains("huff") || lower.contains("cdic") {
        MobiError::Unsupported(MobiUnsupportedReason::HufCompression)
    } else if lower.contains("kf8") || lower.contains("kindlegen") {
        MobiError::Unsupported(MobiUnsupportedReason::Kf8)
    } else {
        MobiError::Parse(message)
    }
}

fn normalize_book(book: MobiBook) -> Result<NormalizedPublication, MobiError> {
    let metadata = PublicationMetadata {
        title: book.title().to_string(),
        identifier: book.identifier().to_string(),
        creator: book.creator().map(ToOwned::to_owned),
        description: book.description().map(ToOwned::to_owned),
        date: book.date().map(ToOwned::to_owned),
        publisher: book.publisher().map(ToOwned::to_owned),
        subject: book.subject().map(ToOwned::to_owned),
    };

    let mut chapters = Vec::new();
    let mut chapter_documents = Vec::new();
    for (index, chapter) in book.chapters().enumerate() {
        let path = format!("OEBPS/text/chapter-{index:04}.xhtml");
        let title = if chapter.title().trim().is_empty() {
            format!("Chapter {}", index + 1)
        } else {
            chapter.title().to_string()
        };
        chapters.push(PublicationChapter {
            title: title.clone(),
            path: path.clone(),
        });
        chapter_documents.push((path, title, chapter.string_data()));
    }

    if chapters.is_empty() {
        return Err(MobiError::Invalid(
            "MOBI contains no readable chapters".to_string(),
        ));
    }

    let mut resources = Vec::new();
    let mut asset_documents = Vec::new();
    if let Some(cover) = book.cover() {
        let bytes = cover
            .data()
            .ok_or_else(|| MobiError::Invalid("cover has no data".to_string()))?
            .to_vec();
        let media_type = sniff_image_media_type(&bytes);
        let path = format!("OEBPS/images/cover.{}", image_extension(media_type));
        resources.push(PublicationResource {
            path: path.clone(),
            media_type: media_type.to_string(),
            bytes: bytes.clone(),
        });
        asset_documents.push((path, media_type.to_string(), bytes));
    }
    for (index, asset) in book.assets().enumerate() {
        let bytes = asset
            .data()
            .ok_or_else(|| MobiError::Invalid("asset has no data".to_string()))?
            .to_vec();
        let media_type = sniff_image_media_type(&bytes);
        let path = format!(
            "OEBPS/images/asset-{index:04}.{}",
            image_extension(media_type)
        );
        resources.push(PublicationResource {
            path: path.clone(),
            media_type: media_type.to_string(),
            bytes: bytes.clone(),
        });
        asset_documents.push((path, media_type.to_string(), bytes));
    }

    let epub = write_epub(&metadata, &chapters, &chapter_documents, &asset_documents)?;
    Ok(NormalizedPublication {
        epub,
        metadata,
        chapters,
        resources,
    })
}

fn write_epub(
    metadata: &PublicationMetadata,
    chapters: &[PublicationChapter],
    chapter_documents: &[(String, String, String)],
    assets: &[(String, String, Vec<u8>)],
) -> Result<Vec<u8>, MobiError> {
    let mut cursor = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(&mut cursor);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

    writer.start_file("mimetype", stored)?;
    writer.write_all(EPUB_MEDIA_TYPE.as_bytes())?;
    writer.start_file("META-INF/container.xml", stored)?;
    writer.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
    )?;

    writer.start_file("OEBPS/content.opf", stored)?;
    writer.write_all(opf(metadata, chapters, assets).as_bytes())?;
    writer.start_file("OEBPS/nav.xhtml", stored)?;
    writer.write_all(nav(chapters).as_bytes())?;

    for (path, _, document) in chapter_documents {
        writer.start_file(path, stored)?;
        writer.write_all(xhtml_document(document).as_bytes())?;
    }
    for (path, _, bytes) in assets {
        writer.start_file(path, stored)?;
        writer.write_all(bytes)?;
    }
    writer.finish()?;
    Ok(cursor.into_inner())
}

fn opf(
    metadata: &PublicationMetadata,
    chapters: &[PublicationChapter],
    assets: &[(String, String, Vec<u8>)],
) -> String {
    let mut manifest = String::new();
    manifest.push_str(
        r#"<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"#,
    );
    for (index, _chapter) in chapters.iter().enumerate() {
        manifest.push_str(&format!(
            r#"<item id="chapter-{index:04}" href="text/chapter-{index:04}.xhtml" media-type="application/xhtml+xml"/>"#
        ));
    }
    for (index, (path, media_type, _)) in assets.iter().enumerate() {
        let href = path.strip_prefix("OEBPS/").unwrap_or(path);
        let properties = if index == 0 && path.contains("/cover.") {
            " properties=\"cover-image\""
        } else {
            ""
        };
        manifest.push_str(&format!(
            r#"<item id="asset-{index:04}" href="{}" media-type="{}"{} />"#,
            xml_escape(href),
            xml_escape(media_type),
            properties
        ));
    }

    let spine = chapters
        .iter()
        .enumerate()
        .map(|(index, _)| format!(r#"<itemref idref="chapter-{index:04}"/>"#))
        .collect::<String>();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="pub-id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="pub-id">{}</dc:identifier><dc:title>{}</dc:title>{}<dc:format>application/epub+zip</dc:format></metadata><manifest>{}</manifest><spine>{}</spine></package>"#,
        xml_escape(if metadata.identifier.is_empty() {
            ADAPTER_VERSION
        } else {
            &metadata.identifier
        }),
        xml_escape(&metadata.title),
        optional_metadata(metadata),
        manifest,
        spine,
    )
}

fn optional_metadata(metadata: &PublicationMetadata) -> String {
    let mut result = String::new();
    if let Some(value) = metadata.creator.as_deref() {
        result.push_str(&format!(
            r#"<dc:creator>{}</dc:creator>"#,
            xml_escape(value)
        ));
    }
    if let Some(value) = metadata.description.as_deref() {
        result.push_str(&format!(
            r#"<dc:description>{}</dc:description>"#,
            xml_escape(value)
        ));
    }
    if let Some(value) = metadata.date.as_deref() {
        result.push_str(&format!(r#"<dc:date>{}</dc:date>"#, xml_escape(value)));
    }
    if let Some(value) = metadata.publisher.as_deref() {
        result.push_str(&format!(
            r#"<dc:publisher>{}</dc:publisher>"#,
            xml_escape(value)
        ));
    }
    if let Some(value) = metadata.subject.as_deref() {
        result.push_str(&format!(
            r#"<dc:subject>{}</dc:subject>"#,
            xml_escape(value)
        ));
    }
    result
}

fn nav(chapters: &[PublicationChapter]) -> String {
    let items = chapters
        .iter()
        .map(|chapter| {
            format!(
                r#"<li><a href="{}">{}</a></li>"#,
                xml_escape(chapter.path.strip_prefix("OEBPS/").unwrap_or(&chapter.path)),
                xml_escape(&chapter.title)
            )
        })
        .collect::<String>();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE html><html xmlns="http://www.w3.org/1999/xhtml"><head><title>Navigation</title></head><body><nav epub:type="toc" xmlns:epub="http://www.idpf.org/2007/ops"><ol>{items}</ol></nav></body></html>"#
    )
}

fn xhtml_document(document: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE html><html xmlns="http://www.w3.org/1999/xhtml"><head><meta charset="UTF-8"/></head><body>{document}</body></html>"#
    )
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn sniff_image_media_type(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        "image/jpeg"
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "image/gif"
    } else {
        "application/octet-stream"
    }
}

fn image_extension(media_type: &str) -> &'static str {
    match media_type {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        _ => "bin",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iepub::prelude::{MobiBuilder, MobiHtml};
    use zip::ZipArchive;

    fn fixture() -> Vec<u8> {
        let mut cover = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut cover)
            .encode(&[255, 0, 0], 1, 1, image::ExtendedColorType::Rgb8)
            .expect("fixture cover should be encoded");
        MobiBuilder::default()
            .with_title("Fixture title")
            .with_identifier("fixture-isbn")
            .with_creator("Fixture author")
            .with_description("Fixture description")
            .with_date("2024-01-02")
            .with_publisher("Fixture publisher")
            .add_chapter(
                MobiHtml::new(1)
                    .with_title("First chapter")
                    .with_data(b"<p>Hello &amp; goodbye</p>".to_vec()),
            )
            .cover(cover)
            .mem()
            .expect("fixture MOBI should be generated")
    }

    #[test]
    fn normalizes_mobi_to_a_valid_epub_with_stable_chapter_paths() {
        let publication = normalize_mobi(&fixture()).expect("fixture should be supported");

        assert_eq!(publication.metadata.title, "Fixture title");
        assert_eq!(
            publication.metadata.creator.as_deref(),
            Some("Fixture author")
        );
        assert_eq!(
            publication.chapters[0].path,
            "OEBPS/text/chapter-0000.xhtml"
        );

        let mut archive = ZipArchive::new(Cursor::new(publication.epub)).expect("valid EPUB ZIP");
        let names = (0..archive.len())
            .map(|index| archive.by_index(index).unwrap().name().unwrap().to_string())
            .collect::<Vec<_>>();
        assert!(names.iter().any(|name| name == "mimetype"));
        assert!(names.iter().any(|name| name == "OEBPS/content.opf"));
        assert!(names.iter().any(|name| name == "OEBPS/nav.xhtml"));
        assert!(
            names
                .iter()
                .any(|name| name == "OEBPS/text/chapter-0000.xhtml")
        );
        assert!(names.iter().any(|name| name == "OEBPS/images/cover.jpg"));
    }

    #[test]
    fn rejects_non_mobi_bytes_as_invalid() {
        assert!(matches!(
            normalize_mobi(b"not a mobi"),
            Err(MobiError::Invalid(_))
        ));
    }

    #[test]
    fn classifies_unsupported_mobi_compression_before_parsing() {
        let mut bytes = fixture();
        let record_offset = u32::from_be_bytes(bytes[78..82].try_into().unwrap()) as usize;
        bytes[record_offset..record_offset + 2].copy_from_slice(&17480_u16.to_be_bytes());
        assert!(matches!(
            normalize_mobi(&bytes),
            Err(MobiError::Unsupported(
                MobiUnsupportedReason::HufCompression
            ))
        ));
    }

    #[test]
    fn classifies_drm_mobi_before_parsing() {
        let mut bytes = fixture();
        let record_offset = u32::from_be_bytes(bytes[78..82].try_into().unwrap()) as usize;
        let mobi_offset = record_offset + 16;
        bytes[mobi_offset + 128..mobi_offset + 132].copy_from_slice(&1_u32.to_be_bytes());
        assert!(matches!(
            normalize_mobi(&bytes),
            Err(MobiError::Unsupported(MobiUnsupportedReason::Drm))
        ));
    }

    #[test]
    fn allows_a_nonzero_palm_doc_reading_position() {
        let mut bytes = fixture();
        let record_offset = u32::from_be_bytes(bytes[78..82].try_into().unwrap()) as usize;
        bytes[record_offset + 12..record_offset + 16].copy_from_slice(&1_u32.to_be_bytes());
        normalize_mobi(&bytes).expect("reading position must not be treated as DRM");
    }

    #[test]
    fn classifies_kf8_mobi_before_parsing() {
        let mut bytes = fixture();
        let record_offset = u32::from_be_bytes(bytes[78..82].try_into().unwrap()) as usize;
        let mobi_offset = record_offset + 16;
        bytes[mobi_offset + 8..mobi_offset + 12].copy_from_slice(&0x0000_00f8_u32.to_be_bytes());
        assert!(matches!(
            normalize_mobi(&bytes),
            Err(MobiError::Unsupported(MobiUnsupportedReason::Kf8))
        ));
    }

    #[test]
    fn reads_a_resource_from_the_normalized_epub_bytes() {
        let publication = normalize_mobi(&fixture()).expect("fixture should be supported");
        let chapter =
            read_epub_resource_from_bytes(&publication.epub, "OEBPS/text/chapter-0000.xhtml")
                .expect("resource lookup should succeed")
                .expect("chapter should exist");
        assert!(String::from_utf8_lossy(&chapter).contains("Hello"));
    }

    #[test]
    fn generates_a_gzipped_epub_navigation_extension() {
        let publication = normalize_mobi(&fixture()).expect("fixture should be supported");
        let blob = publication
            .epub_extension_blob()
            .expect("navigation extension should encode");
        let mut decoded = String::new();
        flate2::read::GzDecoder::new(blob.as_slice())
            .read_to_string(&mut decoded)
            .expect("navigation extension should decode");
        assert!(decoded.contains("chapter-0000.xhtml"));
        assert!(decoded.contains("\"isFixedLayout\":false"));
    }

    #[test]
    fn accepts_the_local_mobi_sample_when_it_is_available() {
        let path =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sample/epub3.mobi");
        if !path.is_file() {
            return;
        }
        let bytes = std::fs::read(path).expect("local MOBI sample should be readable");
        let publication = normalize_mobi(&bytes).expect("local MOBI sample should normalize");
        assert!(!publication.metadata.title.is_empty());
        assert!(publication.metadata.creator.is_some());
        assert!(!publication.chapters.is_empty());
        assert!(
            publication
                .resources
                .iter()
                .any(|resource| resource.path.contains("cover."))
        );
    }

    #[test]
    fn materializes_and_reuses_a_fingerprinted_cache_entry() {
        let nonce = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock should be valid")
                .as_nanos()
        );
        let root = std::env::temp_dir().join(format!("komga-epub-cache-{nonce}"));
        let source = root.join("book.mobi");
        let cache = root.join("cache");
        std::fs::create_dir_all(&root).expect("cache fixture directory should be created");
        std::fs::write(&source, fixture()).expect("cache source should be written");

        let first = materialize_mobi_cache(&source, &cache, "book-1")
            .expect("first cache materialization should succeed");
        let first_bytes = std::fs::read(&first).expect("first cache output should be readable");
        let second = materialize_mobi_cache(&source, &cache, "book-1")
            .expect("second cache materialization should succeed");
        assert_eq!(first, second);
        assert_eq!(
            first_bytes,
            std::fs::read(&second).expect("cached output should be readable")
        );
        assert!(cache.join("book-1/metadata.txt").is_file());

        let _ = std::fs::remove_dir_all(root);
    }
}

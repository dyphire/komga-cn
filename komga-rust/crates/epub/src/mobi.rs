use std::fmt;
use std::io::{Cursor, Write};

use flate2::Compression;
use flate2::write::GzEncoder;
use iepub::prelude::{MobiBook, MobiReader};
use serde_json::json;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::EPUB_MEDIA_TYPE;

pub const MOBI_MEDIA_TYPE: &str = "application/x-mobipocket-ebook";
pub const ADAPTER_VERSION: &str = "mobi-epub-v2";

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
    document: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PublicationResource {
    pub path: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NormalizedPublication {
    pub metadata: PublicationMetadata,
    pub chapters: Vec<PublicationChapter>,
    pub resources: Vec<PublicationResource>,
    pub page_count: u64,
    chapter_page_counts: Vec<u64>,
}

impl NormalizedPublication {
    pub fn resource_bytes(&self, resource_name: &str) -> Result<Option<Vec<u8>>, MobiError> {
        let resource_name = resource_name.trim_start_matches('/');
        if resource_name.is_empty() {
            return Ok(None);
        }

        match resource_name {
            "mimetype" => return Ok(Some(EPUB_MEDIA_TYPE.as_bytes().to_vec())),
            "META-INF/container.xml" => {
                return Ok(Some(
                    br#"<?xml version="1.0" encoding="UTF-8"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#
                        .to_vec(),
                ));
            }
            "OEBPS/content.opf" => {
                return Ok(Some(
                    opf(&self.metadata, &self.chapters, &self.resources).into_bytes(),
                ));
            }
            "OEBPS/nav.xhtml" => {
                return Ok(Some(nav(&self.chapters).into_bytes()));
            }
            _ => {}
        }

        if let Some(chapter) = self
            .chapters
            .iter()
            .find(|chapter| chapter.path == resource_name)
        {
            return Ok(Some(xhtml_document(&chapter.document).into_bytes()));
        }
        Ok(self
            .resources
            .iter()
            .find(|resource| resource.path == resource_name)
            .map(|resource| resource.bytes.clone()))
    }

    pub fn epub_bytes(&self) -> Result<Vec<u8>, MobiError> {
        write_epub(&self.metadata, &self.chapters, &self.resources)
    }

    pub fn epub_extension_blob(&self) -> Result<Vec<u8>, MobiError> {
        let mut positions = Vec::new();
        let mut position = 0_u64;
        for (chapter, page_count) in self.chapters.iter().zip(&self.chapter_page_counts) {
            for page in 0..*page_count {
                positions.push(json!({
                    "href": chapter.path,
                    "type": "application/xhtml+xml",
                    "locations": {
                        "position": position + 1,
                        "progression": page as f64 / (*page_count).max(1) as f64,
                        "totalProgression": position as f64 / self.page_count.max(1) as f64,
                    }
                }));
                position += 1;
            }
        }
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
        let text = read_mobi_text(bytes)?;
        normalize_book(book, &text)
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

fn read_mobi_text(bytes: &[u8]) -> Result<String, MobiError> {
    let pdb_record_count = read_mobi_u16(bytes, 76, "PalmDB record count")? as usize;
    if pdb_record_count < 2 {
        return Err(MobiError::Invalid("MOBI has no text records".to_string()));
    }

    let record_table_end =
        78_usize
            .checked_add(pdb_record_count.checked_mul(8).ok_or_else(|| {
                MobiError::Invalid("PalmDB record table is too large".to_string())
            })?)
            .ok_or_else(|| MobiError::Invalid("PalmDB record table overflows".to_string()))?;
    if record_table_end > bytes.len() {
        return Err(MobiError::Invalid(
            "truncated PalmDB record table".to_string(),
        ));
    }

    let mut record_offsets = Vec::with_capacity(pdb_record_count + 1);
    for index in 0..pdb_record_count {
        let offset = read_mobi_u32(bytes, 78 + index * 8, "PalmDB record offset")? as usize;
        if offset > bytes.len()
            || record_offsets
                .last()
                .is_some_and(|previous| *previous > offset)
        {
            return Err(MobiError::Invalid(
                "invalid PalmDB record offsets".to_string(),
            ));
        }
        record_offsets.push(offset);
    }
    record_offsets.push(bytes.len());

    let record_zero = record_offsets[0];
    let compression = read_mobi_u16(bytes, record_zero, "PalmDOC compression")?;
    let text_record_count =
        read_mobi_u16(bytes, record_zero + 8, "PalmDOC text record count")? as usize;
    if text_record_count == 0 || text_record_count + 1 >= record_offsets.len() {
        return Err(MobiError::Invalid(
            "invalid PalmDOC text record count".to_string(),
        ));
    }

    let mobi_offset = record_zero
        .checked_add(16)
        .ok_or_else(|| MobiError::Invalid("MOBI header offset overflows".to_string()))?;
    let text_encoding = read_mobi_u32(bytes, mobi_offset + 12, "MOBI text encoding")?;
    let header_length = read_mobi_u32(bytes, mobi_offset + 4, "MOBI header length")? as usize;
    let extra_record_data_flags = if header_length >= 228 {
        read_mobi_u32(bytes, mobi_offset + 220, "MOBI extra record data flags")?
    } else {
        0
    };

    let mut text = Vec::new();
    for index in 1..=text_record_count {
        let start = record_offsets[index];
        let end = record_offsets[index + 1];
        let mut record = bytes[start..end].to_vec();
        let tail_length = mobi_extra_record_data_length(&record, extra_record_data_flags);
        record.truncate(record.len().saturating_sub(tail_length));
        if compression == 2 {
            record = uncompress_palm_doc(&record)?;
        }
        text.extend(record);
    }

    if text_encoding == 1252 {
        Ok(text.into_iter().map(char::from).collect())
    } else {
        String::from_utf8(text)
            .map_err(|error| MobiError::Invalid(format!("invalid MOBI text: {error}")))
    }
}

fn read_mobi_u16(bytes: &[u8], offset: usize, field: &str) -> Result<u16, MobiError> {
    let value = bytes
        .get(offset..offset.saturating_add(2))
        .filter(|value| value.len() == 2)
        .ok_or_else(|| MobiError::Invalid(format!("truncated {field}")))?;
    Ok(u16::from_be_bytes([value[0], value[1]]))
}

fn read_mobi_u32(bytes: &[u8], offset: usize, field: &str) -> Result<u32, MobiError> {
    let value = bytes
        .get(offset..offset.saturating_add(4))
        .filter(|value| value.len() == 4)
        .ok_or_else(|| MobiError::Invalid(format!("truncated {field}")))?;
    Ok(u32::from_be_bytes([value[0], value[1], value[2], value[3]]))
}

fn mobi_extra_record_data_length(data: &[u8], flags: u32) -> usize {
    let mut remaining = data.len();
    for _ in 0..(flags >> 1).count_ones() {
        if remaining < 4 {
            break;
        }
        let value = mobi_variable_width_length(&data[..remaining]);
        if value > remaining {
            break;
        }
        remaining -= value;
    }
    if flags & 1 != 0 && remaining > 0 {
        let value = (data[remaining - 1] & 0b11) as usize + 1;
        if value <= remaining {
            remaining -= value;
        }
    }
    data.len() - remaining
}

fn mobi_variable_width_length(data: &[u8]) -> usize {
    let mut value = 0_usize;
    for byte in &data[data.len() - 4..] {
        if byte & 0x80 != 0 {
            value = 0;
        }
        value = (value << 7) | usize::from(byte & 0x7f);
    }
    value
}

fn uncompress_palm_doc(data: &[u8]) -> Result<Vec<u8>, MobiError> {
    let mut output = Vec::new();
    let mut offset = 0;
    while offset < data.len() {
        let byte = data[offset];
        offset += 1;
        match byte {
            0 => output.push(0),
            1..=8 => {
                let end = offset
                    .checked_add(byte as usize)
                    .ok_or_else(|| MobiError::Invalid("PalmDOC literal overflows".to_string()))?;
                if end > data.len() {
                    return Err(MobiError::Invalid("truncated PalmDOC literal".to_string()));
                }
                output.extend_from_slice(&data[offset..end]);
                offset = end;
            }
            9..=127 => output.push(byte),
            128..=191 => {
                let next = *data
                    .get(offset)
                    .ok_or_else(|| MobiError::Invalid("truncated PalmDOC reference".to_string()))?;
                offset += 1;
                let distance = ((((byte as usize) << 8) | next as usize) >> 3) & 0x7ff;
                let length = usize::from(next & 0x7) + 3;
                if distance == 0 || distance > output.len() {
                    return Err(MobiError::Invalid(
                        "invalid PalmDOC back-reference".to_string(),
                    ));
                }
                for _ in 0..length {
                    let index = output.len() - distance;
                    output.push(output[index]);
                }
            }
            _ => {
                output.push(b' ');
                output.push(byte ^ 0x80);
            }
        }
    }
    Ok(output)
}

struct MobiPageSegment {
    document: String,
    start: usize,
    end: usize,
}

fn mobi_body(document: &str) -> (String, usize) {
    let lower = document.to_ascii_lowercase();
    let body_start = lower
        .find("<body")
        .and_then(|start| lower[start..].find('>').map(|end| start + end + 1))
        .unwrap_or(0);
    let body_end = lower[body_start..]
        .find("</body>")
        .map(|end| body_start + end)
        .unwrap_or(document.len());
    (document[body_start..body_end].to_string(), body_start)
}

fn split_mobi_pages_with_offsets(document: &str, base_offset: usize) -> Vec<MobiPageSegment> {
    let lower = document.to_ascii_lowercase();
    let mut pages = Vec::new();
    let mut content_start = 0;
    let mut search_start = 0;

    while let Some(relative_start) = lower[search_start..].find("<mbp:pagebreak") {
        let marker_start = search_start + relative_start;
        let Some(open_end_relative) = lower[marker_start..].find('>') else {
            break;
        };
        let open_end = marker_start + open_end_relative + 1;
        let tag_is_self_closing = lower[marker_start..open_end - 1].trim_end().ends_with('/');
        let marker_end = if tag_is_self_closing {
            open_end
        } else if let Some(close_start_relative) = lower[open_end..].find("</mbp:pagebreak") {
            let close_start = open_end + close_start_relative;
            lower[close_start..]
                .find('>')
                .map(|end| close_start + end + 1)
                .unwrap_or(open_end)
        } else {
            open_end
        };

        pages.push(MobiPageSegment {
            document: document[content_start..marker_start].to_string(),
            start: base_offset + content_start,
            end: base_offset + marker_start,
        });
        content_start = marker_end;
        search_start = marker_end;
    }

    pages.push(MobiPageSegment {
        document: document[content_start..].to_string(),
        start: base_offset + content_start,
        end: base_offset + document.len(),
    });
    pages
}

fn has_mobi_content(document: &str) -> bool {
    !strip_mobi_markup(document).trim().is_empty()
}

fn strip_mobi_markup(document: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for character in document.chars() {
        match character {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => text.push(character),
            _ => {}
        }
    }
    text
}

fn mobi_toc_titles(document: &str, pages: &[MobiPageSegment]) -> Vec<(usize, String)> {
    let lower = document.to_ascii_lowercase();
    let Some(guide_start) = lower.find("<guide") else {
        return Vec::new();
    };
    let Some(guide_end_relative) = lower[guide_start..].find("</guide>") else {
        return Vec::new();
    };
    let guide_end = guide_start + guide_end_relative + "</guide>".len();
    let Some(guide_position) = parse_filepos(&document[guide_start..guide_end]) else {
        return Vec::new();
    };
    let Some(toc_page) = pages
        .iter()
        .find(|page| page.start <= guide_position && guide_position < page.end)
    else {
        return Vec::new();
    };
    parse_mobi_page_titles(&document[toc_page.start..toc_page.end])
}

fn parse_mobi_page_titles(document: &str) -> Vec<(usize, String)> {
    let lower = document.to_ascii_lowercase();
    let mut titles = Vec::new();
    let mut search_start = 0;
    while let Some(relative_start) = lower[search_start..].find("<a") {
        let start = search_start + relative_start;
        let Some(open_end_relative) = lower[start..].find('>') else {
            break;
        };
        let open_end = start + open_end_relative + 1;
        let Some(close_start_relative) = lower[open_end..].find("</a>") else {
            break;
        };
        let close_start = open_end + close_start_relative;
        let Some(filepos) = parse_filepos(&document[start..open_end]) else {
            search_start = close_start + 4;
            continue;
        };
        let title = strip_mobi_markup(&document[open_end..close_start]);
        if !title.trim().is_empty() {
            titles.push((filepos, title.trim().to_string()));
        }
        search_start = close_start + 4;
    }
    titles
}

fn parse_filepos(tag: &str) -> Option<usize> {
    let lower = tag.to_ascii_lowercase();
    let start = lower.find("filepos")? + "filepos".len();
    let value = tag[start..].trim_start().strip_prefix('=')?.trim_start();
    let value = value
        .strip_prefix('"')
        .or_else(|| value.strip_prefix('\''))
        .unwrap_or(value);
    let digits = value
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    (!digits.is_empty()).then(|| digits.parse().ok()).flatten()
}

fn mobi_page_title(
    page: &MobiPageSegment,
    index: usize,
    toc_titles: &[(usize, String)],
    fallback_titles: &[String],
    page_count: usize,
) -> String {
    if let Some((_, title)) = toc_titles
        .iter()
        .find(|(position, _)| page.start <= *position && *position < page.end)
    {
        return title.clone();
    }
    if page_count == 1 {
        if let Some(title) = fallback_titles
            .first()
            .filter(|title| !title.trim().is_empty())
        {
            return title.clone();
        }
    }
    format!("Page {}", index + 1)
}

fn chapter_page_counts(chapters: &[PublicationChapter]) -> Result<Vec<u64>, MobiError> {
    chapters
        .iter()
        .map(|chapter| {
            let bytes = xhtml_document(&chapter.document).into_bytes();
            let mut encoder =
                flate2::write::DeflateEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&bytes)?;
            Ok(encoder.finish()?.len().div_ceil(1024) as u64)
        })
        .collect()
}

fn normalize_book(book: MobiBook, text: &str) -> Result<NormalizedPublication, MobiError> {
    let metadata = PublicationMetadata {
        title: book.title().to_string(),
        identifier: book.identifier().to_string(),
        creator: book.creator().map(ToOwned::to_owned),
        description: book.description().map(ToOwned::to_owned),
        date: book.date().map(ToOwned::to_owned),
        publisher: book.publisher().map(ToOwned::to_owned),
        subject: book.subject().map(ToOwned::to_owned),
    };

    let (body, body_start) = mobi_body(text);
    let page_documents = split_mobi_pages_with_offsets(&body, body_start)
        .into_iter()
        .filter(|page| has_mobi_content(&page.document))
        .collect::<Vec<_>>();
    let toc_titles = mobi_toc_titles(text, &page_documents);
    let fallback_titles = book
        .chapters()
        .map(|chapter| chapter.title().to_string())
        .collect::<Vec<_>>();

    let mut chapters = Vec::new();

    if book.cover().is_some() {
        let path = "OEBPS/text/cover.xhtml".to_string();
        chapters.push(PublicationChapter {
            title: "Cover".to_string(),
            path: path.clone(),
            document: r#"<img src="../images/cover.jpg" alt="Cover"/>"#.to_string(),
        });
    }

    for (index, page) in page_documents.iter().enumerate() {
        let path = format!("OEBPS/text/chapter-{index:04}.xhtml");
        let title = mobi_page_title(
            page,
            index,
            &toc_titles,
            &fallback_titles,
            page_documents.len(),
        );
        chapters.push(PublicationChapter {
            title: title.clone(),
            path: path.clone(),
            document: page.document.clone(),
        });
    }

    if chapters.is_empty() {
        return Err(MobiError::Invalid(
            "MOBI contains no readable chapters".to_string(),
        ));
    }

    let mut resources = Vec::new();
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
    }

    let chapter_page_counts = chapter_page_counts(&chapters)?;
    let page_count = chapter_page_counts.iter().sum();
    Ok(NormalizedPublication {
        metadata,
        chapters,
        resources,
        page_count,
        chapter_page_counts,
    })
}

fn write_epub(
    metadata: &PublicationMetadata,
    chapters: &[PublicationChapter],
    resources: &[PublicationResource],
) -> Result<Vec<u8>, MobiError> {
    let mut cursor = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(&mut cursor);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    writer.start_file("mimetype", stored)?;
    writer.write_all(EPUB_MEDIA_TYPE.as_bytes())?;
    writer.start_file("META-INF/container.xml", stored)?;
    writer.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
    )?;

    writer.start_file("OEBPS/content.opf", stored)?;
    writer.write_all(opf(metadata, chapters, resources).as_bytes())?;
    writer.start_file("OEBPS/nav.xhtml", stored)?;
    writer.write_all(nav(chapters).as_bytes())?;

    for chapter in chapters {
        writer.start_file(&chapter.path, deflated)?;
        writer.write_all(xhtml_document(&chapter.document).as_bytes())?;
    }
    for resource in resources {
        writer.start_file(&resource.path, stored)?;
        writer.write_all(&resource.bytes)?;
    }
    writer.finish()?;
    Ok(cursor.into_inner())
}

fn opf(
    metadata: &PublicationMetadata,
    chapters: &[PublicationChapter],
    resources: &[PublicationResource],
) -> String {
    let mut manifest = String::new();
    manifest.push_str(
        r#"<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"#,
    );
    for (index, chapter) in chapters.iter().enumerate() {
        let href = chapter.path.strip_prefix("OEBPS/").unwrap_or(&chapter.path);
        manifest.push_str(&format!(
            r#"<item id="chapter-{index:04}" href="{}" media-type="application/xhtml+xml"/>"#,
            xml_escape(href)
        ));
    }
    for (index, resource) in resources.iter().enumerate() {
        let href = resource
            .path
            .strip_prefix("OEBPS/")
            .unwrap_or(&resource.path);
        let properties = if index == 0 && resource.path.contains("/cover.") {
            " properties=\"cover-image\""
        } else {
            ""
        };
        manifest.push_str(&format!(
            r#"<item id="asset-{index:04}" href="{}" media-type="{}"{} />"#,
            xml_escape(href),
            xml_escape(&resource.media_type),
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
    let document = strip_mobi_filepos_attributes(document);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE html><html xmlns="http://www.w3.org/1999/xhtml" xmlns:mbp="http://www.mobipocket.com/ns/mbp"><head><meta charset="UTF-8"/></head><body>{document}</body></html>"#
    )
}

fn strip_mobi_filepos_attributes(document: &str) -> String {
    const PREFIX: &str = " filepos=";

    let mut normalized = String::with_capacity(document.len());
    let mut cursor = 0;
    while let Some(relative_start) = document[cursor..].find(PREFIX) {
        let start = cursor + relative_start;
        normalized.push_str(&document[cursor..start]);

        let value_start = start + PREFIX.len();
        let value_end = document[value_start..]
            .find(|character: char| character.is_ascii_whitespace() || character == '>')
            .map(|offset| value_start + offset)
            .unwrap_or(document.len());
        cursor = value_end;
    }
    normalized.push_str(&document[cursor..]);
    normalized
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
    use std::io::Read;

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
    fn normalizes_mobi_and_generates_epub_on_request_with_matching_page_count() {
        let publication = normalize_mobi(&fixture()).expect("fixture should be supported");

        assert_eq!(publication.metadata.title, "Fixture title");
        assert_eq!(
            publication.metadata.creator.as_deref(),
            Some("Fixture author")
        );
        assert!(
            publication
                .chapters
                .iter()
                .any(|chapter| chapter.path == "OEBPS/text/chapter-0000.xhtml")
        );

        let mut archive = ZipArchive::new(Cursor::new(
            publication
                .epub_bytes()
                .expect("valid EPUB should be generated"),
        ))
        .expect("valid EPUB ZIP");
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

        let archive_page_count: u64 = publication
            .chapters
            .iter()
            .map(|chapter| {
                archive
                    .by_name(&chapter.path)
                    .expect("chapter should be in generated EPUB")
                    .compressed_size()
                    .div_ceil(1024)
            })
            .sum();
        assert_eq!(publication.page_count, archive_page_count);
    }

    #[test]
    fn wraps_mobi_html_with_the_mobipocket_namespace() {
        let document = xhtml_document("<mbp:pagebreak></mbp:pagebreak>");

        assert!(document.contains(r#"xmlns:mbp="http://www.mobipocket.com/ns/mbp""#));
    }

    #[test]
    fn wraps_legacy_mobi_markup_as_well_formed_xhtml() {
        let document =
            xhtml_document(r#"<mbp:pagebreak></mbp:pagebreak><a filepos=0000001279>Chapter</a>"#);

        assert!(!document.contains("filepos="));
    }

    #[test]
    fn splits_self_closing_and_paired_mobi_pagebreaks() {
        let pages = split_mobi_pages_with_offsets(
            "before<mbp:pagebreak/>middle<mbp:pagebreak></mbp:pagebreak>after",
            0,
        )
        .into_iter()
        .map(|page| page.document)
        .collect::<Vec<_>>();

        assert_eq!(
            pages,
            vec![
                "before".to_string(),
                "middle".to_string(),
                "after".to_string()
            ]
        );
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
    fn reads_generated_publication_resources_directly() {
        let publication = normalize_mobi(&fixture()).expect("fixture should be supported");

        let chapter = publication
            .resource_bytes("OEBPS/text/chapter-0000.xhtml")
            .expect("chapter lookup should succeed")
            .expect("chapter should exist");
        assert!(String::from_utf8_lossy(&chapter).contains("<html"));

        let package = publication
            .resource_bytes("/OEBPS/content.opf")
            .expect("package lookup should succeed")
            .expect("package should exist");
        assert!(String::from_utf8_lossy(&package).contains("Fixture title"));
        assert_eq!(
            publication
                .resource_bytes("OEBPS/missing.xhtml")
                .expect("missing lookup should succeed"),
            None
        );
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
}

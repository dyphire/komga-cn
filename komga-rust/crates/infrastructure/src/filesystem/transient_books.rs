use quick_xml::Reader as XmlReader;
use quick_xml::events::Event as XmlEvent;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use image::GenericImageView;
use lopdf::{Document as PdfDocument, Object};
use pdfium_render::prelude::*;
use serde_json::{Value, json};
use sqlx::Row;
use zip::ZipArchive;

use crate::load_pdfium;
use crate::metadata::{
    infer_transient_comicinfo_provider_metadata, infer_transient_epub_provider_metadata,
};
use crate::rar_support::{detect_rar_media_type, list_rar_entries, read_rar_entry_bytes};
use crate::resolve_stored_path;
use crate::sqlite::connect_read_pool;

const EPUB_DIVINA_LETTER_COUNT_THRESHOLD: usize = 15;
const KOTLIN_PDF_MIN_EDGE: f64 = 3200.0;
#[derive(Clone, Debug)]
pub struct TransientBookFileMetadata {
    pub file_last_modified_unix_nanos: i128,
    pub size_bytes: u64,
}

#[derive(Clone, Debug)]
pub struct TransientBookAnalysis {
    pub status: String,
    pub media_type: String,
    pub page_count: u32,
    pub pages: Vec<TransientBookPage>,
    pub files: Vec<String>,
    pub comment: String,
    pub number: Option<f64>,
    pub series_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TransientBookPage {
    pub number: u32,
    pub file_name: String,
    pub media_type: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub size_bytes: Option<u64>,
}

#[derive(Default)]
struct TransientMetadataInference {
    series_titles: Vec<String>,
    number: Option<f64>,
}

#[derive(Clone)]
struct TransientEpubManifestItem {
    href: String,
    media_type: String,
}

pub async fn infer_transient_series_and_number(
    database_file: &Path,
    path_or_name: &str,
) -> (Option<String>, Option<f64>) {
    let inferred = infer_transient_metadata(path_or_name);
    let number = inferred.number;
    if inferred.series_titles.is_empty() {
        return (None, number);
    }

    let pool = match connect_read_pool(database_file).await {
        Ok(pool) => pool,
        Err(_) => return (None, number),
    };

    for series_title in &inferred.series_titles {
        let exact_match = sqlx::query(
            r#"SELECT s.ID AS ID
             FROM SERIES s
             LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
             WHERE LOWER(COALESCE(sm.TITLE, s.NAME)) = LOWER(?)
             ORDER BY s.LAST_MODIFIED_DATE DESC, s.ID ASC
             LIMIT 1"#,
        )
        .bind(series_title.as_str())
        .fetch_optional(&pool)
        .await
        .ok()
        .flatten()
        .map(|row| row.get::<String, _>("ID"));
        if let Some(series_id) = exact_match {
            return (Some(series_id), number);
        }
    }

    for series_title in &inferred.series_titles {
        let fuzzy_match = sqlx::query(
            r#"SELECT s.ID AS ID
             FROM SERIES s
             LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
             WHERE LOWER(COALESCE(sm.TITLE, s.NAME)) LIKE LOWER(?)
             ORDER BY s.LAST_MODIFIED_DATE DESC, s.ID ASC
             LIMIT 1"#,
        )
        .bind(format!("%{}%", series_title))
        .fetch_optional(&pool)
        .await
        .ok()
        .flatten()
        .map(|row| row.get::<String, _>("ID"));
        if let Some(series_id) = fuzzy_match {
            return (Some(series_id), number);
        }
    }

    (None, number)
}

pub async fn validate_transient_scan_root(database_file: &Path, root: &Path) -> Result<(), String> {
    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("transient scan validation db open failed: {error}"))?;

    let library_roots = sqlx::query("SELECT ROOT AS ROOT FROM LIBRARY")
        .fetch_all(&pool)
        .await
        .map_err(|error| format!("transient scan library roots query failed: {error}"))?
        .into_iter()
        .map(|row| resolve_stored_path(row.get::<String, _>("ROOT").as_str()))
        .collect::<Vec<_>>();

    for library_root in library_roots {
        if root.starts_with(&library_root) {
            return Err("ERR_1017".to_string());
        }
    }

    if !root.is_dir() || fs::read_dir(root).is_err() {
        return Err("ERR_1016".to_string());
    }

    Ok(())
}

pub fn transient_book_exists(path: &str) -> bool {
    Path::new(path).exists()
}

pub fn load_transient_book_file_metadata(path: &str) -> Option<TransientBookFileMetadata> {
    let metadata = fs::metadata(path).ok()?;
    Some(TransientBookFileMetadata {
        file_last_modified_unix_nanos: to_unix_nanos(newest_file_system_time(
            metadata.created().ok(),
            metadata.modified().ok(),
        )),
        size_bytes: metadata.len(),
    })
}

pub fn load_transient_book_media(path: &str) -> Option<Vec<u8>> {
    fs::read(path).ok()
}

pub fn analyze_transient_book(path: &str) -> TransientBookAnalysis {
    if !transient_book_exists(path) {
        return transient_analysis_error("ERROR", String::new(), "ERR_1018");
    }

    let media_type = transient_book_media_type(path);
    if PathBuf::from(path)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("epub"))
        && media_type != "application/epub+zip"
    {
        return transient_analysis_error("ERROR", media_type, "ERR_1032");
    }

    let analysis_result = if media_type.starts_with("image/") {
        Ok(analyze_transient_image(path))
    } else if media_type == "application/epub+zip" {
        return analyze_transient_epub(path).unwrap_or_else(|error_code| {
            transient_analysis_error("ERROR", media_type, error_code)
        });
    } else if media_type == "application/zip" {
        analyze_transient_zip_archive(path).map_err(|_| "ERR_1008")
    } else if matches!(
        media_type.as_str(),
        "application/vnd.comicbook-rar"
            | "application/x-rar-compressed"
            | "application/x-rar-compressed; version=4"
            | "application/x-rar-compressed; version=5"
    ) {
        analyze_transient_rar_archive(path).map_err(|_| "ERR_1008")
    } else if media_type == "application/pdf" {
        analyze_transient_pdf(path).map_err(|_| "ERR_1005")
    } else {
        return transient_analysis_error("UNSUPPORTED", media_type, "ERR_1001");
    };

    let (pages, files) = match analysis_result {
        Ok(result) => result,
        Err(error_code) => return transient_analysis_error("ERROR", media_type, error_code),
    };

    if pages.is_empty() {
        return transient_analysis_error("ERROR", media_type, "ERR_1006");
    }

    TransientBookAnalysis {
        status: "READY".to_string(),
        media_type,
        page_count: pages.len() as u32,
        pages,
        files,
        comment: String::new(),
        number: None,
        series_id: None,
    }
}

fn transient_analysis_error(
    status: &str,
    media_type: String,
    comment: &str,
) -> TransientBookAnalysis {
    TransientBookAnalysis {
        status: status.to_string(),
        media_type,
        page_count: 0,
        pages: Vec::new(),
        files: Vec::new(),
        comment: comment.to_string(),
        number: None,
        series_id: None,
    }
}

pub fn transient_book_page_content(
    path: &str,
    media_type: &str,
    pages: &[TransientBookPage],
    page_number: u32,
) -> Option<(String, Vec<u8>)> {
    if page_number == 0 {
        return None;
    }

    let media_type = if media_type.is_empty() {
        transient_book_media_type(path)
    } else {
        media_type.to_string()
    };
    let content_type = transient_book_content_type(path, media_type.as_str());

    if media_type.starts_with("image/") {
        if page_number != 1 {
            return None;
        }
        let bytes = load_transient_book_media(path)?;
        return Some((media_type, bytes));
    }

    let page = pages
        .iter()
        .find(|entry| entry.number == page_number)
        .cloned()?;

    if matches!(content_type, "application/zip" | "application/epub+zip") {
        let file = fs::File::open(path).ok()?;
        let mut archive = ZipArchive::new(file).ok()?;
        let mut entry = archive.by_name(page.file_name.as_str()).ok()?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).ok()?;
        return Some((page.media_type, bytes));
    }

    if matches!(
        content_type,
        "application/vnd.comicbook-rar"
            | "application/x-rar-compressed"
            | "application/x-rar-compressed; version=4"
            | "application/x-rar-compressed; version=5"
    ) {
        let bytes = read_rar_entry_bytes(Path::new(path), page.file_name.as_str())
            .ok()
            .flatten()?;
        return Some((page.media_type, bytes));
    }

    if content_type == "application/pdf" {
        let bytes = render_pdf_page_image_bytes(path, page_number)?;
        return Some(("image/jpeg".to_string(), bytes));
    }

    None
}

pub fn transient_book_content_type(path: &str, media_type: &str) -> &'static str {
    if !media_type.is_empty() {
        return match media_type {
            "image/jpeg" => "image/jpeg",
            "image/png" => "image/png",
            "image/gif" => "image/gif",
            "image/webp" => "image/webp",
            "image/avif" => "image/avif",
            "application/pdf" => "application/pdf",
            "application/epub+zip" => "application/epub+zip",
            "application/zip" => "application/zip",
            "application/vnd.comicbook-rar" => "application/vnd.comicbook-rar",
            "application/x-rar-compressed" => "application/x-rar-compressed",
            "application/x-rar-compressed; version=4" => "application/x-rar-compressed; version=4",
            "application/x-rar-compressed; version=5" => "application/x-rar-compressed; version=5",
            _ => "application/octet-stream",
        };
    }

    match PathBuf::from(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("pdf") => "application/pdf",
        Some("epub") => detect_epub_media_type(path),
        Some("cbz") | Some("zip") => "application/zip",
        Some("cbr") | Some("rar") => detect_rar_media_type(Path::new(path)),
        _ => "application/octet-stream",
    }
}

fn detect_epub_media_type(path: &str) -> &'static str {
    let Ok(file) = fs::File::open(path) else {
        return "application/octet-stream";
    };
    let Ok(mut archive) = ZipArchive::new(file) else {
        return "application/octet-stream";
    };

    if archive.by_name("META-INF/container.xml").is_ok() {
        "application/epub+zip"
    } else {
        "application/zip"
    }
}

pub fn transient_book_media_type(path: &str) -> String {
    transient_book_content_type(path, "").to_string()
}

pub fn list_transient_book_entries(path: &Path) -> Vec<Value> {
    let mut entries = Vec::new();
    collect_transient_book_entries(path, &mut entries);
    entries.sort_by(|left, right| {
        left["path"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["path"].as_str().unwrap_or_default())
    });
    entries
}

fn analyze_transient_image(path: &str) -> (Vec<TransientBookPage>, Vec<String>) {
    let file_name = PathBuf::from(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();
    let size_bytes = fs::metadata(path).ok().map(|meta| meta.len());
    let (width, height) = fs::read(path)
        .ok()
        .and_then(|bytes| image_dimensions_from_bytes(&bytes))
        .map(|(width, height)| (Some(width), Some(height)))
        .unwrap_or((None, None));

    (
        vec![TransientBookPage {
            number: 1,
            file_name: file_name.clone(),
            media_type: transient_entry_media_type(&file_name),
            width,
            height,
            size_bytes,
        }],
        vec![file_name],
    )
}

fn analyze_transient_epub(path: &str) -> Result<TransientBookAnalysis, &'static str> {
    let file = fs::File::open(path).map_err(|_| "ERR_1032")?;
    let mut archive = ZipArchive::new(file).map_err(|_| "ERR_1032")?;
    let container_xml = read_zip_entry_bytes_normalized(&mut archive, "META-INF/container.xml")
        .ok_or("ERR_1032")?;
    let rootfile_path = parse_transient_epub_rootfile_path(&container_xml).ok_or("ERR_1032")?;
    let package_document =
        read_zip_entry_bytes_normalized(&mut archive, &rootfile_path).ok_or("ERR_1032")?;
    let manifest = parse_transient_epub_manifest_items(&package_document, &rootfile_path);
    let spine = parse_transient_epub_spine_items(&package_document, &manifest);
    let page_count = compute_transient_epub_page_count(&mut archive, &spine);
    let pages = extract_transient_epub_divina_pages(&mut archive, &manifest, &spine)
        .map_err(|_| "ERR_1032")?;
    let mut files = manifest
        .values()
        .map(|item| item.href.clone())
        .collect::<Vec<_>>();
    files.sort();

    Ok(TransientBookAnalysis {
        status: "READY".to_string(),
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
        let normalized_name = normalize_transient_epub_zip_path(entry.name());
        if !spine_paths.contains(normalized_name.as_str()) {
            continue;
        }
        page_count = page_count.saturating_add(entry.compressed_size().div_ceil(1024));
    }

    page_count.min(u32::MAX as u64) as u32
}

fn analyze_transient_zip_archive(
    path: &str,
) -> Result<(Vec<TransientBookPage>, Vec<String>), String> {
    let file = fs::File::open(path).map_err(|error| format!("open archive: {error}"))?;
    let mut archive = ZipArchive::new(file).map_err(|error| format!("read archive: {error}"))?;

    let mut files = Vec::new();
    let mut pages = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("read archive entry: {error}"))?;
        let file_name = entry.name().trim().to_string();
        if file_name.is_empty() || file_name.ends_with('/') {
            continue;
        }

        files.push(file_name.clone());
        if !is_supported_page_image_file_name(&file_name) {
            continue;
        }

        let dimensions = image_dimensions_from_reader(&mut entry);
        pages.push(TransientBookPage {
            number: (pages.len() as u32) + 1,
            file_name: file_name.clone(),
            media_type: transient_entry_media_type(&file_name),
            width: dimensions.map(|(width, _)| width),
            height: dimensions.map(|(_, height)| height),
            size_bytes: Some(entry.size()),
        });
    }

    files.sort();
    Ok((pages, files))
}

fn extract_transient_epub_divina_pages<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    manifest: &std::collections::HashMap<String, TransientEpubManifestItem>,
    spine: &[TransientEpubManifestItem],
) -> Result<Vec<TransientBookPage>, String> {
    let mut pages = Vec::new();

    for item in spine {
        let page = if item.media_type.starts_with("image/") {
            let bytes = read_zip_entry_bytes_normalized(archive, &item.href)
                .ok_or_else(|| format!("missing epub resource {}", item.href))?;
            let dimensions = image_dimensions_from_bytes(&bytes);
            TransientBookPage {
                number: (pages.len() as u32) + 1,
                file_name: item.href.clone(),
                media_type: item.media_type.clone(),
                width: dimensions.map(|(width, _)| width),
                height: dimensions.map(|(_, height)| height),
                size_bytes: Some(bytes.len() as u64),
            }
        } else if is_transient_epub_html_media_type(&item.media_type) {
            let resource_bytes = read_zip_entry_bytes_normalized(archive, &item.href)
                .ok_or_else(|| format!("missing epub resource {}", item.href))?;
            let Some(image_href) =
                parse_transient_epub_divina_image_href(&resource_bytes, &item.href)
            else {
                return Ok(Vec::new());
            };
            let Some(image_item) = manifest.values().find(|entry| entry.href == image_href) else {
                return Ok(Vec::new());
            };
            if !image_item.media_type.starts_with("image/") {
                return Ok(Vec::new());
            }
            let image_bytes = read_zip_entry_bytes_normalized(archive, &image_href)
                .ok_or_else(|| format!("missing epub image resource {image_href}"))?;
            let dimensions = image_dimensions_from_bytes(&image_bytes);
            TransientBookPage {
                number: (pages.len() as u32) + 1,
                file_name: image_href,
                media_type: image_item.media_type.clone(),
                width: dimensions.map(|(width, _)| width),
                height: dimensions.map(|(_, height)| height),
                size_bytes: Some(image_bytes.len() as u64),
            }
        } else {
            return Ok(Vec::new());
        };

        pages.push(page);
    }

    Ok(pages)
}

fn analyze_transient_rar_archive(
    path: &str,
) -> Result<(Vec<TransientBookPage>, Vec<String>), String> {
    let entries =
        list_rar_entries(Path::new(path)).map_err(|_| "Book analysis failed".to_string())?;
    let mut files = entries
        .iter()
        .map(|entry| entry.file_name.clone())
        .collect::<Vec<_>>();
    files.sort();

    let pages = entries
        .into_iter()
        .filter(|entry| is_supported_page_image_file_name(&entry.file_name))
        .enumerate()
        .map(|(index, entry)| {
            let entry_bytes = read_rar_entry_bytes(Path::new(path), &entry.file_name)
                .ok()
                .flatten();
            let dimensions = entry_bytes.as_deref().and_then(image_dimensions_from_bytes);
            TransientBookPage {
                number: (index as u32) + 1,
                file_name: entry.file_name.clone(),
                media_type: transient_entry_media_type(&entry.file_name),
                width: dimensions.map(|(width, _)| width),
                height: dimensions.map(|(_, height)| height),
                size_bytes: Some(entry.unpacked_size),
            }
        })
        .collect::<Vec<_>>();

    Ok((pages, files))
}

fn analyze_transient_pdf(path: &str) -> Result<(Vec<TransientBookPage>, Vec<String>), String> {
    let document = PdfDocument::load(path).map_err(|error| format!("open pdf: {error}"))?;
    let page_count = document.get_pages().len() as u32;
    let pages = (1..=page_count)
        .map(|number| {
            let dimensions = pdf_page_dimensions(&document, number).map(scale_pdf_page_dimensions);
            TransientBookPage {
                number,
                file_name: number.to_string(),
                media_type: "image/jpeg".to_string(),
                width: dimensions.map(|(width, _)| width),
                height: dimensions.map(|(_, height)| height),
                size_bytes: None,
            }
        })
        .collect::<Vec<_>>();

    let file_name = PathBuf::from(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();

    Ok((pages, vec![file_name]))
}

fn render_pdf_page_image_bytes(path: &str, page_number: u32) -> Option<Vec<u8>> {
    if page_number == 0 {
        return None;
    }

    let pdfium = load_pdfium().ok()?;
    let document = pdfium.load_pdf_from_file(path, None).ok()?;
    let page = document
        .pages()
        .get(i32::try_from(page_number.saturating_sub(1)).ok()?)
        .ok()?;
    let rendered = page
        .render_with_config(
            &PdfRenderConfig::new()
                .set_target_width(1600)
                .set_maximum_height(1600),
        )
        .ok()?
        .as_image()
        .ok()?
        .into_rgb8();

    let mut output = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(rendered)
        .write_to(&mut output, image::ImageFormat::Jpeg)
        .ok()?;
    Some(output.into_inner())
}

fn pdf_page_dimensions(document: &PdfDocument, page_number: u32) -> Option<(u32, u32)> {
    let object_id = *document.get_pages().get(&page_number)?;
    let page = document.get_dictionary(object_id).ok()?;
    let media_box = page.get(b"MediaBox").ok()?.as_array().ok()?;
    if media_box.len() != 4 {
        return None;
    }

    let left = pdf_numeric_value(&media_box[0])?;
    let bottom = pdf_numeric_value(&media_box[1])?;
    let right = pdf_numeric_value(&media_box[2])?;
    let top = pdf_numeric_value(&media_box[3])?;
    let width = (right - left).abs().round();
    let height = (top - bottom).abs().round();
    if width <= 0.0 || height <= 0.0 {
        return None;
    }

    Some((width as u32, height as u32))
}

fn pdf_numeric_value(object: &Object) -> Option<f64> {
    match object {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some((*value).into()),
        _ => None,
    }
}

fn transient_entry_media_type(file_name: &str) -> String {
    match file_name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg".to_string(),
        "png" => "image/png".to_string(),
        "gif" => "image/gif".to_string(),
        "webp" => "image/webp".to_string(),
        "avif" => "image/avif".to_string(),
        "xhtml" | "html" | "htm" => "application/xhtml+xml".to_string(),
        "pdf" => "application/pdf".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}

fn is_supported_page_image_file_name(file_name: &str) -> bool {
    matches!(
        file_name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .unwrap_or_default()
            .as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "avif"
    )
}

fn image_dimensions_from_bytes(bytes: &[u8]) -> Option<(u32, u32)> {
    let image = image::load_from_memory(bytes).ok()?;
    Some(image.dimensions())
}

fn image_dimensions_from_reader(reader: &mut dyn Read) -> Option<(u32, u32)> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).ok()?;
    image_dimensions_from_bytes(&bytes)
}

fn scale_pdf_page_dimensions((width, height): (u32, u32)) -> (u32, u32) {
    let min_edge = f64::from(width.min(height));
    if min_edge <= 0.0 {
        return (width, height);
    }

    let scale = KOTLIN_PDF_MIN_EDGE / min_edge;
    let scaled_width = (f64::from(width) * scale).round().max(1.0) as u32;
    let scaled_height = (f64::from(height) * scale).round().max(1.0) as u32;
    (scaled_width, scaled_height)
}

fn infer_transient_metadata(path_or_name: &str) -> TransientMetadataInference {
    let media_type = transient_book_media_type(path_or_name);
    if media_type == "application/epub+zip"
        && let Some(inferred) = infer_transient_epub_metadata_from_path(path_or_name)
    {
        return inferred;
    }

    if matches!(
        media_type.as_str(),
        "application/zip"
            | "application/vnd.comicbook-rar"
            | "application/x-rar-compressed"
            | "application/x-rar-compressed; version=4"
            | "application/x-rar-compressed; version=5"
    ) && let Some(inferred) =
        infer_transient_comicinfo_provider_metadata_from_path(path_or_name, media_type.as_str())
    {
        return inferred;
    }

    TransientMetadataInference::default()
}

fn merge_transient_metadata_inference(
    target: &mut TransientMetadataInference,
    incoming: TransientMetadataInference,
) {
    for title in incoming.series_titles {
        if !title.trim().is_empty()
            && !target
                .series_titles
                .iter()
                .any(|existing| existing == &title)
        {
            target.series_titles.push(title);
        }
    }

    if target.number.is_none() {
        target.number = incoming.number;
    }
}

fn transient_metadata_inference_from_provider(
    provider_inference: crate::metadata::TransientMetadataProviderInference,
) -> TransientMetadataInference {
    TransientMetadataInference {
        series_titles: provider_inference.series_titles,
        number: provider_inference.number,
    }
}

fn infer_transient_comicinfo_provider_metadata_from_path(
    path: &str,
    media_type: &str,
) -> Option<TransientMetadataInference> {
    let comicinfo_bytes = if media_type == "application/zip" {
        let file = fs::File::open(path).ok()?;
        let mut archive = ZipArchive::new(file).ok()?;
        let mut entry = archive.by_name("ComicInfo.xml").ok()?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).ok()?;
        bytes
    } else {
        read_rar_entry_bytes(Path::new(path), "ComicInfo.xml")
            .ok()
            .flatten()?
    };
    let comicinfo_xml = String::from_utf8(comicinfo_bytes).ok()?;
    Some(transient_metadata_inference_from_provider(
        infer_transient_comicinfo_provider_metadata(&comicinfo_xml),
    ))
}

fn infer_transient_epub_metadata_from_path(path: &str) -> Option<TransientMetadataInference> {
    let file = fs::File::open(path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let container_xml = read_zip_entry_bytes_normalized(&mut archive, "META-INF/container.xml")?;
    let rootfile_path = parse_transient_epub_rootfile_path(&container_xml)?;
    let package_document = read_zip_entry_bytes_normalized(&mut archive, &rootfile_path)?;
    let manifest = parse_transient_epub_manifest_items(&package_document, &rootfile_path);
    let mut inferred = transient_metadata_inference_from_provider(
        infer_transient_epub_provider_metadata(&package_document),
    );
    inferred.number = None;

    if let Some(comicinfo_inference) =
        infer_transient_comicinfo_provider_metadata_from_epub_archive(&mut archive, &manifest)
    {
        merge_transient_metadata_inference(&mut inferred, comicinfo_inference);
    }

    Some(inferred)
}

fn infer_transient_comicinfo_provider_metadata_from_epub_archive<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    manifest: &std::collections::HashMap<String, TransientEpubManifestItem>,
) -> Option<TransientMetadataInference> {
    let comicinfo_path = manifest
        .values()
        .find(|item| item.href == "ComicInfo.xml")
        .map(|item| item.href.as_str())?;
    let comicinfo_bytes = read_zip_entry_bytes_normalized(archive, comicinfo_path)?;
    let comicinfo_xml = String::from_utf8(comicinfo_bytes).ok()?;
    Some(transient_metadata_inference_from_provider(
        infer_transient_comicinfo_provider_metadata(&comicinfo_xml),
    ))
}

fn read_zip_entry_bytes_normalized<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
) -> Option<Vec<u8>> {
    let normalized = normalize_transient_epub_zip_path(path);
    let mut entry = archive.by_name(normalized.as_str()).ok()?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

fn parse_transient_epub_rootfile_path(container_xml: &[u8]) -> Option<String> {
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

fn parse_transient_epub_manifest_items(
    package_document: &[u8],
    rootfile_path: &str,
) -> std::collections::HashMap<String, TransientEpubManifestItem> {
    let mut reader = XmlReader::from_reader(package_document);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut manifest = std::collections::HashMap::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(XmlEvent::Start(event)) | Ok(XmlEvent::Empty(event)) => {
                if !transient_xml_name_matches(event.name().as_ref(), b"item") {
                    buffer.clear();
                    continue;
                }
                let Some(id) = transient_xml_attribute_value(&event, b"id") else {
                    buffer.clear();
                    continue;
                };
                let Some(href) = transient_xml_attribute_value(&event, b"href") else {
                    buffer.clear();
                    continue;
                };
                let media_type = transient_xml_attribute_value(&event, b"media-type")
                    .unwrap_or_else(|| "application/octet-stream".to_string());
                manifest.insert(
                    id,
                    TransientEpubManifestItem {
                        href: normalize_transient_epub_resource_href(rootfile_path, &href),
                        media_type,
                    },
                );
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buffer.clear();
    }
    manifest
}

fn parse_transient_epub_spine_items(
    package_document: &[u8],
    manifest: &std::collections::HashMap<String, TransientEpubManifestItem>,
) -> Vec<TransientEpubManifestItem> {
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
                let Some(idref) = transient_xml_attribute_value(&event, b"idref") else {
                    buffer.clear();
                    continue;
                };
                if let Some(item) = manifest.get(&idref) {
                    spine.push(item.clone());
                }
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buffer.clear();
    }
    spine
}

fn parse_transient_epub_divina_image_href(
    resource_bytes: &[u8],
    page_href: &str,
) -> Option<String> {
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
                if inside_body
                    && transient_xml_name_matches(event.name().as_ref(), b"img")
                    && let Some(src) = transient_xml_attribute_value(&event, b"src")
                    && !src.trim().is_empty()
                {
                    image_sources.push(src);
                }
            }
            Ok(XmlEvent::Empty(event)) => {
                if inside_body
                    && transient_xml_name_matches(event.name().as_ref(), b"img")
                    && let Some(src) = transient_xml_attribute_value(&event, b"src")
                    && !src.trim().is_empty()
                {
                    image_sources.push(src);
                }
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
            Err(_) => return None,
            _ => {}
        }
        buffer.clear();
    }

    if text_len > EPUB_DIVINA_LETTER_COUNT_THRESHOLD {
        return None;
    }
    image_sources.sort();
    image_sources.dedup();
    if image_sources.len() > 1 {
        return None;
    }
    let image_href = image_sources.into_iter().next()?;

    Some(normalize_transient_epub_resource_href(
        page_href,
        &image_href,
    ))
}

fn is_transient_epub_html_media_type(media_type: &str) -> bool {
    matches!(
        media_type,
        "application/xhtml+xml" | "text/html" | "application/xml" | "text/xml"
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

fn normalize_transient_epub_zip_path(path: &str) -> String {
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
                .unescape_value()
                .ok()
                .map(|value| value.into_owned())
        })?
    })
}

fn to_unix_nanos(time: Option<SystemTime>) -> i128 {
    time.and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_nanos() as i128)
        .unwrap_or_default()
}

fn newest_file_system_time(
    left: Option<SystemTime>,
    right: Option<SystemTime>,
) -> Option<SystemTime> {
    match (left, right) {
        (Some(left), Some(right)) => Some(std::cmp::max(left, right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn collect_transient_book_entries(path: &Path, entries: &mut Vec<Value>) {
    let Ok(directory_entries) = fs::read_dir(path) else {
        return;
    };

    for entry in directory_entries.filter_map(Result::ok) {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        let entry_path = entry.path();
        let is_hidden = entry_path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| name.starts_with('.'));
        if is_hidden {
            continue;
        }

        if file_type.is_dir() {
            collect_transient_book_entries(&entry_path, entries);
            continue;
        }

        if !is_recognized_transient_book_file(&entry_path) {
            continue;
        }

        let Some(name) = entry_path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .filter(|stem| !stem.is_empty())
        else {
            continue;
        };

        entries.push(json!({
            "name": name,
            "path": entry_path.to_string_lossy().to_string(),
        }));
    }
}

fn is_recognized_transient_book_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some(extension)
            if extension.eq_ignore_ascii_case("cbz")
                || extension.eq_ignore_ascii_case("cbr")
                || extension.eq_ignore_ascii_case("zip")
                || extension.eq_ignore_ascii_case("rar")
                || extension.eq_ignore_ascii_case("pdf")
                || extension.eq_ignore_ascii_case("epub")
                || extension.eq_ignore_ascii_case("jpg")
                || extension.eq_ignore_ascii_case("jpeg")
                || extension.eq_ignore_ascii_case("png")
                || extension.eq_ignore_ascii_case("gif")
                || extension.eq_ignore_ascii_case("webp")
                || extension.eq_ignore_ascii_case("avif")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};
    use lopdf::{Object, Stream, dictionary};
    use std::fs::File;
    use std::io::Write;
    use zip::CompressionMethod;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    fn unique_temp_path(case: &str, extension: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("komga-transient-{case}-{nanos}.{extension}"))
    }

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let image =
            ImageBuffer::<Rgba<u8>, Vec<u8>>::from_pixel(width, height, Rgba([1, 2, 3, 255]));
        let mut output = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut output, image::ImageFormat::Png)
            .expect("png fixture should encode");
        output.into_inner()
    }

    fn write_zip_as_epub(path: &Path) {
        let file = File::create(path).expect("zip-as-epub fixture should be created");
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o644);
        zip.start_file("page-1.png", options)
            .expect("zip-as-epub page entry should be created");
        zip.write_all(b"not-an-image")
            .expect("zip-as-epub page bytes should be written");
        zip.finish()
            .expect("zip-as-epub fixture should finish successfully");
    }

    fn write_single_page_pdf(path: &Path, width: i64, height: i64) {
        let mut document = PdfDocument::with_version("1.5");
        let pages_id = document.new_object_id();
        let page_id = document.new_object_id();
        let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
        let resources_id = document.add_object(dictionary! {});

        document.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), width.into(), height.into()],
                "Contents" => content_id,
                "Resources" => resources_id,
            }),
        );
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );

        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        document.compress();
        document
            .save(path)
            .expect("single-page pdf fixture should save");
    }

    #[test]
    fn analyze_transient_book_populates_image_dimensions_for_single_image() {
        let path = unique_temp_path("single-image", "png");
        fs::write(&path, png_bytes(3, 5)).expect("transient image fixture should be written");

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref());

        assert_eq!(analysis.status, "READY");
        assert_eq!(analysis.pages.len(), 1);
        assert_eq!(analysis.pages[0].width, Some(3));
        assert_eq!(analysis.pages[0].height, Some(5));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn analyze_transient_book_populates_image_dimensions_for_cbz_pages() {
        let path = unique_temp_path("cbz-image-dimensions", "cbz");
        let file = File::create(&path).expect("cbz fixture should be created");
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o644);
        zip.start_file("page-1.png", options)
            .expect("cbz page entry should be created");
        zip.write_all(&png_bytes(7, 11))
            .expect("cbz page bytes should be written");
        zip.finish()
            .expect("cbz fixture should finish successfully");

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref());

        assert_eq!(analysis.status, "READY");
        assert_eq!(analysis.pages.len(), 1);
        assert_eq!(analysis.pages[0].width, Some(7));
        assert_eq!(analysis.pages[0].height, Some(11));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn analyze_transient_book_populates_pdf_page_dimensions() {
        let path = unique_temp_path("pdf-page-dimensions", "pdf");
        write_single_page_pdf(&path, 595, 842);

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref());

        assert_eq!(analysis.status, "READY");
        assert_eq!(analysis.pages.len(), 1);
        assert_eq!(analysis.pages[0].width, Some(3200));
        assert_eq!(analysis.pages[0].height, Some(4528));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn analyze_transient_book_populates_rar_page_dimensions() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../komga/src/test/resources/archives/rar4.rar");

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref());

        assert_eq!(analysis.status, "READY");
        assert_eq!(
            analysis.media_type,
            "application/x-rar-compressed; version=4"
        );
        assert!(
            !analysis.pages.is_empty(),
            "rar transient pages should not be empty"
        );
        assert!(
            analysis.pages[0].width.is_some(),
            "rar page width should be populated"
        );
        assert!(
            analysis.pages[0].height.is_some(),
            "rar page height should be populated"
        );
    }

    #[test]
    fn analyze_transient_book_populates_rar_page_size_bytes() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../komga/src/test/resources/archives/rar4.rar");

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref());

        assert_eq!(analysis.status, "READY");
        assert!(
            !analysis.pages.is_empty(),
            "rar transient pages should not be empty"
        );
        assert!(
            analysis.pages[0].size_bytes.is_some_and(|size| size > 0),
            "rar page size_bytes should be populated"
        );
    }

    #[test]
    fn analyze_transient_book_returns_err_1018_for_missing_file() {
        let path = unique_temp_path("missing-file", "png");

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref());

        assert_eq!(analysis.status, "ERROR");
        assert_eq!(analysis.media_type, "");
        assert_eq!(analysis.comment, "ERR_1018");
        assert!(analysis.pages.is_empty());
        assert!(analysis.files.is_empty());
    }

    #[test]
    fn analyze_transient_book_returns_err_1032_for_broken_epub() {
        let path = unique_temp_path("broken-epub", "epub");
        write_zip_as_epub(&path);

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref());

        assert_eq!(analysis.status, "ERROR");
        assert_eq!(analysis.media_type, "application/zip");
        assert_eq!(analysis.comment, "ERR_1032");
        assert!(analysis.pages.is_empty());
        assert!(analysis.files.is_empty());

        let _ = fs::remove_file(path);
    }
}

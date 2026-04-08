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
use crate::rar_support::{detect_rar_media_type, list_rar_entries, read_rar_entry_bytes};
use crate::sqlite::connect_pool;

#[derive(Clone, Debug)]
pub struct TransientBookFileMetadata {
    pub file_last_modified_epoch_seconds: i64,
    pub size_bytes: u64,
}

#[derive(Clone, Debug)]
pub struct TransientBookAnalysis {
    pub status: String,
    pub media_type: String,
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

pub async fn infer_transient_series_and_number(
    database_file: &Path,
    file_name: &str,
) -> (Option<String>, Option<f64>) {
    let (series_title_candidate, number) = parse_transient_series_and_number_candidate(file_name);
    if series_title_candidate.is_empty() {
        return (None, number);
    }

    let pool = match connect_pool(database_file, 1).await {
        Ok(pool) => pool,
        Err(_) => return (None, number),
    };

    let exact_match = sqlx::query(
        "SELECT s.ID AS ID \
         FROM SERIES s \
         LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         WHERE LOWER(COALESCE(sm.TITLE, s.NAME)) = LOWER(?) \
         ORDER BY s.LAST_MODIFIED_DATE DESC, s.ID ASC \
         LIMIT 1",
    )
    .bind(series_title_candidate.as_str())
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten()
    .map(|row| row.get::<String, _>("ID"));

    let fuzzy_match = if exact_match.is_none() {
        sqlx::query(
            "SELECT s.ID AS ID \
             FROM SERIES s \
             LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
             WHERE LOWER(COALESCE(sm.TITLE, s.NAME)) LIKE LOWER(?) \
             ORDER BY s.LAST_MODIFIED_DATE DESC, s.ID ASC \
             LIMIT 1",
        )
        .bind(format!("%{}%", series_title_candidate))
        .fetch_optional(&pool)
        .await
        .ok()
        .flatten()
        .map(|row| row.get::<String, _>("ID"))
    } else {
        None
    };

    (exact_match.or(fuzzy_match), number)
}

pub async fn validate_transient_scan_root(database_file: &Path, root: &Path) -> Result<(), String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("transient scan validation db open failed: {error}"))?;

    let library_roots = sqlx::query("SELECT ROOT AS ROOT FROM LIBRARY")
        .fetch_all(&pool)
        .await
        .map_err(|error| format!("transient scan library roots query failed: {error}"))?
        .into_iter()
        .map(|row| PathBuf::from(row.get::<String, _>("ROOT")))
        .collect::<Vec<_>>();
    pool.close().await;

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
        file_last_modified_epoch_seconds: to_unix_seconds(metadata.modified().ok()),
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
    } else if matches!(
        media_type.as_str(),
        "application/zip" | "application/epub+zip"
    ) {
        analyze_transient_zip_archive(path, media_type == "application/epub+zip").map_err(|_| {
            if media_type == "application/epub+zip" {
                "ERR_1032"
            } else {
                "ERR_1008"
            }
        })
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

fn analyze_transient_zip_archive(
    path: &str,
    include_epub_resources: bool,
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
        let include = if include_epub_resources {
            is_epub_page_resource_file_name(&file_name)
        } else {
            is_supported_page_image_file_name(&file_name)
        };
        if !include {
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
            let dimensions = pdf_page_dimensions(&document, number);
            TransientBookPage {
                number,
                file_name: format!("page-{number}.pdf"),
                media_type: "application/pdf".to_string(),
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

fn is_epub_page_resource_file_name(file_name: &str) -> bool {
    matches!(
        file_name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .unwrap_or_default()
            .as_str(),
        "xhtml" | "html" | "htm"
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

fn to_unix_seconds(time: Option<SystemTime>) -> i64 {
    time.and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_secs() as i64)
        .unwrap_or_default()
}

fn parse_transient_series_and_number_candidate(file_name: &str) -> (String, Option<f64>) {
    let file_path = PathBuf::from(file_name);
    let stem = file_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(file_name)
        .trim();
    if stem.is_empty() {
        return (String::new(), None);
    }

    let normalized = stem
        .chars()
        .map(|ch| {
            if ch == '_' || ch == '-' || ch == '.' {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>();

    let mut parts = normalized
        .split_whitespace()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return (String::new(), None);
    }

    let mut number = None;
    if let Some(last) = parts.last()
        && let Ok(parsed_number) = last.parse::<f64>()
    {
        number = Some(parsed_number);
        let _ = parts.pop();
    }

    let series_title_candidate = if parts.is_empty() {
        normalized.trim().to_string()
    } else {
        parts.join(" ")
    };

    (series_title_candidate, number)
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
        assert_eq!(analysis.pages[0].width, Some(595));
        assert_eq!(analysis.pages[0].height, Some(842));

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

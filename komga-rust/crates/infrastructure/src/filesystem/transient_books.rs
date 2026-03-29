use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use lopdf::Document as PdfDocument;
use serde_json::{Value, json};
use sqlx::Row;
use zip::ZipArchive;

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
        "SELECT s.ID AS ID\n         FROM SERIES s\n         LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID\n         WHERE LOWER(COALESCE(sm.TITLE, s.NAME)) = LOWER(?)\n         ORDER BY s.LAST_MODIFIED_DATE DESC, s.ID ASC\n         LIMIT 1",
    )
    .bind(series_title_candidate.as_str())
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten()
    .map(|row| row.get::<String, _>("ID"));

    let fuzzy_match = if exact_match.is_none() {
        sqlx::query(
            "SELECT s.ID AS ID\n             FROM SERIES s\n             LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID\n             WHERE LOWER(COALESCE(sm.TITLE, s.NAME)) LIKE LOWER(?)\n             ORDER BY s.LAST_MODIFIED_DATE DESC, s.ID ASC\n             LIMIT 1",
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

pub fn analyze_transient_book(path: &str) -> Result<TransientBookAnalysis, String> {
    if !transient_book_exists(path) {
        return Err("File not found, it may have moved".to_string());
    }

    let media_type = transient_book_media_type(path);
    let (pages, files) = if transient_media_is_image(path, &media_type) {
        analyze_transient_image(path)
    } else if transient_media_is_zip_archive(path, &media_type) {
        analyze_transient_zip_archive(path)?
    } else if transient_media_is_rar_archive(path, &media_type) {
        analyze_transient_rar_archive(path)?
    } else if transient_media_is_pdf(path, &media_type) {
        analyze_transient_pdf(path)?
    } else {
        return Err(format!("unsupported media type: {media_type}"));
    };

    if pages.is_empty() {
        return Err("Book analysis failed".to_string());
    }

    Ok(TransientBookAnalysis {
        status: "READY".to_string(),
        media_type,
        pages,
        files,
        comment: String::new(),
        number: None,
        series_id: None,
    })
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

    if transient_media_is_image(path, media_type.as_str()) {
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

    if transient_media_is_zip_archive(path, media_type.as_str()) {
        let file = fs::File::open(path).ok()?;
        let mut archive = ZipArchive::new(file).ok()?;
        let mut entry = archive.by_name(page.file_name.as_str()).ok()?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).ok()?;
        return Some((page.media_type, bytes));
    }

    if transient_media_is_rar_archive(path, media_type.as_str()) {
        let bytes = read_rar_entry_bytes_cli(path, page.file_name.as_str())?;
        return Some((page.media_type, bytes));
    }

    if transient_media_is_pdf(path, media_type.as_str()) {
        let bytes = read_pdf_page_content_bytes(path, page_number)?;
        return Some(("application/pdf".to_string(), bytes));
    }

    None
}

pub fn transient_book_content_type(path: &str, media_type: &str) -> &'static str {
    if !media_type.is_empty() {
        return match media_type {
            "application/pdf" => "application/pdf",
            "application/epub+zip" => "application/epub+zip",
            "application/zip" => "application/zip",
            "application/vnd.comicbook-rar" => "application/vnd.comicbook-rar",
            _ => "application/octet-stream",
        };
    }

    match PathBuf::from(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("pdf") => "application/pdf",
        Some("epub") => "application/epub+zip",
        Some("cbz") | Some("zip") => "application/zip",
        Some("cbr") | Some("rar") => "application/vnd.comicbook-rar",
        _ => "application/octet-stream",
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

    (
        vec![TransientBookPage {
            number: 1,
            file_name: file_name.clone(),
            media_type: transient_entry_media_type(&file_name),
            width: None,
            height: None,
            size_bytes,
        }],
        vec![file_name],
    )
}

fn analyze_transient_zip_archive(
    path: &str,
) -> Result<(Vec<TransientBookPage>, Vec<String>), String> {
    let file = fs::File::open(path).map_err(|error| format!("open archive: {error}"))?;
    let mut archive = ZipArchive::new(file).map_err(|error| format!("read archive: {error}"))?;

    let mut files = Vec::new();
    let mut pages = Vec::new();
    for index in 0..archive.len() {
        let entry = archive
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

        pages.push(TransientBookPage {
            number: (pages.len() as u32) + 1,
            file_name: file_name.clone(),
            media_type: transient_entry_media_type(&file_name),
            width: None,
            height: None,
            size_bytes: Some(entry.size()),
        });
    }

    files.sort();
    Ok((pages, files))
}

fn analyze_transient_rar_archive(
    path: &str,
) -> Result<(Vec<TransientBookPage>, Vec<String>), String> {
    let output = Command::new("unrar")
        .arg("lb")
        .arg(path)
        .output()
        .map_err(|error| format!("list rar entries: {error}"))?;
    if !output.status.success() {
        return Err("Book analysis failed".to_string());
    }

    let mut files = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.ends_with('/'))
        .map(str::to_string)
        .collect::<Vec<_>>();
    files.sort();

    let pages = files
        .iter()
        .filter(|file_name| is_supported_page_image_file_name(file_name))
        .enumerate()
        .map(|(index, file_name)| TransientBookPage {
            number: (index as u32) + 1,
            file_name: file_name.clone(),
            media_type: transient_entry_media_type(file_name),
            width: None,
            height: None,
            size_bytes: None,
        })
        .collect::<Vec<_>>();

    Ok((pages, files))
}

fn analyze_transient_pdf(path: &str) -> Result<(Vec<TransientBookPage>, Vec<String>), String> {
    let document = PdfDocument::load(path).map_err(|error| format!("open pdf: {error}"))?;
    let page_count = document.get_pages().len() as u32;
    let pages = (1..=page_count)
        .map(|number| TransientBookPage {
            number,
            file_name: format!("page-{number}.pdf"),
            media_type: "application/pdf".to_string(),
            width: None,
            height: None,
            size_bytes: None,
        })
        .collect::<Vec<_>>();

    let file_name = PathBuf::from(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();

    Ok((pages, vec![file_name]))
}

fn read_rar_entry_bytes_cli(archive_path: &str, entry_name: &str) -> Option<Vec<u8>> {
    let output = Command::new("unrar")
        .arg("p")
        .arg("-inul")
        .arg(archive_path)
        .arg(entry_name)
        .output()
        .ok()?;
    if !output.status.success() || output.stdout.is_empty() {
        return None;
    }
    Some(output.stdout)
}

fn read_pdf_page_content_bytes(path: &str, page_number: u32) -> Option<Vec<u8>> {
    let document = PdfDocument::load(path).ok()?;
    let pages = document.get_pages();
    let object_id = *pages.get(&page_number)?;
    document.get_page_content(object_id).ok()
}

fn transient_media_is_image(path: &str, media_type: &str) -> bool {
    if media_type.starts_with("image/") {
        return true;
    }
    matches!(
        PathBuf::from(path)
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .as_deref(),
        Some("jpg") | Some("jpeg") | Some("png") | Some("gif") | Some("webp") | Some("avif")
    )
}

fn transient_media_is_zip_archive(path: &str, media_type: &str) -> bool {
    matches!(
        transient_book_content_type(path, media_type),
        "application/zip" | "application/epub+zip"
    )
}

fn transient_media_is_rar_archive(path: &str, media_type: &str) -> bool {
    transient_book_content_type(path, media_type) == "application/vnd.comicbook-rar"
}

fn transient_media_is_pdf(path: &str, media_type: &str) -> bool {
    transient_book_content_type(path, media_type) == "application/pdf"
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

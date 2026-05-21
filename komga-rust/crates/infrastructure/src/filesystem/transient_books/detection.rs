use std::fs;
use std::path::{Path, PathBuf};

use zip::ZipArchive;

use crate::rar_support::detect_rar_media_type;

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

pub fn transient_book_media_type(path: &str) -> String {
    transient_book_content_type(path, "").to_string()
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

pub(super) fn transient_entry_media_type(file_name: &str) -> String {
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

pub(super) fn is_supported_page_image_file_name(file_name: &str) -> bool {
    matches!(
        file_name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .unwrap_or_default()
            .as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "avif"
    )
}

pub(super) fn is_recognized_transient_book_file(path: &Path) -> bool {
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

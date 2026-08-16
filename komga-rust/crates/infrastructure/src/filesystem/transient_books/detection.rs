use std::path::Path;

use crate::filesystem::media_analysis::transient_media_type_from_path;

pub(crate) fn transient_book_content_type(path: &str, media_type: &str) -> &'static str {
    if !media_type.is_empty() {
        return known_transient_media_type(media_type);
    }

    let media_type = transient_media_type_from_path(Path::new(path))
        .unwrap_or_else(|| "application/octet-stream".to_string());
    known_transient_media_type(media_type.as_str())
}

pub(crate) fn transient_book_media_type(path: &str) -> String {
    transient_book_content_type(path, "").to_string()
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
                || extension.eq_ignore_ascii_case("mobi")
                || extension.eq_ignore_ascii_case("jpg")
                || extension.eq_ignore_ascii_case("jpeg")
                || extension.eq_ignore_ascii_case("png")
                || extension.eq_ignore_ascii_case("gif")
                || extension.eq_ignore_ascii_case("webp")
                || extension.eq_ignore_ascii_case("avif")
    )
}

fn known_transient_media_type(media_type: &str) -> &'static str {
    match media_type {
        "image/jpeg" => "image/jpeg",
        "image/png" => "image/png",
        "image/gif" => "image/gif",
        "image/webp" => "image/webp",
        "image/avif" => "image/avif",
        "application/pdf" => "application/pdf",
        "application/epub+zip" => "application/epub+zip",
        "application/x-mobipocket-ebook" => "application/x-mobipocket-ebook",
        "application/zip" => "application/zip",
        "application/vnd.comicbook-rar" => "application/vnd.comicbook-rar",
        "application/x-rar-compressed" => "application/x-rar-compressed",
        "application/x-rar-compressed; version=4" => "application/x-rar-compressed; version=4",
        "application/x-rar-compressed; version=5" => "application/x-rar-compressed; version=5",
        _ => "application/octet-stream",
    }
}

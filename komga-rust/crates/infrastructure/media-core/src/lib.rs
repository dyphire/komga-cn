pub mod content;
pub mod formats;

pub use content::ContentResolver;
pub use formats::ZipArchiveBuilder;

pub fn expected_extension_for_media_type(media_type: &str) -> Option<&'static str> {
    match media_type {
        "application/vnd.comicbook-rar"
        | "application/x-rar-compressed"
        | "application/x-rar-compressed; version=4"
        | "application/x-rar-compressed; version=5" => Some("cbr"),
        "application/zip" => Some("cbz"),
        "application/pdf" => Some("pdf"),
        "application/epub+zip" => Some("epub"),
        komga_epub::MOBI_MEDIA_TYPE => Some("mobi"),
        _ => None,
    }
}

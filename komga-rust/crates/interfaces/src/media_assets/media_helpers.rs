use super::types::PersistedBookMedia;

pub(crate) fn content_type_from_filename(file_name: &str, default_mime_type: &str) -> String {
    let extension = file_name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();

    match extension.as_str() {
        "cbz" => "application/vnd.comicbook+zip".to_string(),
        "zip" => "application/zip".to_string(),
        "cbr" => "application/vnd.comicbook-rar".to_string(),
        "pdf" => "application/pdf".to_string(),
        "epub" => "application/epub+zip".to_string(),
        "jpg" | "jpeg" => "image/jpeg".to_string(),
        "png" => "image/png".to_string(),
        "gif" => "image/gif".to_string(),
        "webp" => "image/webp".to_string(),
        "avif" => "image/avif".to_string(),
        _ => default_mime_type.to_string(),
    }
}

pub(crate) fn book_media_supports_page_image(media: &PersistedBookMedia) -> bool {
    content_type_from_filename(&media.file_name, &media.media_type).starts_with("image/")
}

pub(crate) fn book_media_is_single_image(media: &PersistedBookMedia) -> bool {
    book_media_supports_page_image(media)
}

pub(crate) fn book_media_is_epub(media: &PersistedBookMedia) -> bool {
    content_type_from_filename(&media.file_name, &media.media_type) == "application/epub+zip"
}

pub(crate) fn book_media_is_pdf(media: &PersistedBookMedia) -> bool {
    content_type_from_filename(&media.file_name, &media.media_type) == "application/pdf"
}

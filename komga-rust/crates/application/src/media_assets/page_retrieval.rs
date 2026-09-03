use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookMediaRecord {
    pub library_id: String,
    pub file_name: String,
    pub file_path: PathBuf,
    pub media_type: String,
    pub page_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookPageRecord {
    pub number: u64,
    pub file_name: String,
    pub media_type: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub file_size: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedMediaFileRecord {
    pub file_name: String,
    pub media_type: String,
    pub sub_type: Option<String>,
}

pub(crate) fn scale_pdf_page_dimensions(
    width: Option<i64>,
    height: Option<i64>,
) -> (Option<i64>, Option<i64>) {
    const PDF_RESOLUTION: f64 = 3200.0;

    let (Some(width), Some(height)) = (width, height) else {
        return (None, None);
    };
    let min_edge = width.min(height);
    if min_edge <= 0 {
        return (Some(width), Some(height));
    }

    let scale = PDF_RESOLUTION / min_edge as f64;
    let scaled_width = (width as f64 * scale).round().max(1.0) as i64;
    let scaled_height = (height as f64 * scale).round().max(1.0) as i64;
    (Some(scaled_width), Some(scaled_height))
}

pub fn content_type_from_filename(file_name: &str, default_mime_type: &str) -> String {
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
        "mobi" => "application/x-mobipocket-ebook".to_string(),
        "jpg" | "jpeg" => "image/jpeg".to_string(),
        "png" => "image/png".to_string(),
        "gif" => "image/gif".to_string(),
        "webp" => "image/webp".to_string(),
        "avif" => "image/avif".to_string(),
        "html" | "xhtml" => "application/xhtml+xml".to_string(),
        "css" => "text/css".to_string(),
        "svg" => "image/svg+xml".to_string(),
        "xml" => "application/xml".to_string(),
        "ncx" => "application/x-dtbncx+xml".to_string(),
        "opf" => "application/oebps-package+xml".to_string(),
        "woff" => "font/woff".to_string(),
        "woff2" => "font/woff2".to_string(),
        "ttf" => "font/ttf".to_string(),
        "otf" => "font/otf".to_string(),
        "eot" => "application/vnd.ms-fontobject".to_string(),
        _ => default_mime_type.to_string(),
    }
}

pub fn book_media_supports_page_image(media: &BookMediaRecord) -> bool {
    content_type_from_filename(&media.file_name, &media.media_type).starts_with("image/")
}

pub fn book_media_is_single_image(media: &BookMediaRecord) -> bool {
    book_media_supports_page_image(media)
}

pub fn book_media_is_zip_archive(media: &BookMediaRecord) -> bool {
    matches!(
        content_type_from_filename(&media.file_name, &media.media_type).as_str(),
        "application/vnd.comicbook+zip" | "application/epub+zip" | "application/zip"
    )
}

pub fn book_media_is_rar_archive(media: &BookMediaRecord) -> bool {
    matches!(
        content_type_from_filename(&media.file_name, &media.media_type).as_str(),
        "application/x-rar-compressed"
            | "application/x-rar-compressed; version=4"
            | "application/x-rar-compressed; version=5"
            | "application/vnd.comicbook-rar"
    )
}

pub fn book_media_is_epub(media: &BookMediaRecord) -> bool {
    matches!(
        content_type_from_filename(&media.file_name, &media.media_type).as_str(),
        "application/epub+zip" | "application/x-mobipocket-ebook"
    )
}

pub fn book_media_is_pdf(media: &BookMediaRecord) -> bool {
    content_type_from_filename(&media.file_name, &media.media_type) == "application/pdf"
}

pub fn book_media_supports_page_api(media: &BookMediaRecord) -> bool {
    book_media_is_single_image(media)
        || media.page_count > 0
        || book_media_is_zip_archive(media)
        || book_media_is_rar_archive(media)
        || book_media_is_pdf(media)
}

pub fn is_supported_page_image_file_name(file_name: &str) -> bool {
    matches!(
        file_name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .unwrap_or_default()
            .as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "avif"
    )
}

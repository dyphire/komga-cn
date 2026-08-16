use komga_application::media_assets::content_type_from_filename;

use super::types::PersistedBookMedia;

pub(crate) fn book_media_supports_page_image(media: &PersistedBookMedia) -> bool {
    content_type_from_filename(&media.file_name, &media.media_type).starts_with("image/")
}

pub(crate) fn book_media_is_single_image(media: &PersistedBookMedia) -> bool {
    book_media_supports_page_image(media)
}

pub(crate) fn book_media_is_epub(media: &PersistedBookMedia) -> bool {
    matches!(
        content_type_from_filename(&media.file_name, &media.media_type).as_str(),
        "application/epub+zip" | "application/x-mobipocket-ebook"
    )
}

pub(crate) fn book_media_is_pdf(media: &PersistedBookMedia) -> bool {
    content_type_from_filename(&media.file_name, &media.media_type) == "application/pdf"
}

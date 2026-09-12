use komga_application::media_assets::content_type_from_filename;

use super::types::PersistedBookMedia;

pub(crate) fn book_media_is_epub(media: &PersistedBookMedia) -> bool {
    matches!(
        content_type_from_filename(&media.file_name, &media.media_type).as_str(),
        "application/epub+zip" | "application/x-mobipocket-ebook"
    )
}

use komga_application::media_assets::{
    BookMediaContentPort, BookMediaReaderPort, BookMediaRecord, BookPageRecord,
};

use super::media_helpers::{
    book_media_is_pdf, book_media_is_single_image, content_type_from_filename,
};

async fn render_book_page_thumbnail(
    content: &dyn BookMediaContentPort,
    media: &BookMediaRecord,
    page: &BookPageRecord,
    page_number: u64,
    max_edge: u32,
) -> anyhow::Result<Option<Vec<u8>>> {
    content
        .render_page_thumbnail(media, page, page_number, max_edge)
        .await
}

pub(crate) async fn load_book_thumbnail_page_source_bytes(
    reader: &dyn BookMediaReaderPort,
    content: &dyn BookMediaContentPort,
    book_id: &str,
    media: &BookMediaRecord,
) -> anyhow::Result<Option<Vec<u8>>> {
    if book_media_is_single_image(media) {
        return content.read_media_file_bytes(&media.file_path).await;
    }

    if book_media_is_pdf(media) {
        let page_row = reader
            .book_page(book_id, 1)
            .await?
            .map_or_else(|| content.pdf_page_row(media, 1), |row| Ok(Some(row)))?;
        let Some(page_row) = page_row else {
            return Ok(None);
        };
        return render_book_page_thumbnail(content, media, &page_row, 1, 300).await;
    }

    let page_row = match reader.book_page(book_id, 1).await? {
        Some(page_row) => page_row,
        None => {
            let Some(page_row) = content.archive_page_row(media, 1).await? else {
                return Ok(None);
            };
            page_row
        }
    };
    let media_type = page_row_media_type(&page_row, media);
    if !media_type.to_ascii_lowercase().starts_with("image/") {
        return Ok(None);
    }

    content.resolve_page_bytes(media, &page_row, 1).await
}

fn page_row_media_type(page_row: &BookPageRecord, media: &BookMediaRecord) -> String {
    if page_row.media_type.is_empty() {
        content_type_from_filename(&page_row.file_name, &media.media_type)
    } else {
        page_row.media_type.clone()
    }
}

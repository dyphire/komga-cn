use std::path::Path;

use komga_application::media_assets::{BookMediaRecord, BookPageRecord};
use komga_infrastructure::content_resolver::ContentResolver;
use komga_infrastructure::media_reader::MediaReader;

use super::media_helpers::{
    book_media_is_pdf, book_media_is_single_image, content_type_from_filename,
};

pub(crate) async fn load_book_page_row(
    reader: &MediaReader,
    content: &ContentResolver,
    book_id: &str,
    media: &BookMediaRecord,
    page_number: u64,
    allow_pdf_fallback: bool,
) -> Result<Option<BookPageRecord>, String> {
    match reader.book_page(book_id, page_number).await {
        Ok(Some(row)) => Ok(Some(row)),
        Ok(None) if book_media_is_single_image(media) && page_number == 1 => Ok(Some(
            single_image_page_row(content, media, page_number).await,
        )),
        Ok(None) => {
            if let Some(row) = content.archive_page_row(media, page_number).await {
                return Ok(Some(row));
            }
            if allow_pdf_fallback {
                return Ok(content.pdf_page_row(media, page_number));
            }
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

pub(crate) async fn list_book_page_rows(
    reader: &MediaReader,
    content: &ContentResolver,
    book_id: &str,
    media: &BookMediaRecord,
) -> Result<Option<Vec<BookPageRecord>>, String> {
    let page_rows = reader.book_pages(book_id).await?;

    if !page_rows.is_empty() {
        let page_rows = if book_media_is_pdf(media) {
            map_kotlin_pdf_pages(page_rows)
        } else {
            page_rows
        };
        return Ok(Some(page_rows));
    }

    if let Some(archive_rows) = content.archive_page_rows(media).await
        && !archive_rows.is_empty()
    {
        return Ok(Some(archive_rows));
    }

    let generated_pdf_rows = content.generated_pdf_page_rows(media);
    if !generated_pdf_rows.is_empty() {
        return Ok(Some(generated_pdf_rows));
    }

    if !book_media_is_single_image(media) {
        return Ok(None);
    }

    Ok(Some(vec![single_image_page_row(content, media, 1).await]))
}

pub(crate) async fn render_book_page_thumbnail(
    content: &ContentResolver,
    media: &BookMediaRecord,
    page: &BookPageRecord,
    page_number: u64,
    max_edge: u32,
) -> Option<Vec<u8>> {
    content
        .render_page_thumbnail(media, page, page_number, max_edge)
        .await
}

pub(crate) async fn load_book_thumbnail_page_source_bytes(
    reader: &MediaReader,
    content: &ContentResolver,
    book_id: &str,
    media: &BookMediaRecord,
) -> Option<Vec<u8>> {
    if book_media_is_single_image(media) {
        return content.read_media_file_bytes(&media.file_path).await;
    }

    if book_media_is_pdf(media) {
        let page_row = reader
            .book_page(book_id, 1)
            .await
            .ok()
            .flatten()
            .or_else(|| content.pdf_page_row(media, 1))?;
        return render_book_page_thumbnail(content, media, &page_row, 1, 300).await;
    }

    let page_row = if let Some(page_row) = reader.book_page(book_id, 1).await.ok().flatten() {
        page_row
    } else {
        content.archive_page_row(media, 1).await?
    };
    let media_type = page_row_media_type(&page_row, media);
    if !media_type.to_ascii_lowercase().starts_with("image/") {
        return None;
    }

    content.resolve_page_bytes(media, &page_row, 1).await
}

pub(crate) fn page_row_media_type(page_row: &BookPageRecord, media: &BookMediaRecord) -> String {
    if page_row.media_type.is_empty() {
        content_type_from_filename(&page_row.file_name, &media.media_type)
    } else {
        page_row.media_type.clone()
    }
}

async fn single_image_page_row(
    content: &ContentResolver,
    media: &BookMediaRecord,
    page_number: u64,
) -> BookPageRecord {
    let (width, height) = read_media_image_dimensions(content, media.file_path.as_path())
        .await
        .map(|(width, height)| (Some(width), Some(height)))
        .unwrap_or((None, None));
    BookPageRecord {
        number: page_number,
        file_name: media.file_name.clone(),
        media_type: content_type_from_filename(&media.file_name, &media.media_type),
        width,
        height,
        file_size: content
            .read_media_file_size(&media.file_path)
            .await
            .unwrap_or(0),
    }
}

async fn read_media_image_dimensions(content: &ContentResolver, path: &Path) -> Option<(i64, i64)> {
    let bytes = content.read_media_file_bytes(path).await?;
    let image = image::load_from_memory(&bytes).ok()?;
    Some((i64::from(image.width()), i64::from(image.height())))
}

fn map_kotlin_pdf_pages(page_rows: Vec<BookPageRecord>) -> Vec<BookPageRecord> {
    page_rows
        .into_iter()
        .map(|page| {
            let (width, height) = scale_pdf_dimensions(page.width, page.height);
            BookPageRecord {
                media_type: "image/jpeg".to_string(),
                width,
                height,
                ..page
            }
        })
        .collect()
}

fn scale_pdf_dimensions(width: Option<i64>, height: Option<i64>) -> (Option<i64>, Option<i64>) {
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

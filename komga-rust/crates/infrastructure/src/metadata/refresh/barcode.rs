use std::io::Cursor;

use komga_application::media_assets::{
    BookMediaRecord, book_media_is_epub, book_media_is_pdf, book_media_is_single_image,
};
use pdfium_render::prelude::*;
use rxing::{BarcodeFormat, DecodeHints, helpers as rxing_helpers};
use sqlx::SqlitePool;

use crate::filesystem::media_access::page_content::{load_archive_page_row, resolve_book_page_bytes};
use crate::load_pdfium;

use super::BookMetadataImportPatch;
use super::support::normalize_isbn13;

pub(super) async fn refresh_barcode_isbn(pool: &SqlitePool, book_id: &str) -> Result<(), String> {
    let Some(media) = super::load_book_media_for_refresh(pool, book_id).await? else {
        return Ok(());
    };
    if book_media_is_epub(&media) {
        return Ok(());
    }

    let page_count = media.page_count.max(1);
    for page_number in barcode_candidate_pages(page_count) {
        let Some(image_bytes) =
            load_barcode_candidate_image_bytes(pool, book_id, &media, page_number).await?
        else {
            continue;
        };
        let Some(isbn) = decode_ean13_isbn(&image_bytes) else {
            continue;
        };

        super::apply_book_metadata_import_patch(
            pool,
            book_id,
            BookMetadataImportPatch {
                isbn: Some(isbn),
                ..Default::default()
            },
        )
        .await?;
        break;
    }

    Ok(())
}

fn barcode_candidate_pages(page_count: u64) -> Vec<u64> {
    let mut pages = Vec::new();
    for page_number in (1..=page_count).rev().take(3) {
        pages.push(page_number);
    }
    for page_number in 1..=page_count.min(3) {
        if !pages.contains(&page_number) {
            pages.push(page_number);
        }
    }
    pages
}

async fn load_barcode_candidate_image_bytes(
    pool: &SqlitePool,
    book_id: &str,
    media: &BookMediaRecord,
    page_number: u64,
) -> Result<Option<Vec<u8>>, String> {
    if book_media_is_pdf(media) {
        return Ok(Some(render_pdf_page_image_for_barcode(media, page_number)?));
    }

    if book_media_is_single_image(media) && page_number == 1 {
        return tokio::fs::read(&media.file_path)
            .await
            .map(Some)
            .map_err(|error| {
                format!(
                    "failed to read single-image barcode candidate '{}' for '{}': {error}",
                    media.file_path.display(),
                    book_id,
                )
            });
    }

    let page = if let Some(page) =
        super::load_book_page_row_for_refresh(pool, book_id, page_number).await?
    {
        Some(page)
    } else {
        load_archive_page_row(media, page_number).await
    };
    let Some(page) = page else {
        return Ok(None);
    };

    Ok(resolve_book_page_bytes(media, &page, page_number).await)
}

fn render_pdf_page_image_for_barcode(
    media: &BookMediaRecord,
    page_number: u64,
) -> Result<Vec<u8>, String> {
    let pdfium = load_pdfium()?;
    let document = pdfium
        .load_pdf_from_file(&media.file_path, None)
        .map_err(|error| {
            format!(
                "failed to load PDF for barcode refresh '{}': {error}",
                media.file_path.display()
            )
        })?;
    let page = document
        .pages()
        .get(i32::try_from(page_number.saturating_sub(1)).unwrap_or(i32::MAX))
        .map_err(|error| {
            format!(
                "failed to load PDF page {page_number} for barcode refresh '{}': {error}",
                media.file_path.display()
            )
        })?;

    let rendered = page
        .render_with_config(
            &PdfRenderConfig::new()
                .set_target_width(2400)
                .set_maximum_height(3200),
        )
        .map_err(|error| {
            format!(
                "failed to render PDF page {page_number} for barcode refresh '{}': {error}",
                media.file_path.display()
            )
        })?
        .as_image()
        .map_err(|error| {
            format!(
                "failed to convert PDF barcode render to image '{}': {error}",
                media.file_path.display()
            )
        })?
        .into_rgb8();

    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(rendered)
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|error| {
            format!(
                "failed to encode rendered PDF barcode candidate '{}': {error}",
                media.file_path.display()
            )
        })?;
    Ok(output.into_inner())
}

fn decode_ean13_isbn(image_bytes: &[u8]) -> Option<String> {
    let mut hints = DecodeHints {
        TryHarder: Some(true),
        AlsoInverted: Some(true),
        ..Default::default()
    };

    let result = rxing_helpers::detect_in_buffer_with_hints(
        image_bytes,
        Some(BarcodeFormat::EAN_13),
        &mut hints,
    )
    .ok()?;
    normalize_isbn13(result.getText())
}

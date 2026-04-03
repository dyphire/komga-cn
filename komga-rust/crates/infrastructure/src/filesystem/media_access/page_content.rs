use std::fs;
use std::io::Read;
use std::path::Path;

use komga_application::media_assets::{
    BookMediaRecord, BookPageRecord, book_media_is_pdf, book_media_is_rar_archive,
    book_media_is_single_image, book_media_is_zip_archive, content_type_from_filename,
    is_supported_page_image_file_name,
};
use lopdf::Document as PdfDocument;
use zip::ZipArchive;

use crate::rar_support::{list_rar_entries, read_rar_entry_bytes};

pub fn resolve_book_page_bytes(
    media: &BookMediaRecord,
    page: &BookPageRecord,
    page_number: u64,
) -> Option<Vec<u8>> {
    let mut candidates = Vec::new();
    if media.file_path.is_dir() {
        candidates.push(media.file_path.join(&page.file_name));
    }
    if let Some(parent) = media.file_path.parent() {
        candidates.push(parent.join(&page.file_name));
    }
    if book_media_is_single_image(media) && page_number == 1 {
        candidates.push(media.file_path.clone());
    }
    for candidate in candidates {
        if let Ok(bytes) = fs::read(candidate) {
            return Some(bytes);
        }
    }
    read_zip_archive_page_bytes(media, page, page_number)
        .or_else(|| read_rar_archive_page_bytes(media, page, page_number))
        .or_else(|| read_pdf_page_bytes(media, page_number))
}

pub fn load_archive_page_row(media: &BookMediaRecord, page_number: u64) -> Option<BookPageRecord> {
    if page_number == 0 {
        return None;
    }
    load_archive_page_rows(media)?
        .into_iter()
        .nth(usize::try_from(page_number - 1).ok()?)
}

pub fn load_archive_page_rows(media: &BookMediaRecord) -> Option<Vec<BookPageRecord>> {
    if book_media_is_zip_archive(media) {
        return load_zip_archive_page_rows(media);
    }
    if book_media_is_rar_archive(media) {
        return load_rar_archive_page_rows(media);
    }
    None
}

pub fn load_pdf_page_row(media: &BookMediaRecord, page_number: u64) -> Option<BookPageRecord> {
    if page_number == 0 {
        return None;
    }
    load_generated_pdf_page_rows(media)
        .into_iter()
        .nth(usize::try_from(page_number - 1).ok()?)
}

pub fn load_generated_pdf_page_rows(media: &BookMediaRecord) -> Vec<BookPageRecord> {
    if !book_media_is_pdf(media) {
        return vec![];
    }
    let page_count = if media.page_count > 0 {
        media.page_count
    } else {
        detect_pdf_page_count(media).unwrap_or(0)
    };
    if page_count == 0 {
        return vec![];
    }
    let document = PdfDocument::load(&media.file_path).ok();
    (1..=page_count)
        .map(|number| {
            let dimensions = document
                .as_ref()
                .and_then(|document| pdf_page_dimensions(document, number as u32));

            BookPageRecord {
                number,
                file_name: format!("page-{number}.pdf"),
                media_type: "application/pdf".to_string(),
                width: dimensions.map(|(width, _)| i64::from(width)),
                height: dimensions.map(|(_, height)| i64::from(height)),
                file_size: 0,
            }
        })
        .collect()
}

fn read_pdf_page_bytes(media: &BookMediaRecord, page_number: u64) -> Option<Vec<u8>> {
    if !book_media_is_pdf(media) || page_number == 0 {
        return None;
    }
    let document = PdfDocument::load(&media.file_path).ok()?;
    let pages = document.get_pages();
    let object_id = *pages.get(&(page_number as u32))?;
    document.get_page_content(object_id).ok()
}

pub fn read_pdf_page_as_single_page_pdf(
    media: &BookMediaRecord,
    page_number: u64,
) -> Option<Vec<u8>> {
    if !book_media_is_pdf(media) || page_number == 0 {
        return None;
    }
    let mut document = PdfDocument::load(&media.file_path).ok()?;
    let pages = document.get_pages();
    if !pages.contains_key(&(page_number as u32)) {
        return None;
    }
    let to_delete = pages
        .keys()
        .copied()
        .filter(|number| *number != page_number as u32)
        .collect::<Vec<_>>();
    document.delete_pages(&to_delete);
    document.prune_objects();
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).ok()?;
    Some(bytes)
}

pub fn detect_pdf_page_count(media: &BookMediaRecord) -> Option<u64> {
    if !book_media_is_pdf(media) {
        return None;
    }
    Some(PdfDocument::load(&media.file_path).ok()?.get_pages().len() as u64)
}

fn pdf_page_dimensions(document: &PdfDocument, page_number: u32) -> Option<(u32, u32)> {
    let object_id = *document.get_pages().get(&page_number)?;
    let page = document.get_dictionary(object_id).ok()?;
    let media_box = page.get(b"MediaBox").ok()?.as_array().ok()?;
    if media_box.len() != 4 {
        return None;
    }

    let left = pdf_numeric_value(&media_box[0])?;
    let bottom = pdf_numeric_value(&media_box[1])?;
    let right = pdf_numeric_value(&media_box[2])?;
    let top = pdf_numeric_value(&media_box[3])?;
    let width = (right - left).abs().round();
    let height = (top - bottom).abs().round();
    if width <= 0.0 || height <= 0.0 {
        return None;
    }

    Some((width as u32, height as u32))
}

fn pdf_numeric_value(object: &lopdf::Object) -> Option<f64> {
    match object {
        lopdf::Object::Integer(value) => Some(*value as f64),
        lopdf::Object::Real(value) => Some((*value).into()),
        _ => None,
    }
}

fn read_zip_archive_page_bytes(
    media: &BookMediaRecord,
    page: &BookPageRecord,
    page_number: u64,
) -> Option<Vec<u8>> {
    if !book_media_is_zip_archive(media) || page_number == 0 {
        return None;
    }
    let file = fs::File::open(&media.file_path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    if !page.file_name.is_empty()
        && let Ok(mut entry) = archive.by_name(&page.file_name)
        && is_supported_page_image_file_name(entry.name())
    {
        let mut bytes = Vec::new();
        if entry.read_to_end(&mut bytes).is_ok() {
            return Some(bytes);
        }
    }
    let target_index = usize::try_from(page_number.saturating_sub(1)).ok()?;
    let mut logical_index = 0usize;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).ok()?;
        if !is_supported_page_image_file_name(entry.name()) {
            continue;
        }
        if logical_index != target_index {
            logical_index += 1;
            continue;
        }
        let mut bytes = Vec::new();
        if entry.read_to_end(&mut bytes).is_ok() {
            return Some(bytes);
        }
        return None;
    }
    None
}

fn load_zip_archive_page_rows(media: &BookMediaRecord) -> Option<Vec<BookPageRecord>> {
    let file = fs::File::open(&media.file_path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let mut rows = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).ok()?;
        let file_name = entry.name().to_string();
        if !is_supported_page_image_file_name(&file_name) {
            continue;
        }
        rows.push(BookPageRecord {
            number: (rows.len() as u64) + 1,
            media_type: content_type_from_filename(&file_name, "image/jpeg"),
            file_name,
            width: None,
            height: None,
            file_size: entry.size().try_into().unwrap_or(i64::MAX),
        });
    }
    (!rows.is_empty()).then_some(rows)
}

fn load_rar_archive_page_rows(media: &BookMediaRecord) -> Option<Vec<BookPageRecord>> {
    let rows = list_rar_entries(&media.file_path)
        .ok()?
        .into_iter()
        .filter(|entry| is_supported_page_image_file_name(&entry.file_name))
        .enumerate()
        .map(|(index, entry)| BookPageRecord {
            number: (index as u64) + 1,
            file_name: entry.file_name.clone(),
            media_type: content_type_from_filename(&entry.file_name, "image/jpeg"),
            width: None,
            height: None,
            file_size: entry.unpacked_size.try_into().unwrap_or(i64::MAX),
        })
        .collect::<Vec<_>>();
    (!rows.is_empty()).then_some(rows)
}

fn read_rar_archive_page_bytes(
    media: &BookMediaRecord,
    page: &BookPageRecord,
    page_number: u64,
) -> Option<Vec<u8>> {
    if !book_media_is_rar_archive(media) || page_number == 0 {
        return None;
    }
    if !page.file_name.is_empty()
        && let Some(bytes) = read_rar_entry_bytes(&media.file_path, &page.file_name)
            .ok()
            .flatten()
    {
        return Some(bytes);
    }
    let page_index = usize::try_from(page_number.saturating_sub(1)).ok()?;
    let page_file_name = load_rar_archive_page_rows(media)?
        .into_iter()
        .nth(page_index)?
        .file_name;
    read_rar_entry_bytes(&media.file_path, &page_file_name)
        .ok()
        .flatten()
}

pub fn read_media_file_bytes(path: &Path) -> Option<Vec<u8>> {
    fs::read(path).ok()
}

pub fn read_media_file_size(path: &Path) -> Option<i64> {
    fs::metadata(path)
        .ok()
        .and_then(|value| i64::try_from(value.len()).ok())
}

use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::Path;

use image::GenericImageView;
use image::imageops::FilterType;
use komga_application::media_assets::{
    BookMediaRecord, BookPageRecord, MediaImageDimensions, book_media_is_pdf,
    book_media_is_rar_archive, book_media_is_single_image, book_media_is_zip_archive,
    content_type_from_filename, is_supported_page_image_file_name,
};
use lopdf::Document as PdfDocument;
use pdfium_render::prelude::*;
use zip::ZipArchive;

use crate::load_pdfium;
use crate::rar_support::{list_rar_entries, read_rar_entry_bytes};

pub(crate) async fn resolve_book_page_bytes(
    media: &BookMediaRecord,
    page: &BookPageRecord,
    page_number: u64,
) -> Result<Option<Vec<u8>>, String> {
    let mut candidates = Vec::new();
    let media_path_is_directory = match tokio::fs::metadata(&media.file_path).await {
        Ok(metadata) => metadata.is_dir(),
        Err(error) if error.kind() == ErrorKind::NotFound => false,
        Err(error) => {
            return Err(format!(
                "read media path metadata '{}': {error}",
                media.file_path.display()
            ));
        }
    };
    if media_path_is_directory {
        candidates.push(media.file_path.join(&page.file_name));
    }
    if let Some(parent) = media.file_path.parent() {
        candidates.push(parent.join(&page.file_name));
    }
    if book_media_is_single_image(media) && page_number == 1 {
        candidates.push(media.file_path.clone());
    }
    for candidate in candidates {
        if let Some(bytes) = read_media_file_bytes(&candidate).await? {
            return Ok(Some(bytes));
        }
    }
    if let Some(bytes) = read_zip_archive_page_bytes(media, page, page_number).await? {
        return Ok(Some(bytes));
    }
    if let Some(bytes) = read_rar_archive_page_bytes(media, page, page_number)? {
        return Ok(Some(bytes));
    }
    read_pdf_page_bytes(media, page_number)
}

pub(crate) async fn render_book_page_thumbnail(
    media: &BookMediaRecord,
    page: &BookPageRecord,
    page_number: u64,
    max_edge: u32,
) -> Result<Option<Vec<u8>>, String> {
    if book_media_is_pdf(media) {
        return render_pdf_page_thumbnail(media, page_number, max_edge);
    }

    let Some(bytes) = resolve_book_page_bytes(media, page, page_number).await? else {
        return Ok(None);
    };
    render_image_thumbnail_as_jpeg(&bytes, max_edge).map(Some)
}

pub(crate) async fn load_archive_page_row(
    media: &BookMediaRecord,
    page_number: u64,
) -> Result<Option<BookPageRecord>, String> {
    if page_number == 0 {
        return Ok(None);
    }
    Ok(load_archive_page_rows(media)
        .await?
        .unwrap_or_default()
        .into_iter()
        .nth(usize::try_from(page_number - 1).map_err(|error| {
            format!("convert archive page number {page_number} to index: {error}")
        })?))
}

pub(crate) async fn load_archive_page_rows(
    media: &BookMediaRecord,
) -> Result<Option<Vec<BookPageRecord>>, String> {
    if book_media_is_zip_archive(media) {
        return load_zip_archive_page_rows(media).await;
    }
    if book_media_is_rar_archive(media) {
        return load_rar_archive_page_rows(media);
    }
    Ok(None)
}

pub(crate) fn load_pdf_page_row(
    media: &BookMediaRecord,
    page_number: u64,
) -> Result<Option<BookPageRecord>, String> {
    if page_number == 0 {
        return Ok(None);
    }
    Ok(load_generated_pdf_page_rows(media)?.into_iter().nth(
        usize::try_from(page_number - 1)
            .map_err(|error| format!("convert pdf page number {page_number} to index: {error}"))?,
    ))
}

pub(crate) fn load_generated_pdf_page_rows(
    media: &BookMediaRecord,
) -> Result<Vec<BookPageRecord>, String> {
    if !book_media_is_pdf(media) {
        return Ok(vec![]);
    }
    let page_count = if media.page_count > 0 {
        media.page_count
    } else {
        detect_pdf_page_count(media)?.unwrap_or(0)
    };
    if page_count == 0 {
        return Ok(vec![]);
    }
    let document = PdfDocument::load(&media.file_path)
        .map_err(|error| format!("open pdf '{}': {error}", media.file_path.display()))?;
    Ok((1..=page_count)
        .map(|number| {
            let dimensions = document
                .get_pages()
                .get(&(number as u32))
                .and_then(|_| pdf_page_dimensions(&document, number as u32));
            let scaled_dimensions = dimensions.map(scale_pdf_page_dimensions);

            BookPageRecord {
                number,
                file_name: number.to_string(),
                media_type: "image/jpeg".to_string(),
                width: scaled_dimensions.map(|dimensions| i64::from(dimensions.width)),
                height: scaled_dimensions.map(|dimensions| i64::from(dimensions.height)),
                file_size: -1,
            }
        })
        .collect())
}

fn read_pdf_page_bytes(
    media: &BookMediaRecord,
    page_number: u64,
) -> Result<Option<Vec<u8>>, String> {
    if !book_media_is_pdf(media) || page_number == 0 {
        return Ok(None);
    }
    let document = PdfDocument::load(&media.file_path)
        .map_err(|error| format!("open pdf '{}': {error}", media.file_path.display()))?;
    let pages = document.get_pages();
    let Some(object_id) = pages.get(&(page_number as u32)).copied() else {
        return Ok(None);
    };
    Ok(Some(document.get_page_content(object_id)))
}

pub(crate) fn read_pdf_page_as_single_page_pdf(
    media: &BookMediaRecord,
    page_number: u64,
) -> Result<Option<Vec<u8>>, String> {
    if !book_media_is_pdf(media) || page_number == 0 {
        return Ok(None);
    }
    let mut document = PdfDocument::load(&media.file_path)
        .map_err(|error| format!("open pdf '{}': {error}", media.file_path.display()))?;
    let pages = document.get_pages();
    if !pages.contains_key(&(page_number as u32)) {
        return Ok(None);
    }
    let to_delete = pages
        .keys()
        .copied()
        .filter(|number| *number != page_number as u32)
        .collect::<Vec<_>>();
    document.delete_pages(&to_delete);
    document.prune_objects();
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).map_err(|error| {
        format!(
            "save pdf page {page_number} from '{}': {error}",
            media.file_path.display()
        )
    })?;
    Ok(Some(bytes))
}

fn render_pdf_page_thumbnail(
    media: &BookMediaRecord,
    page_number: u64,
    max_edge: u32,
) -> Result<Option<Vec<u8>>, String> {
    if !book_media_is_pdf(media) || page_number == 0 {
        return Ok(None);
    }

    let pdfium = load_pdfium()?;
    let document = pdfium
        .load_pdf_from_file(&media.file_path, None)
        .map_err(|error| format!("open pdf '{}': {error}", media.file_path.display()))?;
    let page = document
        .pages()
        .get(
            i32::try_from(page_number.saturating_sub(1))
                .map_err(|error| format!("convert pdf page number {page_number}: {error}"))?,
        )
        .map_err(|error| {
            format!(
                "load pdf page {page_number} from '{}': {error}",
                media.file_path.display()
            )
        })?;
    let rendered = page
        .render_with_config(
            &PdfRenderConfig::new()
                .set_target_width(i32::try_from(max_edge).unwrap_or(i32::MAX))
                .set_maximum_height(i32::try_from(max_edge).unwrap_or(i32::MAX)),
        )
        .map_err(|error| {
            format!(
                "render pdf page {page_number} from '{}': {error}",
                media.file_path.display()
            )
        })?
        .as_image()
        .map_err(|error| {
            format!(
                "convert pdf page {page_number} from '{}' to image: {error}",
                media.file_path.display()
            )
        })?
        .into_rgb8();

    let mut output = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(rendered)
        .write_to(&mut output, image::ImageFormat::Jpeg)
        .map_err(|error| {
            format!(
                "encode pdf page {page_number} thumbnail from '{}': {error}",
                media.file_path.display()
            )
        })?;
    Ok(Some(output.into_inner()))
}

fn render_image_thumbnail_as_jpeg(bytes: &[u8], max_edge: u32) -> Result<Vec<u8>, String> {
    let image = image::load_from_memory(bytes)
        .map_err(|error| format!("render image thumbnail: decode image bytes: {error}"))?;
    let dimensions = RasterImageDimensions::from_image(&image);
    let resized = if dimensions.max_edge() > max_edge {
        image.resize(max_edge, max_edge, FilterType::Lanczos3)
    } else {
        image
    };
    let mut output = std::io::Cursor::new(Vec::new());
    resized
        .write_to(&mut output, image::ImageFormat::Jpeg)
        .map_err(|error| format!("render image thumbnail: encode jpeg: {error}"))?;
    Ok(output.into_inner())
}

pub(crate) fn detect_pdf_page_count(media: &BookMediaRecord) -> Result<Option<u64>, String> {
    if !book_media_is_pdf(media) {
        return Ok(None);
    }
    let document = PdfDocument::load(&media.file_path)
        .map_err(|error| format!("open pdf '{}': {error}", media.file_path.display()))?;
    Ok(Some(document.get_pages().len() as u64))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RasterImageDimensions {
    width: u32,
    height: u32,
}

impl RasterImageDimensions {
    fn from_image(image: &image::DynamicImage) -> Self {
        let dimensions = image.dimensions();
        Self {
            width: dimensions.0,
            height: dimensions.1,
        }
    }

    fn max_edge(self) -> u32 {
        self.width.max(self.height)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PdfPageDimensions {
    width: u32,
    height: u32,
}

fn pdf_page_dimensions(document: &PdfDocument, page_number: u32) -> Option<PdfPageDimensions> {
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

    Some(PdfPageDimensions {
        width: width as u32,
        height: height as u32,
    })
}

fn scale_pdf_page_dimensions(dimensions: PdfPageDimensions) -> PdfPageDimensions {
    const PDF_RESOLUTION: f64 = 3200.0;

    let min_edge = f64::from(dimensions.width.min(dimensions.height));
    if min_edge <= 0.0 {
        return dimensions;
    }

    let scale = PDF_RESOLUTION / min_edge;
    PdfPageDimensions {
        width: (f64::from(dimensions.width) * scale).round().max(1.0) as u32,
        height: (f64::from(dimensions.height) * scale).round().max(1.0) as u32,
    }
}

fn pdf_numeric_value(object: &lopdf::Object) -> Option<f64> {
    match object {
        lopdf::Object::Integer(value) => Some(*value as f64),
        lopdf::Object::Real(value) => Some((*value).into()),
        _ => None,
    }
}

async fn read_zip_archive_page_bytes(
    media: &BookMediaRecord,
    page: &BookPageRecord,
    page_number: u64,
) -> Result<Option<Vec<u8>>, String> {
    if !book_media_is_zip_archive(media) || page_number == 0 {
        return Ok(None);
    }
    let path = media.file_path.clone();
    let page_file_name = page.file_name.clone();
    tokio::task::spawn_blocking(move || -> Result<Option<Vec<u8>>, String> {
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(format!("open zip archive '{}': {error}", path.display()));
            }
        };
        let mut archive = ZipArchive::new(file)
            .map_err(|error| format!("read zip archive '{}': {error}", path.display()))?;
        if !page_file_name.is_empty()
            && let Ok(mut entry) = archive.by_name(&page_file_name)
            && let Ok(entry_name) = entry.name()
            && is_supported_page_image_file_name(entry_name.as_ref())
        {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(|error| {
                format!(
                    "read zip archive entry '{}' from '{}': {error}",
                    page_file_name,
                    path.display()
                )
            })?;
            return Ok(Some(bytes));
        }
        let target_index = usize::try_from(page_number.saturating_sub(1))
            .map_err(|error| format!("convert zip page number {page_number}: {error}"))?;
        let mut logical_index = 0usize;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(|error| {
                format!(
                    "read zip archive entry #{index} from '{}': {error}",
                    path.display()
                )
            })?;
            let entry_name = entry
                .name()
                .map_err(|error| {
                    format!(
                        "read zip archive entry #{index} name from '{}': {error}",
                        path.display()
                    )
                })?
                .into_owned();
            if !is_supported_page_image_file_name(&entry_name) {
                continue;
            }
            if logical_index != target_index {
                logical_index += 1;
                continue;
            }
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(|error| {
                format!(
                    "read zip archive entry '{}' from '{}': {error}",
                    entry_name,
                    path.display()
                )
            })?;
            return Ok(Some(bytes));
        }
        Ok(None)
    })
    .await
    .map_err(|error| format!("join zip archive page read task: {error}"))?
}

async fn load_zip_archive_page_rows(
    media: &BookMediaRecord,
) -> Result<Option<Vec<BookPageRecord>>, String> {
    let path = media.file_path.clone();
    tokio::task::spawn_blocking(move || -> Result<Option<Vec<BookPageRecord>>, String> {
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(format!("open zip archive '{}': {error}", path.display()));
            }
        };
        let mut archive = ZipArchive::new(file)
            .map_err(|error| format!("read zip archive '{}': {error}", path.display()))?;
        let mut rows = Vec::new();
        for index in 0..archive.len() {
            let entry = archive.by_index(index).map_err(|error| {
                format!(
                    "read zip archive entry #{index} from '{}': {error}",
                    path.display()
                )
            })?;
            let file_name = entry
                .name()
                .map_err(|error| {
                    format!(
                        "read zip archive entry #{index} name from '{}': {error}",
                        path.display()
                    )
                })?
                .into_owned();
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
        Ok((!rows.is_empty()).then_some(rows))
    })
    .await
    .map_err(|error| format!("join zip archive row read task: {error}"))?
}

fn load_rar_archive_page_rows(
    media: &BookMediaRecord,
) -> Result<Option<Vec<BookPageRecord>>, String> {
    let rows = list_rar_entries(&media.file_path)
        .map_err(|error| format!("read rar archive '{}': {error}", media.file_path.display()))?
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
    Ok((!rows.is_empty()).then_some(rows))
}

fn read_rar_archive_page_bytes(
    media: &BookMediaRecord,
    page: &BookPageRecord,
    page_number: u64,
) -> Result<Option<Vec<u8>>, String> {
    if !book_media_is_rar_archive(media) || page_number == 0 {
        return Ok(None);
    }
    if !page.file_name.is_empty()
        && let Some(bytes) = read_rar_entry_bytes(&media.file_path, &page.file_name)?
    {
        return Ok(Some(bytes));
    }
    let page_index = usize::try_from(page_number.saturating_sub(1))
        .map_err(|error| format!("convert rar page number {page_number}: {error}"))?;
    let Some(page_file_name) = load_rar_archive_page_rows(media)?
        .unwrap_or_default()
        .into_iter()
        .nth(page_index)
        .map(|row| row.file_name)
    else {
        return Ok(None);
    };
    read_rar_entry_bytes(&media.file_path, &page_file_name)
}

pub(crate) async fn read_media_file_bytes(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match tokio::fs::read(path).await {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("read media file '{}': {error}", path.display())),
    }
}

pub(crate) async fn read_media_file_size(path: &Path) -> Result<Option<i64>, String> {
    match tokio::fs::metadata(path).await {
        Ok(value) if value.is_file() => i64::try_from(value.len())
            .map(Some)
            .map_err(|error| format!("convert media file size '{}': {error}", path.display())),
        Ok(_) => Err(format!("media path '{}' is not a file", path.display())),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "read media file metadata '{}': {error}",
            path.display()
        )),
    }
}

pub(crate) async fn read_media_image_dimensions(
    path: &Path,
) -> Result<Option<MediaImageDimensions>, String> {
    let Some(bytes) = read_media_file_bytes(path).await? else {
        return Ok(None);
    };
    let image = image::load_from_memory(&bytes).map_err(|error| {
        format!(
            "decode media image dimensions '{}': {error}",
            path.display()
        )
    })?;
    Ok(Some(MediaImageDimensions {
        width: i64::from(image.width()),
        height: i64::from(image.height()),
    }))
}

pub(crate) fn convert_image_bytes(
    bytes: &[u8],
    source_content_type: &str,
    target_content_type: &str,
) -> Result<Option<Vec<u8>>, String> {
    if source_content_type.eq_ignore_ascii_case(target_content_type) {
        return Ok(Some(bytes.to_vec()));
    }

    if !source_content_type
        .to_ascii_lowercase()
        .starts_with("image/")
    {
        return Ok(None);
    }

    let source = image::load_from_memory(bytes)
        .map_err(|error| format!("convert image bytes: decode image bytes: {error}"))?;
    let target_format = match target_content_type {
        "image/jpeg" => image::ImageFormat::Jpeg,
        "image/png" => image::ImageFormat::Png,
        _ => return Ok(None),
    };
    let mut output = std::io::Cursor::new(Vec::new());
    source
        .write_to(&mut output, target_format)
        .map_err(|error| format!("convert image bytes: encode image bytes: {error}"))?;
    Ok(Some(output.into_inner()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use komga_application::media_assets::{BookMediaRecord, BookPageRecord};
    use lopdf::{Document as PdfDocument, Object, Stream, dictionary};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    use super::{
        convert_image_bytes, load_archive_page_rows, load_generated_pdf_page_rows,
        read_media_file_bytes, read_media_file_size, read_media_image_dimensions,
        read_pdf_page_as_single_page_pdf, render_book_page_thumbnail, resolve_book_page_bytes,
    };

    fn unique_temp_path(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
    }

    fn build_test_zip_archive(entries: Vec<(String, Vec<u8>)>) -> Result<Vec<u8>, String> {
        let cursor = std::io::Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(cursor);

        for (file_name, bytes) in entries {
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            writer
                .start_file(file_name.as_str(), options)
                .map_err(|error| format!("start zip entry '{file_name}': {error}"))?;
            writer
                .write_all(&bytes)
                .map_err(|error| format!("write zip entry '{file_name}': {error}"))?;
        }

        writer
            .finish()
            .map(|cursor| cursor.into_inner())
            .map_err(|error| format!("finalize zip archive: {error}"))
    }

    fn write_single_page_pdf(path: &Path) {
        let mut document = PdfDocument::with_version("1.5");
        let pages_id = document.new_object_id();
        let page_id = document.new_object_id();
        let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
        let resources_id = document.add_object(dictionary! {});

        document.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
                "Contents" => content_id,
                "Resources" => resources_id,
            }),
        );
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );

        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        document.compress();
        document
            .save(path)
            .expect("single-page pdf should be saved");
    }

    #[tokio::test]
    async fn resolve_book_page_bytes_does_not_use_whole_archive_for_non_image() {
        let file_path = unique_temp_path("komga-media-archive");
        let archive = build_test_zip_archive(vec![("meta.txt".to_string(), b"meta".to_vec())])
            .expect("zip payload should be created");
        fs::write(&file_path, archive).expect("archive test file should be written");

        let media = BookMediaRecord {
            library_id: "lib".to_string(),
            file_name: "book.cbz".to_string(),
            file_path: file_path.clone(),
            media_type: "application/vnd.comicbook+zip".to_string(),
            page_count: 12,
        };
        let page = BookPageRecord {
            number: 5,
            file_name: "page-005.jpg".to_string(),
            media_type: "image/jpeg".to_string(),
            width: None,
            height: None,
            file_size: 0,
        };

        let bytes = resolve_book_page_bytes(&media, &page, 5)
            .await
            .expect("non-image archive content should not fail");
        assert!(bytes.is_none());

        let _ = fs::remove_file(file_path);
    }

    #[tokio::test]
    async fn resolve_book_page_bytes_allows_single_image_first_page() {
        let file_path = unique_temp_path("komga-media-image");
        fs::write(&file_path, b"image-bytes").expect("image test file should be written");

        let media = BookMediaRecord {
            library_id: "lib".to_string(),
            file_name: "cover.jpg".to_string(),
            file_path: file_path.clone(),
            media_type: "image/jpeg".to_string(),
            page_count: 1,
        };
        let page = BookPageRecord {
            number: 1,
            file_name: "missing-derived-page.jpg".to_string(),
            media_type: "image/jpeg".to_string(),
            width: None,
            height: None,
            file_size: 0,
        };

        let bytes = resolve_book_page_bytes(&media, &page, 1)
            .await
            .expect("single image page should read");
        assert_eq!(bytes, Some(b"image-bytes".to_vec()));

        let _ = fs::remove_file(file_path);
    }

    #[tokio::test]
    async fn render_book_page_thumbnail_propagates_image_decode_errors() {
        let file_path = unique_temp_path("komga-media-invalid-image-thumbnail");
        fs::write(&file_path, b"not-an-image").expect("invalid image fixture should be written");

        let media = BookMediaRecord {
            library_id: "lib".to_string(),
            file_name: "cover.jpg".to_string(),
            file_path: file_path.clone(),
            media_type: "image/jpeg".to_string(),
            page_count: 1,
        };
        let page = BookPageRecord {
            number: 1,
            file_name: "cover.jpg".to_string(),
            media_type: "image/jpeg".to_string(),
            width: None,
            height: None,
            file_size: 0,
        };

        let error = render_book_page_thumbnail(&media, &page, 1, 300)
            .await
            .expect_err("invalid image bytes must not become a missing thumbnail");

        assert!(
            error.contains("render image thumbnail"),
            "unexpected thumbnail render error: {error}"
        );

        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn convert_image_bytes_propagates_image_decode_errors() {
        let error = convert_image_bytes(b"not-an-image", "image/png", "image/jpeg")
            .expect_err("invalid image bytes must not become a missing converted page");

        assert!(
            error.contains("convert image bytes"),
            "unexpected conversion error: {error}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn resolve_book_page_bytes_propagates_media_path_probe_errors() {
        let root = unique_temp_path("komga-media-symlink-loop");
        fs::create_dir(&root).expect("media symlink fixture root should be created");
        let media_path = root.join("loop");
        std::os::unix::fs::symlink(&media_path, &media_path)
            .expect("media path symlink loop should be created");

        let media = BookMediaRecord {
            library_id: "lib".to_string(),
            file_name: "book".to_string(),
            file_path: media_path.clone(),
            media_type: "application/octet-stream".to_string(),
            page_count: 1,
        };
        let page = BookPageRecord {
            number: 1,
            file_name: "page.jpg".to_string(),
            media_type: "image/jpeg".to_string(),
            width: None,
            height: None,
            file_size: 0,
        };

        let error = resolve_book_page_bytes(&media, &page, 1)
            .await
            .expect_err("media path probe errors must not become missing page bytes");

        assert!(
            error.contains("read media path metadata"),
            "unexpected media path probe error: {error}"
        );

        let _ = fs::remove_file(media_path);
        let _ = fs::remove_dir(root);
    }

    #[tokio::test]
    async fn read_media_image_dimensions_reports_file_read_errors() {
        let path = unique_temp_path("komga-media-image-directory");
        fs::create_dir(&path).expect("image directory fixture should be created");

        let error = read_media_image_dimensions(&path)
            .await
            .expect_err("directory media path should fail image byte loading");

        assert!(
            error.contains("read media file"),
            "unexpected error: {error}"
        );
        let _ = fs::remove_dir(path);
    }

    #[tokio::test]
    async fn read_media_file_size_reports_non_file_paths() {
        let path = unique_temp_path("komga-media-size-directory");
        fs::create_dir(&path).expect("size directory fixture should be created");

        let error = read_media_file_size(&path)
            .await
            .expect_err("directory media path should fail size loading");

        assert!(error.contains("not a file"), "unexpected error: {error}");
        let _ = fs::remove_dir(path);
    }

    #[tokio::test]
    async fn read_media_image_dimensions_propagates_image_decode_errors() {
        let path = unique_temp_path("komga-media-invalid-image");
        fs::write(&path, b"not an image").expect("invalid image fixture should be written");

        let error = read_media_image_dimensions(&path)
            .await
            .expect_err("invalid image bytes should fail dimension loading");

        assert!(
            error.contains("decode media image dimensions"),
            "unexpected error: {error}"
        );
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn load_archive_page_rows_uses_zip_image_entries_only() {
        let file_path = unique_temp_path("komga-media-zip-rows");
        let archive = build_test_zip_archive(vec![
            ("001.jpg".to_string(), b"page-1".to_vec()),
            ("meta.txt".to_string(), b"meta".to_vec()),
            ("002.png".to_string(), b"page-2".to_vec()),
        ])
        .expect("zip payload should be created");
        fs::write(&file_path, archive).expect("zip test file should be written");

        let media = BookMediaRecord {
            library_id: "lib".to_string(),
            file_name: "book.cbz".to_string(),
            file_path: file_path.clone(),
            media_type: "application/vnd.comicbook+zip".to_string(),
            page_count: 2,
        };

        let rows = load_archive_page_rows(&media)
            .await
            .expect("archive row load should not fail")
            .expect("archive rows should be parsed");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].number, 1);
        assert_eq!(rows[0].file_name, "001.jpg");
        assert_eq!(rows[1].number, 2);
        assert_eq!(rows[1].file_name, "002.png");

        let _ = fs::remove_file(file_path);
    }

    #[tokio::test]
    async fn resolve_book_page_bytes_extracts_zip_page_by_logical_index() {
        let file_path = unique_temp_path("komga-media-zip-by-index");
        let archive = build_test_zip_archive(vec![
            ("001.jpg".to_string(), b"page-1".to_vec()),
            ("meta.txt".to_string(), b"meta".to_vec()),
            ("002.png".to_string(), b"page-2".to_vec()),
        ])
        .expect("zip payload should be created");
        fs::write(&file_path, archive).expect("zip test file should be written");

        let media = BookMediaRecord {
            library_id: "lib".to_string(),
            file_name: "book.cbz".to_string(),
            file_path: file_path.clone(),
            media_type: "application/vnd.comicbook+zip".to_string(),
            page_count: 2,
        };
        let page = BookPageRecord {
            number: 2,
            file_name: "not-present.jpg".to_string(),
            media_type: "image/jpeg".to_string(),
            width: None,
            height: None,
            file_size: 0,
        };

        let bytes = resolve_book_page_bytes(&media, &page, 2)
            .await
            .expect("zip page bytes should read");
        assert_eq!(bytes, Some(b"page-2".to_vec()));

        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn generated_pdf_rows_use_detected_page_count_when_media_count_missing() {
        let file_path = unique_temp_path("komga-media-pdf-archive");
        write_single_page_pdf(&file_path);

        let media = BookMediaRecord {
            library_id: "lib".to_string(),
            file_name: "book.pdf".to_string(),
            file_path: file_path.clone(),
            media_type: "application/pdf".to_string(),
            page_count: 0,
        };

        let rows = load_generated_pdf_page_rows(&media).expect("generated PDF rows should load");
        assert_eq!(rows.len(), 1);
        let bytes = read_pdf_page_as_single_page_pdf(&media, 1)
            .expect("single pdf page extraction should not fail");
        assert!(bytes.is_some());

        let _ = fs::remove_file(file_path);
    }

    #[tokio::test]
    async fn resolve_book_page_bytes_propagates_invalid_zip_errors() {
        let file_path = unique_temp_path("komga-media-invalid-zip");
        fs::write(&file_path, b"not-a-zip").expect("invalid zip test file should be written");

        let media = BookMediaRecord {
            library_id: "lib".to_string(),
            file_name: "book.cbz".to_string(),
            file_path: file_path.clone(),
            media_type: "application/vnd.comicbook+zip".to_string(),
            page_count: 1,
        };
        let page = BookPageRecord {
            number: 1,
            file_name: "001.jpg".to_string(),
            media_type: "image/jpeg".to_string(),
            width: None,
            height: None,
            file_size: 0,
        };

        let error = resolve_book_page_bytes(&media, &page, 1)
            .await
            .expect_err("invalid zip must not become missing page bytes");

        assert!(
            error.contains("read zip archive"),
            "unexpected zip page error: {error}"
        );

        let _ = fs::remove_file(file_path);
    }

    #[tokio::test]
    async fn load_archive_page_rows_propagates_invalid_zip_errors() {
        let file_path = unique_temp_path("komga-media-invalid-zip-rows");
        fs::write(&file_path, b"not-a-zip").expect("invalid zip row file should be written");

        let media = BookMediaRecord {
            library_id: "lib".to_string(),
            file_name: "book.cbz".to_string(),
            file_path: file_path.clone(),
            media_type: "application/vnd.comicbook+zip".to_string(),
            page_count: 1,
        };

        let error = load_archive_page_rows(&media)
            .await
            .expect_err("invalid zip must not become missing archive rows");

        assert!(
            error.contains("read zip archive"),
            "unexpected zip row error: {error}"
        );

        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn read_pdf_page_as_single_page_pdf_propagates_pdf_load_errors() {
        let file_path = unique_temp_path("komga-media-invalid-pdf");
        fs::write(&file_path, b"not-a-pdf").expect("invalid pdf file should be written");

        let media = BookMediaRecord {
            library_id: "lib".to_string(),
            file_name: "book.pdf".to_string(),
            file_path: file_path.clone(),
            media_type: "application/pdf".to_string(),
            page_count: 1,
        };

        let error = read_pdf_page_as_single_page_pdf(&media, 1)
            .expect_err("invalid pdf must not become missing raw page");

        assert!(
            error.contains("open pdf"),
            "unexpected raw pdf page error: {error}"
        );

        let _ = fs::remove_file(file_path);
    }

    #[tokio::test]
    async fn resolve_book_page_bytes_propagates_invalid_pdf_errors() {
        let file_path = unique_temp_path("komga-media-invalid-pdf-page-bytes");
        fs::write(&file_path, b"not-a-pdf").expect("invalid pdf file should be written");

        let media = BookMediaRecord {
            library_id: "lib".to_string(),
            file_name: "book.pdf".to_string(),
            file_path: file_path.clone(),
            media_type: "application/pdf".to_string(),
            page_count: 1,
        };
        let page = BookPageRecord {
            number: 1,
            file_name: "1".to_string(),
            media_type: "image/jpeg".to_string(),
            width: None,
            height: None,
            file_size: -1,
        };

        let error = resolve_book_page_bytes(&media, &page, 1)
            .await
            .expect_err("invalid pdf must not become missing page bytes");

        assert!(
            error.contains("open pdf"),
            "unexpected pdf page bytes error: {error}"
        );

        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn generated_pdf_rows_propagates_invalid_pdf_errors() {
        let file_path = unique_temp_path("komga-media-invalid-pdf-rows");
        fs::write(&file_path, b"not-a-pdf").expect("invalid pdf file should be written");

        let media = BookMediaRecord {
            library_id: "lib".to_string(),
            file_name: "book.pdf".to_string(),
            file_path: file_path.clone(),
            media_type: "application/pdf".to_string(),
            page_count: 0,
        };

        let error = load_generated_pdf_page_rows(&media)
            .expect_err("invalid pdf must not become empty generated page rows");

        assert!(
            error.contains("open pdf"),
            "unexpected generated pdf row error: {error}"
        );

        let _ = fs::remove_file(file_path);
    }

    #[tokio::test]
    async fn read_media_file_bytes_distinguishes_read_errors_from_missing_files() {
        let missing_path = unique_temp_path("komga-media-missing-file");
        let missing = read_media_file_bytes(&missing_path)
            .await
            .expect("missing file should not be an internal read error");
        assert!(missing.is_none());

        let directory_path = unique_temp_path("komga-media-file-read-error");
        fs::create_dir(&directory_path).expect("read error fixture should be a directory");
        let error = read_media_file_bytes(&directory_path)
            .await
            .expect_err("directory read error must not become a missing file");

        assert!(
            error.contains("read media file"),
            "unexpected media file read error: {error}"
        );

        let _ = fs::remove_dir(directory_path);
    }
}

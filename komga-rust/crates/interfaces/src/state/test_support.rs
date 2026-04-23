#[cfg(test)]
pub(crate) use super::identity::seed_koreader_book_target;
#[cfg(test)]
pub(crate) use super::tests::{
    NoopDiscoveryDetailService, NoopMediaAssetsService, NoopOpdsCatalogService,
    NoopOpdsPersistedService, NoopOperationalRuntimeService, NoopOperationalSettingsService,
    NoopPersistedDiscoveryService,
};

#[cfg(test)]
pub(crate) mod media_assets {
    use std::fs;
    use std::io::Read;

    use komga_application::media_assets::{BookMediaRecord, BookPageRecord};
    use lopdf::Document as PdfDocument;
    use zip::ZipArchive;

    fn content_type_from_filename(file_name: &str, default_mime_type: &str) -> String {
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

    fn book_media_supports_page_image(media: &BookMediaRecord) -> bool {
        content_type_from_filename(&media.file_name, &media.media_type).starts_with("image/")
    }

    fn book_media_is_single_image(media: &BookMediaRecord) -> bool {
        book_media_supports_page_image(media)
    }

    fn book_media_is_zip_archive(media: &BookMediaRecord) -> bool {
        matches!(
            content_type_from_filename(&media.file_name, &media.media_type).as_str(),
            "application/vnd.comicbook+zip" | "application/epub+zip" | "application/zip"
        )
    }

    fn book_media_is_pdf(media: &BookMediaRecord) -> bool {
        content_type_from_filename(&media.file_name, &media.media_type) == "application/pdf"
    }

    fn is_supported_page_image_file_name(file_name: &str) -> bool {
        content_type_from_filename(file_name, "application/octet-stream").starts_with("image/")
    }

    pub(crate) async fn resolve_book_page_bytes(
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
            if let Ok(bytes) = tokio::fs::read(candidate).await {
                return Some(bytes);
            }
        }
        read_zip_archive_page_bytes(media, page, page_number)
            .await
            .or_else(|| {
                if book_media_is_single_image(media) && page_number == 1 {
                    fs::read(&media.file_path).ok()
                } else {
                    None
                }
            })
    }

    pub(crate) async fn load_archive_page_rows(
        media: &BookMediaRecord,
    ) -> Option<Vec<BookPageRecord>> {
        if !book_media_is_zip_archive(media) {
            return None;
        }

        let file = tokio::fs::File::open(&media.file_path)
            .await
            .ok()?
            .into_std()
            .await;
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

    async fn read_zip_archive_page_bytes(
        media: &BookMediaRecord,
        page: &BookPageRecord,
        page_number: u64,
    ) -> Option<Vec<u8>> {
        if !book_media_is_zip_archive(media) || page_number == 0 {
            return None;
        }
        let file = tokio::fs::File::open(&media.file_path)
            .await
            .ok()?
            .into_std()
            .await;
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

    pub(crate) fn load_generated_pdf_page_rows(media: &BookMediaRecord) -> Vec<BookPageRecord> {
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
            .map(|number| BookPageRecord {
                number,
                file_name: format!("page-{number}.pdf"),
                media_type: "application/pdf".to_string(),
                width: document
                    .as_ref()
                    .and_then(|document| pdf_page_dimensions(document, number as u32))
                    .map(|(width, _)| i64::from(width)),
                height: document
                    .as_ref()
                    .and_then(|document| pdf_page_dimensions(document, number as u32))
                    .map(|(_, height)| i64::from(height)),
                file_size: 0,
            })
            .collect()
    }

    pub(crate) fn read_pdf_page_as_single_page_pdf(
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

    fn detect_pdf_page_count(media: &BookMediaRecord) -> Option<u64> {
        if !book_media_is_pdf(media) {
            return None;
        }
        Some(PdfDocument::load(&media.file_path).ok()?.get_pages().len() as u64)
    }
}

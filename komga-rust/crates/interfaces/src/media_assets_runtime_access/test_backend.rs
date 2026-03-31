use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek};
use std::sync::Arc;

use flate2::read::GzDecoder;
use komga_application::media_assets::{
    BookMediaRecord, BookMetadataPatch, BookPageRecord, BooksImportPayload,
};
use komga_application::task_processing::TaskQueueRecord;
use lopdf::Document as PdfDocument;
use quick_xml::Reader as XmlReader;
use quick_xml::events::Event as XmlEvent;
use serde_json::{Value, json};
use zip::ZipArchive;

use super::backend::{
    MediaAssetsRuntimeAccessBackend, RuntimeBookMetadataService, RuntimeMediaImportService,
};

struct DefaultRuntimeMediaImportService;

impl RuntimeMediaImportService for DefaultRuntimeMediaImportService {
    fn enqueue_books(
        &self,
        _payload: BooksImportPayload,
        _next_task_id: &mut dyn FnMut() -> String,
    ) -> Result<Vec<TaskQueueRecord>, String> {
        Ok(vec![])
    }

    fn process_queued_books_payload<'a>(
        &'a self,
        _task_payload: &'a str,
    ) -> futures_util::future::BoxFuture<'a, Result<Vec<TaskQueueRecord>, String>> {
        Box::pin(async { Ok(vec![]) })
    }

    fn process_queued_book_payload<'a>(
        &'a self,
        _task_payload: &'a str,
    ) -> futures_util::future::BoxFuture<'a, Result<Vec<TaskQueueRecord>, String>> {
        Box::pin(async { Ok(vec![]) })
    }
}

struct DefaultRuntimeBookMetadataService;

impl RuntimeBookMetadataService for DefaultRuntimeBookMetadataService {
    fn update_book_metadata<'a>(
        &'a self,
        _book_id: &'a str,
        _patch: &'a BookMetadataPatch,
    ) -> futures_util::future::BoxFuture<'a, Result<Option<Option<String>>, String>> {
        Box::pin(async { Ok(None) })
    }

    fn batch_update_book_metadata<'a>(
        &'a self,
        _updates: Vec<(String, BookMetadataPatch)>,
    ) -> futures_util::future::BoxFuture<'a, Result<Vec<String>, String>> {
        Box::pin(async { Ok(vec![]) })
    }
}

pub(super) fn default_test_backend() -> MediaAssetsRuntimeAccessBackend {
    MediaAssetsRuntimeAccessBackend {
        media_import_service: Arc::new(|_| Box::new(DefaultRuntimeMediaImportService)),
        book_metadata_service: Arc::new(|_| Box::new(DefaultRuntimeBookMetadataService)),
        persist_book_page_hashes_with_media_content: Arc::new(|_, _| Box::pin(async { Ok(()) })),
        decode_epub_positions: Arc::new(|blob| decode_epub_positions_for_tests(&blob)),
        load_epub_archive_positions: Arc::new(|media| {
            load_epub_archive_positions_for_tests(&media)
        }),
        read_media_file_bytes: Arc::new(|path| fs::read(path).ok()),
        read_media_file_size: Arc::new(|path| {
            fs::metadata(path)
                .ok()
                .map(|metadata| metadata.len().min(i64::MAX as u64) as i64)
        }),
        load_persisted_book_media: Arc::new(|_, _| Box::pin(async { Ok(None) })),
        book_media_is_ready_status: Arc::new(|_, _| Box::pin(async { Ok(false) })),
        load_persisted_series_thumbnail_media: Arc::new(|_, _| Box::pin(async { Ok(None) })),
        load_persisted_book_pages: Arc::new(|_, _| Box::pin(async { Ok(vec![]) })),
        load_persisted_book_page_row: Arc::new(|_, _, _| Box::pin(async { Ok(None) })),
        resolve_book_page_bytes: Arc::new(|media, page, page_number| {
            resolve_book_page_bytes_for_tests(&media, &page, page_number)
        }),
        load_archive_page_row: Arc::new(|media, page_number| {
            load_archive_page_row_for_tests(&media, page_number)
        }),
        load_archive_page_rows: Arc::new(|media| load_archive_page_rows_for_tests(&media)),
        load_pdf_page_row: Arc::new(|media, page_number| {
            load_pdf_page_row_for_tests(&media, page_number)
        }),
        load_generated_pdf_page_rows: Arc::new(|media| {
            load_generated_pdf_page_rows_for_tests(&media)
        }),
        read_pdf_page_as_single_page_pdf: Arc::new(|media, page_number| {
            read_pdf_page_as_single_page_pdf_for_tests(&media, page_number)
        }),
        detect_pdf_page_count: Arc::new(|media| detect_pdf_page_count_for_tests(&media)),
        load_persisted_epub_extension_blob: Arc::new(|_, _| Box::pin(async { Ok(None) })),
        load_series_book_ids: Arc::new(|_, _| Box::pin(async { Ok(vec![]) })),
        refresh_series_read_progress_row: Arc::new(|_, _, _| Box::pin(async { Ok(()) })),
        load_series_tachiyomi_progress: Arc::new(|_, _, _| Box::pin(async { Ok(None) })),
        load_book_progression: Arc::new(|_, _, _| Box::pin(async { Ok(None) })),
        persist_read_progress: Arc::new(|_, _, _, _, _| Box::pin(async { Ok(()) })),
        delete_persisted_read_progress: Arc::new(|_, _, _| Box::pin(async { Ok(()) })),
        readlist_tachiyomi_counters: Arc::new(|_, _, _| Box::pin(async { Ok(None) })),
        persist_readlist_tachiyomi_progress: Arc::new(|_, _, _, _| Box::pin(async { Ok(None) })),
        load_selected_book_thumbnail: Arc::new(|_, _| Box::pin(async { Ok(None) })),
        load_book_thumbnail_by_id: Arc::new(|_, _, _| Box::pin(async { Ok(None) })),
        load_persisted_book_thumbnails: Arc::new(|_, _| Box::pin(async { Ok(vec![]) })),
        insert_book_thumbnail: Arc::new(|_, _, _, _, _, _, _| {
            Box::pin(async {
                Err(
                    "media assets runtime access backend is not configured for thumbnail writes"
                        .to_string(),
                )
            })
        }),
        select_book_thumbnail: Arc::new(|_, _, _| Box::pin(async { Ok(false) })),
        delete_book_thumbnail: Arc::new(|_, _, _| Box::pin(async { Ok(false) })),
        load_persisted_readlist_thumbnails: Arc::new(|_, _| Box::pin(async { Ok(vec![]) })),
        insert_readlist_thumbnail: Arc::new(|_, _, _, _, _, _, _| {
            Box::pin(async {
                Err(
                    "media assets runtime access backend is not configured for thumbnail writes"
                        .to_string(),
                )
            })
        }),
        select_readlist_thumbnail: Arc::new(|_, _, _| Box::pin(async { Ok(false) })),
        delete_readlist_thumbnail: Arc::new(|_, _, _| Box::pin(async { Ok(false) })),
        load_persisted_collection_thumbnails: Arc::new(|_, _| Box::pin(async { Ok(vec![]) })),
        insert_collection_thumbnail: Arc::new(|_, _, _, _, _, _, _| {
            Box::pin(async {
                Err(
                    "media assets runtime access backend is not configured for thumbnail writes"
                        .to_string(),
                )
            })
        }),
        select_collection_thumbnail: Arc::new(|_, _, _| Box::pin(async { Ok(false) })),
        delete_collection_thumbnail: Arc::new(|_, _, _| Box::pin(async { Ok(false) })),
        load_selected_series_thumbnail: Arc::new(|_, _| Box::pin(async { Ok(None) })),
        load_series_thumbnail_by_id: Arc::new(|_, _, _| Box::pin(async { Ok(None) })),
        load_persisted_series_thumbnails: Arc::new(|_, _| Box::pin(async { Ok(vec![]) })),
        insert_series_thumbnail: Arc::new(|_, _, _, _, _, _, _| {
            Box::pin(async {
                Err(
                    "media assets runtime access backend is not configured for thumbnail writes"
                        .to_string(),
                )
            })
        }),
        select_series_thumbnail: Arc::new(|_, _, _| Box::pin(async { Ok(false) })),
        delete_series_thumbnail: Arc::new(|_, _, _| Box::pin(async { Ok(false) })),
        load_persisted_readlist_name: Arc::new(|_, _| Box::pin(async { Ok(None) })),
        load_book_restrictions: Arc::new(|_, _| Box::pin(async { Ok(None) })),
        load_readlist_archive_entries: Arc::new(|_, _| Box::pin(async { Ok(vec![]) })),
        load_series_archive_entries: Arc::new(|_, _| Box::pin(async { Ok(None) })),
        is_font_resource: Arc::new(|_| false),
        read_epub_resource_bytes: Arc::new(|_, _| None),
        load_persisted_manifest_book: Arc::new(|_, _| Box::pin(async { Ok(None) })),
        persisted_book_exists: Arc::new(|_, _| Box::pin(async { Ok(false) })),
        persisted_book_ids: Arc::new(|_| Box::pin(async { Ok(vec![]) })),
        persisted_series_exists: Arc::new(|_, _| Box::pin(async { Ok(false) })),
        persisted_readlist_exists: Arc::new(|_, _| Box::pin(async { Ok(false) })),
        persisted_collection_exists: Arc::new(|_, _| Box::pin(async { Ok(false) })),
        load_series_book_number_sorts: Arc::new(|_, _| Box::pin(async { Ok(vec![]) })),
        load_book_page_count: Arc::new(|_, _| Box::pin(async { Ok(None) })),
        persist_book_progression: Arc::new(|_, _, _, _| Box::pin(async { Ok(()) })),
    }
}

fn content_type_from_filename_for_tests(file_name: &str, default_mime_type: &str) -> String {
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

fn book_media_supports_page_image_for_tests(media: &BookMediaRecord) -> bool {
    content_type_from_filename_for_tests(&media.file_name, &media.media_type).starts_with("image/")
}

fn book_media_is_single_image_for_tests(media: &BookMediaRecord) -> bool {
    book_media_supports_page_image_for_tests(media)
}

fn book_media_is_zip_archive_for_tests(media: &BookMediaRecord) -> bool {
    matches!(
        content_type_from_filename_for_tests(&media.file_name, &media.media_type).as_str(),
        "application/vnd.comicbook+zip" | "application/epub+zip" | "application/zip"
    )
}

fn book_media_is_pdf_for_tests(media: &BookMediaRecord) -> bool {
    content_type_from_filename_for_tests(&media.file_name, &media.media_type) == "application/pdf"
}

fn is_supported_page_image_file_name_for_tests(file_name: &str) -> bool {
    content_type_from_filename_for_tests(file_name, "application/octet-stream")
        .starts_with("image/")
}

fn resolve_book_page_bytes_for_tests(
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
    if book_media_is_single_image_for_tests(media) && page_number == 1 {
        candidates.push(media.file_path.clone());
    }
    for candidate in candidates {
        if let Ok(bytes) = fs::read(candidate) {
            return Some(bytes);
        }
    }
    read_zip_archive_page_bytes_for_tests(media, page, page_number).or_else(|| {
        if book_media_is_single_image_for_tests(media) && page_number == 1 {
            fs::read(&media.file_path).ok()
        } else {
            None
        }
    })
}

fn load_archive_page_row_for_tests(
    media: &BookMediaRecord,
    page_number: u64,
) -> Option<BookPageRecord> {
    if page_number == 0 {
        return None;
    }
    load_archive_page_rows_for_tests(media)?
        .into_iter()
        .nth(usize::try_from(page_number - 1).ok()?)
}

fn load_archive_page_rows_for_tests(media: &BookMediaRecord) -> Option<Vec<BookPageRecord>> {
    if !book_media_is_zip_archive_for_tests(media) {
        return None;
    }

    let file = fs::File::open(&media.file_path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let mut rows = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).ok()?;
        let file_name = entry.name().to_string();
        if !is_supported_page_image_file_name_for_tests(&file_name) {
            continue;
        }
        rows.push(BookPageRecord {
            number: (rows.len() as u64) + 1,
            media_type: content_type_from_filename_for_tests(&file_name, "image/jpeg"),
            file_name,
            width: None,
            height: None,
            file_size: entry.size().try_into().unwrap_or(i64::MAX),
        });
    }
    (!rows.is_empty()).then_some(rows)
}

fn read_zip_archive_page_bytes_for_tests(
    media: &BookMediaRecord,
    page: &BookPageRecord,
    page_number: u64,
) -> Option<Vec<u8>> {
    if !book_media_is_zip_archive_for_tests(media) || page_number == 0 {
        return None;
    }
    let file = fs::File::open(&media.file_path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    if !page.file_name.is_empty()
        && let Ok(mut entry) = archive.by_name(&page.file_name)
        && is_supported_page_image_file_name_for_tests(entry.name())
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
        if !is_supported_page_image_file_name_for_tests(entry.name()) {
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

fn load_pdf_page_row_for_tests(
    media: &BookMediaRecord,
    page_number: u64,
) -> Option<BookPageRecord> {
    if page_number == 0 {
        return None;
    }
    load_generated_pdf_page_rows_for_tests(media)
        .into_iter()
        .nth(usize::try_from(page_number - 1).ok()?)
}

fn load_generated_pdf_page_rows_for_tests(media: &BookMediaRecord) -> Vec<BookPageRecord> {
    if !book_media_is_pdf_for_tests(media) {
        return vec![];
    }
    let page_count = if media.page_count > 0 {
        media.page_count
    } else {
        detect_pdf_page_count_for_tests(media).unwrap_or(0)
    };
    if page_count == 0 {
        return vec![];
    }
    (1..=page_count)
        .map(|number| BookPageRecord {
            number,
            file_name: format!("page-{number}.pdf"),
            media_type: "application/pdf".to_string(),
            width: None,
            height: None,
            file_size: 0,
        })
        .collect()
}

fn read_pdf_page_as_single_page_pdf_for_tests(
    media: &BookMediaRecord,
    page_number: u64,
) -> Option<Vec<u8>> {
    if !book_media_is_pdf_for_tests(media) || page_number == 0 {
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

fn detect_pdf_page_count_for_tests(media: &BookMediaRecord) -> Option<u64> {
    if !book_media_is_pdf_for_tests(media) {
        return None;
    }
    Some(PdfDocument::load(&media.file_path).ok()?.get_pages().len() as u64)
}

fn decode_epub_positions_for_tests(blob: &[u8]) -> Result<Vec<Value>, String> {
    let mut decoder = GzDecoder::new(blob);
    let mut decoded = Vec::new();
    decoder
        .read_to_end(&mut decoded)
        .map_err(|error| format!("decode epub extension blob: {error}"))?;

    let extension = serde_json::from_slice::<Value>(&decoded)
        .map_err(|error| format!("parse epub extension blob json: {error}"))?;
    Ok(extension
        .get("positions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

fn load_epub_archive_positions_for_tests(media: &BookMediaRecord) -> Option<Vec<Value>> {
    if content_type_from_filename_for_tests(&media.file_name, &media.media_type)
        != "application/epub+zip"
    {
        return None;
    }

    let file = fs::File::open(&media.file_path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let container_xml =
        read_zip_entry_bytes_normalized_for_tests(&mut archive, "META-INF/container.xml")?;
    let rootfile_path = parse_epub_rootfile_path_for_tests(&container_xml)?;
    let package_document = read_zip_entry_bytes_normalized_for_tests(&mut archive, &rootfile_path)?;
    let spine_entries = parse_epub_spine_entries_for_tests(&package_document, &rootfile_path);
    if spine_entries.is_empty() {
        return None;
    }
    let fixed_layout = parse_epub_fixed_layout_for_tests(&package_document);
    let resources = spine_entries
        .into_iter()
        .map(|entry| {
            let bytes = read_zip_entry_bytes_normalized_for_tests(&mut archive, &entry.href)
                .unwrap_or_default();
            let kobo_spans = if fixed_layout {
                vec![]
            } else {
                parse_epub_kobo_spans_for_tests(&bytes)
            };
            (entry, bytes, kobo_spans)
        })
        .collect::<Vec<_>>();

    let mut raw_positions = Vec::new();
    for (entry, bytes, spans) in resources {
        let position_count = if fixed_layout {
            1usize
        } else {
            ((bytes.len() as f64 / 1024.0).ceil() as usize).max(1)
        };

        for segment_index in 0..position_count {
            let progression = if fixed_layout {
                0.0
            } else {
                segment_index as f64 / position_count as f64
            };
            let kobo_span = if fixed_layout || position_count == 1 || segment_index == 0 {
                Some("kobo.1.1".to_string())
            } else {
                closest_kobo_span_for_tests(&spans, progression)
            };
            raw_positions.push((
                entry.href.clone(),
                entry.media_type.clone(),
                progression,
                kobo_span,
            ));
        }
    }

    if raw_positions.is_empty() {
        return None;
    }

    let total_positions = raw_positions.len() as f64;
    Some(
        raw_positions
            .into_iter()
            .enumerate()
            .map(|(index, (href, media_type, progression, kobo_span))| {
                let position = index + 1;
                let mut locator = json!({
                    "href": href,
                    "type": media_type,
                    "locations": {
                        "position": position,
                        "progression": progression,
                        "totalProgression": position as f64 / total_positions,
                    },
                });
                if let Some(kobo_span) = kobo_span
                    && let Some(object) = locator.as_object_mut()
                {
                    object.insert("koboSpan".to_string(), Value::String(kobo_span));
                }
                locator
            })
            .collect(),
    )
}

fn read_zip_entry_bytes_normalized_for_tests<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
) -> Option<Vec<u8>> {
    if let Some(bytes) = read_zip_entry_bytes_for_tests(archive, path) {
        return Some(bytes);
    }

    let normalized = path.trim_start_matches('/');
    if normalized != path {
        return read_zip_entry_bytes_for_tests(archive, normalized);
    }
    None
}

fn read_zip_entry_bytes_for_tests<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    entry_name: &str,
) -> Option<Vec<u8>> {
    let mut entry = archive.by_name(entry_name).ok()?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

fn parse_epub_rootfile_path_for_tests(container_xml: &[u8]) -> Option<String> {
    let mut reader = XmlReader::from_reader(container_xml);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer).ok()? {
            XmlEvent::Start(event) | XmlEvent::Empty(event) => {
                if !xml_name_matches_for_tests(event.name().as_ref(), b"rootfile") {
                    buffer.clear();
                    continue;
                }
                for attribute in event.attributes().flatten() {
                    if xml_name_matches_for_tests(attribute.key.as_ref(), b"full-path") {
                        return attribute
                            .unescape_value()
                            .ok()
                            .map(|value| normalize_epub_zip_path_for_tests(value.as_ref()));
                    }
                }
            }
            XmlEvent::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    None
}

#[derive(Clone)]
struct EpubSpineEntryForTests {
    href: String,
    media_type: String,
}

fn parse_epub_spine_entries_for_tests(
    package_document: &[u8],
    rootfile_path: &str,
) -> Vec<EpubSpineEntryForTests> {
    let mut reader = XmlReader::from_reader(package_document);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut manifest = HashMap::<String, EpubSpineEntryForTests>::new();
    let mut spine = Vec::<String>::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(XmlEvent::Start(event)) | Ok(XmlEvent::Empty(event)) => {
                if xml_name_matches_for_tests(event.name().as_ref(), b"item") {
                    let mut id = None::<String>;
                    let mut href = None::<String>;
                    let mut media_type = None::<String>;
                    for attribute in event.attributes().flatten() {
                        if xml_name_matches_for_tests(attribute.key.as_ref(), b"id") {
                            id = attribute
                                .unescape_value()
                                .ok()
                                .map(|value| value.into_owned());
                        } else if xml_name_matches_for_tests(attribute.key.as_ref(), b"href") {
                            href = attribute
                                .unescape_value()
                                .ok()
                                .map(|value| value.into_owned());
                        } else if xml_name_matches_for_tests(attribute.key.as_ref(), b"media-type")
                        {
                            media_type = attribute
                                .unescape_value()
                                .ok()
                                .map(|value| value.into_owned());
                        }
                    }
                    if let (Some(id), Some(href)) = (id, href) {
                        manifest.insert(
                            id,
                            EpubSpineEntryForTests {
                                href: normalize_epub_resource_href_for_tests(rootfile_path, &href),
                                media_type: media_type
                                    .unwrap_or_else(|| "application/xhtml+xml".to_string()),
                            },
                        );
                    }
                } else if xml_name_matches_for_tests(event.name().as_ref(), b"itemref") {
                    for attribute in event.attributes().flatten() {
                        if xml_name_matches_for_tests(attribute.key.as_ref(), b"idref")
                            && let Some(idref) = attribute
                                .unescape_value()
                                .ok()
                                .map(|value| value.into_owned())
                        {
                            spine.push(idref);
                        }
                    }
                }
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buffer.clear();
    }

    if spine.is_empty() {
        return vec![];
    }

    spine
        .into_iter()
        .filter_map(|idref| manifest.get(&idref).cloned())
        .collect()
}

fn parse_epub_fixed_layout_for_tests(package_document: &[u8]) -> bool {
    let mut reader = XmlReader::from_reader(package_document);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut awaiting_rendition_layout_text = false;

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(XmlEvent::Start(event)) | Ok(XmlEvent::Empty(event)) => {
                if !xml_name_matches_for_tests(event.name().as_ref(), b"meta") {
                    buffer.clear();
                    continue;
                }

                let mut property = None::<String>;
                let mut name = None::<String>;
                let mut content = None::<String>;
                for attribute in event.attributes().flatten() {
                    if xml_name_matches_for_tests(attribute.key.as_ref(), b"property") {
                        property = attribute
                            .unescape_value()
                            .ok()
                            .map(|value| value.into_owned());
                    } else if xml_name_matches_for_tests(attribute.key.as_ref(), b"name") {
                        name = attribute
                            .unescape_value()
                            .ok()
                            .map(|value| value.into_owned());
                    } else if xml_name_matches_for_tests(attribute.key.as_ref(), b"content") {
                        content = attribute
                            .unescape_value()
                            .ok()
                            .map(|value| value.into_owned());
                    }
                }

                if property
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case("rendition:layout"))
                {
                    if content
                        .as_deref()
                        .is_some_and(|value| value.eq_ignore_ascii_case("pre-paginated"))
                    {
                        return true;
                    }
                    awaiting_rendition_layout_text = true;
                }
                if name
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case("fixed-layout"))
                    && content
                        .as_deref()
                        .is_some_and(|value| value.eq_ignore_ascii_case("true"))
                {
                    return true;
                }
            }
            Ok(XmlEvent::Text(text)) if awaiting_rendition_layout_text => {
                let value = String::from_utf8_lossy(text.as_ref()).trim().to_string();
                if value.eq_ignore_ascii_case("pre-paginated") {
                    return true;
                }
            }
            Ok(XmlEvent::End(event))
                if xml_name_matches_for_tests(event.name().as_ref(), b"meta") =>
            {
                awaiting_rendition_layout_text = false;
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buffer.clear();
    }

    false
}

fn parse_epub_kobo_spans_for_tests(resource_bytes: &[u8]) -> Vec<(String, f64)> {
    let content = String::from_utf8_lossy(resource_bytes);
    if content.is_empty() {
        return vec![];
    }

    let mut spans = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut cursor = 0usize;
    let total_len = content.len().max(1) as f64;

    while let Some(relative_start) = content[cursor..].find("<span") {
        let span_start = cursor + relative_start;
        let Some(relative_end) = content[span_start..].find('>') else {
            break;
        };
        let span_end = span_start + relative_end;
        let tag = &content[span_start..=span_end];
        if !tag.to_ascii_lowercase().contains("kobospan") {
            cursor = span_end.saturating_add(1);
            if cursor >= content.len() {
                break;
            }
            continue;
        }

        let id = extract_html_attribute_for_tests(tag, "id").unwrap_or_default();
        if id.starts_with("kobo.") && seen.insert(id.clone()) {
            let progression = (span_end as f64 / total_len).clamp(0.0, 1.0);
            spans.push((id, progression));
        }

        cursor = span_end.saturating_add(1);
        if cursor >= content.len() {
            break;
        }
    }

    spans
}

fn extract_html_attribute_for_tests(tag: &str, attribute: &str) -> Option<String> {
    let double_quoted = format!("{attribute}=\"");
    if let Some(start) = tag.find(&double_quoted) {
        let value_start = start + double_quoted.len();
        let value_end = tag[value_start..].find('"')? + value_start;
        return Some(tag[value_start..value_end].to_string());
    }

    let single_quoted = format!("{attribute}='");
    if let Some(start) = tag.find(&single_quoted) {
        let value_start = start + single_quoted.len();
        let value_end = tag[value_start..].find('\'')? + value_start;
        return Some(tag[value_start..value_end].to_string());
    }

    None
}

fn closest_kobo_span_for_tests(spans: &[(String, f64)], progression: f64) -> Option<String> {
    spans
        .iter()
        .min_by(|left, right| {
            let left_distance = (left.1 - progression).abs();
            let right_distance = (right.1 - progression).abs();
            left_distance
                .partial_cmp(&right_distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(id, _)| id.clone())
}

fn normalize_epub_resource_href_for_tests(rootfile_path: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or_default();
    if href.starts_with('/') {
        return normalize_epub_zip_path_for_tests(href);
    }

    let base = rootfile_path
        .trim_start_matches('/')
        .rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or_default();
    let joined = if base.is_empty() {
        href.to_string()
    } else {
        format!("{base}/{href}")
    };
    normalize_epub_zip_path_for_tests(joined.as_str())
}

fn normalize_epub_zip_path_for_tests(path: &str) -> String {
    let normalized_path = path.replace('\\', "/");
    let mut normalized_segments = Vec::<&str>::new();
    for segment in normalized_path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                normalized_segments.pop();
            }
            _ => normalized_segments.push(segment),
        }
    }

    if normalized_segments.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", normalized_segments.join("/"))
    }
}

fn xml_name_matches_for_tests(actual: &[u8], expected: &[u8]) -> bool {
    actual == expected || actual.ends_with(expected)
}

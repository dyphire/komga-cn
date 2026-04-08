#![allow(clippy::type_complexity)]

use axum::http::Uri;
use language_tags::LanguageTag;
use serde::Deserialize;
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::hash::Hash;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use komga_application::media_assets::{
    BookMediaRecord, BookMetadata, BookMetadataAuthor, BookMetadataLink, BookPageRecord,
    book_media_is_epub, book_media_is_pdf, book_media_is_single_image, content_type_from_filename,
};
use pdfium_render::prelude::*;
use quick_xml::Reader as XmlReader;
use quick_xml::events::{BytesStart as XmlBytesStart, Event as XmlEvent};
use rxing::{BarcodeFormat, DecodeHints, helpers as rxing_helpers};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::filesystem::{
    load_archive_page_row, load_epub_cover_bytes, load_epub_package_document,
    resolve_book_page_bytes,
};
use crate::load_pdfium;
use crate::sqlite::connect_pool;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RefreshBookMetadataOutcome {
    pub series_id: Option<String>,
    pub changed_readlist_ids: Vec<String>,
}

fn thumbnail_max_edge_from_setting(value: Option<&str>) -> u32 {
    match value.unwrap_or("DEFAULT") {
        "MEDIUM" => 600,
        "LARGE" => 900,
        "XLARGE" => 1200,
        _ => 300,
    }
}

pub fn refresh_book_metadata(
    database_file: &Path,
    book_id: &str,
    capabilities: &BTreeSet<String>,
) -> Result<RefreshBookMetadataOutcome, String> {
    let database_file = database_file.to_path_buf();
    let book_id = book_id.to_string();
    let capabilities = capabilities.clone();

    run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        let capabilities = capabilities.clone();
        Box::pin(async move {
            let mut changed_readlist_ids = BTreeSet::new();
            let book_row = sqlx::query(
                r#"
                SELECT b.URL AS BOOK_URL,
                       l.ROOT AS LIBRARY_ROOT,
                       l.IMPORT_COMICINFO_BOOK AS IMPORT_COMICINFO_BOOK,
                       l.IMPORT_COMICINFO_READLIST AS IMPORT_COMICINFO_READLIST,
                       l.IMPORT_EPUB_BOOK AS IMPORT_EPUB_BOOK,
                       l.IMPORT_BARCODE_ISBN AS IMPORT_BARCODE_ISBN
                FROM BOOK b
                JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
                WHERE b.ID = ?
                LIMIT 1
                "#,
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!("failed to resolve book path for metadata refresh '{book_id}': {error}")
            })?;

            if let Some(book_row) = &book_row {
                let book_url = book_row.get::<String, _>("BOOK_URL");
                let library_root = book_row.get::<String, _>("LIBRARY_ROOT");
                let import_comicinfo_book = book_row.get::<bool, _>("IMPORT_COMICINFO_BOOK");
                let import_comicinfo_readlist =
                    book_row.get::<bool, _>("IMPORT_COMICINFO_READLIST");
                let import_epub_book = book_row.get::<bool, _>("IMPORT_EPUB_BOOK");
                let import_barcode_isbn = book_row.get::<bool, _>("IMPORT_BARCODE_ISBN");
                if let Some(sidecar_url) =
                    load_sidecar_url_for_parent(&pool, &book_url, true).await?
                {
                    let sidecar_path = PathBuf::from(library_root).join(sidecar_url);
                    if let Ok(xml) = fs::read_to_string(&sidecar_path)
                        && comicinfo_provider_matches_capabilities(&capabilities)
                    {
                        if import_comicinfo_book {
                            let patch = extract_comicinfo_book_patch(&xml);
                            apply_book_metadata_import_patch(&pool, &book_id, patch).await?;
                        }

                        if import_comicinfo_readlist {
                            for readlist in extract_comicinfo_readlists(&xml) {
                                if let Some(readlist_id) =
                                    upsert_comicinfo_readlist(&pool, &book_id, readlist).await?
                                {
                                    changed_readlist_ids.insert(readlist_id);
                                }
                            }
                        }
                    }
                }

                if import_epub_book && epub_provider_matches_capabilities(&capabilities) {
                    if let Some(media) = load_book_media_for_refresh(&pool, &book_id).await? {
                        if let Some(package_document) = load_epub_package_document(&media) {
                            let patch = extract_epub_book_patch(&package_document);
                            apply_book_metadata_import_patch(&pool, &book_id, patch).await?;
                        }
                    }
                }

                if import_barcode_isbn && barcode_provider_matches_capabilities(&capabilities) {
                    refresh_barcode_isbn(&pool, &book_id).await?;
                }
            }

            sqlx::query(
                r#"
                UPDATE BOOK_METADATA
                SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE BOOK_ID = ?
                "#,
            )
            .bind(&book_id)
            .execute(&pool)
            .await
            .map_err(|error| format!("failed to refresh BOOK_METADATA for '{book_id}': {error}"))?;

            sqlx::query(
                r#"
                UPDATE BOOK
                SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE ID = ?
                "#,
            )
            .bind(&book_id)
            .execute(&pool)
            .await
            .map_err(|error| {
                format!("failed to refresh BOOK row timestamp for '{book_id}': {error}")
            })?;

            let series_id = sqlx::query(
                r#"
                SELECT SERIES_ID
                FROM BOOK
                WHERE ID = ?
                LIMIT 1
                "#,
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| format!("failed to resolve SERIES_ID for '{book_id}': {error}"))?
            .and_then(|row| row.get::<Option<String>, _>("SERIES_ID"));

            Ok(RefreshBookMetadataOutcome {
                series_id,
                changed_readlist_ids: changed_readlist_ids.into_iter().collect(),
            })
        })
    })
}

struct ComicInfoReadListEntry {
    name: String,
    number: Option<i64>,
}

#[derive(Default)]
struct BookMetadataImportPatch {
    title: Option<String>,
    summary: Option<String>,
    number: Option<String>,
    number_sort: Option<f64>,
    release_date: Option<String>,
    authors: Option<Vec<BookMetadataAuthor>>,
    tags: Option<Vec<String>>,
    isbn: Option<String>,
    links: Option<Vec<BookMetadataLink>>,
}

#[derive(Clone, Debug, Default)]
struct SeriesMetadataImportPatch {
    title: Option<String>,
    title_sort: Option<String>,
    status: Option<String>,
    summary: Option<String>,
    reading_direction: Option<String>,
    publisher: Option<String>,
    age_rating: Option<u32>,
    language: Option<String>,
    genres: Option<Vec<String>>,
    total_book_count: Option<u32>,
    collections: Vec<String>,
}

struct SeriesBookRefreshSource {
    media: BookMediaRecord,
}

struct SeriesMetadataRefreshState {
    status: String,
    status_lock: bool,
    title: String,
    title_lock: bool,
    title_sort: String,
    title_sort_lock: bool,
    summary: String,
    summary_lock: bool,
    reading_direction: Option<String>,
    reading_direction_lock: bool,
    publisher: String,
    publisher_lock: bool,
    age_rating: Option<u32>,
    age_rating_lock: bool,
    language: String,
    language_lock: bool,
    genres: Vec<String>,
    genres_lock: bool,
    total_book_count: Option<u32>,
    total_book_count_lock: bool,
}

struct PersistedCollectionMembership {
    id: String,
    name: String,
    ordered: bool,
    series_ids: Vec<String>,
}

#[derive(Deserialize)]
struct MylarSeriesFile {
    metadata: MylarSeriesMetadata,
}

#[derive(Deserialize)]
struct MylarSeriesMetadata {
    publisher: String,
    name: String,
    year: i64,
    #[serde(rename = "description_text")]
    description_text: Option<String>,
    #[serde(rename = "description_formatted")]
    description_formatted: Option<String>,
    volume: Option<i64>,
    #[serde(rename = "age_rating")]
    age_rating: Option<MylarAgeRating>,
    #[serde(rename = "total_issues")]
    total_issues: i64,
    status: MylarStatus,
}

#[derive(Deserialize)]
enum MylarStatus {
    Ended,
    Continuing,
}

#[derive(Deserialize)]
enum MylarAgeRating {
    #[serde(rename = "All")]
    All,
    #[serde(rename = "9+")]
    Nine,
    #[serde(rename = "12+")]
    Twelve,
    #[serde(rename = "15+")]
    Fifteen,
    #[serde(rename = "17+")]
    Seventeen,
    #[serde(rename = "Adult")]
    Adult,
}

fn comicinfo_provider_matches_capabilities(capabilities: &BTreeSet<String>) -> bool {
    capabilities.iter().any(|capability| {
        matches!(
            capability.as_str(),
            "TITLE"
                | "SUMMARY"
                | "NUMBER"
                | "NUMBER_SORT"
                | "RELEASE_DATE"
                | "AUTHORS"
                | "READ_LISTS"
                | "LINKS"
        )
    })
}

fn epub_provider_matches_capabilities(capabilities: &BTreeSet<String>) -> bool {
    capabilities.iter().any(|capability| {
        matches!(
            capability.as_str(),
            "TITLE" | "SUMMARY" | "RELEASE_DATE" | "AUTHORS" | "ISBN"
        )
    })
}

fn barcode_provider_matches_capabilities(capabilities: &BTreeSet<String>) -> bool {
    capabilities.contains("ISBN")
}

async fn refresh_barcode_isbn(pool: &SqlitePool, book_id: &str) -> Result<(), String> {
    let Some(media) = load_book_media_for_refresh(pool, book_id).await? else {
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

        apply_book_metadata_import_patch(
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
        return render_pdf_page_image_for_barcode(media, page_number).map(Some);
    }

    if book_media_is_single_image(media) && page_number == 1 {
        return fs::read(&media.file_path).map(Some).map_err(|error| {
            format!(
                "failed to read single-image barcode candidate '{}' for '{}': {error}",
                media.file_path.display(),
                book_id,
            )
        });
    }

    let page = load_book_page_row_for_refresh(pool, book_id, page_number)
        .await?
        .or_else(|| load_archive_page_row(media, page_number));
    let Some(page) = page else {
        return Ok(None);
    };

    Ok(resolve_book_page_bytes(media, &page, page_number))
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
    let mut hints = DecodeHints::default();
    hints.TryHarder = Some(true);
    hints.AlsoInverted = Some(true);

    let result = rxing_helpers::detect_in_buffer_with_hints(
        image_bytes,
        Some(BarcodeFormat::EAN_13),
        &mut hints,
    )
    .ok()?;
    normalize_isbn13(&result.getText().to_string())
}

enum EpubTextTarget {
    Title,
    Description,
    Date,
    Identifier,
    Creator {
        id: Option<String>,
        role_attr: Option<String>,
    },
    RoleMeta {
        refines: Option<String>,
    },
    GroupPosition {
        refines: Option<String>,
    },
}

fn extract_epub_book_patch(package_document: &[u8]) -> BookMetadataImportPatch {
    let mut reader = XmlReader::from_reader(package_document);
    reader.config_mut().trim_text(true);

    let mut buffer = Vec::new();
    let mut current_target = None::<EpubTextTarget>;
    let mut current_text = String::new();

    let mut title = None::<String>;
    let mut description = None::<String>;
    let mut release_date = None::<String>;
    let mut identifiers = Vec::<String>::new();
    let mut authors = Vec::<BookMetadataAuthor>::new();
    let mut refined_roles = HashMap::<String, String>::new();
    let mut collection_id = None::<String>;
    let mut group_positions = HashMap::<String, String>::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(XmlEvent::Start(event)) => {
                if xml_name_matches_local(event.name().as_ref(), b"meta") {
                    handle_epub_meta_event(
                        &event,
                        &mut current_target,
                        &mut current_text,
                        &mut refined_roles,
                        &mut collection_id,
                        &mut group_positions,
                    );
                } else if let Some(target) = epub_text_target_from_start(&event) {
                    current_target = Some(target);
                    current_text.clear();
                }
            }
            Ok(XmlEvent::Empty(event)) => {
                if xml_name_matches_local(event.name().as_ref(), b"meta") {
                    handle_epub_meta_event(
                        &event,
                        &mut current_target,
                        &mut current_text,
                        &mut refined_roles,
                        &mut collection_id,
                        &mut group_positions,
                    );
                }
            }
            Ok(XmlEvent::Text(text)) if current_target.is_some() => {
                current_text.push_str(String::from_utf8_lossy(text.as_ref()).as_ref());
            }
            Ok(XmlEvent::CData(text)) if current_target.is_some() => {
                current_text.push_str(String::from_utf8_lossy(text.as_ref()).as_ref());
            }
            Ok(XmlEvent::End(event))
                if epub_text_target_matches_end(current_target.as_ref(), event.name().as_ref()) =>
            {
                let target = current_target
                    .take()
                    .expect("epub text target should exist");
                finalize_epub_text_target(
                    target,
                    current_text.trim().to_string(),
                    &mut title,
                    &mut description,
                    &mut release_date,
                    &mut identifiers,
                    &mut authors,
                    &mut refined_roles,
                    &mut group_positions,
                );
                current_text.clear();
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buffer.clear();
    }

    let number = collection_id
        .as_deref()
        .and_then(|id| group_positions.get(id))
        .cloned()
        .filter(|value| !value.trim().is_empty());
    let number_sort = number
        .as_deref()
        .and_then(|value| value.parse::<f64>().ok());

    BookMetadataImportPatch {
        title,
        summary: description
            .map(|value| strip_markup_tags(&value))
            .filter(|value| !value.is_empty()),
        number,
        number_sort,
        release_date,
        authors: (!authors.is_empty()).then_some(authors),
        tags: None,
        isbn: identifiers
            .into_iter()
            .find_map(|value| normalize_epub_identifier_isbn(&value)),
        links: None,
    }
}

fn epub_text_target_from_start(event: &XmlBytesStart<'_>) -> Option<EpubTextTarget> {
    let name = event.name();
    let name = name.as_ref();
    if xml_name_matches_local(name, b"title") {
        Some(EpubTextTarget::Title)
    } else if xml_name_matches_local(name, b"description") {
        Some(EpubTextTarget::Description)
    } else if xml_name_matches_local(name, b"date") {
        Some(EpubTextTarget::Date)
    } else if xml_name_matches_local(name, b"identifier") {
        Some(EpubTextTarget::Identifier)
    } else if xml_name_matches_local(name, b"creator") {
        Some(EpubTextTarget::Creator {
            id: attribute_value(event, b"id"),
            role_attr: attribute_value(event, b"role"),
        })
    } else {
        None
    }
}

fn epub_text_target_matches_end(target: Option<&EpubTextTarget>, name: &[u8]) -> bool {
    match target {
        Some(EpubTextTarget::Title) => xml_name_matches_local(name, b"title"),
        Some(EpubTextTarget::Description) => xml_name_matches_local(name, b"description"),
        Some(EpubTextTarget::Date) => xml_name_matches_local(name, b"date"),
        Some(EpubTextTarget::Identifier) => xml_name_matches_local(name, b"identifier"),
        Some(EpubTextTarget::Creator { .. }) => xml_name_matches_local(name, b"creator"),
        Some(EpubTextTarget::RoleMeta { .. }) | Some(EpubTextTarget::GroupPosition { .. }) => {
            xml_name_matches_local(name, b"meta")
        }
        None => false,
    }
}

fn handle_epub_meta_event(
    event: &XmlBytesStart<'_>,
    current_target: &mut Option<EpubTextTarget>,
    current_text: &mut String,
    refined_roles: &mut HashMap<String, String>,
    collection_id: &mut Option<String>,
    group_positions: &mut HashMap<String, String>,
) {
    let property = attribute_value(event, b"property");
    let content = attribute_value(event, b"content");

    if property
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("role"))
    {
        let scheme = attribute_value(event, b"scheme");
        if !scheme
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("marc:relators"))
        {
            return;
        }
        let refines = attribute_value(event, b"refines").map(normalize_epub_refines);
        if let Some(value) = content.and_then(nonblank_string) {
            if let Some(refines) = refines {
                refined_roles.entry(refines).or_insert(value);
            }
        } else {
            *current_target = Some(EpubTextTarget::RoleMeta { refines });
            current_text.clear();
        }
    } else if property
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("belongs-to-collection"))
    {
        if collection_id.is_none() {
            *collection_id = attribute_value(event, b"id").and_then(nonblank_string);
        }
    } else if property
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("group-position"))
    {
        let refines = attribute_value(event, b"refines").map(normalize_epub_refines);
        if let Some(value) = content.and_then(nonblank_string) {
            if let Some(refines) = refines {
                group_positions.entry(refines).or_insert(value);
            }
        } else {
            *current_target = Some(EpubTextTarget::GroupPosition { refines });
            current_text.clear();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn finalize_epub_text_target(
    target: EpubTextTarget,
    value: String,
    title: &mut Option<String>,
    description: &mut Option<String>,
    release_date: &mut Option<String>,
    identifiers: &mut Vec<String>,
    authors: &mut Vec<BookMetadataAuthor>,
    refined_roles: &mut HashMap<String, String>,
    group_positions: &mut HashMap<String, String>,
) {
    match target {
        EpubTextTarget::Title => {
            if title.is_none() {
                *title = nonblank_string(value);
            }
        }
        EpubTextTarget::Description => {
            if description.is_none() {
                *description = nonblank_string(value);
            }
        }
        EpubTextTarget::Date => {
            if release_date.is_none() {
                *release_date = normalize_epub_date(&value);
            }
        }
        EpubTextTarget::Identifier => {
            if let Some(value) = nonblank_string(value) {
                identifiers.push(value);
            }
        }
        EpubTextTarget::Creator { id, role_attr } => {
            if let Some(name) = nonblank_string(value) {
                let refined_role = id
                    .as_deref()
                    .and_then(|id| refined_roles.get(id))
                    .map(String::as_str);
                authors.push(BookMetadataAuthor {
                    name,
                    role: map_epub_author_role(role_attr.as_deref().or(refined_role)).to_string(),
                });
            }
        }
        EpubTextTarget::RoleMeta { refines } => {
            if let (Some(refines), Some(value)) = (refines, nonblank_string(value)) {
                refined_roles.entry(refines).or_insert(value);
            }
        }
        EpubTextTarget::GroupPosition { refines } => {
            if let (Some(refines), Some(value)) = (refines, nonblank_string(value)) {
                group_positions.entry(refines).or_insert(value);
            }
        }
    }
}

fn normalize_epub_identifier_isbn(value: &str) -> Option<String> {
    let lowered = value.trim().to_ascii_lowercase();
    let candidate = lowered.strip_prefix("isbn:").unwrap_or(lowered.as_str());

    normalize_isbn13(candidate)
}

fn normalize_epub_date(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let candidate = if trimmed.len() >= 10 {
        &trimmed[..10]
    } else {
        trimmed
    };
    if candidate.len() != 10 {
        return None;
    }

    let bytes = candidate.as_bytes();
    if bytes.get(4) != Some(&b'-') || bytes.get(7) != Some(&b'-') {
        return None;
    }

    let year = candidate[0..4].parse::<i32>().ok()?;
    let month = candidate[5..7].parse::<u8>().ok()?;
    let day = candidate[8..10].parse::<u8>().ok()?;
    if !is_valid_calendar_date(year, month, day) {
        return None;
    }

    Some(format!("{year:04}-{month:02}-{day:02}"))
}

fn map_epub_author_role(value: Option<&str>) -> &'static str {
    let value = value.unwrap_or("writer").trim().to_ascii_lowercase();
    match value.as_str() {
        "aut" => "writer",
        "clr" => "colorist",
        "cov" => "cover",
        "edt" => "editor",
        "art" | "ill" => "penciller",
        "trl" => "translator",
        _ => "writer",
    }
}

fn strip_markup_tags(value: &str) -> String {
    let mut output = String::new();
    let mut inside_tag = false;

    for character in value.chars() {
        match character {
            '<' => {
                inside_tag = true;
                if !output.ends_with(' ') {
                    output.push(' ');
                }
            }
            '>' => {
                inside_tag = false;
                if !output.ends_with(' ') {
                    output.push(' ');
                }
            }
            _ if !inside_tag => output.push(character),
            _ => {}
        }
    }

    output
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn attribute_value(event: &XmlBytesStart<'_>, key: &[u8]) -> Option<String> {
    event.attributes().flatten().find_map(|attribute| {
        xml_name_matches_local(attribute.key.as_ref(), key)
            .then(|| {
                attribute
                    .unescape_value()
                    .ok()
                    .map(|value| value.into_owned())
            })
            .flatten()
    })
}

fn normalize_epub_refines(value: String) -> String {
    value.trim().trim_start_matches('#').to_string()
}

fn nonblank_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn xml_name_matches_local(actual: &[u8], expected: &[u8]) -> bool {
    actual == expected || actual.ends_with(expected)
}

fn extract_comicinfo_book_patch(xml: &str) -> BookMetadataImportPatch {
    let number = extract_xml_tag(xml, "Number");

    BookMetadataImportPatch {
        title: extract_xml_tag(xml, "Title"),
        summary: extract_xml_tag(xml, "Summary"),
        number_sort: number
            .as_deref()
            .and_then(|value| value.parse::<f64>().ok()),
        number,
        release_date: extract_comicinfo_release_date(xml),
        authors: extract_comicinfo_authors(xml),
        tags: extract_comicinfo_tags(xml),
        isbn: extract_comicinfo_isbn(xml),
        links: extract_comicinfo_links(xml),
    }
}

async fn apply_book_metadata_import_patch(
    pool: &SqlitePool,
    book_id: &str,
    patch: BookMetadataImportPatch,
) -> Result<(), String> {
    let Some(mut metadata) = load_book_metadata_for_refresh(pool, book_id).await? else {
        return Ok(());
    };

    let mut changed = false;

    if let Some(title) = patch.title
        && !metadata.title_lock
        && metadata.title != title
    {
        metadata.title = title;
        changed = true;
    }

    if let Some(summary) = patch.summary
        && !metadata.summary_lock
        && metadata.summary != summary
    {
        metadata.summary = summary;
        changed = true;
    }

    if let Some(number) = patch.number
        && !metadata.number_lock
        && metadata.number != number
    {
        metadata.number = number;
        changed = true;
    }

    if let Some(number_sort) = patch.number_sort
        && !metadata.number_sort_lock
        && metadata.number_sort != number_sort
    {
        metadata.number_sort = number_sort;
        changed = true;
    }

    if let Some(release_date) = patch.release_date
        && !metadata.release_date_lock
        && metadata.release_date.as_deref() != Some(release_date.as_str())
    {
        metadata.release_date = Some(release_date);
        changed = true;
    }

    if let Some(authors) = patch.authors
        && !metadata.authors_lock
        && metadata.authors != authors
    {
        metadata.authors = authors;
        changed = true;
    }

    if let Some(tags) = patch.tags
        && !metadata.tags_lock
        && metadata.tags != tags
    {
        metadata.tags = tags;
        changed = true;
    }

    if let Some(isbn) = patch.isbn
        && !metadata.isbn_lock
        && metadata.isbn != isbn
    {
        metadata.isbn = isbn;
        changed = true;
    }

    if let Some(links) = patch.links
        && !metadata.links_lock
        && metadata.links != links
    {
        metadata.links = links;
        changed = true;
    }

    if changed {
        let persisted = persist_book_metadata_for_refresh(pool, book_id, &metadata).await?;
        if !persisted {
            return Err(format!(
                "book metadata row disappeared before metadata refresh for '{book_id}'"
            ));
        }
    }

    Ok(())
}

async fn load_book_media_for_refresh(
    pool: &SqlitePool,
    book_id: &str,
) -> Result<Option<BookMediaRecord>, String> {
    let row = sqlx::query(
        "SELECT b.LIBRARY_ID AS LIBRARY_ID, b.NAME AS FILE_NAME, b.URL AS BOOK_URL, \
                l.ROOT AS LIBRARY_ROOT, \
                COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE, \
                COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT \
         FROM BOOK b \
         JOIN LIBRARY l ON l.ID = b.LIBRARY_ID \
         LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID \
         WHERE b.ID = ?",
    )
    .bind(book_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query persisted book media for refresh: {error}"))?;

    Ok(row.map(|row| BookMediaRecord {
        library_id: row.get::<String, _>("LIBRARY_ID"),
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        file_path: PathBuf::from(row.get::<String, _>("LIBRARY_ROOT"))
            .join(row.get::<String, _>("BOOK_URL")),
        file_name: row.get::<String, _>("FILE_NAME"),
        page_count: row.get::<i64, _>("PAGE_COUNT").max(0) as u64,
    }))
}

async fn load_book_page_row_for_refresh(
    pool: &SqlitePool,
    book_id: &str,
    page_number: u64,
) -> Result<Option<BookPageRecord>, String> {
    let row = sqlx::query(
        "SELECT NUMBER, FILE_NAME, MEDIA_TYPE, WIDTH, HEIGHT, CASE WHEN FILE_SIZE IS NULL THEN -1 ELSE FILE_SIZE END AS FILE_SIZE \
         FROM MEDIA_PAGE WHERE BOOK_ID = ? AND NUMBER = ? LIMIT 1",
    )
    .bind(book_id)
    .bind(page_number as i64)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query single persisted book page for refresh: {error}"))?;

    Ok(row.map(|row| BookPageRecord {
        number: row.get::<i64, _>("NUMBER") as u64,
        file_name: row.get::<String, _>("FILE_NAME"),
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        width: row.get::<Option<i64>, _>("WIDTH"),
        height: row.get::<Option<i64>, _>("HEIGHT"),
        file_size: row.get::<i64, _>("FILE_SIZE"),
    }))
}

async fn load_book_metadata_for_refresh(
    pool: &SqlitePool,
    book_id: &str,
) -> Result<Option<BookMetadata>, String> {
    let row = sqlx::query(
        "SELECT TITLE, TITLE_LOCK, SUMMARY, SUMMARY_LOCK, NUMBER, NUMBER_LOCK, NUMBER_SORT, \
                NUMBER_SORT_LOCK, RELEASE_DATE, RELEASE_DATE_LOCK, AUTHORS_LOCK, TAGS_LOCK, ISBN, \
                ISBN_LOCK, LINKS_LOCK \
         FROM BOOK_METADATA \
         WHERE BOOK_ID = ? \
         LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query existing book metadata for refresh: {error}"))?;

    let Some(row) = row else {
        return Ok(None);
    };

    let author_rows = sqlx::query(
        "SELECT NAME, ROLE FROM BOOK_METADATA_AUTHOR WHERE BOOK_ID = ? ORDER BY ROLE ASC, NAME ASC",
    )
    .bind(book_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query existing book metadata authors for refresh: {error}"))?;

    let tag_rows = sqlx::query(
        "SELECT TAG FROM BOOK_METADATA_TAG WHERE BOOK_ID = ? ORDER BY TAG COLLATE NOCASE ASC",
    )
    .bind(book_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query existing book metadata tags for refresh: {error}"))?;

    let link_rows = sqlx::query(
        "SELECT LABEL, URL FROM BOOK_METADATA_LINK WHERE BOOK_ID = ? ORDER BY LABEL COLLATE NOCASE ASC, URL ASC",
    )
    .bind(book_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query existing book metadata links for refresh: {error}"))?;

    Ok(Some(BookMetadata {
        title: row.get::<String, _>("TITLE"),
        title_lock: row.get::<i64, _>("TITLE_LOCK") != 0,
        summary: row.get::<String, _>("SUMMARY"),
        summary_lock: row.get::<i64, _>("SUMMARY_LOCK") != 0,
        number: row.get::<String, _>("NUMBER"),
        number_lock: row.get::<i64, _>("NUMBER_LOCK") != 0,
        number_sort: row.get::<f64, _>("NUMBER_SORT"),
        number_sort_lock: row.get::<i64, _>("NUMBER_SORT_LOCK") != 0,
        release_date: row.get::<Option<String>, _>("RELEASE_DATE"),
        release_date_lock: row.get::<i64, _>("RELEASE_DATE_LOCK") != 0,
        authors: author_rows
            .into_iter()
            .map(|entry| BookMetadataAuthor {
                name: entry.get::<String, _>("NAME"),
                role: entry.get::<String, _>("ROLE"),
            })
            .collect(),
        authors_lock: row.get::<i64, _>("AUTHORS_LOCK") != 0,
        tags: tag_rows
            .into_iter()
            .map(|entry| entry.get::<String, _>("TAG"))
            .collect(),
        tags_lock: row.get::<i64, _>("TAGS_LOCK") != 0,
        isbn: row.get::<String, _>("ISBN"),
        isbn_lock: row.get::<i64, _>("ISBN_LOCK") != 0,
        links: link_rows
            .into_iter()
            .map(|entry| BookMetadataLink {
                label: entry.get::<String, _>("LABEL"),
                url: entry.get::<String, _>("URL"),
            })
            .collect(),
        links_lock: row.get::<i64, _>("LINKS_LOCK") != 0,
    }))
}

async fn persist_book_metadata_for_refresh(
    pool: &SqlitePool,
    book_id: &str,
    metadata: &BookMetadata,
) -> Result<bool, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin book metadata refresh tx: {error}"))?;

    let exists = sqlx::query("SELECT 1 AS FOUND FROM BOOK_METADATA WHERE BOOK_ID = ? LIMIT 1")
        .bind(book_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| format!("query book metadata existence for refresh: {error}"))?
        .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback book metadata refresh tx: {error}"))?;
        return Ok(false);
    }

    sqlx::query(
        "UPDATE BOOK_METADATA \
         SET TITLE = ?, TITLE_LOCK = ?, SUMMARY = ?, SUMMARY_LOCK = ?, NUMBER = ?, \
             NUMBER_LOCK = ?, NUMBER_SORT = ?, NUMBER_SORT_LOCK = ?, RELEASE_DATE = ?, \
             RELEASE_DATE_LOCK = ?, AUTHORS_LOCK = ?, TAGS_LOCK = ?, ISBN = ?, ISBN_LOCK = ?, \
             LINKS_LOCK = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
         WHERE BOOK_ID = ?",
    )
    .bind(&metadata.title)
    .bind(metadata.title_lock)
    .bind(&metadata.summary)
    .bind(metadata.summary_lock)
    .bind(&metadata.number)
    .bind(metadata.number_lock)
    .bind(metadata.number_sort)
    .bind(metadata.number_sort_lock)
    .bind(metadata.release_date.as_deref())
    .bind(metadata.release_date_lock)
    .bind(metadata.authors_lock)
    .bind(metadata.tags_lock)
    .bind(&metadata.isbn)
    .bind(metadata.isbn_lock)
    .bind(metadata.links_lock)
    .bind(book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("update book metadata for refresh: {error}"))?;

    sqlx::query("UPDATE BOOK SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP WHERE ID = ?")
        .bind(book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("touch book last modified after metadata refresh: {error}"))?;

    sqlx::query("DELETE FROM BOOK_METADATA_AUTHOR WHERE BOOK_ID = ?")
        .bind(book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("delete existing book metadata authors for refresh: {error}"))?;
    for author in &metadata.authors {
        sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
            .bind(book_id)
            .bind(&author.name)
            .bind(&author.role)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("insert refreshed book metadata author: {error}"))?;
    }

    sqlx::query("DELETE FROM BOOK_METADATA_TAG WHERE BOOK_ID = ?")
        .bind(book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("delete existing book metadata tags for refresh: {error}"))?;
    for tag in &metadata.tags {
        sqlx::query("INSERT INTO BOOK_METADATA_TAG (BOOK_ID, TAG) VALUES (?, ?)")
            .bind(book_id)
            .bind(tag)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("insert refreshed book metadata tag: {error}"))?;
    }

    sqlx::query("DELETE FROM BOOK_METADATA_LINK WHERE BOOK_ID = ?")
        .bind(book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("delete existing book metadata links for refresh: {error}"))?;
    for link in &metadata.links {
        sqlx::query("INSERT INTO BOOK_METADATA_LINK (BOOK_ID, LABEL, URL) VALUES (?, ?, ?)")
            .bind(book_id)
            .bind(&link.label)
            .bind(&link.url)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("insert refreshed book metadata link: {error}"))?;
    }

    tx.commit()
        .await
        .map_err(|error| format!("commit book metadata refresh tx: {error}"))?;
    Ok(true)
}

fn extract_comicinfo_release_date(xml: &str) -> Option<String> {
    let year = extract_xml_tag(xml, "Year")?.parse::<i32>().ok()?;
    let month = extract_xml_tag(xml, "Month")
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(1);
    let day = extract_xml_tag(xml, "Day")
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(1);
    if !is_valid_calendar_date(year, month, day) {
        return None;
    }

    Some(format!("{year:04}-{month:02}-{day:02}"))
}

fn is_valid_calendar_date(year: i32, month: u8, day: u8) -> bool {
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => return false,
    };

    (1..=max_day).contains(&day)
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn extract_comicinfo_authors(xml: &str) -> Option<Vec<BookMetadataAuthor>> {
    let mut authors = Vec::new();

    for (tag, role) in [
        ("Writer", "writer"),
        ("Penciller", "penciller"),
        ("Inker", "inker"),
        ("Colorist", "colorist"),
        ("Letterer", "letterer"),
        ("CoverArtist", "cover"),
        ("Editor", "editor"),
        ("Translator", "translator"),
    ] {
        if let Some(value) = extract_xml_tag(xml, tag) {
            authors.extend(
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|name| BookMetadataAuthor {
                        name: name.to_string(),
                        role: role.to_string(),
                    }),
            );
        }
    }

    (!authors.is_empty()).then_some(authors)
}

fn extract_comicinfo_tags(xml: &str) -> Option<Vec<String>> {
    let mut tags = extract_xml_tag(xml, "Tags")?
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .collect::<Vec<_>>();
    tags.sort();
    tags.dedup();

    (!tags.is_empty()).then_some(tags)
}

fn extract_comicinfo_isbn(xml: &str) -> Option<String> {
    normalize_isbn13(&extract_xml_tag(xml, "GTIN")?)
}

fn extract_comicinfo_links(xml: &str) -> Option<Vec<BookMetadataLink>> {
    let links = extract_xml_tag(xml, "Web")?
        .split(' ')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter_map(|value| {
            let uri = value.parse::<Uri>().ok()?;
            let scheme = uri.scheme_str()?;
            if !matches!(scheme, "http" | "https") {
                return None;
            }
            Some(BookMetadataLink {
                label: uri.host()?.to_string(),
                url: value.to_string(),
            })
        })
        .collect::<Vec<_>>();

    (!links.is_empty()).then_some(links)
}

fn normalize_isbn13(value: &str) -> Option<String> {
    let digits = value
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect::<String>();
    if digits.len() != 13 {
        return None;
    }

    let checksum = digits
        .chars()
        .take(12)
        .enumerate()
        .map(|(index, digit)| {
            let digit = digit.to_digit(10).unwrap_or_default();
            if index % 2 == 0 { digit } else { digit * 3 }
        })
        .sum::<u32>();
    let expected_check_digit = (10 - (checksum % 10)) % 10;
    let actual_check_digit = digits.chars().nth(12)?.to_digit(10)?;

    (actual_check_digit == expected_check_digit).then_some(digits)
}

fn extract_comicinfo_readlists(xml: &str) -> Vec<ComicInfoReadListEntry> {
    let mut readlists = Vec::new();

    if let Some(alternate_series) = extract_xml_tag(xml, "AlternateSeries") {
        readlists.push(ComicInfoReadListEntry {
            number: extract_xml_tag(xml, "AlternateNumber").and_then(|value| value.parse().ok()),
            name: alternate_series,
        });
    }

    if let Some(story_arc) = extract_xml_tag(xml, "StoryArc") {
        let arcs = story_arc
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        let numbers = extract_xml_tag(xml, "StoryArcNumber")
            .map(|numbers| {
                numbers
                    .split(',')
                    .map(|value| value.trim().parse::<i64>().ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if numbers.is_empty() {
            readlists.extend(
                arcs.into_iter()
                    .map(|name| ComicInfoReadListEntry { name, number: None }),
            );
        } else {
            for (name, number) in arcs.into_iter().zip(numbers.into_iter()) {
                if let Some(number) = number {
                    readlists.push(ComicInfoReadListEntry {
                        name,
                        number: Some(number),
                    });
                }
            }
        }
    }

    readlists
}

async fn upsert_comicinfo_readlist(
    pool: &SqlitePool,
    book_id: &str,
    readlist: ComicInfoReadListEntry,
) -> Result<Option<String>, String> {
    let readlist_id = sqlx::query("SELECT ID FROM READLIST WHERE NAME = ? LIMIT 1")
        .bind(&readlist.name)
        .fetch_optional(pool)
        .await
        .map_err(|error| {
            format!(
                "failed to load readlist '{}' for '{}': {error}",
                readlist.name, book_id
            )
        })?
        .map(|row| row.get::<String, _>("ID"));

    let readlist_id = match readlist_id {
        Some(readlist_id) => readlist_id,
        None => {
            let generated_id = generated_readlist_id(&readlist.name);
            sqlx::query(
                "INSERT INTO READLIST (ID, NAME, BOOK_COUNT, SUMMARY, ORDERED) VALUES (?, ?, 0, '', 1)",
            )
            .bind(&generated_id)
            .bind(&readlist.name)
            .execute(pool)
            .await
            .map_err(|error| {
                format!(
                    "failed to create ComicInfo readlist '{}' for '{}': {error}",
                    readlist.name, book_id,
                )
            })?;
            generated_id
        }
    };

    let book_already_in_readlist =
        sqlx::query("SELECT 1 FROM READLIST_BOOK WHERE READLIST_ID = ? AND BOOK_ID = ? LIMIT 1")
            .bind(&readlist_id)
            .bind(book_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| {
                format!(
                    "failed to check ComicInfo readlist membership '{}' for '{}': {error}",
                    readlist.name, book_id,
                )
            })?
            .is_some();
    if book_already_in_readlist {
        return Ok(None);
    }

    let assigned_number = assign_comicinfo_readlist_number(pool, &readlist_id, readlist.number)
        .await
        .map_err(|error| {
            format!(
                "failed to assign ComicInfo readlist number '{}' for '{}': {error}",
                readlist.name, book_id,
            )
        })?;

    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind(&readlist_id)
        .bind(book_id)
        .bind(assigned_number)
        .execute(pool)
        .await
        .map_err(|error| {
            format!(
                "failed to insert ComicInfo readlist membership '{}' for '{}': {error}",
                readlist.name, book_id,
            )
        })?;

    sqlx::query(
        "UPDATE READLIST SET BOOK_COUNT = (SELECT COUNT(*) FROM READLIST_BOOK WHERE READLIST_ID = ?), LAST_MODIFIED_DATE = CURRENT_TIMESTAMP WHERE ID = ?",
    )
    .bind(&readlist_id)
    .bind(&readlist_id)
    .execute(pool)
    .await
    .map_err(|error| {
        format!(
            "failed to update ComicInfo readlist counters '{}' for '{}': {error}",
            readlist.name, book_id,
        )
    })?;

    Ok(Some(readlist_id))
}

async fn assign_comicinfo_readlist_number(
    pool: &SqlitePool,
    readlist_id: &str,
    requested_number: Option<i64>,
) -> Result<i64, String> {
    let max_number = sqlx::query(
        "SELECT COALESCE(MAX(NUMBER), -1) AS MAX_NUMBER FROM READLIST_BOOK WHERE READLIST_ID = ?",
    )
    .bind(readlist_id)
    .fetch_one(pool)
    .await
    .map_err(|error| format!("query ComicInfo readlist max position: {error}"))?
    .get::<i64, _>("MAX_NUMBER");

    let Some(requested_number) = requested_number else {
        return Ok(max_number + 1);
    };

    let number_taken =
        sqlx::query("SELECT 1 FROM READLIST_BOOK WHERE READLIST_ID = ? AND NUMBER = ? LIMIT 1")
            .bind(readlist_id)
            .bind(requested_number)
            .fetch_optional(pool)
            .await
            .map_err(|error| format!("query ComicInfo readlist position collision: {error}"))?
            .is_some();

    if number_taken {
        Ok(max_number + 1)
    } else {
        Ok(requested_number)
    }
}

fn generated_readlist_id(name: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let slug = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    format!("readlist-{slug}-{timestamp:x}")
}

fn generated_collection_id(name: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let slug = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    format!("collection-{slug}-{timestamp:x}")
}

fn most_frequent_owned<T>(values: impl IntoIterator<Item = T>) -> Option<T>
where
    T: Eq + Hash + Clone,
{
    let mut counts = HashMap::<T, (usize, usize)>::new();

    for (index, value) in values.into_iter().enumerate() {
        let entry = counts.entry(value).or_insert((0, index));
        entry.0 += 1;
    }

    counts
        .into_iter()
        .max_by(|(_, (count_a, index_a)), (_, (count_b, index_b))| {
            count_a.cmp(count_b).then_with(|| index_b.cmp(index_a))
        })
        .map(|(value, _)| value)
}

fn dedupe_strings_preserve_order(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut output = Vec::new();

    for value in values {
        if !output.iter().any(|existing| existing == &value) {
            output.push(value);
        }
    }

    output
}

fn canonicalize_string_set(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut output = dedupe_strings_preserve_order(values);
    output.sort_by(|left, right| {
        left.to_ascii_lowercase()
            .cmp(&right.to_ascii_lowercase())
            .then_with(|| left.cmp(right))
    });
    output
}

fn split_comicinfo_list(value: Option<String>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split(',')
        .filter_map(|entry| nonblank_string(entry.to_string()))
        .collect::<Vec<_>>()
}

fn compute_series_from_series_and_volume(
    series: Option<String>,
    volume: Option<i64>,
) -> Option<String> {
    let series = series.and_then(nonblank_string)?;
    Some(match volume {
        Some(1) | None => series,
        Some(volume) => format!("{series} ({volume})"),
    })
}

fn normalize_optional_bcp47_language(value: Option<String>) -> Option<String> {
    let value = value.and_then(nonblank_string)?;
    let tag = LanguageTag::parse(&value).ok()?;
    let primary_language = tag.primary_language();

    if !(2..=3).contains(&primary_language.len())
        || primary_language
            .bytes()
            .any(|byte| !byte.is_ascii_alphabetic())
        || tag.validate().is_err()
        || (primary_language >= "qaa" && primary_language <= "qtz")
    {
        return None;
    }

    Some(tag.into_string())
}

fn normalize_comicinfo_age_rating(value: &str) -> Option<u32> {
    let normalized = value.trim().to_ascii_lowercase().replace(' ', "");

    match normalized.as_str() {
        "adultsonly18+" | "r18+" | "x18+" => Some(18),
        "earlychildhood" => Some(3),
        "everyone" | "g" => Some(0),
        "everyone10+" => Some(10),
        "kidstoadults" => Some(6),
        "m" | "mature17+" => Some(17),
        "ma15+" => Some(15),
        "pg" => Some(8),
        "teen" => Some(13),
        _ => None,
    }
}

fn mylar_age_rating_value(value: MylarAgeRating) -> u32 {
    match value {
        MylarAgeRating::All => 0,
        MylarAgeRating::Nine => 9,
        MylarAgeRating::Twelve => 12,
        MylarAgeRating::Fifteen => 15,
        MylarAgeRating::Seventeen => 17,
        MylarAgeRating::Adult => 18,
    }
}

fn load_mylar_series_patch(series_dir: &Path) -> Option<SeriesMetadataImportPatch> {
    let series_json_path = series_dir.join("series.json");
    let json = fs::read_to_string(&series_json_path).ok()?;
    let metadata = serde_json::from_str::<MylarSeriesFile>(&json)
        .ok()?
        .metadata;
    let title = if metadata.volume.is_none() || metadata.volume == Some(1) {
        metadata.name
    } else {
        format!("{} ({})", metadata.name, metadata.year)
    };

    Some(SeriesMetadataImportPatch {
        title: Some(title.clone()),
        title_sort: Some(title),
        status: Some(match metadata.status {
            MylarStatus::Ended => "ENDED".to_string(),
            MylarStatus::Continuing => "ONGOING".to_string(),
        }),
        summary: match metadata.description_formatted {
            Some(summary) => Some(summary),
            None => metadata.description_text,
        },
        reading_direction: None,
        publisher: Some(metadata.publisher),
        age_rating: metadata.age_rating.map(mylar_age_rating_value),
        language: None,
        genres: None,
        total_book_count: u32::try_from(metadata.total_issues).ok(),
        collections: Vec::new(),
    })
}

fn extract_comicinfo_series_patch(
    xml: &str,
    append_volume_to_title: bool,
) -> SeriesMetadataImportPatch {
    let series = if append_volume_to_title {
        compute_series_from_series_and_volume(
            extract_xml_tag(xml, "Series"),
            extract_xml_tag(xml, "Volume").and_then(|value| value.parse::<i64>().ok()),
        )
    } else {
        extract_xml_tag(xml, "Series")
    };
    let genres = canonicalize_string_set(split_comicinfo_list(extract_xml_tag(xml, "Genre")));
    let collections =
        dedupe_strings_preserve_order(split_comicinfo_list(extract_xml_tag(xml, "SeriesGroup")));

    SeriesMetadataImportPatch {
        title: series.clone(),
        title_sort: series,
        status: None,
        summary: None,
        reading_direction: match extract_xml_tag(xml, "Manga")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "no" => Some("LEFT_TO_RIGHT".to_string()),
            "yesandrighttoleft" => Some("RIGHT_TO_LEFT".to_string()),
            _ => None,
        },
        publisher: extract_xml_tag(xml, "Publisher"),
        age_rating: extract_xml_tag(xml, "AgeRating")
            .as_deref()
            .and_then(normalize_comicinfo_age_rating),
        language: normalize_optional_bcp47_language(extract_xml_tag(xml, "LanguageISO")),
        genres: (!genres.is_empty()).then_some(genres),
        total_book_count: extract_xml_tag(xml, "Count")
            .and_then(|value| value.parse::<i64>().ok())
            .and_then(|value| u32::try_from(value).ok()),
        collections,
    }
}

#[derive(Clone, Copy)]
enum EpubSeriesTextTarget {
    Collection,
    Publisher,
    Language,
    Subject,
}

fn extract_epub_series_patch(package_document: &[u8]) -> SeriesMetadataImportPatch {
    let mut reader = XmlReader::from_reader(package_document);
    reader.config_mut().trim_text(true);

    let mut buffer = Vec::new();
    let mut current_target = None::<EpubSeriesTextTarget>;
    let mut current_text = String::new();

    let mut collection = None::<String>;
    let mut publisher = None::<String>;
    let mut language = None::<String>;
    let mut genres = Vec::<String>::new();
    let mut reading_direction = None::<String>;

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(XmlEvent::Start(event)) => {
                if xml_name_matches_local(event.name().as_ref(), b"spine") {
                    reading_direction = match attribute_value(&event, b"page-progression-direction")
                        .unwrap_or_default()
                        .trim()
                        .to_ascii_lowercase()
                        .as_str()
                    {
                        "rtl" => Some("RIGHT_TO_LEFT".to_string()),
                        "ltr" => Some("LEFT_TO_RIGHT".to_string()),
                        _ => reading_direction,
                    };
                } else if xml_name_matches_local(event.name().as_ref(), b"meta") {
                    if let Some(target) = epub_series_text_target_from_meta(&event) {
                        if let Some(value) =
                            attribute_value(&event, b"content").and_then(nonblank_string)
                        {
                            apply_epub_series_text_target(
                                target,
                                value,
                                &mut collection,
                                &mut publisher,
                                &mut language,
                                &mut genres,
                            );
                        } else {
                            current_target = Some(target);
                            current_text.clear();
                        }
                    }
                } else if let Some(target) = epub_series_text_target_from_start(&event) {
                    current_target = Some(target);
                    current_text.clear();
                }
            }
            Ok(XmlEvent::Empty(event)) => {
                if xml_name_matches_local(event.name().as_ref(), b"spine") {
                    reading_direction = match attribute_value(&event, b"page-progression-direction")
                        .unwrap_or_default()
                        .trim()
                        .to_ascii_lowercase()
                        .as_str()
                    {
                        "rtl" => Some("RIGHT_TO_LEFT".to_string()),
                        "ltr" => Some("LEFT_TO_RIGHT".to_string()),
                        _ => reading_direction,
                    };
                } else if xml_name_matches_local(event.name().as_ref(), b"meta")
                    && let Some(target) = epub_series_text_target_from_meta(&event)
                    && let Some(value) =
                        attribute_value(&event, b"content").and_then(nonblank_string)
                {
                    apply_epub_series_text_target(
                        target,
                        value,
                        &mut collection,
                        &mut publisher,
                        &mut language,
                        &mut genres,
                    );
                }
            }
            Ok(XmlEvent::Text(text)) if current_target.is_some() => {
                current_text.push_str(String::from_utf8_lossy(text.as_ref()).as_ref());
            }
            Ok(XmlEvent::CData(text)) if current_target.is_some() => {
                current_text.push_str(String::from_utf8_lossy(text.as_ref()).as_ref());
            }
            Ok(XmlEvent::End(event))
                if epub_series_text_target_matches_end(
                    current_target.as_ref(),
                    event.name().as_ref(),
                ) =>
            {
                let target = current_target
                    .take()
                    .expect("epub series target should exist");
                apply_epub_series_text_target(
                    target,
                    current_text.trim().to_string(),
                    &mut collection,
                    &mut publisher,
                    &mut language,
                    &mut genres,
                );
                current_text.clear();
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }

        buffer.clear();
    }

    let genres = canonicalize_string_set(genres);

    SeriesMetadataImportPatch {
        title: collection.clone(),
        title_sort: collection,
        status: None,
        summary: None,
        reading_direction,
        publisher,
        age_rating: None,
        language: normalize_optional_bcp47_language(language),
        genres: (!genres.is_empty()).then_some(genres),
        total_book_count: None,
        collections: Vec::new(),
    }
}

fn epub_series_text_target_from_meta(event: &XmlBytesStart<'_>) -> Option<EpubSeriesTextTarget> {
    match attribute_value(event, b"property")?
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "belongs-to-collection" => Some(EpubSeriesTextTarget::Collection),
        _ => None,
    }
}

fn epub_series_text_target_from_start(event: &XmlBytesStart<'_>) -> Option<EpubSeriesTextTarget> {
    let name = event.name();
    let name = name.as_ref();
    if xml_name_matches_local(name, b"publisher") {
        Some(EpubSeriesTextTarget::Publisher)
    } else if xml_name_matches_local(name, b"language") {
        Some(EpubSeriesTextTarget::Language)
    } else if xml_name_matches_local(name, b"subject") {
        Some(EpubSeriesTextTarget::Subject)
    } else {
        None
    }
}

fn epub_series_text_target_matches_end(target: Option<&EpubSeriesTextTarget>, name: &[u8]) -> bool {
    match target {
        Some(EpubSeriesTextTarget::Collection) => xml_name_matches_local(name, b"meta"),
        Some(EpubSeriesTextTarget::Publisher) => xml_name_matches_local(name, b"publisher"),
        Some(EpubSeriesTextTarget::Language) => xml_name_matches_local(name, b"language"),
        Some(EpubSeriesTextTarget::Subject) => xml_name_matches_local(name, b"subject"),
        None => false,
    }
}

fn apply_epub_series_text_target(
    target: EpubSeriesTextTarget,
    value: String,
    collection: &mut Option<String>,
    publisher: &mut Option<String>,
    language: &mut Option<String>,
    genres: &mut Vec<String>,
) {
    let Some(value) = nonblank_string(value) else {
        return;
    };

    match target {
        EpubSeriesTextTarget::Collection => {
            if collection.is_none() {
                *collection = Some(value);
            }
        }
        EpubSeriesTextTarget::Publisher => {
            if publisher.is_none() {
                *publisher = Some(value);
            }
        }
        EpubSeriesTextTarget::Language => {
            if language.is_none() {
                *language = Some(value);
            }
        }
        EpubSeriesTextTarget::Subject => genres.push(value),
    }
}

async fn load_series_books_for_refresh(
    pool: &SqlitePool,
    series_id: &str,
    library_root: &Path,
) -> Result<Vec<SeriesBookRefreshSource>, String> {
    let rows = sqlx::query(
        r#"
        SELECT b.LIBRARY_ID AS LIBRARY_ID,
               b.NAME AS FILE_NAME,
               b.URL AS BOOK_URL,
               COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
               COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT
        FROM BOOK b
        LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
        WHERE b.SERIES_ID = ?
        "#,
    )
    .bind(series_id)
    .fetch_all(pool)
    .await
    .map_err(|error| {
        format!("failed to load series books for metadata refresh '{series_id}': {error}")
    })?;

    Ok(rows
        .into_iter()
        .map(|row| SeriesBookRefreshSource {
            media: BookMediaRecord {
                library_id: row.get::<String, _>("LIBRARY_ID"),
                media_type: row.get::<String, _>("MEDIA_TYPE"),
                file_path: library_root.join(row.get::<String, _>("BOOK_URL")),
                file_name: row.get::<String, _>("FILE_NAME"),
                page_count: row.get::<i64, _>("PAGE_COUNT").max(0) as u64,
            },
        })
        .collect())
}

fn load_archive_entry_bytes(archive_path: &Path, entry_name: &str) -> Option<Vec<u8>> {
    let file = fs::File::open(archive_path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut entry = archive.by_name(entry_name).ok()?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

fn load_comicinfo_series_patch_for_book(
    source: &SeriesBookRefreshSource,
    append_volume_to_title: bool,
) -> Option<SeriesMetadataImportPatch> {
    let xml = load_archive_entry_bytes(source.media.file_path.as_path(), "ComicInfo.xml")?;
    let xml = String::from_utf8(xml).ok()?;
    Some(extract_comicinfo_series_patch(&xml, append_volume_to_title))
}

fn load_epub_series_patch_for_book(
    source: &SeriesBookRefreshSource,
) -> Option<SeriesMetadataImportPatch> {
    if !book_media_is_epub(&source.media) {
        return None;
    }

    let package_document = load_epub_package_document(&source.media)?;
    Some(extract_epub_series_patch(&package_document))
}

fn aggregate_series_metadata_import_patches(
    patches: &[SeriesMetadataImportPatch],
) -> Option<SeriesMetadataImportPatch> {
    if patches.is_empty() {
        return None;
    }

    let genres = canonicalize_string_set(
        patches
            .iter()
            .filter_map(|patch| patch.genres.clone())
            .flatten(),
    );
    let collections =
        dedupe_strings_preserve_order(patches.iter().flat_map(|patch| patch.collections.clone()));

    let aggregated = SeriesMetadataImportPatch {
        title: most_frequent_owned(patches.iter().filter_map(|patch| patch.title.clone())),
        title_sort: most_frequent_owned(
            patches.iter().filter_map(|patch| patch.title_sort.clone()),
        ),
        status: most_frequent_owned(patches.iter().filter_map(|patch| patch.status.clone())),
        summary: None,
        reading_direction: most_frequent_owned(
            patches
                .iter()
                .filter_map(|patch| patch.reading_direction.clone()),
        ),
        publisher: most_frequent_owned(patches.iter().filter_map(|patch| patch.publisher.clone())),
        age_rating: patches.iter().filter_map(|patch| patch.age_rating).max(),
        language: most_frequent_owned(patches.iter().filter_map(|patch| patch.language.clone())),
        genres: (!genres.is_empty()).then_some(genres),
        total_book_count: patches
            .iter()
            .filter_map(|patch| patch.total_book_count)
            .max(),
        collections,
    };

    (aggregated.title.is_some()
        || aggregated.title_sort.is_some()
        || aggregated.status.is_some()
        || aggregated.reading_direction.is_some()
        || aggregated.publisher.is_some()
        || aggregated.age_rating.is_some()
        || aggregated.language.is_some()
        || aggregated.genres.is_some()
        || aggregated.total_book_count.is_some()
        || !aggregated.collections.is_empty())
    .then_some(aggregated)
}

async fn load_series_metadata_refresh_state(
    pool: &SqlitePool,
    series_id: &str,
) -> Result<Option<SeriesMetadataRefreshState>, String> {
    let row = sqlx::query(
        r#"
        SELECT STATUS, STATUS_LOCK, TITLE, TITLE_LOCK, TITLE_SORT, TITLE_SORT_LOCK, SUMMARY,
               SUMMARY_LOCK, READING_DIRECTION, READING_DIRECTION_LOCK, PUBLISHER,
               PUBLISHER_LOCK, AGE_RATING, AGE_RATING_LOCK, LANGUAGE, LANGUAGE_LOCK,
               GENRES_LOCK, TOTAL_BOOK_COUNT, TOTAL_BOOK_COUNT_LOCK
        FROM SERIES_METADATA
        WHERE SERIES_ID = ?
        LIMIT 1
        "#,
    )
    .bind(series_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        format!("failed to load existing series metadata for '{series_id}': {error}")
    })?;

    let Some(row) = row else {
        return Ok(None);
    };

    let genres = sqlx::query(
        "SELECT GENRE FROM SERIES_METADATA_GENRE WHERE SERIES_ID = ? ORDER BY GENRE COLLATE NOCASE ASC",
    )
    .bind(series_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("failed to load existing series genres for '{series_id}': {error}"))?
    .into_iter()
    .map(|row| row.get::<String, _>("GENRE"))
    .collect::<Vec<_>>();

    Ok(Some(SeriesMetadataRefreshState {
        status: row.get::<String, _>("STATUS"),
        status_lock: row.get::<bool, _>("STATUS_LOCK"),
        title: row.get::<String, _>("TITLE"),
        title_lock: row.get::<bool, _>("TITLE_LOCK"),
        title_sort: row.get::<String, _>("TITLE_SORT"),
        title_sort_lock: row.get::<bool, _>("TITLE_SORT_LOCK"),
        summary: row.get::<String, _>("SUMMARY"),
        summary_lock: row.get::<bool, _>("SUMMARY_LOCK"),
        reading_direction: row.get::<Option<String>, _>("READING_DIRECTION"),
        reading_direction_lock: row.get::<bool, _>("READING_DIRECTION_LOCK"),
        publisher: row.get::<String, _>("PUBLISHER"),
        publisher_lock: row.get::<bool, _>("PUBLISHER_LOCK"),
        age_rating: row
            .get::<Option<i64>, _>("AGE_RATING")
            .map(|value| value.clamp(0, i64::from(i32::MAX)) as u32),
        age_rating_lock: row.get::<bool, _>("AGE_RATING_LOCK"),
        language: row.get::<String, _>("LANGUAGE"),
        language_lock: row.get::<bool, _>("LANGUAGE_LOCK"),
        genres,
        genres_lock: row.get::<bool, _>("GENRES_LOCK"),
        total_book_count: row
            .get::<Option<i64>, _>("TOTAL_BOOK_COUNT")
            .map(|value| value.clamp(0, i64::from(i32::MAX)) as u32),
        total_book_count_lock: row.get::<bool, _>("TOTAL_BOOK_COUNT_LOCK"),
    }))
}

async fn persist_series_metadata_refresh_state(
    pool: &SqlitePool,
    series_id: &str,
    state: &SeriesMetadataRefreshState,
) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|error| {
        format!("failed to begin series metadata refresh update tx for '{series_id}': {error}")
    })?;

    let updated = sqlx::query(
        r#"
        UPDATE SERIES_METADATA
        SET STATUS = ?,
            TITLE = ?,
            TITLE_SORT = ?,
            SUMMARY = ?,
            READING_DIRECTION = ?,
            PUBLISHER = ?,
            AGE_RATING = ?,
            LANGUAGE = ?,
            TOTAL_BOOK_COUNT = ?,
            LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
        WHERE SERIES_ID = ?
        "#,
    )
    .bind(&state.status)
    .bind(&state.title)
    .bind(&state.title_sort)
    .bind(&state.summary)
    .bind(state.reading_direction.as_deref())
    .bind(&state.publisher)
    .bind(state.age_rating.map(i64::from))
    .bind(&state.language)
    .bind(state.total_book_count.map(i64::from))
    .bind(series_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("failed to update series metadata for '{series_id}': {error}"))?
    .rows_affected()
        > 0;

    if !updated {
        tx.rollback().await.map_err(|error| {
            format!("failed to rollback missing series metadata update for '{series_id}': {error}")
        })?;
        return Err(format!(
            "series metadata row disappeared before metadata refresh for '{series_id}'"
        ));
    }

    sqlx::query("DELETE FROM SERIES_METADATA_GENRE WHERE SERIES_ID = ?")
        .bind(series_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("failed to clear series genres for '{series_id}': {error}"))?;

    for genre in &state.genres {
        sqlx::query("INSERT INTO SERIES_METADATA_GENRE (SERIES_ID, GENRE) VALUES (?, ?)")
            .bind(series_id)
            .bind(genre)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("failed to insert series genre for '{series_id}': {error}"))?;
    }

    tx.commit().await.map_err(|error| {
        format!("failed to commit series metadata refresh update for '{series_id}': {error}")
    })?;
    Ok(())
}

async fn apply_series_metadata_import_patch(
    pool: &SqlitePool,
    series_id: &str,
    patch: SeriesMetadataImportPatch,
) -> Result<(), String> {
    let Some(mut state) = load_series_metadata_refresh_state(pool, series_id).await? else {
        return Ok(());
    };

    let mut changed = false;

    if let Some(status) = patch.status
        && !state.status_lock
        && state.status != status
    {
        state.status = status;
        changed = true;
    }

    if let Some(title) = patch.title
        && !state.title_lock
        && state.title != title
    {
        state.title = title;
        changed = true;
    }

    if let Some(title_sort) = patch.title_sort
        && !state.title_sort_lock
        && state.title_sort != title_sort
    {
        state.title_sort = title_sort;
        changed = true;
    }

    if let Some(summary) = patch.summary
        && !state.summary_lock
        && state.summary != summary
    {
        state.summary = summary;
        changed = true;
    }

    if let Some(reading_direction) = patch.reading_direction
        && !state.reading_direction_lock
        && state.reading_direction.as_deref() != Some(reading_direction.as_str())
    {
        state.reading_direction = Some(reading_direction);
        changed = true;
    }

    if let Some(publisher) = patch.publisher
        && !state.publisher_lock
        && state.publisher != publisher
    {
        state.publisher = publisher;
        changed = true;
    }

    if let Some(age_rating) = patch.age_rating
        && !state.age_rating_lock
        && state.age_rating != Some(age_rating)
    {
        state.age_rating = Some(age_rating);
        changed = true;
    }

    if let Some(language) = patch.language
        && !state.language_lock
        && state.language != language
    {
        state.language = language;
        changed = true;
    }

    if let Some(genres) = patch.genres
        && !state.genres_lock
        && state.genres != genres
    {
        state.genres = genres;
        changed = true;
    }

    if let Some(total_book_count) = patch.total_book_count
        && !state.total_book_count_lock
        && state.total_book_count != Some(total_book_count)
    {
        state.total_book_count = Some(total_book_count);
        changed = true;
    }

    if changed {
        persist_series_metadata_refresh_state(pool, series_id, &state).await?;
    }

    Ok(())
}

async fn load_collection_membership_by_name(
    pool: &SqlitePool,
    collection_name: &str,
) -> Result<Option<PersistedCollectionMembership>, String> {
    let row = sqlx::query(
        r#"
        SELECT ID, NAME, ORDERED
        FROM COLLECTION
        WHERE NAME = ? COLLATE NOCASE
        LIMIT 1
        "#,
    )
    .bind(collection_name)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("failed to load collection by name '{collection_name}': {error}"))?;

    let Some(row) = row else {
        return Ok(None);
    };

    let collection_id = row.get::<String, _>("ID");
    let series_ids = sqlx::query(
        "SELECT SERIES_ID FROM COLLECTION_SERIES WHERE COLLECTION_ID = ? ORDER BY NUMBER ASC",
    )
    .bind(&collection_id)
    .fetch_all(pool)
    .await
    .map_err(|error| {
        format!("failed to load collection series ids for '{collection_name}': {error}")
    })?
    .into_iter()
    .map(|series_row| series_row.get::<String, _>("SERIES_ID"))
    .collect::<Vec<_>>();

    Ok(Some(PersistedCollectionMembership {
        id: collection_id,
        name: row.get::<String, _>("NAME"),
        ordered: row.get::<bool, _>("ORDERED"),
        series_ids,
    }))
}

async fn add_series_to_collection_for_refresh(
    pool: &SqlitePool,
    series_id: &str,
    collection_name: &str,
) -> Result<(), String> {
    let Some(collection_name) = nonblank_string(collection_name.to_string()) else {
        return Ok(());
    };

    if let Some(existing) = load_collection_membership_by_name(pool, &collection_name).await? {
        if existing
            .series_ids
            .iter()
            .any(|existing_series_id| existing_series_id == series_id)
        {
            return Ok(());
        }

        let mut tx = pool.begin().await.map_err(|error| {
            format!("failed to begin collection update tx for '{collection_name}': {error}")
        })?;

        sqlx::query(
            "UPDATE COLLECTION SET NAME = ?, ORDERED = ?, SERIES_COUNT = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP WHERE ID = ?",
        )
        .bind(&existing.name)
        .bind(existing.ordered)
        .bind((existing.series_ids.len() + 1) as i64)
        .bind(&existing.id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("failed to update collection '{collection_name}': {error}"))?;

        sqlx::query(
            "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
        )
        .bind(&existing.id)
        .bind(series_id)
        .bind(existing.series_ids.len() as i64)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            format!("failed to append series to collection '{collection_name}': {error}")
        })?;

        tx.commit().await.map_err(|error| {
            format!("failed to commit collection update for '{collection_name}': {error}")
        })?;
        return Ok(());
    }

    let collection_id = generated_collection_id(&collection_name);
    let mut tx = pool.begin().await.map_err(|error| {
        format!("failed to begin collection create tx for '{collection_name}': {error}")
    })?;

    sqlx::query(
        r#"
        INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT, CREATED_DATE, LAST_MODIFIED_DATE)
        VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        "#,
    )
    .bind(&collection_id)
    .bind(&collection_name)
    .bind(false)
    .bind(1_i64)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("failed to create collection '{collection_name}': {error}"))?;

    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind(&collection_id)
    .bind(series_id)
    .bind(0_i64)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        format!("failed to seed collection '{collection_name}' membership: {error}")
    })?;

    tx.commit().await.map_err(|error| {
        format!("failed to commit collection create for '{collection_name}': {error}")
    })?;
    Ok(())
}

async fn apply_series_collection_imports(
    pool: &SqlitePool,
    series_id: &str,
    collection_names: impl IntoIterator<Item = String>,
) -> Result<(), String> {
    for collection_name in dedupe_strings_preserve_order(collection_names) {
        add_series_to_collection_for_refresh(pool, series_id, &collection_name).await?;
    }

    Ok(())
}

async fn apply_mylar_series_import(
    pool: &SqlitePool,
    series_id: &str,
    library_root: &Path,
    series_url: &str,
    import_mylar_series: bool,
    oneshot: bool,
) -> Result<(), String> {
    if !import_mylar_series || oneshot {
        return Ok(());
    }

    let series_dir = library_root.join(series_url);
    let Some(patch) = load_mylar_series_patch(series_dir.as_path()) else {
        return Ok(());
    };

    apply_series_metadata_import_patch(pool, series_id, patch).await
}

async fn apply_series_metadata_from_book_imports(
    pool: &SqlitePool,
    series_id: &str,
    library_root: &Path,
    import_comicinfo_series: bool,
    import_comicinfo_collection: bool,
    import_comicinfo_series_append_volume: bool,
    import_epub_series: bool,
) -> Result<(), String> {
    if !(import_comicinfo_series || import_comicinfo_collection || import_epub_series) {
        return Ok(());
    }

    let books = load_series_books_for_refresh(pool, series_id, library_root).await?;

    if import_comicinfo_series || import_comicinfo_collection {
        let patches = books
            .iter()
            .filter_map(|source| {
                load_comicinfo_series_patch_for_book(source, import_comicinfo_series_append_volume)
            })
            .collect::<Vec<_>>();

        if import_comicinfo_series
            && let Some(aggregated) = aggregate_series_metadata_import_patches(&patches)
        {
            apply_series_metadata_import_patch(pool, series_id, aggregated).await?;
        }

        if import_comicinfo_collection {
            apply_series_collection_imports(
                pool,
                series_id,
                patches.iter().flat_map(|patch| patch.collections.clone()),
            )
            .await?;
        }
    }

    if import_epub_series {
        let patches = books
            .iter()
            .filter_map(load_epub_series_patch_for_book)
            .collect::<Vec<_>>();

        if let Some(aggregated) = aggregate_series_metadata_import_patches(&patches) {
            apply_series_metadata_import_patch(pool, series_id, aggregated).await?;
        }
    }

    Ok(())
}

pub fn refresh_series_metadata(database_file: &Path, series_id: &str) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let series_id = series_id.to_string();

    run_database_query(database_file, move |pool| {
        let series_id = series_id.clone();
        Box::pin(async move {
            let series_row = sqlx::query(
                r#"
                SELECT s.URL AS SERIES_URL,
                       l.ROOT AS LIBRARY_ROOT,
                       COALESCE(s.ONESHOT, 0) AS ONESHOT,
                       l.IMPORT_COMICINFO_SERIES AS IMPORT_COMICINFO_SERIES,
                       l.IMPORT_COMICINFO_COLLECTION AS IMPORT_COMICINFO_COLLECTION,
                       l.IMPORT_COMICINFO_SERIES_APPEND_VOLUME AS IMPORT_COMICINFO_SERIES_APPEND_VOLUME,
                       l.IMPORT_EPUB_SERIES AS IMPORT_EPUB_SERIES,
                       l.IMPORT_MYLAR_SERIES AS IMPORT_MYLAR_SERIES
                FROM SERIES s
                JOIN LIBRARY l ON l.ID = s.LIBRARY_ID
                WHERE s.ID = ?
                LIMIT 1
                "#,
            )
            .bind(&series_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!("failed to resolve series path for metadata refresh '{series_id}': {error}")
            })?;

            if let Some(series_row) = &series_row {
                let series_url = series_row.get::<String, _>("SERIES_URL");
                let library_root = series_row.get::<String, _>("LIBRARY_ROOT");
                let oneshot = series_row.get::<i64, _>("ONESHOT") != 0;
                let import_comicinfo_series = series_row.get::<bool, _>("IMPORT_COMICINFO_SERIES");
                let import_comicinfo_collection =
                    series_row.get::<bool, _>("IMPORT_COMICINFO_COLLECTION");
                let import_comicinfo_series_append_volume =
                    series_row.get::<bool, _>("IMPORT_COMICINFO_SERIES_APPEND_VOLUME");
                let import_epub_series = series_row.get::<bool, _>("IMPORT_EPUB_SERIES");
                let import_mylar_series = series_row.get::<bool, _>("IMPORT_MYLAR_SERIES");

                apply_series_metadata_from_book_imports(
                    &pool,
                    &series_id,
                    Path::new(&library_root),
                    import_comicinfo_series,
                    import_comicinfo_collection,
                    import_comicinfo_series_append_volume,
                    import_epub_series,
                )
                .await?;

                apply_mylar_series_import(
                    &pool,
                    &series_id,
                    Path::new(&library_root),
                    &series_url,
                    import_mylar_series,
                    oneshot,
                )
                .await?;

                if oneshot {
                    apply_oneshot_series_metadata_import(&pool, &series_id).await?;
                }
            }

            sqlx::query(
                r#"
                UPDATE SERIES_METADATA
                SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE SERIES_ID = ?
                "#,
            )
            .bind(&series_id)
            .execute(&pool)
            .await
            .map_err(|error| {
                format!("failed to refresh SERIES_METADATA for '{series_id}': {error}")
            })?;

            sqlx::query(
                r#"
                UPDATE SERIES
                SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE ID = ?
                "#,
            )
            .bind(&series_id)
            .execute(&pool)
            .await
            .map_err(|error| format!("failed to refresh SERIES row for '{series_id}': {error}"))?;

            Ok(())
        })
    })
}

async fn apply_oneshot_series_metadata_import(
    pool: &SqlitePool,
    series_id: &str,
) -> Result<(), String> {
    let book_row = sqlx::query(
        r#"
        SELECT bm.TITLE AS TITLE,
               bm.SUMMARY AS SUMMARY
        FROM BOOK b
        JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
        WHERE b.SERIES_ID = ?
        LIMIT 1
        "#,
    )
    .bind(series_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        format!("failed to load oneshot series source book metadata for '{series_id}': {error}")
    })?;

    let Some(book_row) = book_row else {
        return Ok(());
    };

    let title = book_row.get::<String, _>("TITLE");
    let summary = book_row.get::<String, _>("SUMMARY");

    sqlx::query(
        r#"
        UPDATE SERIES_METADATA
        SET TITLE = CASE WHEN TITLE_LOCK = 0 THEN ? ELSE TITLE END,
            TITLE_SORT = CASE WHEN TITLE_SORT_LOCK = 0 THEN ? ELSE TITLE_SORT END,
            STATUS = CASE WHEN STATUS_LOCK = 0 THEN 'ENDED' ELSE STATUS END,
            SUMMARY = CASE WHEN SUMMARY_LOCK = 0 THEN ? ELSE SUMMARY END,
            TOTAL_BOOK_COUNT = CASE WHEN TOTAL_BOOK_COUNT_LOCK = 0 THEN 1 ELSE TOTAL_BOOK_COUNT END,
            LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
        WHERE SERIES_ID = ?
        "#,
    )
    .bind(&title)
    .bind(&title)
    .bind(&summary)
    .bind(series_id)
    .execute(pool)
    .await
    .map_err(|error| {
        format!("failed to apply oneshot series metadata import for '{series_id}': {error}")
    })?;

    Ok(())
}

pub fn aggregate_series_metadata(database_file: &Path, series_id: &str) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let series_id = series_id.to_string();

    run_database_query(database_file, move |pool| {
        let series_id = series_id.clone();
        Box::pin(async move {
            let mut tx = pool.begin().await.map_err(|error| {
                format!(
                    "failed to start series metadata aggregation transaction for '{series_id}': {error}"
                )
            })?;

            let row = sqlx::query(
                r#"
                SELECT ID
                FROM SERIES
                WHERE ID = ?
                LIMIT 1
                "#,
            )
            .bind(&series_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|error| {
                format!("failed to load series for aggregation '{series_id}': {error}")
            })?;

            let Some(row) = row else {
                return Ok(());
            };

            let _series_id = row.get::<String, _>("ID");
            let aggregate = load_series_book_metadata_aggregate(&mut tx, &series_id).await?;

            sqlx::query(
                r#"
                INSERT INTO BOOK_METADATA_AGGREGATION (
                    SERIES_ID,
                    RELEASE_DATE,
                    SUMMARY,
                    SUMMARY_NUMBER,
                    LAST_MODIFIED_DATE
                )
                VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP)
                ON CONFLICT(SERIES_ID) DO UPDATE SET
                    RELEASE_DATE = excluded.RELEASE_DATE,
                    SUMMARY = excluded.SUMMARY,
                    SUMMARY_NUMBER = excluded.SUMMARY_NUMBER,
                    LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                "#,
            )
            .bind(&series_id)
            .bind(aggregate.release_date.as_deref())
            .bind(&aggregate.summary)
            .bind(&aggregate.summary_number)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                format!("failed to upsert BOOK_METADATA_AGGREGATION for '{series_id}': {error}")
            })?;

            sqlx::query("DELETE FROM BOOK_METADATA_AGGREGATION_AUTHOR WHERE SERIES_ID = ?")
                .bind(&series_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| {
                    format!(
                        "failed to clear BOOK_METADATA_AGGREGATION_AUTHOR for '{series_id}': {error}"
                    )
                })?;

            for author in aggregate.authors {
                sqlx::query(
                    r#"
                    INSERT INTO BOOK_METADATA_AGGREGATION_AUTHOR (SERIES_ID, NAME, ROLE)
                    VALUES (?, ?, ?)
                    "#,
                )
                .bind(&series_id)
                .bind(author.name)
                .bind(author.role)
                .execute(&mut *tx)
                .await
                .map_err(|error| {
                    format!(
                        "failed to populate BOOK_METADATA_AGGREGATION_AUTHOR for '{series_id}': {error}"
                    )
                })?;
            }

            sqlx::query("DELETE FROM BOOK_METADATA_AGGREGATION_TAG WHERE SERIES_ID = ?")
                .bind(&series_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| {
                    format!(
                        "failed to clear BOOK_METADATA_AGGREGATION_TAG for '{series_id}': {error}"
                    )
                })?;

            for tag in aggregate.tags {
                sqlx::query(
                    r#"
                    INSERT INTO BOOK_METADATA_AGGREGATION_TAG (SERIES_ID, TAG)
                    VALUES (?, ?)
                    "#,
                )
                .bind(&series_id)
                .bind(tag)
                .execute(&mut *tx)
                .await
                .map_err(|error| {
                    format!(
                        "failed to populate BOOK_METADATA_AGGREGATION_TAG for '{series_id}': {error}"
                    )
                })?;
            }

            sqlx::query(
                r#"
                UPDATE SERIES
                SET BOOK_COUNT = (
                        SELECT COUNT(*)
                        FROM BOOK
                        WHERE BOOK.SERIES_ID = SERIES.ID
                          AND BOOK.DELETED_DATE IS NULL
                    ),
                    LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE ID = ?
                "#,
            )
            .bind(&series_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                format!("failed to aggregate SERIES counters for '{series_id}': {error}")
            })?;

            tx.commit().await.map_err(|error| {
                format!(
                    "failed to commit series metadata aggregation transaction for '{series_id}': {error}"
                )
            })?;

            Ok(())
        })
    })
}

#[derive(Default)]
struct SeriesBookMetadataAggregate {
    authors: Vec<AggregatedAuthor>,
    tags: Vec<String>,
    release_date: Option<String>,
    summary: String,
    summary_number: String,
}

struct AggregatedAuthor {
    name: String,
    role: String,
}

async fn load_series_book_metadata_aggregate(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    series_id: &str,
) -> Result<SeriesBookMetadataAggregate, String> {
    let metadata_rows = sqlx::query(
        r#"
        SELECT COALESCE(bm.NUMBER, '') AS NUMBER,
               bm.NUMBER_SORT AS NUMBER_SORT,
               COALESCE(bm.SUMMARY, '') AS SUMMARY,
               bm.RELEASE_DATE AS RELEASE_DATE
        FROM BOOK b
        JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
        WHERE b.SERIES_ID = ?
          AND b.DELETED_DATE IS NULL
        ORDER BY bm.NUMBER_SORT ASC, b.ID ASC
        "#,
    )
    .bind(series_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| format!("failed to load book metadata rows for '{series_id}': {error}"))?;

    let mut summary = String::new();
    let mut summary_number = String::new();
    let mut release_date: Option<String> = None;

    for row in metadata_rows {
        let row_summary = row.get::<String, _>("SUMMARY");
        if summary.is_empty() && !row_summary.trim().is_empty() {
            summary = row_summary;
            summary_number = row.get::<String, _>("NUMBER");
        }

        if let Some(row_release_date) = row.get::<Option<String>, _>("RELEASE_DATE") {
            if release_date
                .as_ref()
                .is_none_or(|current| row_release_date < *current)
            {
                release_date = Some(row_release_date);
            }
        }
    }

    let author_rows = sqlx::query(
        r#"
        SELECT bmaa.NAME AS NAME,
               bmaa.ROLE AS ROLE
        FROM BOOK b
        JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
        JOIN BOOK_METADATA_AUTHOR bmaa ON bmaa.BOOK_ID = b.ID
        WHERE b.SERIES_ID = ?
          AND b.DELETED_DATE IS NULL
        ORDER BY bm.NUMBER_SORT ASC, b.ID ASC, bmaa.ROWID ASC
        "#,
    )
    .bind(series_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| format!("failed to load aggregated authors for '{series_id}': {error}"))?;

    let mut authors = Vec::new();
    let mut seen_authors = std::collections::HashSet::new();
    for row in author_rows {
        let name = row.get::<String, _>("NAME");
        let role = row.get::<String, _>("ROLE");
        let dedupe_key = format!("{role}__{name}");
        if seen_authors.insert(dedupe_key) {
            authors.push(AggregatedAuthor { name, role });
        }
    }

    let tag_rows = sqlx::query(
        r#"
        SELECT bmt.TAG AS TAG
        FROM BOOK b
        JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
        JOIN BOOK_METADATA_TAG bmt ON bmt.BOOK_ID = b.ID
        WHERE b.SERIES_ID = ?
          AND b.DELETED_DATE IS NULL
        ORDER BY bm.NUMBER_SORT ASC, b.ID ASC, bmt.ROWID ASC
        "#,
    )
    .bind(series_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| format!("failed to load aggregated tags for '{series_id}': {error}"))?;

    let mut tags = Vec::new();
    let mut seen_tags = std::collections::HashSet::new();
    for row in tag_rows {
        let tag = row.get::<String, _>("TAG");
        if seen_tags.insert(tag.clone()) {
            tags.push(tag);
        }
    }

    Ok(SeriesBookMetadataAggregate {
        authors,
        tags,
        release_date,
        summary,
        summary_number,
    })
}

pub fn refresh_book_local_artwork(database_file: &Path, book_id: &str) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let book_id = book_id.to_string();

    run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        Box::pin(async move {
            let book_row = sqlx::query(
                r#"
                SELECT b.URL AS BOOK_URL, l.ROOT AS LIBRARY_ROOT
                FROM BOOK b
                JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
                WHERE b.ID = ?
                LIMIT 1
                "#,
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!("failed to resolve book path for artwork refresh '{book_id}': {error}")
            })?;

            if let Some(book_row) = &book_row {
                let import_local_artwork = sqlx::query(
                    r#"
                    SELECT l.IMPORT_LOCAL_ARTWORK AS IMPORT_LOCAL_ARTWORK
                    FROM BOOK b
                    JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
                    WHERE b.ID = ?
                    LIMIT 1
                    "#,
                )
                .bind(&book_id)
                .fetch_optional(&pool)
                .await
                .map_err(|error| {
                    format!("failed to resolve import-local-artwork flag for '{book_id}': {error}")
                })?
                .map(|row| row.get::<bool, _>("IMPORT_LOCAL_ARTWORK"))
                .unwrap_or(false);
                if !import_local_artwork {
                    return Ok(());
                }
                let book_url = book_row.get::<String, _>("BOOK_URL");
                let library_root = PathBuf::from(book_row.get::<String, _>("LIBRARY_ROOT"));
                for (index, artwork_url) in load_book_local_artwork_urls(&library_root, &book_url)?
                    .into_iter()
                    .enumerate()
                {
                    import_book_local_artwork_thumbnail(
                        &pool,
                        &book_id,
                        &library_root,
                        &artwork_url,
                        if index == 0 {
                            MarkSelectedPreference::IfNoneOrGenerated
                        } else {
                            MarkSelectedPreference::No
                        },
                    )
                    .await?;
                }
            }

            sqlx::query(
                r#"
                UPDATE THUMBNAIL_BOOK
                SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE BOOK_ID = ?
                "#,
            )
            .bind(&book_id)
            .execute(&pool)
            .await
            .map_err(|error| {
                format!("failed to refresh THUMBNAIL_BOOK rows for '{book_id}': {error}")
            })?;

            sqlx::query(
                r#"
                UPDATE BOOK
                SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE ID = ?
                "#,
            )
            .bind(&book_id)
            .execute(&pool)
            .await
            .map_err(|error| format!("failed to refresh BOOK row while updating local artwork for '{book_id}': {error}"))?;

            Ok(())
        })
    })
}

pub fn generate_book_thumbnail(database_file: &Path, book_id: &str) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let book_id = book_id.to_string();

    run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        Box::pin(async move {
            let media_row = sqlx::query(
                r#"
                SELECT b.LIBRARY_ID AS LIBRARY_ID,
                       b.NAME AS FILE_NAME,
                       b.URL AS BOOK_URL,
                       l.ROOT AS LIBRARY_ROOT,
                       COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
                       COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT
                FROM BOOK b
                JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
                LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
                WHERE b.ID = ?
                  AND b.DELETED_DATE IS NULL
                LIMIT 1
                "#,
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!(
                    "failed to resolve book media for thumbnail generation '{book_id}': {error}"
                )
            })?;

            let Some(media_row) = media_row else {
                return Ok(());
            };

            let library_root = media_row.get::<String, _>("LIBRARY_ROOT");
            let media = BookMediaRecord {
                library_id: media_row.get::<String, _>("LIBRARY_ID"),
                media_type: media_row.get::<String, _>("MEDIA_TYPE"),
                file_path: PathBuf::from(&library_root)
                    .join(media_row.get::<String, _>("BOOK_URL")),
                file_name: media_row.get::<String, _>("FILE_NAME"),
                page_count: media_row.get::<i64, _>("PAGE_COUNT").max(0) as u64,
            };

            let thumbnail_size_setting = sqlx::query(
                r#"
                SELECT VALUE
                FROM SERVER_SETTINGS
                WHERE KEY = 'THUMBNAIL_SIZE'
                LIMIT 1
                "#,
            )
            .fetch_optional(&pool)
            .await
            .map_err(|error| format!("failed to load thumbnail size setting: {error}"))?
            .and_then(|row| row.get::<Option<String>, _>("VALUE"));
            let configured_max_edge =
                thumbnail_max_edge_from_setting(thumbnail_size_setting.as_deref());

            let epub_cover = if book_media_is_epub(&media) {
                load_epub_cover_bytes(&media)
            } else {
                None
            };

            let persisted_page_row = sqlx::query(
                r#"
                SELECT NUMBER,
                       FILE_NAME,
                       MEDIA_TYPE,
                       WIDTH,
                       HEIGHT,
                       COALESCE(FILE_SIZE, 0) AS FILE_SIZE
                FROM MEDIA_PAGE
                WHERE BOOK_ID = ? AND NUMBER = 1
                LIMIT 1
                "#,
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!("failed to resolve page row for thumbnail generation '{book_id}': {error}")
            })?;

            let (thumbnail_bytes, thumbnail_media_type, width, height) =
                if book_media_is_pdf(&media) {
                    let Some(rendered) = render_pdf_thumbnail(&media, configured_max_edge)? else {
                        return Ok(());
                    };
                    rendered
                } else if let Some((bytes, _media_type)) = epub_cover {
                    render_generated_thumbnail_from_image_bytes(
                        &book_id,
                        &bytes,
                        configured_max_edge,
                    )?
                } else {
                    let page_row = if let Some(row) = persisted_page_row {
                        Some(BookPageRecord {
                            number: row.get::<i64, _>("NUMBER") as u64,
                            file_name: row.get::<String, _>("FILE_NAME"),
                            media_type: row.get::<String, _>("MEDIA_TYPE"),
                            width: row.get::<Option<i64>, _>("width"),
                            height: row.get::<Option<i64>, _>("height"),
                            file_size: row.get::<i64, _>("FILE_SIZE"),
                        })
                    } else if book_media_is_single_image(&media) {
                        Some(BookPageRecord {
                            number: 1,
                            file_name: media.file_name.clone(),
                            media_type: content_type_from_filename(
                                &media.file_name,
                                &media.media_type,
                            ),
                            width: None,
                            height: None,
                            file_size: fs::metadata(&media.file_path)
                                .ok()
                                .map(|metadata| metadata.len() as i64)
                                .unwrap_or(0),
                        })
                    } else {
                        load_archive_page_row(&media, 1)
                    };

                    let Some(page_row) = page_row else {
                        return Ok(());
                    };
                    let Some(thumbnail_bytes) = resolve_book_page_bytes(&media, &page_row, 1)
                    else {
                        return Ok(());
                    };
                    let thumbnail_media_type = if page_row.media_type.is_empty() {
                        content_type_from_filename(&page_row.file_name, &media.media_type)
                    } else {
                        page_row.media_type.clone()
                    };
                    if !thumbnail_media_type
                        .to_ascii_lowercase()
                        .starts_with("image/")
                    {
                        return Ok(());
                    }
                    render_generated_thumbnail_from_image_bytes(
                        &book_id,
                        &thumbnail_bytes,
                        configured_max_edge,
                    )?
                };

            let selected_thumbnail_type = sqlx::query(
                r#"
                SELECT TYPE
                FROM THUMBNAIL_BOOK
                WHERE BOOK_ID = ? AND SELECTED = 1
                ORDER BY LAST_MODIFIED_DATE DESC, ID ASC
                LIMIT 1
                "#,
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!("failed to query selected thumbnail for '{book_id}': {error}")
            })?
            .map(|row| row.get::<String, _>("TYPE"));
            let should_select = selected_thumbnail_type
                .as_deref()
                .is_none_or(|thumbnail_type| thumbnail_type == "GENERATED");

            let mut tx = pool
                .begin()
                .await
                .map_err(|error| format!("begin generate thumbnail tx: {error}"))?;

            sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? AND TYPE = 'GENERATED'")
                .bind(&book_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| {
                    format!("failed to delete prior generated thumbnails for '{book_id}': {error}")
                })?;

            if should_select {
                sqlx::query("UPDATE THUMBNAIL_BOOK SET SELECTED = 0 WHERE BOOK_ID = ?")
                    .bind(&book_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| {
                        format!("failed to clear selected thumbnails for '{book_id}': {error}")
                    })?;
            }

            let thumbnail_id = format!("thumbnail-book-generated:{book_id}");
            sqlx::query(
                r#"
                INSERT INTO THUMBNAIL_BOOK
                    (ID, SELECTED, THUMBNAIL, TYPE, BOOK_ID, MEDIA_TYPE, FILE_SIZE, WIDTH, HEIGHT, LAST_MODIFIED_DATE)
                VALUES (?, ?, ?, 'GENERATED', ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
                "#,
            )
            .bind(&thumbnail_id)
            .bind(should_select)
            .bind(&thumbnail_bytes)
            .bind(&book_id)
            .bind(&thumbnail_media_type)
            .bind(thumbnail_bytes.len() as i64)
            .bind(width)
            .bind(height)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("failed to insert generated thumbnail for '{book_id}': {error}"))?;

            if !should_select {
                book_thumbnail_housekeeping(&mut tx, &book_id, Path::new(&library_root)).await?;
            }

            tx.commit()
                .await
                .map_err(|error| format!("commit generate thumbnail tx: {error}"))?;

            Ok(())
        })
    })
}

async fn book_thumbnail_housekeeping(
    tx: &mut Transaction<'_, Sqlite>,
    book_id: &str,
    library_root: &Path,
) -> Result<(), String> {
    let rows = sqlx::query(
        r#"
        SELECT ID, URL, THUMBNAIL, SELECTED
        FROM THUMBNAIL_BOOK
        WHERE BOOK_ID = ?
        ORDER BY LAST_MODIFIED_DATE DESC, ID ASC
        "#,
    )
    .bind(book_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| format!("failed to load thumbnails for '{book_id}' housekeeping: {error}"))?;

    let mut retained_ids = Vec::new();
    let mut selected_ids = Vec::new();
    for row in rows {
        let thumbnail_id = row.get::<String, _>("ID");
        let thumbnail_url = row.get::<Option<String>, _>("URL");
        let thumbnail_blob = row.get::<Option<Vec<u8>>, _>("THUMBNAIL");
        let selected = row.get::<bool, _>("SELECTED");

        let exists = thumbnail_blob
            .as_ref()
            .is_some_and(|thumbnail| !thumbnail.is_empty())
            || thumbnail_url.as_deref().is_some_and(|url| {
                let path = Path::new(url);
                let resolved = if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    library_root.join(path)
                };
                resolved.exists()
            });

        if !exists {
            sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE ID = ?")
                .bind(&thumbnail_id)
                .execute(&mut **tx)
                .await
                .map_err(|error| {
                    format!(
                        "failed to delete invalid thumbnail '{thumbnail_id}' for '{book_id}': {error}"
                    )
                })?;
            continue;
        }

        if selected {
            selected_ids.push(thumbnail_id.clone());
        }
        retained_ids.push(thumbnail_id);
    }

    let Some(target_selected_id) = (if selected_ids.len() > 1 {
        selected_ids.into_iter().next()
    } else if selected_ids.is_empty() {
        retained_ids.into_iter().next()
    } else {
        None
    }) else {
        return Ok(());
    };

    sqlx::query(
        "UPDATE THUMBNAIL_BOOK SET SELECTED = CASE WHEN ID = ? THEN 1 ELSE 0 END WHERE BOOK_ID = ?",
    )
    .bind(target_selected_id)
    .bind(book_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| format!("failed to normalize selected thumbnail for '{book_id}': {error}"))?;

    Ok(())
}

pub fn refresh_series_local_artwork(database_file: &Path, series_id: &str) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let series_id = series_id.to_string();

    run_database_query(database_file, move |pool| {
        let series_id = series_id.clone();
        Box::pin(async move {
            let series_row = sqlx::query(
                r#"
                SELECT s.URL AS SERIES_URL,
                       l.ROOT AS LIBRARY_ROOT,
                       l.IMPORT_LOCAL_ARTWORK AS IMPORT_LOCAL_ARTWORK,
                       COALESCE(s.ONESHOT, 0) AS ONESHOT
                FROM SERIES s
                JOIN LIBRARY l ON l.ID = s.LIBRARY_ID
                WHERE s.ID = ?
                LIMIT 1
                "#,
            )
            .bind(&series_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!("failed to resolve series path for artwork refresh '{series_id}': {error}")
            })?;

            if let Some(series_row) = &series_row {
                let series_url = series_row.get::<String, _>("SERIES_URL");
                let import_local_artwork = series_row.get::<bool, _>("IMPORT_LOCAL_ARTWORK");
                if !import_local_artwork {
                    return Ok(());
                }

                let oneshot = series_row.get::<i64, _>("ONESHOT") != 0;
                if oneshot {
                    return Ok(());
                }

                let library_root = PathBuf::from(series_row.get::<String, _>("LIBRARY_ROOT"));
                for (index, artwork_url) in
                    load_series_local_artwork_urls(&library_root, &series_url)?
                        .into_iter()
                        .enumerate()
                {
                    import_series_local_artwork_thumbnail(
                        &pool,
                        &series_id,
                        &library_root,
                        &artwork_url,
                        if index == 0 {
                            MarkSelectedPreference::IfNoneOrGenerated
                        } else {
                            MarkSelectedPreference::No
                        },
                    )
                    .await?;
                }
            }

            sqlx::query(
                r#"
                UPDATE THUMBNAIL_SERIES
                SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE SERIES_ID = ?
                "#,
            )
            .bind(&series_id)
            .execute(&pool)
            .await
            .map_err(|error| {
                format!("failed to refresh THUMBNAIL_SERIES rows for '{series_id}': {error}")
            })?;

            sqlx::query(
                r#"
                UPDATE SERIES
                SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE ID = ?
                "#,
            )
            .bind(&series_id)
            .execute(&pool)
            .await
            .map_err(|error| format!("failed to refresh SERIES row while updating local artwork for '{series_id}': {error}"))?;

            Ok(())
        })
    })
}

fn render_generated_thumbnail_from_image_bytes(
    book_id: &str,
    thumbnail_bytes: &[u8],
    configured_max_edge: u32,
) -> Result<(Vec<u8>, String, i64, i64), String> {
    let image = image::load_from_memory(thumbnail_bytes).map_err(|error| {
        format!("failed to decode generated thumbnail source for '{book_id}': {error}")
    })?;
    let source_max_edge = image.width().max(image.height()).max(1);
    let effective_max_edge = configured_max_edge.min(source_max_edge);
    let resized = image.thumbnail(effective_max_edge, effective_max_edge);
    let width = i64::from(resized.width());
    let height = i64::from(resized.height());
    let mut output = Cursor::new(Vec::new());
    resized
        .write_to(&mut output, image::ImageFormat::Jpeg)
        .map_err(|error| {
            format!("failed to encode generated thumbnail for '{book_id}': {error}")
        })?;
    Ok((output.into_inner(), "image/jpeg".to_string(), width, height))
}

fn render_pdf_thumbnail(
    media: &BookMediaRecord,
    configured_max_edge: u32,
) -> Result<Option<(Vec<u8>, String, i64, i64)>, String> {
    let pdfium = load_pdfium()?;
    let document = pdfium
        .load_pdf_from_file(&media.file_path, None)
        .map_err(|error| {
            format!(
                "failed to load PDF for thumbnail generation '{}': {error}",
                media.file_path.display()
            )
        })?;
    let page = match document.pages().first() {
        Ok(page) => page,
        Err(_) => return Ok(None),
    };

    let rendered = page
        .render_with_config(
            &PdfRenderConfig::new()
                .set_target_width(i32::try_from(configured_max_edge).unwrap_or(i32::MAX))
                .set_maximum_height(i32::try_from(configured_max_edge).unwrap_or(i32::MAX)),
        )
        .map_err(|error| {
            format!(
                "failed to render PDF page for thumbnail generation '{}': {error}",
                media.file_path.display()
            )
        })?
        .as_image()
        .map_err(|error| {
            format!(
                "failed to convert PDF render to image '{}': {error}",
                media.file_path.display()
            )
        })?
        .into_rgb8();

    let width = i64::from(rendered.width());
    let height = i64::from(rendered.height());
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(rendered)
        .write_to(&mut output, image::ImageFormat::Jpeg)
        .map_err(|error| {
            format!(
                "failed to encode PDF thumbnail for '{}': {error}",
                media.file_path.display()
            )
        })?;

    Ok(Some((
        output.into_inner(),
        "image/jpeg".to_string(),
        width,
        height,
    )))
}

type BoxFuture<T> = std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, String>> + Send>>;

fn run_database_query<T>(
    database_file: PathBuf,
    operation: impl FnOnce(SqlitePool) -> BoxFuture<T> + Send + 'static,
) -> Result<T, String>
where
    T: Send + 'static,
{
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("failed to build metadata runtime: {error}"))?;

        runtime.block_on(async move {
            let pool = connect_pool(&database_file, 1)
                .await
                .map_err(|error| format!("failed to open sqlite pool: {error}"))?;
            operation(pool).await
        })
    })
    .join()
    .map_err(|_| "metadata worker thread panicked".to_string())?
}

async fn load_sidecar_url_for_parent(
    pool: &SqlitePool,
    parent_url: &str,
    metadata_only: bool,
) -> Result<Option<String>, String> {
    let sql = if metadata_only {
        r#"
        SELECT URL
        FROM SIDECAR
        WHERE PARENT_URL = ?
          AND LOWER(URL) LIKE '%.xml'
        ORDER BY LAST_MODIFIED_TIME DESC
        LIMIT 1
        "#
    } else {
        r#"
        SELECT URL
        FROM SIDECAR
        WHERE PARENT_URL = ?
          AND (
                LOWER(URL) LIKE '%.jpg'
             OR LOWER(URL) LIKE '%.jpeg'
             OR LOWER(URL) LIKE '%.png'
             OR LOWER(URL) LIKE '%.webp'
             OR LOWER(URL) LIKE '%.gif'
             OR LOWER(URL) LIKE '%.avif'
          )
        ORDER BY LAST_MODIFIED_TIME DESC
        LIMIT 1
        "#
    };

    let row = sqlx::query(sql)
        .bind(parent_url)
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("failed to load sidecar for '{parent_url}': {error}"))?;
    Ok(row.map(|row| row.get::<String, _>("URL")))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MarkSelectedPreference {
    IfNoneOrGenerated,
    No,
}

fn load_book_local_artwork_urls(
    library_root: &Path,
    book_url: &str,
) -> Result<Vec<String>, String> {
    let book_path = library_root.join(book_url);
    let Some(book_dir) = book_path.parent() else {
        return Ok(Vec::new());
    };
    let Some(book_base_name) = book_path.file_stem().and_then(|value| value.to_str()) else {
        return Ok(Vec::new());
    };

    let mut artwork_urls = Vec::new();
    let entries = fs::read_dir(book_dir).map_err(|error| {
        format!(
            "failed to scan local artwork directory '{}' for '{}': {error}",
            book_dir.display(),
            book_url,
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "failed to read local artwork entry in '{}' for '{}': {error}",
                book_dir.display(),
                book_url,
            )
        })?;
        let path = entry.path();
        if !path.is_file() || !supported_book_local_artwork_path(path.as_path()) {
            continue;
        }
        let Some(candidate_stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if !book_local_artwork_name_matches(candidate_stem, book_base_name) {
            continue;
        }

        let relative_url = path
            .strip_prefix(library_root)
            .map_err(|error| {
                format!(
                    "failed to relativize local artwork '{}' against library root '{}': {error}",
                    path.display(),
                    library_root.display(),
                )
            })?
            .to_string_lossy()
            .replace('\\', "/");
        artwork_urls.push(relative_url);
    }

    Ok(artwork_urls)
}

fn load_series_local_artwork_urls(
    library_root: &Path,
    series_url: &str,
) -> Result<Vec<String>, String> {
    let series_path = library_root.join(series_url);
    let mut artwork_urls = Vec::new();
    let entries = fs::read_dir(&series_path).map_err(|error| {
        format!(
            "failed to scan series local artwork directory '{}' for '{}': {error}",
            series_path.display(),
            series_url,
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "failed to read series local artwork entry in '{}' for '{}': {error}",
                series_path.display(),
                series_url,
            )
        })?;
        let path = entry.path();
        if !path.is_file() || !supported_series_local_artwork_path(path.as_path()) {
            continue;
        }
        let Some(candidate_stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if !series_local_artwork_name_matches(candidate_stem) {
            continue;
        }

        let relative_url = path
            .strip_prefix(library_root)
            .map_err(|error| {
                format!(
                    "failed to relativize series local artwork '{}' against library root '{}': {error}",
                    path.display(),
                    library_root.display(),
                )
            })?
            .to_string_lossy()
            .replace('\\', "/");
        artwork_urls.push(relative_url);
    }

    Ok(artwork_urls)
}

fn supported_series_local_artwork_path(path: &Path) -> bool {
    supported_book_local_artwork_path(path)
}

fn series_local_artwork_name_matches(candidate_stem: &str) -> bool {
    matches!(
        candidate_stem.to_ascii_lowercase().as_str(),
        "cover" | "default" | "folder" | "poster" | "series"
    )
}

fn supported_book_local_artwork_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .as_deref(),
        Some("png") | Some("jpeg") | Some("jpg") | Some("tbn") | Some("webp") | Some("gif")
    )
}

fn book_local_artwork_name_matches(candidate_stem: &str, book_base_name: &str) -> bool {
    let candidate_stem = candidate_stem.to_ascii_lowercase();
    let book_base_name = book_base_name.to_ascii_lowercase();
    if candidate_stem == book_base_name {
        return true;
    }

    candidate_stem
        .strip_prefix(&format!("{book_base_name}-"))
        .is_some_and(|suffix| !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit()))
}

async fn import_book_local_artwork_thumbnail(
    pool: &SqlitePool,
    book_id: &str,
    library_root: &Path,
    artwork_url: &str,
    selected_preference: MarkSelectedPreference,
) -> Result<(), String> {
    let artwork_path = library_root.join(artwork_url);
    let metadata = fs::metadata(&artwork_path).map_err(|error| {
        format!(
            "failed to read local artwork '{}' for book '{}': {error}",
            artwork_path.display(),
            book_id,
        )
    })?;
    let thumbnail_id = format!("thumbnail-book-sidecar:{book_id}:{artwork_url}");
    let selected = should_select_book_local_artwork(pool, book_id, selected_preference).await?;

    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? AND TYPE = 'SIDECAR' AND URL = ?")
        .bind(book_id)
        .bind(artwork_url)
        .execute(pool)
        .await
        .map_err(|error| {
            format!(
                "failed to remove duplicated sidecar thumbnail '{}' for '{}': {error}",
                artwork_url, book_id,
            )
        })?;

    sqlx::query(
        r#"
        INSERT INTO THUMBNAIL_BOOK
            (ID, URL, SELECTED, TYPE, BOOK_ID, MEDIA_TYPE, FILE_SIZE, LAST_MODIFIED_DATE)
        VALUES (?, ?, ?, 'SIDECAR', ?, ?, ?, CURRENT_TIMESTAMP)
        "#,
    )
    .bind(&thumbnail_id)
    .bind(artwork_url)
    .bind(selected)
    .bind(book_id)
    .bind(media_type_from_sidecar_path(artwork_path.as_path()))
    .bind(metadata.len() as i64)
    .execute(pool)
    .await
    .map_err(|error| {
        format!(
            "failed to insert local artwork '{}' for book '{}': {error}",
            artwork_url, book_id,
        )
    })?;

    if selected {
        sqlx::query(
            "UPDATE THUMBNAIL_BOOK SET SELECTED = CASE WHEN ID = ? THEN 1 ELSE 0 END WHERE BOOK_ID = ?",
        )
        .bind(&thumbnail_id)
        .bind(book_id)
        .execute(pool)
        .await
        .map_err(|error| {
            format!(
                "failed to mark local artwork '{}' as selected for '{}': {error}",
                artwork_url, book_id,
            )
        })?;
    }

    Ok(())
}

async fn import_series_local_artwork_thumbnail(
    pool: &SqlitePool,
    series_id: &str,
    library_root: &Path,
    artwork_url: &str,
    selected_preference: MarkSelectedPreference,
) -> Result<(), String> {
    let artwork_path = library_root.join(artwork_url);
    let metadata = fs::metadata(&artwork_path).map_err(|error| {
        format!(
            "failed to read series local artwork '{}' for '{}': {error}",
            artwork_path.display(),
            series_id,
        )
    })?;
    let thumbnail_id = format!("thumbnail-series-sidecar:{series_id}:{artwork_url}");
    let selected = should_select_series_local_artwork(pool, series_id, selected_preference).await?;

    sqlx::query(
        "DELETE FROM THUMBNAIL_SERIES WHERE SERIES_ID = ? AND TYPE = 'SIDECAR' AND URL = ?",
    )
    .bind(series_id)
    .bind(artwork_url)
    .execute(pool)
    .await
    .map_err(|error| {
        format!(
            "failed to remove duplicated series sidecar thumbnail '{}' for '{}': {error}",
            artwork_url, series_id,
        )
    })?;

    sqlx::query(
        r#"
        INSERT INTO THUMBNAIL_SERIES
            (ID, URL, SELECTED, TYPE, SERIES_ID, MEDIA_TYPE, FILE_SIZE, LAST_MODIFIED_DATE)
        VALUES (?, ?, ?, 'SIDECAR', ?, ?, ?, CURRENT_TIMESTAMP)
        "#,
    )
    .bind(&thumbnail_id)
    .bind(artwork_url)
    .bind(selected)
    .bind(series_id)
    .bind(media_type_from_sidecar_path(artwork_path.as_path()))
    .bind(metadata.len() as i64)
    .execute(pool)
    .await
    .map_err(|error| {
        format!(
            "failed to insert series local artwork '{}' for '{}': {error}",
            artwork_url, series_id,
        )
    })?;

    if selected {
        sqlx::query(
            "UPDATE THUMBNAIL_SERIES SET SELECTED = CASE WHEN ID = ? THEN 1 ELSE 0 END WHERE SERIES_ID = ?",
        )
        .bind(&thumbnail_id)
        .bind(series_id)
        .execute(pool)
        .await
        .map_err(|error| {
            format!(
                "failed to mark series local artwork '{}' as selected for '{}': {error}",
                artwork_url, series_id,
            )
        })?;
    }

    Ok(())
}

async fn should_select_book_local_artwork(
    pool: &SqlitePool,
    book_id: &str,
    selected_preference: MarkSelectedPreference,
) -> Result<bool, String> {
    if selected_preference == MarkSelectedPreference::No {
        return Ok(false);
    }

    let selected_row = sqlx::query(
        "SELECT TYPE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? AND SELECTED = 1 ORDER BY ID ASC LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("failed to load selected thumbnail for '{}': {error}", book_id))?;

    Ok(match selected_row {
        None => true,
        Some(row) => row.get::<String, _>("TYPE") == "GENERATED",
    })
}

async fn should_select_series_local_artwork(
    pool: &SqlitePool,
    series_id: &str,
    selected_preference: MarkSelectedPreference,
) -> Result<bool, String> {
    if selected_preference == MarkSelectedPreference::No {
        return Ok(false);
    }

    let selected_row = sqlx::query(
        "SELECT TYPE FROM THUMBNAIL_SERIES WHERE SERIES_ID = ? AND SELECTED = 1 ORDER BY ID ASC LIMIT 1",
    )
    .bind(series_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("failed to load selected series thumbnail for '{}': {error}", series_id))?;

    Ok(match selected_row {
        None => true,
        Some(row) => row.get::<String, _>("TYPE") == "GENERATED",
    })
}

fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    let value = xml[start..end].trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn media_type_from_sidecar_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("tbn") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("avif") => "image/avif",
        _ => "application/octet-stream",
    }
}

#![allow(clippy::type_complexity)]

use std::collections::BTreeSet;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use komga_application::media_assets::{
    BookMediaRecord, BookMetadata, BookMetadataAuthor, BookMetadataLink, BookPageRecord,
    book_media_is_epub, book_media_is_pdf, book_media_is_single_image,
};
use komga_application::runtime_sse::register_runtime_sse_event;
use pdfium_render::prelude::*;
use rxing::{BarcodeFormat, DecodeHints, helpers as rxing_helpers};
use serde_json::json;
use sqlx::{Row, SqlitePool};

use crate::filesystem::media_access::epub::load_epub_package_document;
use crate::filesystem::media_access::page_content::{
    load_archive_page_row, resolve_book_page_bytes,
};
use crate::load_pdfium;
use crate::sqlite::connect_pool;
use crate::{resolve_library_item_path, resolve_stored_path};

mod artwork_refresh;
mod artwork_support;
mod epub;
mod series_aggregation;
mod series_metadata;
mod sidecar_query;
mod sources;
mod support;

pub use artwork_refresh::{
    generate_book_thumbnail, refresh_book_local_artwork, refresh_series_local_artwork,
};
use epub::{extract_epub_book_patch, extract_epub_series_patch};
pub use series_aggregation::aggregate_series_metadata;
use series_metadata::{
    apply_mylar_series_import, apply_oneshot_series_metadata_import,
    apply_series_metadata_from_book_imports,
};
use sidecar_query::load_sidecar_url_for_parent;
use sources::{
    extract_comicinfo_book_patch, extract_comicinfo_readlists, extract_comicinfo_series_patch,
};
use support::{generated_readlist_id, normalize_isbn13};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RefreshBookMetadataOutcome {
    pub series_id: Option<String>,
    pub library_id: Option<String>,
    pub changed_readlist_ids: Vec<String>,
    pub book_changed: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TransientMetadataProviderInference {
    pub series_titles: Vec<String>,
    pub number: Option<f64>,
}

fn push_transient_series_title(series_titles: &mut Vec<String>, title: Option<String>) {
    let Some(title) = title.map(|value| value.trim().to_string()) else {
        return;
    };
    if title.is_empty() || series_titles.iter().any(|existing| existing == &title) {
        return;
    }
    series_titles.push(title);
}

pub fn infer_transient_epub_provider_metadata(
    package_document: &[u8],
) -> TransientMetadataProviderInference {
    let book_patch = extract_epub_book_patch(package_document);
    let series_patch = extract_epub_series_patch(package_document);
    let mut series_titles = Vec::new();
    push_transient_series_title(&mut series_titles, series_patch.title);

    TransientMetadataProviderInference {
        series_titles,
        number: book_patch.number_sort,
    }
}

pub fn infer_transient_comicinfo_provider_metadata(xml: &str) -> TransientMetadataProviderInference {
    let book_patch = extract_comicinfo_book_patch(xml);
    let append_volume_patch = extract_comicinfo_series_patch(xml, true);
    let plain_patch = extract_comicinfo_series_patch(xml, false);
    let mut series_titles = Vec::new();
    push_transient_series_title(&mut series_titles, append_volume_patch.title);
    push_transient_series_title(&mut series_titles, plain_patch.title);

    TransientMetadataProviderInference {
        series_titles,
        number: book_patch.number_sort,
    }
}

fn emit_book_changed_event(book_id: &str, series_id: &str, library_id: &str) {
    register_runtime_sse_event(
        "BookChanged",
        json!({
            "bookId": book_id,
            "seriesId": series_id,
            "libraryId": library_id,
        }),
        false,
        None,
    );
}

fn emit_readlist_event(readlist_id: &str, book_ids: &[String], created: bool) {
    register_runtime_sse_event(
        if created {
            "ReadListAdded"
        } else {
            "ReadListChanged"
        },
        json!({
            "readListId": readlist_id,
            "bookIds": book_ids,
        }),
        false,
        None,
    );
}

fn emit_series_changed_event(series_id: &str, library_id: &str) {
    register_runtime_sse_event(
        "SeriesChanged",
        json!({
            "seriesId": series_id,
            "libraryId": library_id,
        }),
        false,
        None,
    );
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
    let book_id_for_events = book_id.clone();
    let capabilities = capabilities.clone();

    let outcome = run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        let capabilities = capabilities.clone();
        Box::pin(async move {
            let mut changed_readlist_ids = BTreeSet::new();
            let mut should_emit_book_changed = false;
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
                should_emit_book_changed |=
                    import_comicinfo_book && comicinfo_provider_matches_capabilities(&capabilities);
                should_emit_book_changed |=
                    import_epub_book && epub_provider_matches_capabilities(&capabilities);
                should_emit_book_changed |=
                    import_barcode_isbn && barcode_provider_matches_capabilities(&capabilities);
                if let Some(sidecar_url) =
                    load_sidecar_url_for_parent(&pool, &book_url, true).await?
                {
                    let sidecar_path = resolve_library_item_path(&library_root, &sidecar_url);
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

                if import_epub_book
                    && epub_provider_matches_capabilities(&capabilities)
                    && let Some(media) = load_book_media_for_refresh(&pool, &book_id).await?
                    && let Some(package_document) = load_epub_package_document(&media)
                {
                    let patch = extract_epub_book_patch(&package_document);
                    apply_book_metadata_import_patch(&pool, &book_id, patch).await?;
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

            let book_context = sqlx::query(
                r#"
                SELECT SERIES_ID, LIBRARY_ID
                FROM BOOK
                WHERE ID = ?
                LIMIT 1
                "#,
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!("failed to resolve book SSE context for '{book_id}': {error}")
            })?;
            let series_id = book_context
                .as_ref()
                .and_then(|row| row.get::<Option<String>, _>("SERIES_ID"));
            let library_id = book_context
                .as_ref()
                .and_then(|row| row.get::<Option<String>, _>("LIBRARY_ID"));

            Ok(RefreshBookMetadataOutcome {
                series_id,
                library_id,
                changed_readlist_ids: changed_readlist_ids.into_iter().collect(),
                book_changed: should_emit_book_changed,
            })
        })
    })?;

    if outcome.book_changed
        && let (Some(series_id), Some(library_id)) =
            (outcome.series_id.as_deref(), outcome.library_id.as_deref())
    {
        emit_book_changed_event(&book_id_for_events, series_id, library_id);
    }

    Ok(outcome)
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
        r#"
        SELECT b.LIBRARY_ID AS LIBRARY_ID, b.NAME AS FILE_NAME, b.URL AS BOOK_URL,
               l.ROOT AS LIBRARY_ROOT,
               COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
               COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT
        FROM BOOK b
        JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
        LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
        WHERE b.ID = ?
        "#,
    )
    .bind(book_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query persisted book media for refresh: {error}"))?;

    Ok(row.map(|row| BookMediaRecord {
        library_id: row.get::<String, _>("LIBRARY_ID"),
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        file_path: resolve_library_item_path(
            row.get::<String, _>("LIBRARY_ROOT").as_str(),
            row.get::<String, _>("BOOK_URL").as_str(),
        ),
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
        r#"
        SELECT NUMBER, FILE_NAME, MEDIA_TYPE, WIDTH, HEIGHT,
               CASE WHEN FILE_SIZE IS NULL THEN -1 ELSE FILE_SIZE END AS FILE_SIZE
        FROM MEDIA_PAGE
        WHERE BOOK_ID = ? AND NUMBER = ?
        LIMIT 1
        "#,
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
        r#"
        SELECT TITLE, TITLE_LOCK, SUMMARY, SUMMARY_LOCK, NUMBER, NUMBER_LOCK, NUMBER_SORT,
               NUMBER_SORT_LOCK, RELEASE_DATE, RELEASE_DATE_LOCK, AUTHORS_LOCK, TAGS_LOCK, ISBN,
               ISBN_LOCK, LINKS_LOCK
        FROM BOOK_METADATA
        WHERE BOOK_ID = ?
        LIMIT 1
        "#,
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
        r#"
        UPDATE BOOK_METADATA
        SET TITLE = ?, TITLE_LOCK = ?, SUMMARY = ?, SUMMARY_LOCK = ?, NUMBER = ?,
            NUMBER_LOCK = ?, NUMBER_SORT = ?, NUMBER_SORT_LOCK = ?, RELEASE_DATE = ?,
            RELEASE_DATE_LOCK = ?, AUTHORS_LOCK = ?, TAGS_LOCK = ?, ISBN = ?, ISBN_LOCK = ?,
            LINKS_LOCK = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
        WHERE BOOK_ID = ?
        "#,
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

async fn load_readlist_book_ids(
    pool: &SqlitePool,
    readlist_id: &str,
) -> Result<Vec<String>, String> {
    sqlx::query("SELECT BOOK_ID FROM READLIST_BOOK WHERE READLIST_ID = ? ORDER BY NUMBER ASC")
        .bind(readlist_id)
        .fetch_all(pool)
        .await
        .map_err(|error| format!("failed to load readlist books for '{readlist_id}': {error}"))
        .map(|rows| {
            rows.into_iter()
                .map(|row| row.get::<String, _>("BOOK_ID"))
                .collect()
        })
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

    let (readlist_id, created) = match readlist_id {
        Some(readlist_id) => (readlist_id, false),
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
            (generated_id, true)
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
        r#"
        UPDATE READLIST
        SET BOOK_COUNT = (SELECT COUNT(*) FROM READLIST_BOOK WHERE READLIST_ID = ?),
            LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
        WHERE ID = ?
        "#,
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

    let readlist_book_ids = load_readlist_book_ids(pool, &readlist_id).await?;
    emit_readlist_event(&readlist_id, &readlist_book_ids, created);

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

pub fn refresh_series_metadata(database_file: &Path, series_id: &str) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let series_id = series_id.to_string();
    let series_id_for_events = series_id.clone();

    let (library_id, should_emit_series_changed) = run_database_query(
        database_file,
        move |pool| {
            let series_id = series_id.clone();
            Box::pin(async move {
                let mut should_emit_series_changed = false;
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
                    let resolved_library_root = resolve_stored_path(&library_root);
                    let oneshot = series_row.get::<i64, _>("ONESHOT") != 0;
                    let import_comicinfo_series =
                        series_row.get::<bool, _>("IMPORT_COMICINFO_SERIES");
                    let import_comicinfo_collection =
                        series_row.get::<bool, _>("IMPORT_COMICINFO_COLLECTION");
                    let import_comicinfo_series_append_volume =
                        series_row.get::<bool, _>("IMPORT_COMICINFO_SERIES_APPEND_VOLUME");
                    let import_epub_series = series_row.get::<bool, _>("IMPORT_EPUB_SERIES");
                    let import_mylar_series = series_row.get::<bool, _>("IMPORT_MYLAR_SERIES");
                    should_emit_series_changed = import_comicinfo_series
                        || import_epub_series
                        || import_mylar_series
                        || oneshot;

                    apply_series_metadata_from_book_imports(
                        &pool,
                        &series_id,
                        resolved_library_root.as_path(),
                        import_comicinfo_series,
                        import_comicinfo_collection,
                        import_comicinfo_series_append_volume,
                        import_epub_series,
                    )
                    .await?;

                    apply_mylar_series_import(
                        &pool,
                        &series_id,
                        resolved_library_root.as_path(),
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
                .map_err(|error| {
                    format!("failed to refresh SERIES row for '{series_id}': {error}")
                })?;

                sqlx::query(
                    r#"
                SELECT LIBRARY_ID
                FROM SERIES
                WHERE ID = ?
                LIMIT 1
                "#,
                )
                .bind(&series_id)
                .fetch_optional(&pool)
                .await
                .map_err(|error| {
                    format!(
                        "failed to resolve LIBRARY_ID for refreshed series '{series_id}': {error}"
                    )
                })
                .map(|row| {
                    (
                        row.and_then(|row| row.get::<Option<String>, _>("LIBRARY_ID")),
                        should_emit_series_changed,
                    )
                })
            })
        },
    )?;

    if should_emit_series_changed && let Some(library_id) = library_id.as_deref() {
        emit_series_changed_event(&series_id_for_events, library_id);
    }
    Ok(())
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

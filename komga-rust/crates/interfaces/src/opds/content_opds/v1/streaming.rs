use super::*;
use crate::state::OpdsState;

pub(super) async fn build_book_feed_acquisition_entries(
    app: &OpdsState,
    headers: &HeaderMap,
    books: Vec<PersistedBookFeedItem>,
) -> Vec<OpdsV1AcquisitionEntry> {
    let mut entries = Vec::with_capacity(books.len());
    for book in books {
        let extra_links = book_feed_page_streaming_links(app, headers, &book).await;
        let extension = std::path::Path::new(book.file_name.as_str())
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let mut content = format!("{extension} - {}", book.file_size);
        if !book.summary.trim().is_empty() {
            content.push_str("\n\n");
            content.push_str(book.summary.trim());
        }

        entries.push(OpdsV1AcquisitionEntry {
            id: book.id.clone(),
            title: format!("{} {}: {}", book.series_title, book.number, book.title),
            updated: Some(book.last_modified),
            content,
            authors: book.authors,
            acquisition_media_type: book.media_type,
            acquisition_href_path: format!(
                "/opds/v1.2/books/{}/file/{}",
                book.id,
                query_escape(book.file_name.as_str())
            ),
            thumbnail_href_path: format!("/opds/v1.2/books/{}/thumbnail/small", book.id),
            image_href_path: format!("/opds/v1.2/books/{}/thumbnail", book.id),
            extra_links,
        });
    }

    entries
}

pub(super) fn localized_opds_updated(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if OffsetDateTime::parse(trimmed, &Rfc3339).is_ok() {
        return Some(trimmed.to_string());
    }

    let sqlite_format = format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
    let iso_naive_format = format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]");
    let parsed = PrimitiveDateTime::parse(trimmed, sqlite_format)
        .or_else(|_| PrimitiveDateTime::parse(trimmed, iso_naive_format));

    Some(match parsed {
        Ok(value) => value
            .assume_utc()
            .to_offset(UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC))
            .format(&Rfc3339)
            .unwrap_or_else(|_| trimmed.to_string()),
        Err(_) => trimmed.to_string(),
    })
}

pub(super) async fn series_book_page_streaming_links(
    app: &OpdsState,
    headers: &HeaderMap,
    book: &PersistedSeriesBook,
) -> Vec<String> {
    opds_book_page_streaming_links(
        app,
        headers,
        &book.id,
        &book.media_type,
        book.page_count,
        book.epub_divina_compatible,
        book.last_read,
        book.last_read_date.as_deref(),
    )
    .await
}

async fn book_feed_page_streaming_links(
    app: &OpdsState,
    headers: &HeaderMap,
    book: &PersistedBookFeedItem,
) -> Vec<String> {
    opds_book_page_streaming_links(
        app,
        headers,
        &book.id,
        &book.media_type,
        book.page_count,
        book.epub_divina_compatible,
        book.last_read,
        book.last_read_date.as_deref(),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn opds_book_page_streaming_links(
    app: &OpdsState,
    headers: &HeaderMap,
    book_id: &str,
    media_type: &str,
    page_count: i64,
    epub_divina_compatible: bool,
    last_read: Option<i64>,
    last_read_date: Option<&str>,
) -> Vec<String> {
    let media_types = opds_book_page_stream_media_types(
        app.reader.as_ref(),
        app.content.as_ref(),
        book_id,
        media_type,
        page_count,
        epub_divina_compatible,
    )
    .await;
    if media_types.is_empty() {
        return vec![];
    }

    let supported_formats = ["image/jpeg", "image/png", "image/gif"];
    let (link_type, href) = if media_types.len() == 1
        && supported_formats.contains(&media_types[0].as_str())
    {
        (
            media_types[0].clone(),
            app_absolute_url(
                headers,
                format!("/opds/v1.2/books/{book_id}/pages/{{pageNumber}}").as_str(),
            ),
        )
    } else {
        (
            "image/jpeg".to_string(),
            app_absolute_url(
                headers,
                format!("/opds/v1.2/books/{book_id}/pages/{{pageNumber}}?convert=jpeg").as_str(),
            ),
        )
    };

    let mut read_progress_attributes = String::new();
    if let Some(last_read) = last_read {
        read_progress_attributes
            .push_str(format!(" pse:lastRead=\"{}\"", last_read.max(0)).as_str());
        if let Some(last_read_date) = last_read_date.map(str::trim)
            && !last_read_date.is_empty()
        {
            read_progress_attributes.push_str(
                format!(
                    " pse:lastReadDate=\"{}\"",
                    xml_escape(&normalize_opds_updated(last_read_date)),
                )
                .as_str(),
            );
        }
    }

    vec![format!(
        "<link type=\"{}\" rel=\"http://vaemendis.net/opds-pse/stream\" href=\"{}\" pse:count=\"{}\"{}/>",
        xml_escape(&link_type),
        xml_escape(&href),
        page_count,
        read_progress_attributes,
    )]
}

async fn opds_book_page_stream_media_types(
    reader: &dyn komga_application::media_assets::MediaReaderPort,
    content: &dyn komga_application::media_assets::ContentResolverPort,
    book_id: &str,
    media_type: &str,
    page_count: i64,
    epub_divina_compatible: bool,
) -> Vec<String> {
    if page_count <= 0 && media_type != "application/pdf" && !media_type.starts_with("image/") {
        return vec![];
    }

    if media_type == "application/pdf" {
        return vec!["image/jpeg".to_string()];
    }

    if media_type.starts_with("image/")
        || matches!(
            media_type,
            "application/vnd.comicbook+zip" | "application/vnd.comicbook-rar"
        )
        || (media_type == "application/epub+zip" && epub_divina_compatible)
    {
        return load_divina_page_media_types_for_opds(reader, content, book_id).await;
    }

    vec![]
}

async fn load_divina_page_media_types_for_opds(
    reader: &dyn komga_application::media_assets::MediaReaderPort,
    content: &dyn komga_application::media_assets::ContentResolverPort,
    book_id: &str,
) -> Vec<String> {
    let persisted = reader
        .book_pages(book_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|page| {
            if page.media_type.is_empty() {
                content_type_from_filename(&page.file_name, "image/jpeg")
            } else {
                page.media_type
            }
        })
        .collect::<Vec<_>>();
    if !persisted.is_empty() {
        return dedup_media_types(persisted);
    }

    let Ok(Some(media)) = reader.book_media(book_id).await else {
        return vec![];
    };

    let media_content_type = content_type_from_filename(&media.file_name, &media.media_type);
    if media_content_type.starts_with("image/") {
        return vec![media_content_type];
    }

    dedup_media_types(
        content
            .archive_page_rows(&media)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|page| {
                if page.media_type.is_empty() {
                    content_type_from_filename(&page.file_name, "image/jpeg")
                } else {
                    page.media_type
                }
            })
            .collect(),
    )
}

fn dedup_media_types(media_types: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for media_type in media_types {
        if !deduped.contains(&media_type) {
            deduped.push(media_type);
        }
    }
    deduped
}

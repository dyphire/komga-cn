use super::feeds::normalize_opds_updated;
use super::types::{PersistedBookFeedItem, PersistedSeriesBook};
use super::*;
use crate::media_assets_runtime_access::{
    load_archive_page_rows, load_persisted_book_media, load_persisted_book_pages,
};
use komga_application::media_assets::content_type_from_filename;
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::{OffsetDateTime, PrimitiveDateTime, UtcOffset};

pub(super) fn opds_v1_basic_unauthorized_response() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Basic realm=\"Realm\""),
        )],
    )
        .into_response()
}

pub(crate) async fn opds_v1_catalog(headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }
    let search_href = app_absolute_url(&headers, "/opds/v1.2/search");
    let alternate_href = app_absolute_url(&headers, "/opds/v2/catalog");
    opds_v1_navigation_feed_response_with_extra_links(
        &headers,
        "root",
        "Komga OPDS catalog",
        "/opds/v1.2/catalog",
        vec![
            nav_entry_with_content(
                "keepReading",
                "Keep Reading",
                "Continue reading your in progress books",
                "/opds/v1.2/keep-reading",
            ),
            nav_entry_with_content(
                "ondeck",
                "On Deck",
                "Browse what to read next",
                "/opds/v1.2/ondeck",
            ),
            nav_entry_with_content(
                "allSeries",
                "All series",
                "Browse by series",
                "/opds/v1.2/series",
            ),
            nav_entry_with_content(
                "latestSeries",
                "Latest series",
                "Browse latest series",
                "/opds/v1.2/series/latest",
            ),
            nav_entry_with_content(
                "latestBooks",
                "Latest books",
                "Browse latest books",
                "/opds/v1.2/books/latest",
            ),
            nav_entry_with_content(
                "allLibraries",
                "All libraries",
                "Browse by library",
                "/opds/v1.2/libraries",
            ),
            nav_entry_with_content(
                "allCollections",
                "All collections",
                "Browse by collection",
                "/opds/v1.2/collections",
            ),
            nav_entry_with_content(
                "allReadLists",
                "All read lists",
                "Browse by read lists",
                "/opds/v1.2/readlists",
            ),
            nav_entry_with_content(
                "allPublishers",
                "All publishers",
                "Browse by publishers",
                "/opds/v1.2/publishers",
            ),
        ],
        None,
        None,
        &[
            format!(
                "<link type=\"application/opensearchdescription+xml\" rel=\"search\" href=\"{}\"/>",
                xml_escape(&search_href)
            ),
            format!(
                "<link type=\"application/opds+json\" rel=\"alternate\" href=\"{}\"/>",
                xml_escape(&alternate_href)
            ),
        ],
    )
}

pub(crate) async fn opds_v1_search(headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let template_href = app_absolute_url(&headers, "/opds/v1.2/series?search={searchTerms}");
    let payload = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><OpenSearchDescription xmlns=\"http://a9.com/-/spec/opensearch/1.1/\"><ShortName>Search</ShortName><Description>Search for series</Description><InputEncoding>UTF-8</InputEncoding><OutputEncoding>UTF-8</OutputEncoding><Url type=\"application/atom+xml;profile=opds-catalog;kind=acquisition\" template=\"{}\"/></OpenSearchDescription>",
        xml_escape(&template_href)
    );

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/xml"),
        )],
        payload,
    )
        .into_response()
}

pub(crate) async fn opds_v1_on_deck(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let user_id = user_id(&user).to_string();
    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let restrictions = opds_restrictions(&headers);
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let books = load_on_deck_books(database_file, &user_id, None)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|book| {
            library_visible(&allowed_library_ids, &book.library_id)
                && content_allowed_by_restrictions(
                    restrictions.as_ref(),
                    book.age_rating,
                    &book.sharing_labels,
                )
        })
        .collect::<Vec<_>>();
    let (books, has_next) = paginate_vec(books, page, size);

    let entries = build_book_feed_acquisition_entries(database_file, &headers, books).await;

    opds_v1_acquisition_feed_response_with_entries(
        &headers,
        "ondeck",
        "On Deck",
        "/opds/v1.2/ondeck",
        entries,
        None,
        Some((page, has_next)),
    )
}

pub(crate) async fn opds_v1_keep_reading(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let user_id = user_id(&user).to_string();
    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let restrictions = opds_restrictions(&headers);

    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let books = load_keep_reading_books(database_file, &user_id, None)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|book| {
            library_visible(&allowed_library_ids, &book.library_id)
                && content_allowed_by_restrictions(
                    restrictions.as_ref(),
                    book.age_rating,
                    &book.sharing_labels,
                )
        })
        .collect::<Vec<_>>();
    let (books, has_next) = paginate_vec(books, page, size);

    let entries = build_book_feed_acquisition_entries(database_file, &headers, books).await;

    opds_v1_acquisition_feed_response_with_entries(
        &headers,
        "keepReading",
        "Keep Reading",
        "/opds/v1.2/keep-reading",
        entries,
        None,
        Some((page, has_next)),
    )
}

pub(crate) async fn opds_v1_series_latest(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let restrictions = opds_restrictions(&headers);

    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let visible_offset = page.saturating_mul(size);
    let mut raw_offset = 0_i64;
    let batch_limit = (size + 1).max(20) as i64;
    let mut visible_seen = 0usize;
    let mut rows = Vec::with_capacity(size + 1);
    let has_next = loop {
        let batch = load_latest_series_paged(
            database_file,
            &allowed_library_ids,
            None,
            raw_offset,
            batch_limit,
        )
        .await
        .unwrap_or_default();
        if batch.is_empty() {
            break false;
        }
        let batch_len = batch.len();
        raw_offset += batch_len as i64;

        for series in batch.iter().filter(|series| {
            library_visible(&allowed_library_ids, &series.library_id)
                && content_allowed_by_restrictions(
                    restrictions.as_ref(),
                    series.age_rating,
                    &series.sharing_labels,
                )
        }) {
            if visible_seen < visible_offset {
                visible_seen += 1;
                continue;
            }
            rows.push(series.clone());
            if rows.len() > size {
                break;
            }
        }

        if rows.len() > size {
            break true;
        }
        if batch_len < batch_limit as usize {
            break false;
        }
    };
    let rows = rows.into_iter().take(size).collect::<Vec<_>>();

    opds_v1_navigation_feed_response(
        &headers,
        "latestSeries",
        "Latest series",
        "/opds/v1.2/series/latest",
        rows.into_iter()
            .map(|series| {
                let series_id = series.id.clone();
                OpdsV1NavigationEntry {
                    id: series_id.clone(),
                    title: series.title,
                    content: String::new(),
                    href_path: format!("/opds/v1.2/series/{series_id}"),
                    updated: Some(series.last_modified),
                }
            })
            .collect(),
        Some((page, has_next)),
    )
}

pub(crate) async fn opds_v1_books_latest(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let current_user_id = user_id(&user).to_string();

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let restrictions = opds_restrictions(&headers);

    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let visible_offset = page.saturating_mul(size);
    let mut raw_offset = 0_i64;
    let batch_limit = (size + 1).max(20) as i64;
    let mut visible_seen = 0usize;
    let mut books = Vec::with_capacity(size + 1);
    let has_next = loop {
        let batch = load_latest_books_paged(
            database_file,
            &allowed_library_ids,
            Some(&current_user_id),
            None,
            raw_offset,
            batch_limit,
        )
        .await
        .unwrap_or_default();
        if batch.is_empty() {
            break false;
        }
        let batch_len = batch.len();
        raw_offset += batch_len as i64;

        for book in batch.into_iter().filter(|book| {
            library_visible(&allowed_library_ids, &book.library_id)
                && content_allowed_by_restrictions(
                    restrictions.as_ref(),
                    book.age_rating,
                    &book.sharing_labels,
                )
        }) {
            if visible_seen < visible_offset {
                visible_seen += 1;
                continue;
            }
            books.push(book);
            if books.len() > size {
                break;
            }
        }

        if books.len() > size {
            break true;
        }
        if batch_len < batch_limit as usize {
            break false;
        }
    };
    let books = books.into_iter().take(size).collect::<Vec<_>>();
    let entries = build_book_feed_acquisition_entries(database_file, &headers, books).await;
    opds_v1_acquisition_feed_response_with_entries(
        &headers,
        "latestBooks",
        "Latest books",
        "/opds/v1.2/books/latest",
        entries,
        None,
        Some((page, has_next)),
    )
}

async fn build_book_feed_acquisition_entries(
    database_file: &Path,
    headers: &HeaderMap,
    books: Vec<PersistedBookFeedItem>,
) -> Vec<OpdsV1AcquisitionEntry> {
    let mut entries = Vec::with_capacity(books.len());
    for book in books {
        let extra_links = book_feed_page_streaming_links(database_file, headers, &book).await;
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

pub(crate) async fn opds_v1_libraries(headers: HeaderMap, database_file: &Path) -> Response {
    if let Some(response) = require_auth(&headers) {
        let _ = response;
        return opds_v1_basic_unauthorized_response();
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let rows = load_libraries(database_file)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|library| library_visible(&allowed_library_ids, &library.id))
        .collect::<Vec<_>>();
    let rows = rows
        .into_iter()
        .map(|library| OpdsV1NavigationEntry {
            id: library.id.clone(),
            title: library.name,
            content: String::new(),
            href_path: format!("/opds/v1.2/libraries/{}", library.id),
            updated: Some(library.last_modified),
        })
        .collect::<Vec<_>>();

    opds_v1_navigation_feed_response(
        &headers,
        "allLibraries",
        "All libraries",
        "/opds/v1.2/libraries",
        rows,
        None,
    )
}

pub(crate) async fn opds_v1_collections(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let mut rows = Vec::new();
    for collection in load_collections(database_file, None)
        .await
        .unwrap_or_default()
    {
        let series = load_collection_series(database_file, &collection.id, collection.ordered)
            .await
            .unwrap_or_default();
        let has_visible_series = series
            .iter()
            .any(|series| library_visible(&allowed_library_ids, &series.library_id));
        let keep_empty_collection_visible = series.is_empty() && allowed_library_ids.is_none();
        let visible = has_visible_series || keep_empty_collection_visible;
        if visible {
            let updated = localized_opds_updated(&collection.last_modified);
            rows.push(OpdsV1NavigationEntry {
                id: collection.id.clone(),
                title: collection.name,
                content: String::new(),
                href_path: format!("/opds/v1.2/collections/{}", collection.id),
                updated,
            });
        }
    }

    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let (rows, has_next) = paginate_vec(rows, page, size);

    opds_v1_navigation_feed_response(
        &headers,
        "allCollections",
        "All collections",
        "/opds/v1.2/collections",
        rows,
        Some((page, has_next)),
    )
}

fn localized_opds_updated(value: &str) -> Option<String> {
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

pub(crate) async fn opds_v1_readlists(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let mut rows = Vec::new();
    for readlist in load_all_readlists(database_file).await.unwrap_or_default() {
        let books = load_readlist_books(database_file, &readlist.id)
            .await
            .unwrap_or_default();
        if books
            .iter()
            .any(|book| library_visible(&allowed_library_ids, &book.library_id))
        {
            rows.push(OpdsV1NavigationEntry {
                id: readlist.id.clone(),
                title: readlist.name,
                content: String::new(),
                href_path: format!("/opds/v1.2/readlists/{}", readlist.id),
                updated: Some(readlist.last_modified),
            });
        }
    }

    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let (rows, has_next) = paginate_vec(rows, page, size);

    opds_v1_navigation_feed_response(
        &headers,
        "allReadLists",
        "All read lists",
        "/opds/v1.2/readlists",
        rows,
        Some((page, has_next)),
    )
}

pub(crate) async fn opds_v1_publishers(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        let _ = response;
        return opds_v1_basic_unauthorized_response();
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let publishers = load_publishers(database_file, &allowed_library_ids)
        .await
        .unwrap_or_default();
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let rows = publishers
        .into_iter()
        .map(|publisher| OpdsV1NavigationEntry {
            id: publisher_entry_id(&publisher),
            title: publisher.clone(),
            content: String::new(),
            href_path: format!("/opds/v1.2/series?publisher={}", query_escape(&publisher)),
            updated: None,
        })
        .collect::<Vec<_>>();
    let (rows, has_next) = paginate_vec(rows, page, size);
    opds_v1_navigation_feed_response(
        &headers,
        "allPublishers",
        "All publishers",
        "/opds/v1.2/publishers",
        rows,
        Some((page, has_next)),
    )
}

pub(crate) async fn opds_v1_series_detail(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
    series_id: &str,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let current_user_id = user_id(&user).to_string();

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let Some(series) = load_series(database_file, series_id).await.unwrap_or(None) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !library_visible(&allowed_library_ids, &series.library_id) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let restrictions = opds_restrictions(&headers);
    if !content_allowed_by_restrictions(
        restrictions.as_ref(),
        series.age_rating,
        &series.sharing_labels,
    ) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let feed_updated = localized_opds_updated(&series.last_modified);
    let books = load_series_books_paged(
        database_file,
        &series.id,
        &current_user_id,
        page.saturating_mul(size) as i64,
        (size + 1) as i64,
    )
    .await
    .unwrap_or_default()
    .into_iter();
    let mut entries = Vec::new();
    for book in books {
        let updated = localized_opds_updated(&book.last_modified);
        let extra_links = series_book_page_streaming_links(database_file, &headers, &book).await;
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
            title: book.title,
            updated,
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
    let (entries, has_next) = paginate_vec(entries, 0, size);
    opds_v1_acquisition_feed_response_with_entries(
        &headers,
        series.id.as_str(),
        series.title.as_str(),
        format!("/opds/v1.2/series/{series_id}").as_str(),
        entries,
        feed_updated.as_deref(),
        Some((page, has_next)),
    )
}

async fn series_book_page_streaming_links(
    database_file: &Path,
    headers: &HeaderMap,
    book: &PersistedSeriesBook,
) -> Vec<String> {
    opds_book_page_streaming_links(
        database_file,
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
    database_file: &Path,
    headers: &HeaderMap,
    book: &PersistedBookFeedItem,
) -> Vec<String> {
    opds_book_page_streaming_links(
        database_file,
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

async fn opds_book_page_streaming_links(
    database_file: &Path,
    headers: &HeaderMap,
    book_id: &str,
    media_type: &str,
    page_count: i64,
    epub_divina_compatible: bool,
    last_read: Option<i64>,
    last_read_date: Option<&str>,
) -> Vec<String> {
    let media_types = opds_book_page_stream_media_types(
        database_file,
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
    database_file: &Path,
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
        return load_divina_page_media_types_for_opds(database_file, book_id).await;
    }

    vec![]
}

async fn load_divina_page_media_types_for_opds(database_file: &Path, book_id: &str) -> Vec<String> {
    let persisted = load_persisted_book_pages(database_file, book_id)
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

    let Ok(Some(media)) = load_persisted_book_media(database_file, book_id).await else {
        return vec![];
    };

    let media_content_type = content_type_from_filename(&media.file_name, &media.media_type);
    if media_content_type.starts_with("image/") {
        return vec![media_content_type];
    }

    dedup_media_types(
        load_archive_page_rows(&media)
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

pub(crate) async fn opds_v1_library_detail(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
    library_id: &str,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(library) = load_library(database_file, library_id)
        .await
        .unwrap_or(None)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !library_visible(&allowed_library_ids, library_id) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let query = uri.query().unwrap_or_default();
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .clamp(1, 100);
    let visible_offset = page.saturating_mul(size);
    let restrictions = opds_restrictions(&headers);
    let mut raw_offset = 0_i64;
    let batch_limit = (size + 1).max(20) as i64;
    let mut visible_seen = 0usize;
    let mut entries = Vec::with_capacity(size + 1);
    let has_next = loop {
        let batch = load_library_series(database_file, library_id, raw_offset, batch_limit)
            .await
            .unwrap_or_default();
        if batch.is_empty() {
            break false;
        }
        let batch_len = batch.len();
        raw_offset += batch_len as i64;

        for item in batch.into_iter().filter(|item| {
            library_visible(&allowed_library_ids, &item.library_id)
                && content_allowed_by_restrictions(
                    restrictions.as_ref(),
                    item.age_rating,
                    &item.sharing_labels,
                )
        }) {
            if visible_seen < visible_offset {
                visible_seen += 1;
                continue;
            }
            entries.push(item);
            if entries.len() > size {
                break;
            }
        }

        if entries.len() > size {
            break true;
        }
        if batch_len < batch_limit as usize {
            break false;
        }
    };
    let entries = entries.into_iter().take(size).collect::<Vec<_>>();

    opds_v1_library_series_feed_response(
        &headers,
        library.id.as_str(),
        library.name.as_str(),
        format!("/opds/v1.2/libraries/{library_id}").as_str(),
        entries,
        Some(library.last_modified.as_str()),
        page,
        has_next,
    )
}

pub(crate) async fn opds_v1_collection_detail(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
    collection_id: &str,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(collection) = load_collection(database_file, collection_id)
        .await
        .unwrap_or(None)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let restrictions = opds_restrictions(&headers);

    let visible_series = load_collection_series(database_file, collection_id, collection.ordered)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|series| library_visible(&allowed_library_ids, &series.library_id))
        .collect::<Vec<_>>();
    if visible_series.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let series = visible_series
        .into_iter()
        .filter(|series| {
            content_allowed_by_restrictions(
                restrictions.as_ref(),
                series.age_rating,
                &series.sharing_labels,
            )
        })
        .collect::<Vec<_>>();
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let (series, has_next) = paginate_vec(series, page, size);

    let entries = series
        .into_iter()
        .map(|series| {
            let id = series.id;
            OpdsV1NavigationEntry {
                id: id.clone(),
                title: series.title,
                content: String::new(),
                href_path: format!("/opds/v1.2/series/{id}"),
                updated: Some(series.last_modified),
            }
        })
        .collect::<Vec<_>>();

    opds_v1_navigation_feed_response_with_feed_updated(
        &headers,
        collection.id.as_str(),
        collection.name.as_str(),
        format!("/opds/v1.2/collections/{collection_id}").as_str(),
        entries,
        Some(collection.last_modified.as_str()),
        Some((page, has_next)),
    )
}

pub(crate) async fn opds_v1_readlist_detail(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
    readlist_id: &str,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(readlist) = load_readlist(database_file, readlist_id)
        .await
        .unwrap_or(None)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let restrictions = opds_restrictions(&headers);

    let visible_books = load_readlist_books(database_file, readlist_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|book| library_visible(&allowed_library_ids, &book.library_id))
        .collect::<Vec<_>>();
    if visible_books.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let mut filtered_books = visible_books
        .into_iter()
        .filter(|book| {
            book.media_status.as_deref() == Some("READY")
                && content_allowed_by_restrictions(
                    restrictions.as_ref(),
                    book.age_rating,
                    &book.sharing_labels,
                )
        })
        .collect::<Vec<_>>();
    if !readlist.ordered {
        filtered_books.sort_by_key(|book| book.release_date.clone());
    }

    let entries = filtered_books
        .into_iter()
        .map(|book| {
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

            OpdsV1AcquisitionEntry {
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
                extra_links: vec![],
            }
        })
        .collect::<Vec<_>>();
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let (entries, has_next) = paginate_vec(entries, page, size);
    opds_v1_acquisition_feed_response_with_entries(
        &headers,
        readlist.id.as_str(),
        readlist.name.as_str(),
        format!("/opds/v1.2/readlists/{readlist_id}").as_str(),
        entries,
        Some(readlist.last_modified.as_str()),
        Some((page, has_next)),
    )
}

pub(crate) async fn opds_v1_series(headers: HeaderMap, uri: Uri, database_file: &Path) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let query = uri.query().unwrap_or_default();
    let (page, size) = parse_page_size(query);
    let search = query_value(query, "search")
        .map(percent_decode)
        .and_then(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then_some(trimmed.to_string())
        });
    let publishers = query_values(query, "publisher");
    let self_path = series_feed_self_path(search.as_deref(), publishers.as_slice());

    let search_rows = if let Some(search_term) = search.as_deref() {
        load_opds_v1_series_search_results(
            database_file,
            &allowed_library_ids,
            search_term,
            publishers.as_slice(),
        )
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|series| {
            let series_id = series.id;
            OpdsV1NavigationEntry {
                id: series_id.clone(),
                title: series.title,
                content: String::new(),
                href_path: format!("/opds/v1.2/series/{series_id}"),
                updated: Some(series.last_modified),
            }
        })
        .collect::<Vec<_>>()
    } else {
        load_series_page(
            database_file,
            &allowed_library_ids,
            None,
            publishers.as_slice(),
            page.saturating_mul(size) as i64,
            (size + 1) as i64,
        )
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|series| {
            let series_id = series.id;
            OpdsV1NavigationEntry {
                id: series_id.clone(),
                title: series.title,
                content: String::new(),
                href_path: format!("/opds/v1.2/series/{series_id}"),
                updated: Some(series.last_modified),
            }
        })
        .collect::<Vec<_>>()
    };
    let (entries, has_next) = if search.is_some() {
        paginate_vec(search_rows, page, size)
    } else {
        paginate_vec(search_rows, 0, size)
    };

    opds_v1_navigation_feed_response(
        &headers,
        "allSeries",
        search
            .as_deref()
            .map(|term| format!("Series search for: {term}"))
            .unwrap_or_else(|| "All series".to_string())
            .as_str(),
        self_path.as_str(),
        entries,
        Some((page, has_next)),
    )
}

fn series_feed_self_path(search: Option<&str>, publishers: &[String]) -> String {
    let mut query_parts = Vec::new();
    if let Some(search) = search {
        query_parts.push(format!("search={}", query_escape(search)));
    }
    for publisher in publishers {
        query_parts.push(format!("publisher={}", query_escape(publisher)));
    }

    if query_parts.is_empty() {
        "/opds/v1.2/series".to_string()
    } else {
        format!("/opds/v1.2/series?{}", query_parts.join("&"))
    }
}

fn nav_entry_with_content(
    id: &str,
    title: &str,
    content: &str,
    href_path: &str,
) -> OpdsV1NavigationEntry {
    OpdsV1NavigationEntry {
        id: id.to_string(),
        title: title.to_string(),
        content: content.to_string(),
        href_path: href_path.to_string(),
        updated: None,
    }
}

fn publisher_entry_id(publisher: &str) -> String {
    format!("publisher:{}", query_escape(publisher))
}

#[cfg(test)]
mod tests {
    use super::publisher_entry_id;

    #[test]
    fn publisher_entry_id_matches_kotlin_prefix_and_encoding() {
        assert_eq!(publisher_entry_id("ACME Press"), "publisher:ACME%20Press");
    }
}

use super::streaming::{localized_opds_updated, series_book_page_streaming_links};
use super::*;
use crate::identity_access::auth::{AuthUser, user_id};
use crate::opds::types::PersistedSeries;
use crate::state::{OpdsFeedUserContext, OpdsPersistedService, OpdsSeriesEntry, OpdsState};

fn persisted_series(entry: OpdsSeriesEntry) -> PersistedSeries {
    PersistedSeries {
        id: entry.id,
        library_id: entry.library_id,
        title: entry.title,
        summary: String::new(),
        age_rating: entry.age_rating,
        sharing_labels: entry.sharing_labels,
        last_modified: entry.last_modified,
    }
}

pub(crate) async fn opds_v1_series_detail(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    series_id: &str,
    user: &AuthUser,
) -> Response {
    let current_user_id = user_id(user).to_string();

    let allowed_library_ids = allowed_library_ids_for_user(user);

    let Some(series) = load_series(app.opds_persisted.as_ref(), series_id)
        .await
        .unwrap_or(None)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !library_visible(&allowed_library_ids, &series.library_id) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let restrictions = opds_restrictions_for_user(user);
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
        app.opds_persisted.as_ref(),
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
        let extra_links = series_book_page_streaming_links(app, &headers, &book).await;
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
            authors: book.authors.into_iter().map(|author| author.name).collect(),
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

pub(crate) async fn opds_v1_library_detail(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    library_id: &str,
    user: &AuthUser,
) -> Response {
    let allowed_library_ids = allowed_library_ids_for_user(user);
    let Some(library) = load_library(app.opds_persisted.as_ref(), library_id)
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
    let restrictions = opds_restrictions_for_user(user);
    let mut raw_offset = 0_i64;
    let batch_limit = (size + 1).max(20) as i64;
    let mut visible_seen = 0usize;
    let mut entries = Vec::with_capacity(size + 1);
    let has_next = loop {
        let batch = app
            .opds_catalog
            .load_library_series(library_id, raw_offset, batch_limit)
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
    let entries = entries
        .into_iter()
        .take(size)
        .map(persisted_series)
        .collect::<Vec<_>>();

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
    app: &OpdsState,
    collection_id: &str,
    user: &AuthUser,
) -> Response {
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let persisted_service = OpdsPersistedService::new(app.opds_persisted.as_ref());
    let Some(detail) = persisted_service
        .collection_detail(&feed_user, collection_id)
        .await
        .unwrap_or(None)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let collection = detail.collection;
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let (series, has_next) = paginate_vec(detail.series, page, size);

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
    app: &OpdsState,
    readlist_id: &str,
    user: &AuthUser,
) -> Response {
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let persisted_service = OpdsPersistedService::new(app.opds_persisted.as_ref());
    let Some(detail) = persisted_service
        .readlist_detail(&feed_user, readlist_id)
        .await
        .unwrap_or(None)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let readlist = detail.readlist;

    let entries = detail
        .books
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
                authors: book.authors.into_iter().map(|author| author.name).collect(),
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

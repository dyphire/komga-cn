use std::path::Path;

use super::streaming::{localized_opds_updated, series_book_page_streaming_links};
use crate::state::OpdsState;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_application::identity_access::{AuthUser, user_id};
use komga_application::opds::{
    OpdsBookFeedEntry, OpdsFeedService, OpdsFeedUserContext, OpdsPersistedService,
    OpdsSeriesAccessError, OpdsSeriesEntry,
};

use super::super::feeds::{
    OpdsPageNavigation, OpdsPageRequest, OpdsV1AcquisitionEntry, OpdsV1FeedHeader,
    OpdsV1NavigationEntry, opds_v1_acquisition_feed_response_with_entries,
    opds_v1_library_series_feed_response, opds_v1_navigation_feed_response, paginate_vec,
    parse_page_size, query_escape,
};
use super::super::persisted::{allowed_library_ids_for_user, library_visible, load_library};
use super::super::types::PersistedSeries;

fn persisted_series(entry: OpdsSeriesEntry) -> PersistedSeries {
    PersistedSeries {
        id: entry.id,
        title: entry.title,
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
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let persisted_service = OpdsPersistedService::new(app.opds_series_persisted.as_ref());
    let series = match persisted_service
        .visible_series(&feed_user, series_id)
        .await
    {
        Ok(series) => series,
        Err(OpdsSeriesAccessError::Forbidden) => return StatusCode::FORBIDDEN.into_response(),
        Err(OpdsSeriesAccessError::NotFound) => return StatusCode::NOT_FOUND.into_response(),
        Err(OpdsSeriesAccessError::Load(_)) => {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let page_request = parse_page_size(uri.query().unwrap_or_default());
    let feed_updated = localized_opds_updated(&series.last_modified);
    let books = match persisted_service
        .series_books_page(
            &feed_user,
            &series.id,
            &current_user_id,
            page_request.page.saturating_mul(page_request.size) as i64,
            (page_request.size + 1) as i64,
        )
        .await
    {
        Ok(books) => books,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
    .into_iter();
    let mut entries = Vec::new();
    for book in books.map(OpdsBookFeedEntry::from) {
        let updated = localized_opds_updated(&book.last_modified);
        let extra_links = match series_book_page_streaming_links(app, &headers, &book).await {
            Ok(extra_links) => extra_links,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        let extension = Path::new(book.file_name.as_str())
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
    let entries_page = paginate_vec(
        entries,
        OpdsPageRequest {
            page: 0,
            size: page_request.size,
        },
    );
    opds_v1_acquisition_feed_response_with_entries(
        &headers,
        series.id.as_str(),
        series.title.as_str(),
        format!("/opds/v1.2/series/{series_id}").as_str(),
        entries_page.items,
        feed_updated.as_deref(),
        Some(OpdsPageNavigation {
            page: page_request.page,
            has_next: entries_page.has_next,
        }),
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
    let library = match load_library(app.opds_library_persisted.as_ref(), library_id).await {
        Ok(Some(library)) => library,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !library_visible(&allowed_library_ids, library_id) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let page_request = parse_page_size(uri.query().unwrap_or_default());
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let feed_service = OpdsFeedService::new(app.opds_feed_catalog.as_ref());
    let page_result = match feed_service
        .library_series_page(&feed_user, library_id, page_request.page, page_request.size)
        .await
    {
        Ok(page) => page,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let entries = page_result
        .series
        .into_iter()
        .map(persisted_series)
        .collect::<Vec<_>>();

    opds_v1_library_series_feed_response(
        OpdsV1FeedHeader {
            headers: &headers,
            feed_id: library.id.as_str(),
            title: library.name.as_str(),
            self_path: format!("/opds/v1.2/libraries/{library_id}").as_str(),
            feed_updated: Some(library.last_modified.as_str()),
            pagination: Some(OpdsPageNavigation {
                page: page_request.page,
                has_next: page_result.has_next,
            }),
        },
        entries,
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
    let persisted_service =
        OpdsPersistedService::new(app.opds_collection_detail_persisted.as_ref());
    let detail = match persisted_service
        .collection_detail(&feed_user, collection_id)
        .await
    {
        Ok(Some(detail)) => detail,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let collection = detail.collection;
    let page_request = parse_page_size(uri.query().unwrap_or_default());
    let series_page = paginate_vec(detail.series, page_request);

    let entries = series_page
        .items
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

    opds_v1_navigation_feed_response(
        OpdsV1FeedHeader {
            headers: &headers,
            feed_id: collection.id.as_str(),
            title: collection.name.as_str(),
            self_path: format!("/opds/v1.2/collections/{collection_id}").as_str(),
            feed_updated: Some(collection.last_modified.as_str()),
            pagination: Some(OpdsPageNavigation {
                page: page_request.page,
                has_next: series_page.has_next,
            }),
        },
        entries,
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
    let persisted_service = OpdsPersistedService::new(app.opds_readlist_detail_persisted.as_ref());
    let detail = match persisted_service
        .readlist_detail(&feed_user, readlist_id)
        .await
    {
        Ok(Some(detail)) => detail,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
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
    let page_request = parse_page_size(uri.query().unwrap_or_default());
    let entries_page = paginate_vec(entries, page_request);
    opds_v1_acquisition_feed_response_with_entries(
        &headers,
        readlist.id.as_str(),
        readlist.name.as_str(),
        format!("/opds/v1.2/readlists/{readlist_id}").as_str(),
        entries_page.items,
        Some(readlist.last_modified.as_str()),
        Some(OpdsPageNavigation {
            page: page_request.page,
            has_next: entries_page.has_next,
        }),
    )
}

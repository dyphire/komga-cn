use super::helpers::{nav_entry_with_content, publisher_entry_id, series_feed_self_path};
use super::streaming::{build_book_feed_acquisition_entries, localized_opds_updated};
use crate::state::OpdsState;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use komga_application::identity_access::AuthUser;
use komga_application::opds::{
    OpdsBookFeedEntry, OpdsFeedService, OpdsFeedUserContext, OpdsPersistedService,
};

use crate::request_urls::app_absolute_url;

use super::super::feeds::{
    OpdsPageNavigation, OpdsPageRequest, OpdsV1FeedHeader, OpdsV1NavigationEntry, OpdsV1XmlLink,
    opds_v1_acquisition_feed_response_with_entries, opds_v1_navigation_feed_response,
    opds_v1_navigation_feed_response_with_extra_links, paginate_vec, parse_page_size,
    percent_decode, query_escape, query_value, query_values, xml_escape,
};
use super::super::persisted::{allowed_library_ids_for_user, load_opds_v1_series_search_results};
use super::super::types::PersistedBookFeedItem;

fn persisted_book_feed_item(entry: OpdsBookFeedEntry) -> PersistedBookFeedItem {
    PersistedBookFeedItem {
        id: entry.id,
        title: entry.title,
        series_title: entry.series_title,
        number: entry.number,
        summary: entry.summary,
        authors: entry
            .authors
            .into_iter()
            .map(|author| author.name)
            .collect(),
        file_name: entry.file_name,
        file_size: entry.file_size,
        media_type: entry.media_type,
        page_count: entry.page_count,
        epub_divina_compatible: entry.epub_divina_compatible,
        last_read: entry.last_read,
        last_read_date: entry.last_read_date,
        last_modified: entry.last_modified,
    }
}

pub(crate) async fn opds_v1_catalog(_app: &OpdsState, headers: HeaderMap) -> Response {
    let search_href = app_absolute_url(&headers, "/opds/v1.2/search");
    let alternate_href = app_absolute_url(&headers, "/opds/v2/catalog");
    opds_v1_navigation_feed_response_with_extra_links(
        OpdsV1FeedHeader {
            headers: &headers,
            feed_id: "root",
            title: "Komga OPDS catalog",
            self_path: "/opds/v1.2/catalog",
            feed_updated: None,
            pagination: None,
        },
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
        vec![
            OpdsV1XmlLink::new(
                "application/opensearchdescription+xml",
                "search",
                search_href,
            ),
            OpdsV1XmlLink::new("application/opds+json", "alternate", alternate_href),
        ],
    )
}

pub(crate) async fn opds_v1_search(_app: &OpdsState, headers: HeaderMap) -> Response {
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
    app: &OpdsState,
    user: &AuthUser,
) -> Response {
    let page_request = parse_page_size(uri.query().unwrap_or_default());
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let feed_service = OpdsFeedService::new(app.opds_feed_catalog.as_ref());
    let page_result = match feed_service
        .on_deck_page(&feed_user, None, page_request.page, page_request.size)
        .await
    {
        Ok(page) => page,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let has_next = page_result.has_next;
    let books = page_result
        .books
        .into_iter()
        .map(persisted_book_feed_item)
        .collect::<Vec<_>>();

    let entries = match build_book_feed_acquisition_entries(app, &headers, books).await {
        Ok(entries) => entries,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    opds_v1_acquisition_feed_response_with_entries(
        &headers,
        "ondeck",
        "On Deck",
        "/opds/v1.2/ondeck",
        entries,
        None,
        Some(OpdsPageNavigation {
            page: page_request.page,
            has_next,
        }),
    )
}

pub(crate) async fn opds_v1_keep_reading(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    user: &AuthUser,
) -> Response {
    let page_request = parse_page_size(uri.query().unwrap_or_default());
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let feed_service = OpdsFeedService::new(app.opds_feed_catalog.as_ref());
    let page_result = match feed_service
        .keep_reading_page(&feed_user, None, page_request.page, page_request.size)
        .await
    {
        Ok(page) => page,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let has_next = page_result.has_next;
    let books = page_result
        .books
        .into_iter()
        .map(persisted_book_feed_item)
        .collect::<Vec<_>>();

    let entries = match build_book_feed_acquisition_entries(app, &headers, books).await {
        Ok(entries) => entries,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    opds_v1_acquisition_feed_response_with_entries(
        &headers,
        "keepReading",
        "Keep Reading",
        "/opds/v1.2/keep-reading",
        entries,
        None,
        Some(OpdsPageNavigation {
            page: page_request.page,
            has_next,
        }),
    )
}

pub(crate) async fn opds_v1_series_latest(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    user: &AuthUser,
) -> Response {
    let page_request = parse_page_size(uri.query().unwrap_or_default());
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let feed_service = OpdsFeedService::new(app.opds_feed_catalog.as_ref());
    let page_result = match feed_service
        .latest_series_page_including_one_shots(
            &feed_user,
            None,
            page_request.page,
            page_request.size,
        )
        .await
    {
        Ok(page) => page,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let has_next = page_result.has_next;
    let rows = page_result
        .series
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
        .collect::<Vec<_>>();

    opds_v1_navigation_feed_response(
        OpdsV1FeedHeader {
            headers: &headers,
            feed_id: "latestSeries",
            title: "Latest series",
            self_path: "/opds/v1.2/series/latest",
            feed_updated: None,
            pagination: Some(OpdsPageNavigation {
                page: page_request.page,
                has_next,
            }),
        },
        rows,
    )
}

pub(crate) async fn opds_v1_books_latest(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    user: &AuthUser,
) -> Response {
    let page_request = parse_page_size(uri.query().unwrap_or_default());
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let feed_service = OpdsFeedService::new(app.opds_feed_catalog.as_ref());
    let page_result = match feed_service
        .latest_books_page_with_read_progress(
            &feed_user,
            None,
            page_request.page,
            page_request.size,
        )
        .await
    {
        Ok(page) => page,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let has_next = page_result.has_next;
    let books = page_result
        .books
        .into_iter()
        .map(persisted_book_feed_item)
        .collect::<Vec<_>>();
    let entries = match build_book_feed_acquisition_entries(app, &headers, books).await {
        Ok(entries) => entries,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    opds_v1_acquisition_feed_response_with_entries(
        &headers,
        "latestBooks",
        "Latest books",
        "/opds/v1.2/books/latest",
        entries,
        None,
        Some(OpdsPageNavigation {
            page: page_request.page,
            has_next,
        }),
    )
}

pub(crate) async fn opds_v1_libraries(
    headers: HeaderMap,
    app: &OpdsState,
    user: &AuthUser,
) -> Response {
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let persisted_service = OpdsPersistedService::new(app.opds_library_persisted.as_ref());

    let rows = match persisted_service.visible_libraries(&feed_user).await {
        Ok(libraries) => libraries,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
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
        OpdsV1FeedHeader {
            headers: &headers,
            feed_id: "allLibraries",
            title: "All libraries",
            self_path: "/opds/v1.2/libraries",
            feed_updated: None,
            pagination: None,
        },
        rows,
    )
}

pub(crate) async fn opds_v1_collections(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    user: &AuthUser,
) -> Response {
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let persisted_service = OpdsPersistedService::new(app.opds_feed_persisted.as_ref());
    let rows = match persisted_service
        .all_collections(&feed_user, None, true)
        .await
    {
        Ok(collections) => collections,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
    .into_iter()
    .map(|collection| {
        let updated = localized_opds_updated(&collection.last_modified);
        OpdsV1NavigationEntry {
            id: collection.id.clone(),
            title: collection.name,
            content: String::new(),
            href_path: format!("/opds/v1.2/collections/{}", collection.id),
            updated,
        }
    })
    .collect::<Vec<_>>();

    let page_request = parse_page_size(uri.query().unwrap_or_default());
    let rows_page = paginate_vec(rows, page_request);

    opds_v1_navigation_feed_response(
        OpdsV1FeedHeader {
            headers: &headers,
            feed_id: "allCollections",
            title: "All collections",
            self_path: "/opds/v1.2/collections",
            feed_updated: None,
            pagination: Some(OpdsPageNavigation {
                page: page_request.page,
                has_next: rows_page.has_next,
            }),
        },
        rows_page.items,
    )
}

pub(crate) async fn opds_v1_readlists(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    user: &AuthUser,
) -> Response {
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let persisted_service = OpdsPersistedService::new(app.opds_feed_persisted.as_ref());
    let rows = match persisted_service.all_readlists(&feed_user, None).await {
        Ok(readlists) => readlists,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
    .into_iter()
    .map(|readlist| OpdsV1NavigationEntry {
        id: readlist.id.clone(),
        title: readlist.name,
        content: String::new(),
        href_path: format!("/opds/v1.2/readlists/{}", readlist.id),
        updated: Some(readlist.last_modified),
    })
    .collect::<Vec<_>>();

    let page_request = parse_page_size(uri.query().unwrap_or_default());
    let rows_page = paginate_vec(rows, page_request);

    opds_v1_navigation_feed_response(
        OpdsV1FeedHeader {
            headers: &headers,
            feed_id: "allReadLists",
            title: "All read lists",
            self_path: "/opds/v1.2/readlists",
            feed_updated: None,
            pagination: Some(OpdsPageNavigation {
                page: page_request.page,
                has_next: rows_page.has_next,
            }),
        },
        rows_page.items,
    )
}

pub(crate) async fn opds_v1_publishers(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    user: &AuthUser,
) -> Response {
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let persisted_service = OpdsPersistedService::new(app.opds_publisher_persisted.as_ref());
    let publishers = match persisted_service.publishers(&feed_user).await {
        Ok(publishers) => publishers,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let page_request = parse_page_size(uri.query().unwrap_or_default());
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
    let rows_page = paginate_vec(rows, page_request);
    opds_v1_navigation_feed_response(
        OpdsV1FeedHeader {
            headers: &headers,
            feed_id: "allPublishers",
            title: "All publishers",
            self_path: "/opds/v1.2/publishers",
            feed_updated: None,
            pagination: Some(OpdsPageNavigation {
                page: page_request.page,
                has_next: rows_page.has_next,
            }),
        },
        rows_page.items,
    )
}

pub(crate) async fn opds_v1_series(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    user: &AuthUser,
) -> Response {
    let allowed_library_ids = allowed_library_ids_for_user(user);

    let query = uri.query().unwrap_or_default();
    let page_request = parse_page_size(query);
    let search = query_value(query, "search")
        .map(percent_decode)
        .and_then(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then_some(trimmed.to_string())
        });
    let publishers = query_values(query, "publisher");
    let self_path = series_feed_self_path(search.as_deref(), publishers.as_slice());

    let search_rows = if let Some(search_term) = search.as_deref() {
        match load_opds_v1_series_search_results(
            app.opds_search_persisted.as_ref(),
            app.opds_browse_catalog.as_ref(),
            &allowed_library_ids,
            search_term,
            publishers.as_slice(),
        )
        .await
        {
            Ok(series) => series,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
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
        match app
            .opds_browse_catalog
            .load_series_page(
                allowed_library_ids.as_ref(),
                None,
                &publishers,
                page_request.page.saturating_mul(page_request.size) as i64,
                (page_request.size + 1) as i64,
            )
            .await
        {
            Ok(series) => series,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
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
    let entries_page = if search.is_some() {
        paginate_vec(search_rows, page_request)
    } else {
        paginate_vec(
            search_rows,
            OpdsPageRequest {
                page: 0,
                size: page_request.size,
            },
        )
    };

    opds_v1_navigation_feed_response(
        OpdsV1FeedHeader {
            headers: &headers,
            feed_id: "allSeries",
            title: search
                .as_deref()
                .map(|term| format!("Series search for: {term}"))
                .unwrap_or_else(|| "All series".to_string())
                .as_str(),
            self_path: self_path.as_str(),
            feed_updated: None,
            pagination: Some(OpdsPageNavigation {
                page: page_request.page,
                has_next: entries_page.has_next,
            }),
        },
        entries_page.items,
    )
}

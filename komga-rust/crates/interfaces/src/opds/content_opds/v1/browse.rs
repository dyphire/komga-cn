use super::helpers::{nav_entry_with_content, publisher_entry_id, series_feed_self_path};
use super::streaming::{build_book_feed_acquisition_entries, localized_opds_updated};
use super::*;
use crate::state::OpdsBookFeedEntry;

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
        library_id: entry.library_id,
        age_rating: entry.age_rating,
        sharing_labels: entry.sharing_labels,
        last_modified: entry.last_modified,
    }
}

pub(crate) async fn opds_v1_catalog(app: &HttpAppState, headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&*app.services.runtime_identity, &headers) {
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

pub(crate) async fn opds_v1_search(app: &HttpAppState, headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&*app.services.runtime_identity, &headers) {
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

pub(crate) async fn opds_v1_on_deck(headers: HeaderMap, uri: Uri, app: &HttpAppState) -> Response {
    if let Some(response) = require_auth(&*app.services.runtime_identity, &headers) {
        return response;
    }

    let Some(user) = resolved_auth_user(&*app.services.runtime_identity, &headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let user_id = user_id(&user).to_string();
    let Some(allowed_library_ids) = allowed_library_ids(&*app.services.runtime_identity, &headers)
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let restrictions = opds_restrictions(&*app.services.runtime_identity, &headers);
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let books = app
        .services
        .opds_catalog
        .load_on_deck_books(user_id.clone(), None)
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
    let books = books
        .into_iter()
        .map(persisted_book_feed_item)
        .collect::<Vec<_>>();

    let entries = build_book_feed_acquisition_entries(app, &headers, books).await;

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
    app: &HttpAppState,
) -> Response {
    if let Some(response) = require_auth(&*app.services.runtime_identity, &headers) {
        return response;
    }

    let Some(user) = resolved_auth_user(&*app.services.runtime_identity, &headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let user_id = user_id(&user).to_string();
    let Some(allowed_library_ids) = allowed_library_ids(&*app.services.runtime_identity, &headers)
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let restrictions = opds_restrictions(&*app.services.runtime_identity, &headers);

    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let books = app
        .services
        .opds_catalog
        .load_keep_reading_books(user_id.clone(), None)
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
    let books = books
        .into_iter()
        .map(persisted_book_feed_item)
        .collect::<Vec<_>>();

    let entries = build_book_feed_acquisition_entries(app, &headers, books).await;

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
    app: &HttpAppState,
) -> Response {
    if let Some(response) = require_auth(&*app.services.runtime_identity, &headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&*app.services.runtime_identity, &headers)
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let restrictions = opds_restrictions(&*app.services.runtime_identity, &headers);

    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let visible_offset = page.saturating_mul(size);
    let mut raw_offset = 0_i64;
    let batch_limit = (size + 1).max(20) as i64;
    let mut visible_seen = 0usize;
    let mut rows = Vec::with_capacity(size + 1);
    let has_next = loop {
        let batch = app
            .services
            .opds_catalog
            .load_latest_series_paged(allowed_library_ids.clone(), None, raw_offset, batch_limit)
            .await
            .unwrap_or_default();
        if batch.is_empty() {
            break false;
        }
        let batch_len = batch.len();
        raw_offset += batch_len as i64;

        for series in batch.into_iter().filter(|series| {
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
            let series_id = series.id;
            rows.push(OpdsV1NavigationEntry {
                id: series_id.clone(),
                title: series.title,
                content: String::new(),
                href_path: format!("/opds/v1.2/series/{series_id}"),
                updated: Some(series.last_modified),
            });
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
        rows,
        Some((page, has_next)),
    )
}

pub(crate) async fn opds_v1_books_latest(
    headers: HeaderMap,
    uri: Uri,
    app: &HttpAppState,
) -> Response {
    if let Some(response) = require_auth(&*app.services.runtime_identity, &headers) {
        return response;
    }

    let Some(user) = resolved_auth_user(&*app.services.runtime_identity, &headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let current_user_id = user_id(&user).to_string();

    let Some(allowed_library_ids) = allowed_library_ids(&*app.services.runtime_identity, &headers)
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let restrictions = opds_restrictions(&*app.services.runtime_identity, &headers);

    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let visible_offset = page.saturating_mul(size);
    let mut raw_offset = 0_i64;
    let batch_limit = (size + 1).max(20) as i64;
    let mut visible_seen = 0usize;
    let mut books = Vec::with_capacity(size + 1);
    let has_next = loop {
        let batch = app
            .services
            .opds_catalog
            .load_latest_books_paged(
                allowed_library_ids.clone(),
                Some(current_user_id.clone()),
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
    let books = books
        .into_iter()
        .take(size)
        .map(persisted_book_feed_item)
        .collect::<Vec<_>>();
    let entries = build_book_feed_acquisition_entries(app, &headers, books).await;
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

pub(crate) async fn opds_v1_libraries(headers: HeaderMap, app: &HttpAppState) -> Response {
    if require_auth(&*app.services.runtime_identity, &headers).is_some() {
        return opds_v1_basic_unauthorized_response();
    }

    let Some(allowed_library_ids) = allowed_library_ids(&*app.services.runtime_identity, &headers)
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let rows = load_libraries(app.services.opds_persisted.as_ref())
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
    app: &HttpAppState,
) -> Response {
    if let Some(response) = require_auth(&*app.services.runtime_identity, &headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&*app.services.runtime_identity, &headers)
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let mut rows = Vec::new();
    for collection in load_collections(app.services.opds_persisted.as_ref(), None)
        .await
        .unwrap_or_default()
    {
        let series = load_collection_series(
            app.services.opds_persisted.as_ref(),
            &collection.id,
            collection.ordered,
        )
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

pub(crate) async fn opds_v1_readlists(
    headers: HeaderMap,
    uri: Uri,
    app: &HttpAppState,
) -> Response {
    if let Some(response) = require_auth(&*app.services.runtime_identity, &headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&*app.services.runtime_identity, &headers)
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let mut rows = Vec::new();
    for readlist in app
        .services
        .opds_catalog
        .load_all_readlists()
        .await
        .unwrap_or_default()
    {
        let books = load_readlist_books(app.services.opds_persisted.as_ref(), &readlist.id)
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
    app: &HttpAppState,
) -> Response {
    if require_auth(&*app.services.runtime_identity, &headers).is_some() {
        return opds_v1_basic_unauthorized_response();
    }

    let Some(allowed_library_ids) = allowed_library_ids(&*app.services.runtime_identity, &headers)
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let publishers = load_publishers(app.services.opds_persisted.as_ref(), &allowed_library_ids)
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

pub(crate) async fn opds_v1_series(headers: HeaderMap, uri: Uri, app: &HttpAppState) -> Response {
    if let Some(response) = require_auth(&*app.services.runtime_identity, &headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&*app.services.runtime_identity, &headers)
    else {
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
            app.services.opds_persisted.as_ref(),
            app.services.opds_catalog.as_ref(),
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
        app.services
            .opds_catalog
            .load_series_page(
                allowed_library_ids.clone(),
                None,
                publishers.clone(),
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

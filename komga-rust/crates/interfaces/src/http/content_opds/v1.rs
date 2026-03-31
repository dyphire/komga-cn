use super::*;

pub(crate) async fn opds_v1_catalog(headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }
    opds_v1_navigation_feed_response(
        &headers,
        "root",
        "Komga OPDS catalog",
        "/opds/v1.2/catalog",
        vec![
            (
                "keepReading".to_string(),
                "Keep Reading".to_string(),
                "/opds/v1.2/keep-reading".to_string(),
            ),
            (
                "ondeck".to_string(),
                "On Deck".to_string(),
                "/opds/v1.2/ondeck".to_string(),
            ),
            (
                "allSeries".to_string(),
                "All series".to_string(),
                "/opds/v1.2/series".to_string(),
            ),
            (
                "latestSeries".to_string(),
                "Latest series".to_string(),
                "/opds/v1.2/series/latest".to_string(),
            ),
            (
                "latestBooks".to_string(),
                "Latest books".to_string(),
                "/opds/v1.2/books/latest".to_string(),
            ),
            (
                "allLibraries".to_string(),
                "All libraries".to_string(),
                "/opds/v1.2/libraries".to_string(),
            ),
            (
                "allCollections".to_string(),
                "All collections".to_string(),
                "/opds/v1.2/collections".to_string(),
            ),
            (
                "allReadLists".to_string(),
                "All read lists".to_string(),
                "/opds/v1.2/readlists".to_string(),
            ),
            (
                "allPublishers".to_string(),
                "All publishers".to_string(),
                "/opds/v1.2/publishers".to_string(),
            ),
        ],
        None,
    )
}

pub(crate) async fn opds_v1_search(headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let template_href = app_absolute_url(&headers, "/opds/v1.2/series?search={searchTerms}");
    let payload = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><OpenSearchDescription xmlns=\"http://a9.com/-/spec/opensearch/1.1/\"><ShortName>Search</ShortName><Description>Search for series</Description><Url type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" template=\"{}\"/></OpenSearchDescription>",
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

    opds_v1_acquisition_feed_response(
        &headers,
        "ondeck",
        "On Deck",
        "/opds/v1.2/ondeck",
        books,
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

    opds_v1_acquisition_feed_response(
        &headers,
        "keepReading",
        "Keep Reading",
        "/opds/v1.2/keep-reading",
        books,
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

    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let rows = load_latest_series_paged(
        database_file,
        &allowed_library_ids,
        None,
        page.saturating_mul(size) as i64,
        (size + 1) as i64,
    )
    .await
    .unwrap_or_default();
    let (rows, has_next) = paginate_vec(rows, 0, size);

    opds_v1_navigation_feed_response(
        &headers,
        "latestSeries",
        "Latest series",
        "/opds/v1.2/series/latest",
        rows.into_iter()
            .map(|series| {
                let series_id = series.id.clone();
                (
                    series_id.clone(),
                    series.title,
                    format!("/opds/v1.2/series/{series_id}"),
                )
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

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let restrictions = opds_restrictions(&headers);

    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let books = load_latest_books_paged(
        database_file,
        &allowed_library_ids,
        None,
        page.saturating_mul(size) as i64,
        (size + 1) as i64,
    )
    .await
    .unwrap_or_default();
    let books = books
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
    let (books, has_next) = paginate_vec(books, 0, size);

    opds_v1_acquisition_feed_response(
        &headers,
        "latestBooks",
        "Latest books",
        "/opds/v1.2/books/latest",
        books,
        None,
        Some((page, has_next)),
    )
}

pub(crate) async fn opds_v1_libraries(headers: HeaderMap, database_file: &Path) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let rows = load_libraries(database_file)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|library| library_visible(&allowed_library_ids, &library.id))
        .map(|library| {
            (
                library.id.clone(),
                library.name,
                format!("/opds/v1.2/libraries/{}", library.id),
            )
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
        let books = load_collection_books(database_file, &collection.id)
            .await
            .unwrap_or_default();
        if books
            .iter()
            .any(|book| library_visible(&allowed_library_ids, &book.library_id))
        {
            rows.push((
                collection.id.clone(),
                collection.name,
                format!("/opds/v1.2/collections/{}", collection.id),
            ));
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
            rows.push((
                readlist.id.clone(),
                readlist.name,
                format!("/opds/v1.2/readlists/{}", readlist.id),
            ));
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
        return response;
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
        .map(|publisher| {
            (
                publisher.clone(),
                publisher.clone(),
                format!("/opds/v1.2/series?publisher={}", query_escape(&publisher)),
            )
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

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let Some(series) = load_series(database_file, series_id).await.unwrap_or(None) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !library_visible(&allowed_library_ids, &series.library_id) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let books = load_series_books_paged(
        database_file,
        &series.id,
        page.saturating_mul(size) as i64,
        (size + 1) as i64,
    )
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|book| PersistedBookFeedItem {
        id: book.id,
        title: book.title,
        file_name: book.file_name,
        media_type: book.media_type,
        library_id: series.library_id.clone(),
        age_rating: None,
        sharing_labels: vec![],
        last_modified: book.last_modified,
    })
    .collect::<Vec<_>>();
    let (books, has_next) = paginate_vec(books, 0, size);
    opds_v1_acquisition_feed_response(
        &headers,
        series.id.as_str(),
        series.title.as_str(),
        format!("/opds/v1.2/series/{series_id}").as_str(),
        books,
        Some(series.last_modified.as_str()),
        Some((page, has_next)),
    )
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
    let offset = page.saturating_mul(size);

    let series = load_library_series(database_file, library_id, offset as i64, (size + 1) as i64)
        .await
        .unwrap_or_default();
    let mut entries = series
        .into_iter()
        .filter(|item| library_visible(&allowed_library_ids, &item.library_id))
        .collect::<Vec<_>>();
    let has_next = entries.len() > size;
    if has_next {
        entries.truncate(size);
    }

    opds_v1_library_series_feed_response(
        &headers,
        library.id.as_str(),
        library.name.as_str(),
        format!("/opds/v1.2/libraries/{library_id}").as_str(),
        entries,
        None,
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

    let series = load_collection_series(database_file, collection_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|series| library_visible(&allowed_library_ids, &series.library_id))
        .collect::<Vec<_>>();
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let (series, has_next) = paginate_vec(series, page, size);
    if series.is_empty() {
        return StatusCode::FORBIDDEN.into_response();
    }

    let entries = series
        .into_iter()
        .map(|series| {
            let id = series.id;
            (id.clone(), series.title, format!("/opds/v1.2/series/{id}"))
        })
        .collect::<Vec<_>>();

    opds_v1_navigation_feed_response(
        &headers,
        collection.id.as_str(),
        collection.name.as_str(),
        format!("/opds/v1.2/collections/{collection_id}").as_str(),
        entries,
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

    let books = load_readlist_books(database_file, readlist_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|book| library_visible(&allowed_library_ids, &book.library_id))
        .map(|book| PersistedBookFeedItem {
            id: book.id,
            title: book.title,
            file_name: book.file_name,
            media_type: book.media_type,
            library_id: book.library_id,
            age_rating: book.age_rating,
            sharing_labels: book.sharing_labels,
            last_modified: book.last_modified,
        })
        .collect::<Vec<_>>();
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let (books, has_next) = paginate_vec(books, page, size);
    if books.is_empty() {
        return StatusCode::FORBIDDEN.into_response();
    }
    opds_v1_acquisition_feed_response(
        &headers,
        readlist.id.as_str(),
        readlist.name.as_str(),
        format!("/opds/v1.2/readlists/{readlist_id}").as_str(),
        books,
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
            (
                series_id.clone(),
                series.title,
                format!("/opds/v1.2/series/{series_id}"),
            )
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
            (
                series_id.clone(),
                series.title,
                format!("/opds/v1.2/series/{series_id}"),
            )
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
        "/opds/v1.2/series",
        entries,
        Some((page, has_next)),
    )
}

use super::super::types::{PersistedBookSearchResult, PersistedReadlistBook, PersistedSeriesBook};
use super::*;
use serde_json::Value;

pub(crate) async fn opds_v2_libraries_collections(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
) -> Response {
    opds_v2_collections_feed(headers, uri, database_file, None).await
}

pub(crate) async fn opds_v2_library_collections(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
    library_id: &str,
) -> Response {
    opds_v2_collections_feed(headers, uri, database_file, Some(library_id)).await
}

pub(crate) async fn opds_v2_collection(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
    collection_id: &str,
) -> Response {
    if require_auth(&headers).is_some() {
        return opds_catalog_unauthorized_response(&headers);
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return opds_catalog_unauthorized_response(&headers);
    };

    let Some(collection) = (match load_collection(database_file, collection_id).await {
        Ok(collection) => collection,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS collection: {error}") })),
            )
                .into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let restrictions = opds_restrictions(&headers);
    let series =
        match load_collection_series(database_file, collection_id, collection.ordered).await {
            Ok(series) => series,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("load OPDS collection series: {error}") })),
                )
                    .into_response();
            }
        };

    let visible_series = series
        .into_iter()
        .filter(|series| library_visible(&allowed_library_ids, &series.library_id))
        .collect::<Vec<_>>();

    if visible_series.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let filtered_series = visible_series
        .into_iter()
        .filter(|series| {
            content_allowed_by_restrictions(
                restrictions.as_ref(),
                series.age_rating,
                &series.sharing_labels,
            )
        })
        .collect::<Vec<_>>();
    let total_filtered_series = filtered_series.len();
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let navigation = filtered_series
        .into_iter()
        .skip(page.saturating_mul(size))
        .take(size)
        .map(|series| {
            opds_navigation_link(
                &headers,
                series.title.as_str(),
                format!("/opds/v2/series/{}", series.id).as_str(),
            )
        })
        .collect::<Vec<_>>();

    if navigation.is_empty() {
        let self_path = format!("/opds/v2/collections/{collection_id}");
        let modified = collection
            .last_modified
            .trim()
            .is_empty()
            .then(opds_now_timestamp)
            .unwrap_or_else(|| normalize_opds_updated(collection.last_modified.as_str()));

        let mut links = vec![
            json!({
                "rel": "self",
                "href": app_absolute_url(&headers, self_path.as_str()),
            }),
            json!({
                "title": "Home",
                "rel": "start",
                "href": app_absolute_url(&headers, "/opds/v2/catalog"),
                "type": "application/opds+json",
            }),
            json!({
                "title": "Search",
                "rel": "search",
                "href": app_absolute_url(&headers, "/opds/v2/search{?query}"),
                "type": "application/opds+json",
                "templated": true,
            }),
        ];
        if page > 0 {
            let previous_path = if self_path.contains('?') {
                format!("{self_path}&page={}", page.saturating_sub(1))
            } else {
                format!("{self_path}?page={}", page.saturating_sub(1))
            };
            links.push(json!({
                "rel": "previous",
                "href": app_absolute_url(&headers, previous_path.as_str()),
            }));
        }
        if page.saturating_add(1).saturating_mul(size) < total_filtered_series {
            let next_path = if self_path.contains('?') {
                format!("{self_path}&page={}", page + 1)
            } else {
                format!("{self_path}?page={}", page + 1)
            };
            links.push(json!({
                "rel": "next",
                "href": app_absolute_url(&headers, next_path.as_str()),
            }));
        }

        return (
            StatusCode::OK,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/opds+json"),
            )],
            Json(json!({
                "metadata": {
                    "title": collection.name,
                    "modified": modified,
                    "itemsPerPage": size,
                    "currentPage": page + 1,
                    "numberOfItems": total_filtered_series,
                },
                "links": links,
                "navigation": [],
            })),
        )
            .into_response();
    }

    opds_navigation_response_with_paging(
        &headers,
        collection.name.as_str(),
        format!("/opds/v2/collections/{collection_id}").as_str(),
        Some(collection.last_modified.as_str()),
        navigation,
        page,
        size,
        total_filtered_series,
    )
}

pub(crate) async fn opds_v2_libraries_readlists(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
) -> Response {
    opds_v2_readlists_feed(headers, uri, database_file, None).await
}

pub(crate) async fn opds_v2_library_readlists(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
    library_id: &str,
) -> Response {
    opds_v2_readlists_feed(headers, uri, database_file, Some(library_id)).await
}

pub(crate) async fn opds_v2_series(
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

    let Some(series) = (match load_series(database_file, series_id).await {
        Ok(series) => series,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS series: {error}") })),
            )
                .into_response();
        }
    }) else {
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
    let Some(user) = resolved_auth_user(&headers) else {
        return opds_catalog_unauthorized_response(&headers);
    };
    let current_user_id = user_id(&user).to_string();

    let tag = uri.query().and_then(|raw| {
        raw.split('&').find_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            (key == "tag").then_some(percent_decode(&value.replace('+', " ")))
        })
    });
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());

    let books =
        match load_series_books_paged(database_file, &series.id, &current_user_id, 0, i64::MAX)
            .await
        {
            Ok(books) => books,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("load OPDS series books: {error}") })),
                )
                    .into_response();
            }
        };

    let visible_books = books
        .into_iter()
        .filter(|book| library_visible(&allowed_library_ids, &book.library_id))
        .filter(|book| {
            content_allowed_by_restrictions(
                restrictions.as_ref(),
                book.age_rating,
                &book.sharing_labels,
            )
        })
        .collect::<Vec<_>>();

    let series_tags = match load_series_tags(database_file, &series.id).await {
        Ok(tags) => tags,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS series tags: {error}") })),
            )
                .into_response();
        }
    };

    let tag_links = series_tags
        .into_iter()
        .map(|tag_value| {
            let mut link = serde_json::Map::new();
            link.insert("title".to_string(), Value::String(tag_value.clone()));
            link.insert(
                "href".to_string(),
                Value::String(app_absolute_url(
                    &headers,
                    format!(
                        "/opds/v2/series/{series_id}?tag={}",
                        query_escape(tag_value.as_str())
                    )
                    .as_str(),
                )),
            );
            link.insert(
                "type".to_string(),
                Value::String("application/opds+json".to_string()),
            );
            if tag.as_deref() == Some(tag_value.as_str()) {
                link.insert("rel".to_string(), Value::String("self".to_string()));
            }
            Value::Object(link)
        })
        .collect::<Vec<_>>();

    let filtered_books = visible_books
        .into_iter()
        .filter(|book| {
            tag.as_ref()
                .is_none_or(|selected| book.tags.iter().any(|value| value == selected))
        })
        .collect::<Vec<_>>();

    let total_filtered_books = filtered_books.len();
    let publications = filtered_books
        .into_iter()
        .skip(page.saturating_mul(size))
        .take(size)
        .map(|book| opds_publication_for_feed_entry(&headers, &series_book_feed_entry(book)))
        .collect::<Vec<_>>();

    let self_path = format!("/opds/v2/series/{series_id}");
    let page_path = if let Some(selected_tag) = tag.as_deref() {
        format!("{self_path}?tag={}&size={size}", query_escape(selected_tag))
    } else {
        format!("{self_path}?size={size}")
    };
    let modified = series
        .last_modified
        .trim()
        .is_empty()
        .then(opds_now_timestamp)
        .unwrap_or_else(|| normalize_opds_updated(series.last_modified.as_str()));

    let mut links = vec![
        json!({
            "rel": "self",
            "href": app_absolute_url(&headers, self_path.as_str()),
        }),
        json!({
            "title": "Home",
            "rel": "start",
            "href": app_absolute_url(&headers, "/opds/v2/catalog"),
            "type": "application/opds+json",
        }),
        json!({
            "title": "Search",
            "rel": "search",
            "href": app_absolute_url(&headers, "/opds/v2/search{?query}"),
            "type": "application/opds+json",
            "templated": true,
        }),
    ];
    if page > 0 {
        links.push(json!({
            "rel": "previous",
            "href": app_absolute_url(&headers, series_page_link_path(page_path.as_str(), page.saturating_sub(1)).as_str()),
        }));
    }
    if page.saturating_add(1).saturating_mul(size) < total_filtered_books {
        links.push(json!({
            "rel": "next",
            "href": app_absolute_url(&headers, series_page_link_path(page_path.as_str(), page + 1).as_str()),
        }));
    }

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(json!({
            "metadata": {
                "title": series.title,
                "description": series.summary,
                "modified": modified,
                "itemsPerPage": size,
                "currentPage": page + 1,
                "numberOfItems": total_filtered_books,
            },
            "links": links,
            "publications": publications,
            "facets": if tag_links.is_empty() {
                Value::Null
            } else {
                Value::Array(vec![json!({
                    "metadata": { "title": "Tag" },
                    "links": tag_links,
                })])
            },
        })),
    )
        .into_response()
}

fn series_book_feed_entry(
    book: PersistedSeriesBook,
) -> crate::opds_catalog_access::OpdsBookFeedEntry {
    crate::opds_catalog_access::OpdsBookFeedEntry {
        id: book.id,
        series_id: book.series_id,
        title: book.title,
        series_title: book.series_title,
        number: book.number,
        number_sort: book.number_sort,
        summary: book.summary,
        isbn: book.isbn,
        authors: book.authors,
        tags: book.tags,
        file_name: book.file_name,
        file_size: book.file_size,
        media_type: book.media_type,
        page_count: book.page_count,
        epub_divina_compatible: book.epub_divina_compatible,
        last_read: book.last_read,
        last_read_date: book.last_read_date,
        library_id: book.library_id,
        age_rating: book.age_rating,
        sharing_labels: book.sharing_labels,
        last_modified: book.last_modified,
        release_date: book.release_date,
    }
}

fn series_page_link_path(self_path: &str, page: usize) -> String {
    if self_path.contains('?') {
        format!("{self_path}&page={page}")
    } else {
        format!("{self_path}?page={page}")
    }
}

pub(crate) async fn opds_v2_readlist(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
    readlist_id: &str,
) -> Response {
    if require_auth(&headers).is_some() {
        return opds_catalog_unauthorized_response(&headers);
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return opds_catalog_unauthorized_response(&headers);
    };

    let Some(readlist) = (match load_readlist(database_file, readlist_id).await {
        Ok(readlist) => readlist,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS readlist: {error}") })),
            )
                .into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let restrictions = opds_restrictions(&headers);

    let books = match load_readlist_books(database_file, &readlist.id).await {
        Ok(books) => books,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS readlist books: {error}") })),
            )
                .into_response();
        }
    };

    let visible_books = books
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

    let total_visible_books = filtered_books.len();
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let publications = filtered_books
        .into_iter()
        .skip(page.saturating_mul(size))
        .take(size)
        .map(|book| opds_publication_for_feed_entry(&headers, &readlist_book_feed_entry(book)))
        .collect::<Vec<_>>();

    opds_publications_response_with_paging(
        &headers,
        readlist.name.as_str(),
        format!("/opds/v2/readlists/{readlist_id}").as_str(),
        Some(readlist.last_modified.as_str()),
        publications,
        page,
        size,
        total_visible_books,
    )
}

fn readlist_book_feed_entry(
    book: PersistedReadlistBook,
) -> crate::opds_catalog_access::OpdsBookFeedEntry {
    crate::opds_catalog_access::OpdsBookFeedEntry {
        id: book.id,
        series_id: book.series_id,
        title: book.title,
        series_title: book.series_title,
        number: book.number,
        number_sort: book.number_sort,
        summary: book.summary,
        isbn: book.isbn,
        authors: book.authors,
        tags: book.tags,
        file_name: book.file_name,
        file_size: book.file_size,
        media_type: book.media_type,
        page_count: book.page_count,
        epub_divina_compatible: book.epub_divina_compatible,
        last_read: None,
        last_read_date: None,
        library_id: book.library_id,
        age_rating: book.age_rating,
        sharing_labels: book.sharing_labels,
        last_modified: book.last_modified,
        release_date: book.release_date,
    }
}

fn search_book_feed_entry(
    book: PersistedBookSearchResult,
) -> crate::opds_catalog_access::OpdsBookFeedEntry {
    crate::opds_catalog_access::OpdsBookFeedEntry {
        id: book.id,
        series_id: book.series_id,
        title: book.title,
        series_title: book.series_title,
        number: book.number,
        number_sort: book.number_sort,
        summary: book.summary,
        isbn: book.isbn,
        authors: book.authors,
        tags: book.tags,
        file_name: book.file_name,
        file_size: book.file_size,
        media_type: book.media_type,
        page_count: book.page_count,
        epub_divina_compatible: book.epub_divina_compatible,
        last_read: None,
        last_read_date: None,
        library_id: book.library_id,
        age_rating: book.age_rating,
        sharing_labels: book.sharing_labels,
        last_modified: book.last_modified,
        release_date: book.release_date,
    }
}

pub(crate) async fn opds_v2_search(
    headers: HeaderMap,
    database_file: &Path,
    query: Option<&str>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let restrictions = opds_restrictions(&headers);

    let search_query = query.unwrap_or_default().trim();

    let (series, books, collections, readlists) =
        match load_unified_search_results(database_file, search_query).await {
            Ok(results) => results,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("load OPDS search results: {error}") })),
                )
                    .into_response();
            }
        };

    let mut series_navigation = series
        .into_iter()
        .filter(|item| {
            library_visible(&allowed_library_ids, &item.library_id)
                && content_allowed_by_restrictions(
                    restrictions.as_ref(),
                    item.age_rating,
                    &item.sharing_labels,
                )
        })
        .map(|item| {
            json!({
                "title": item.title,
                "href": app_absolute_url(&headers, format!("/opds/v2/series/{}", item.id).as_str()),
                "type": "application/opds+json",
            })
        })
        .collect::<Vec<_>>();
    series_navigation.truncate(20);

    let mut book_publications = books
        .into_iter()
        .filter(|item| {
            library_visible(&allowed_library_ids, &item.library_id)
                && content_allowed_by_restrictions(
                    restrictions.as_ref(),
                    item.age_rating,
                    &item.sharing_labels,
                )
        })
        .map(|item| opds_publication_for_feed_entry(&headers, &search_book_feed_entry(item)))
        .collect::<Vec<_>>();
    book_publications.truncate(20);

    let mut collections_navigation = Vec::new();
    for item in collections {
        let books = match load_collection_books(database_file, &item.id).await {
            Ok(books) => books,
            Err(_) => continue,
        };
        if books.iter().any(|book| {
            library_visible(&allowed_library_ids, &book.library_id)
                && content_allowed_by_restrictions(
                    restrictions.as_ref(),
                    book.age_rating,
                    &book.sharing_labels,
                )
        }) {
            collections_navigation.push(json!({
                "title": item.name,
                "href": app_absolute_url(&headers, format!("/opds/v2/collections/{}", item.id).as_str()),
                "type": "application/opds+json",
            }));
        }
    }
    collections_navigation.truncate(20);

    let mut readlist_navigation = Vec::new();
    for item in readlists {
        let books = match load_readlist_books(database_file, &item.id).await {
            Ok(books) => books,
            Err(_) => continue,
        };
        if books.iter().any(|book| {
            library_visible(&allowed_library_ids, &book.library_id)
                && content_allowed_by_restrictions(
                    restrictions.as_ref(),
                    book.age_rating,
                    &book.sharing_labels,
                )
        }) {
            readlist_navigation.push(json!({
                "title": item.name,
                "href": app_absolute_url(&headers, format!("/opds/v2/readlists/{}", item.id).as_str()),
                "type": "application/opds+json",
            }));
        }
    }
    readlist_navigation.truncate(20);

    let mut groups = Vec::new();
    if !series_navigation.is_empty() {
        groups.push(json!({
            "metadata": {
                "title": "Series",
            },
            "navigation": series_navigation,
        }));
    }
    if !book_publications.is_empty() {
        groups.push(json!({
            "metadata": {
                "title": "Books",
            },
            "publications": book_publications,
        }));
    }
    if !collections_navigation.is_empty() {
        groups.push(json!({
            "metadata": {
                "title": "Collections",
            },
            "navigation": collections_navigation,
        }));
    }
    if !readlist_navigation.is_empty() {
        groups.push(json!({
            "metadata": {
                "title": "Read Lists",
            },
            "navigation": readlist_navigation,
        }));
    }

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(json!({
            "metadata": {
                "title": "Search results",
                "modified": opds_now_timestamp(),
            },
            "links": [
                {
                    "rel": "start",
                    "href": app_absolute_url(&headers, "/opds/v2/catalog"),
                    "type": "application/opds+json",
                },
                {
                    "rel": "search",
                    "href": app_absolute_url(&headers, "/opds/v2/search{?query}"),
                    "type": "application/opds+json",
                    "templated": true,
                }
            ],
            "groups": groups,
        })),
    )
        .into_response()
}

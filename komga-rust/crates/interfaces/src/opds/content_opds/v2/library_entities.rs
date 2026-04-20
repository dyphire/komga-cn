use super::super::types::{
    PersistedBookFeedItem, PersistedBookSearchResult, PersistedCollection,
    PersistedCollectionSearchResult, PersistedReadlist, PersistedReadlistBook,
    PersistedReadlistSearchResult, PersistedSeries, PersistedSeriesBook,
    PersistedSeriesSearchResult,
};
use super::*;
use serde_json::Value;

async fn load_collection_from_services(
    app: &HttpAppState,
    collection_id: &str,
) -> Result<Option<PersistedCollection>, String> {
    Ok(app
        .services
        .opds_persisted
        .load_collection(app.auth_db.database_file.clone(), collection_id.to_string())
        .await?
        .map(|row| PersistedCollection {
            id: row.id,
            name: row.name,
            last_modified: row.last_modified,
            ordered: row.ordered,
        }))
}

async fn load_collection_series_from_services(
    app: &HttpAppState,
    collection_id: &str,
    ordered: bool,
) -> Result<Vec<PersistedSeries>, String> {
    Ok(app
        .services
        .opds_persisted
        .load_collection_series(
            app.auth_db.database_file.clone(),
            collection_id.to_string(),
            ordered,
        )
        .await?
        .into_iter()
        .map(|row| PersistedSeries {
            id: row.id,
            library_id: row.library_id,
            title: row.title,
            summary: row.summary,
            age_rating: row.age_rating,
            sharing_labels: row.sharing_labels,
            last_modified: row.last_modified,
        })
        .collect())
}

async fn load_series_from_services(
    app: &HttpAppState,
    series_id: &str,
) -> Result<Option<PersistedSeries>, String> {
    Ok(app
        .services
        .opds_persisted
        .load_series(app.auth_db.database_file.clone(), series_id.to_string())
        .await?
        .map(|row| PersistedSeries {
            id: row.id,
            library_id: row.library_id,
            title: row.title,
            summary: row.summary,
            age_rating: row.age_rating,
            sharing_labels: row.sharing_labels,
            last_modified: row.last_modified,
        }))
}

async fn load_series_books_paged_from_services(
    app: &HttpAppState,
    series_id: &str,
    user_id: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<PersistedSeriesBook>, String> {
    Ok(app
        .services
        .opds_persisted
        .load_series_books_paged(
            app.auth_db.database_file.clone(),
            series_id.to_string(),
            user_id.to_string(),
            offset,
            limit,
        )
        .await?
        .into_iter()
        .map(|row| PersistedSeriesBook {
            id: row.id,
            series_id: row.series_id,
            title: row.title,
            series_title: row.series_title,
            number: row.number,
            number_sort: row.number_sort,
            summary: row.summary,
            isbn: row.isbn,
            authors: row
                .authors
                .into_iter()
                .map(|author| crate::state::OpdsBookAuthorEntry {
                    name: author.name,
                    role: author.role,
                })
                .collect(),
            tags: row.tags,
            file_name: row.file_name,
            file_size: row.file_size,
            media_type: row.media_type,
            page_count: row.page_count,
            epub_divina_compatible: row.epub_divina_compatible,
            last_read: row.last_read,
            last_read_date: row.last_read_date,
            library_id: row.library_id,
            age_rating: row.age_rating,
            sharing_labels: row.sharing_labels,
            last_modified: row.last_modified,
            release_date: row.release_date,
        })
        .collect())
}

async fn load_series_tags_from_services(
    app: &HttpAppState,
    series_id: &str,
) -> Result<Vec<String>, String> {
    app.services
        .opds_persisted
        .load_series_tags(app.auth_db.database_file.clone(), series_id.to_string())
        .await
}

async fn load_readlist_from_services(
    app: &HttpAppState,
    readlist_id: &str,
) -> Result<Option<PersistedReadlist>, String> {
    Ok(app
        .services
        .opds_persisted
        .load_readlist(app.auth_db.database_file.clone(), readlist_id.to_string())
        .await?
        .map(|row| PersistedReadlist {
            id: row.id,
            name: row.name,
            last_modified: row.last_modified,
            ordered: row.ordered,
        }))
}

async fn load_readlist_books_from_services(
    app: &HttpAppState,
    readlist_id: &str,
) -> Result<Vec<PersistedReadlistBook>, String> {
    Ok(app
        .services
        .opds_persisted
        .load_readlist_books(app.auth_db.database_file.clone(), readlist_id.to_string())
        .await?
        .into_iter()
        .map(|row| PersistedReadlistBook {
            id: row.id,
            series_id: row.series_id,
            title: row.title,
            series_title: row.series_title,
            number: row.number,
            number_sort: row.number_sort,
            summary: row.summary,
            isbn: row.isbn,
            authors: row
                .authors
                .into_iter()
                .map(|author| crate::state::OpdsBookAuthorEntry {
                    name: author.name,
                    role: author.role,
                })
                .collect(),
            tags: row.tags,
            file_name: row.file_name,
            file_size: row.file_size,
            media_type: row.media_type,
            media_status: row.media_status,
            page_count: row.page_count,
            epub_divina_compatible: row.epub_divina_compatible,
            library_id: row.library_id,
            age_rating: row.age_rating,
            sharing_labels: row.sharing_labels,
            last_modified: row.last_modified,
            release_date: row.release_date,
        })
        .collect())
}

async fn load_collection_books_from_services(
    app: &HttpAppState,
    collection_id: &str,
) -> Result<Vec<PersistedBookFeedItem>, String> {
    Ok(app
        .services
        .opds_persisted
        .load_collection_books(app.auth_db.database_file.clone(), collection_id.to_string())
        .await?
        .into_iter()
        .map(|row| PersistedBookFeedItem {
            id: row.id,
            title: row.title,
            series_title: String::new(),
            number: String::new(),
            summary: String::new(),
            authors: vec![],
            file_name: row.file_name,
            file_size: 0,
            media_type: row.media_type,
            page_count: 0,
            epub_divina_compatible: false,
            last_read: None,
            last_read_date: None,
            library_id: row.library_id,
            age_rating: row.age_rating,
            sharing_labels: row.sharing_labels,
            last_modified: row.last_modified,
        })
        .collect())
}

async fn load_unified_search_results_from_services(
    app: &HttpAppState,
    query: &str,
) -> Result<
    (
        Vec<PersistedSeriesSearchResult>,
        Vec<PersistedBookSearchResult>,
        Vec<PersistedCollectionSearchResult>,
        Vec<PersistedReadlistSearchResult>,
    ),
    String,
> {
    let (series_rows, book_rows, collection_rows, readlist_rows) = app
        .services
        .opds_persisted
        .load_unified_search_results(app.auth_db.database_file.clone(), query.to_string())
        .await?;

    Ok((
        series_rows
            .into_iter()
            .map(|row| PersistedSeriesSearchResult {
                id: row.id,
                title: row.title,
                library_id: row.library_id,
                age_rating: row.age_rating,
                sharing_labels: row.sharing_labels,
                last_modified: row.last_modified,
            })
            .collect(),
        book_rows
            .into_iter()
            .map(|row| PersistedBookSearchResult {
                id: row.id,
                series_id: row.series_id,
                title: row.title,
                series_title: row.series_title,
                number: row.number,
                number_sort: row.number_sort,
                summary: row.summary,
                isbn: row.isbn,
                authors: row
                    .authors
                    .into_iter()
                    .map(|author| crate::state::OpdsBookAuthorEntry {
                        name: author.name,
                        role: author.role,
                    })
                    .collect(),
                tags: row.tags,
                file_name: row.file_name,
                file_size: row.file_size,
                media_type: row.media_type,
                page_count: row.page_count,
                epub_divina_compatible: row.epub_divina_compatible,
                library_id: row.library_id,
                age_rating: row.age_rating,
                sharing_labels: row.sharing_labels,
                last_modified: row.last_modified,
                release_date: row.release_date,
            })
            .collect(),
        collection_rows
            .into_iter()
            .map(|row| PersistedCollectionSearchResult {
                id: row.id,
                name: row.name,
            })
            .collect(),
        readlist_rows
            .into_iter()
            .map(|row| PersistedReadlistSearchResult {
                id: row.id,
                name: row.name,
            })
            .collect(),
    ))
}

pub(crate) async fn opds_v2_libraries_collections(
    headers: HeaderMap,
    uri: Uri,
    app: &HttpAppState,
) -> Response {
    opds_v2_collections_feed(headers, uri, app, None).await
}

pub(crate) async fn opds_v2_library_collections(
    headers: HeaderMap,
    uri: Uri,
    app: &HttpAppState,
    library_id: &str,
) -> Response {
    opds_v2_collections_feed(headers, uri, app, Some(library_id)).await
}

pub(crate) async fn opds_v2_collection(
    headers: HeaderMap,
    uri: Uri,
    app: &HttpAppState,
    collection_id: &str,
) -> Response {
    if require_auth(&headers).is_some() {
        return opds_catalog_unauthorized_response(&headers);
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return opds_catalog_unauthorized_response(&headers);
    };

    let Some(collection) = (match load_collection_from_services(app, collection_id).await {
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
        match load_collection_series_from_services(app, collection_id, collection.ordered).await {
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
    app: &HttpAppState,
) -> Response {
    opds_v2_readlists_feed(headers, uri, app, None).await
}

pub(crate) async fn opds_v2_library_readlists(
    headers: HeaderMap,
    uri: Uri,
    app: &HttpAppState,
    library_id: &str,
) -> Response {
    opds_v2_readlists_feed(headers, uri, app, Some(library_id)).await
}

pub(crate) async fn opds_v2_series(
    headers: HeaderMap,
    uri: Uri,
    app: &HttpAppState,
    series_id: &str,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let Some(series) = (match load_series_from_services(app, series_id).await {
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
        match load_series_books_paged_from_services(app, &series.id, &current_user_id, 0, i64::MAX)
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

    let series_tags = match load_series_tags_from_services(app, &series.id).await {
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

fn series_book_feed_entry(book: PersistedSeriesBook) -> crate::state::OpdsBookFeedEntry {
    crate::state::OpdsBookFeedEntry {
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
    app: &HttpAppState,
    readlist_id: &str,
) -> Response {
    if require_auth(&headers).is_some() {
        return opds_catalog_unauthorized_response(&headers);
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return opds_catalog_unauthorized_response(&headers);
    };

    let Some(readlist) = (match load_readlist_from_services(app, readlist_id).await {
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

    let books = match load_readlist_books_from_services(app, &readlist.id).await {
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

fn readlist_book_feed_entry(book: PersistedReadlistBook) -> crate::state::OpdsBookFeedEntry {
    crate::state::OpdsBookFeedEntry {
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

fn search_book_feed_entry(book: PersistedBookSearchResult) -> crate::state::OpdsBookFeedEntry {
    crate::state::OpdsBookFeedEntry {
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
    app: &HttpAppState,
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
        match load_unified_search_results_from_services(app, search_query).await {
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
        let books = match load_collection_books_from_services(app, &item.id).await {
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
        let books = match load_readlist_books_from_services(app, &item.id).await {
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

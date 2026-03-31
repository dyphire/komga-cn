use super::*;

pub(crate) async fn opds_v2_libraries_collections(
    headers: HeaderMap,
    database_file: &Path,
) -> Response {
    opds_v2_collections_feed(headers, database_file, None).await
}

pub(crate) async fn opds_v2_library_collections(
    headers: HeaderMap,
    database_file: &Path,
    library_id: &str,
) -> Response {
    opds_v2_collections_feed(headers, database_file, Some(library_id)).await
}

pub(crate) async fn opds_v2_collection(
    headers: HeaderMap,
    database_file: &Path,
    collection_id: &str,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
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

    let books = match load_collection_books(database_file, collection_id).await {
        Ok(books) => books,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS collection books: {error}") })),
            )
                .into_response();
        }
    };

    let visible_books = books
        .into_iter()
        .filter(|book| library_visible(&allowed_library_ids, &book.library_id))
        .collect::<Vec<_>>();

    if visible_books.is_empty()
        && !collection_empty_for_authorized_user(database_file, collection_id, &allowed_library_ids)
            .await
    {
        return StatusCode::FORBIDDEN.into_response();
    }

    let publications = visible_books
        .into_iter()
        .map(|book| opds_publication_for_book(&headers, &book.id, &book.title, &book.media_type))
        .collect::<Vec<_>>();

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(json!({
            "metadata": {
                "title": collection.name,
            },
            "links": [
                {
                    "rel": "self",
                    "href": app_absolute_url(&headers, format!("/opds/v2/collections/{collection_id}").as_str()),
                    "type": "application/opds+json",
                },
                {
                    "rel": "start",
                    "href": app_absolute_url(&headers, "/opds/v2/catalog"),
                    "type": "application/opds+json",
                }
            ],
            "publications": publications,
        })),
    )
        .into_response()
}

pub(crate) async fn opds_v2_libraries_readlists(
    headers: HeaderMap,
    database_file: &Path,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let readlists = match load_all_readlists(database_file).await {
        Ok(readlists) => readlists,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS readlists: {error}") })),
            )
                .into_response();
        }
    };

    let mut navigation = Vec::new();
    for readlist in readlists {
        let readlist_books = match load_readlist_books(database_file, &readlist.id).await {
            Ok(books) => books,
            Err(_) => continue,
        };
        if readlist_books
            .iter()
            .any(|book| library_visible(&allowed_library_ids, &book.library_id))
        {
            navigation.push(json!({
                "title": readlist.name,
                "href": app_absolute_url(&headers, format!("/opds/v2/readlists/{}", readlist.id).as_str()),
                "type": "application/opds+json",
            }));
        }
    }

    opds_navigation_response(
        &headers,
        "Read lists",
        app_absolute_url(&headers, "/opds/v2/libraries/readlists").as_str(),
        navigation,
    )
}

pub(crate) async fn opds_v2_book_thumbnail_small(headers: HeaderMap, book_id: &str) -> Response {
    redirect_to_opds_v2(headers, &format!("/opds/v2/books/{book_id}/thumbnail"))
}

pub(crate) async fn opds_v2_library_readlists(
    headers: HeaderMap,
    database_file: &Path,
    library_id: &str,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    if !library_visible(&allowed_library_ids, library_id) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let readlists = match load_readlists_for_library(database_file, library_id).await {
        Ok(readlists) => readlists,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS library readlists: {error}") })),
            )
                .into_response();
        }
    };

    let navigation = readlists
        .into_iter()
        .map(|readlist| {
            json!({
                "title": readlist.name,
                "href": app_absolute_url(&headers, format!("/opds/v2/readlists/{}", readlist.id).as_str()),
                "type": "application/opds+json",
            })
        })
        .collect::<Vec<_>>();

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(json!({
            "metadata": {
                "title": "Read lists",
            },
            "links": [
                {
                    "rel": "self",
                    "href": app_absolute_url(&headers, format!("/opds/v2/libraries/{library_id}/readlists").as_str()),
                    "type": "application/opds+json",
                },
                {
                    "rel": "start",
                    "href": app_absolute_url(&headers, "/opds/v2/catalog"),
                    "type": "application/opds+json",
                }
            ],
            "navigation": navigation,
        })),
    )
        .into_response()
}

pub(crate) async fn opds_v2_series(
    headers: HeaderMap,
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

    let books = match load_series_books(database_file, &series.id).await {
        Ok(books) => books,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS series books: {error}") })),
            )
                .into_response();
        }
    };

    let publications = books
        .into_iter()
        .map(|book| {
            json!({
                "metadata": {
                    "title": book.title,
                },
                "links": [
                    {
                        "rel": "self",
                        "href": app_absolute_url(&headers, format!("/opds/v2/books/{}/manifest", book.id).as_str()),
                        "type": "application/opds-publication+json",
                    },
                    {
                        "rel": "http://opds-spec.org/acquisition",
                        "href": app_absolute_url(&headers, format!("/opds/v2/books/{}/file", book.id).as_str()),
                        "type": book.media_type,
                    }
                ],
            })
        })
        .collect::<Vec<_>>();

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(json!({
            "metadata": {
                "title": series.title,
            },
            "links": [
                {
                    "rel": "self",
                    "href": app_absolute_url(&headers, format!("/opds/v2/series/{}", series.id).as_str()),
                    "type": "application/opds+json",
                },
                {
                    "rel": "start",
                    "href": app_absolute_url(&headers, "/opds/v2/catalog"),
                    "type": "application/opds+json",
                }
            ],
            "publications": publications,
        })),
    )
        .into_response()
}

pub(crate) async fn opds_v2_readlist(
    headers: HeaderMap,
    database_file: &Path,
    readlist_id: &str,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
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
        return StatusCode::FORBIDDEN.into_response();
    }

    let publications = visible_books
        .into_iter()
        .map(|book| {
            json!({
                "metadata": {
                    "title": book.title,
                },
                "links": [
                    {
                        "rel": "self",
                        "href": app_absolute_url(&headers, format!("/opds/v2/books/{}/manifest", book.id).as_str()),
                        "type": "application/opds-publication+json",
                    },
                    {
                        "rel": "http://opds-spec.org/acquisition",
                        "href": app_absolute_url(&headers, format!("/opds/v2/books/{}/file", book.id).as_str()),
                        "type": book.media_type,
                    }
                ],
            })
        })
        .collect::<Vec<_>>();

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(json!({
            "metadata": {
                "title": readlist.name,
            },
            "links": [
                {
                    "rel": "self",
                    "href": app_absolute_url(&headers, format!("/opds/v2/readlists/{}", readlist.id).as_str()),
                    "type": "application/opds+json",
                },
                {
                    "rel": "start",
                    "href": app_absolute_url(&headers, "/opds/v2/catalog"),
                    "type": "application/opds+json",
                }
            ],
            "publications": publications,
        })),
    )
        .into_response()
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

    let search_query = query.unwrap_or_default().trim();

    let (series, books, collections, readlists) =
        match load_search_results(database_file, search_query).await {
            Ok(results) => results,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("load OPDS search results: {error}") })),
                )
                    .into_response();
            }
        };

    let series_navigation = series
        .into_iter()
        .filter(|item| library_visible(&allowed_library_ids, &item.library_id))
        .map(|item| {
            json!({
                "title": item.title,
                "href": app_absolute_url(&headers, format!("/opds/v2/series/{}", item.id).as_str()),
                "type": "application/opds+json",
            })
        })
        .collect::<Vec<_>>();

    let book_publications = books
        .into_iter()
        .filter(|item| library_visible(&allowed_library_ids, &item.library_id))
        .map(|item| {
            json!({
                "metadata": {
                    "title": item.title,
                },
                "links": [
                    {
                        "rel": "self",
                        "href": app_absolute_url(&headers, format!("/opds/v2/books/{}/manifest", item.id).as_str()),
                        "type": "application/opds-publication+json",
                    }
                ],
            })
        })
        .collect::<Vec<_>>();

    let mut collections_navigation = Vec::new();
    for item in collections {
        let books = match load_collection_books(database_file, &item.id).await {
            Ok(books) => books,
            Err(_) => continue,
        };
        if books
            .iter()
            .any(|book| library_visible(&allowed_library_ids, &book.library_id))
        {
            collections_navigation.push(json!({
                "title": item.name,
                "href": app_absolute_url(&headers, format!("/opds/v2/collections/{}", item.id).as_str()),
                "type": "application/opds+json",
            }));
        }
    }

    let mut readlist_navigation = Vec::new();
    for item in readlists {
        let books = match load_readlist_books(database_file, &item.id).await {
            Ok(books) => books,
            Err(_) => continue,
        };
        if books
            .iter()
            .any(|book| library_visible(&allowed_library_ids, &book.library_id))
        {
            readlist_navigation.push(json!({
                "title": item.name,
                "href": app_absolute_url(&headers, format!("/opds/v2/readlists/{}", item.id).as_str()),
                "type": "application/opds+json",
            }));
        }
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
            "groups": [
                {
                    "metadata": {
                        "title": "Series",
                    },
                    "navigation": series_navigation,
                },
                {
                    "metadata": {
                        "title": "Books",
                    },
                    "publications": book_publications,
                },
                {
                    "metadata": {
                        "title": "Collections",
                    },
                    "navigation": collections_navigation,
                },
                {
                    "metadata": {
                        "title": "Read Lists",
                    },
                    "navigation": readlist_navigation,
                }
            ],
        })),
    )
        .into_response()
}

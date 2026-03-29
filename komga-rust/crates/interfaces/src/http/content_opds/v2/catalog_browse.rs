use super::*;
use crate::opds_catalog_access::{
    load_keep_reading_books as load_catalog_keep_reading_books,
    load_latest_books as load_catalog_latest_books,
    load_latest_series as load_catalog_latest_series,
    load_on_deck_books as load_catalog_on_deck_books,
};

pub(crate) async fn opds_catalog(headers: HeaderMap, database_file: &Path) -> Response {
    if require_auth(&headers).is_none() {
        return opds_v2_libraries(headers, database_file).await;
    }

    opds_catalog_unauthorized_response(&headers)
}

pub(crate) async fn opds_v2_libraries(headers: HeaderMap, database_file: &Path) -> Response {
    opds_v2_recommended(headers, database_file, None).await
}

pub(crate) async fn opds_v2_library(
    headers: HeaderMap,
    database_file: &Path,
    library_id: &str,
) -> Response {
    opds_v2_recommended(headers, database_file, Some(library_id)).await
}

async fn opds_v2_recommended(
    headers: HeaderMap,
    database_file: &Path,
    library_id: Option<&str>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let libraries = match load_libraries(database_file).await {
        Ok(libraries) => libraries,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS libraries: {error}") })),
            )
                .into_response();
        }
    };

    let selected_library = if let Some(id) = library_id {
        let Some(library) = libraries.iter().find(|library| library.id == id).cloned() else {
            return StatusCode::NOT_FOUND.into_response();
        };
        if !library_visible(&allowed_library_ids, id) {
            return StatusCode::FORBIDDEN.into_response();
        }
        Some(library)
    } else {
        None
    };

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let user_id_value = user_id(&user).to_string();

    let library_segment = selected_library
        .as_ref()
        .map(|library| format!("/{}", library.id))
        .unwrap_or_default();
    let self_path = format!("/opds/v2/libraries{library_segment}");
    let restrictions = opds_restrictions(&headers);

    let mut keep_reading =
        load_catalog_keep_reading_books(database_file, &user_id_value, library_id)
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
    keep_reading.truncate(5);
    let keep_reading_publications = keep_reading
        .into_iter()
        .map(|book| opds_publication_for_book(&headers, &book.id, &book.title, &book.media_type))
        .collect::<Vec<_>>();

    let mut on_deck = load_catalog_on_deck_books(database_file, &user_id_value, library_id)
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
    on_deck.truncate(5);
    let on_deck_publications = on_deck
        .into_iter()
        .map(|book| opds_publication_for_book(&headers, &book.id, &book.title, &book.media_type))
        .collect::<Vec<_>>();

    let mut latest_books = load_catalog_latest_books(database_file, library_id, 5)
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
    latest_books.truncate(5);
    let latest_books_publications = latest_books
        .into_iter()
        .map(|book| opds_publication_for_book(&headers, &book.id, &book.title, &book.media_type))
        .collect::<Vec<_>>();

    let mut latest_series = load_catalog_latest_series(database_file, library_id, 5)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|series| library_visible(&allowed_library_ids, &series.library_id))
        .collect::<Vec<_>>();
    latest_series.truncate(5);
    let latest_series_navigation = latest_series
        .into_iter()
        .map(|series| {
            json!({
                "title": series.title,
                "href": app_absolute_url(&headers, format!("/opds/v2/series/{}", series.id).as_str()),
                "type": "application/opds+json",
            })
        })
        .collect::<Vec<_>>();

    let has_visible_collections = has_visible_collections_for_scope(
        database_file,
        &allowed_library_ids,
        restrictions.as_ref(),
        library_id,
    )
    .await;
    let has_visible_readlists = has_visible_readlists_for_scope(
        database_file,
        &allowed_library_ids,
        restrictions.as_ref(),
        library_id,
    )
    .await;

    let mut navigation = vec![
        opds_subsection_navigation_link(&headers, "Recommended", self_path.as_str()),
        opds_subsection_navigation_link(
            &headers,
            "Browse",
            format!("/opds/v2/libraries{library_segment}/browse").as_str(),
        ),
    ];
    if has_visible_collections {
        navigation.push(opds_subsection_navigation_link(
            &headers,
            "Collections",
            format!("/opds/v2/libraries{library_segment}/collections").as_str(),
        ));
    }
    if has_visible_readlists {
        navigation.push(opds_subsection_navigation_link(
            &headers,
            "Read lists",
            format!("/opds/v2/libraries{library_segment}/readlists").as_str(),
        ));
    }

    let mut groups = Vec::new();
    if selected_library.is_none() {
        let libraries_navigation = libraries
            .into_iter()
            .filter(|library| library_visible(&allowed_library_ids, &library.id))
            .map(|library| {
                opds_navigation_link(
                    &headers,
                    library.name.as_str(),
                    format!("/opds/v2/libraries/{}", library.id).as_str(),
                )
            })
            .collect::<Vec<_>>();
        if !libraries_navigation.is_empty() {
            groups.push(json!({
                "metadata": { "title": "Libraries" },
                "links": [{
                    "rel": "self",
                    "href": app_absolute_url(&headers, "/opds/v2/libraries"),
                    "type": "application/opds+json",
                }],
                "navigation": libraries_navigation,
            }));
        }
    }
    if !keep_reading_publications.is_empty() {
        groups.push(json!({
            "metadata": { "title": "Keep Reading" },
            "links": [{
                "rel": "self",
                "href": app_absolute_url(&headers, format!("/opds/v2/libraries{library_segment}/keep-reading").as_str()),
                "type": "application/opds+json",
            }],
            "publications": keep_reading_publications,
        }));
    }
    if !on_deck_publications.is_empty() {
        groups.push(json!({
            "metadata": { "title": "On Deck" },
            "links": [{
                "rel": "self",
                "href": app_absolute_url(&headers, format!("/opds/v2/libraries{library_segment}/on-deck").as_str()),
                "type": "application/opds+json",
            }],
            "publications": on_deck_publications,
        }));
    }
    if !latest_books_publications.is_empty() {
        groups.push(json!({
            "metadata": { "title": "Latest Books" },
            "links": [{
                "rel": "self",
                "href": app_absolute_url(&headers, format!("/opds/v2/libraries{library_segment}/books/latest").as_str()),
                "type": "application/opds+json",
            }],
            "publications": latest_books_publications,
        }));
    }
    if !latest_series_navigation.is_empty() {
        groups.push(json!({
            "metadata": { "title": "Latest Series" },
            "links": [{
                "rel": "self",
                "href": app_absolute_url(&headers, format!("/opds/v2/libraries{library_segment}/series/latest").as_str()),
                "type": "application/opds+json",
            }],
            "navigation": latest_series_navigation,
        }));
    }

    let modified = selected_library
        .as_ref()
        .map(|library| library.last_modified.as_str())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(opds_now_timestamp);

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(json!({
            "metadata": {
                "title": selected_library
                    .as_ref()
                    .map(|library| format!("{} - Recommended", library.name))
                    .unwrap_or_else(|| "All libraries - Recommended".to_string()),
                "modified": modified,
            },
            "links": [
                {
                    "rel": "self",
                    "href": app_absolute_url(&headers, self_path.as_str()),
                    "type": "application/opds+json",
                },
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
            "navigation": navigation,
            "groups": groups,
        })),
    )
        .into_response()
}

pub(crate) async fn opds_v2_libraries_keep_reading(
    headers: HeaderMap,
    database_file: &Path,
) -> Response {
    opds_v2_keep_reading_feed(headers, database_file, None).await
}

pub(crate) async fn opds_v2_library_keep_reading(
    headers: HeaderMap,
    database_file: &Path,
    library_id: &str,
) -> Response {
    opds_v2_keep_reading_feed(headers, database_file, Some(library_id)).await
}

pub(crate) async fn opds_v2_libraries_on_deck(
    headers: HeaderMap,
    database_file: &Path,
) -> Response {
    opds_v2_on_deck_feed(headers, database_file, None).await
}

pub(crate) async fn opds_v2_library_on_deck(
    headers: HeaderMap,
    database_file: &Path,
    library_id: &str,
) -> Response {
    opds_v2_on_deck_feed(headers, database_file, Some(library_id)).await
}

pub(crate) async fn opds_v2_libraries_latest_books(
    headers: HeaderMap,
    database_file: &Path,
) -> Response {
    opds_v2_latest_books_feed(headers, database_file, None).await
}

pub(crate) async fn opds_v2_library_latest_books(
    headers: HeaderMap,
    database_file: &Path,
    library_id: &str,
) -> Response {
    opds_v2_latest_books_feed(headers, database_file, Some(library_id)).await
}

pub(crate) async fn opds_v2_libraries_latest_series(
    headers: HeaderMap,
    database_file: &Path,
) -> Response {
    opds_v2_latest_series_feed(headers, database_file, None).await
}

pub(crate) async fn opds_v2_library_latest_series(
    headers: HeaderMap,
    database_file: &Path,
    library_id: &str,
) -> Response {
    opds_v2_latest_series_feed(headers, database_file, Some(library_id)).await
}

pub(crate) async fn opds_v2_libraries_browse(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
) -> Response {
    opds_v2_library_browse(headers, uri, database_file, None).await
}

pub(crate) async fn opds_v2_library_browse(
    headers: HeaderMap,
    uri: Uri,
    database_file: &Path,
    library_id: Option<&str>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(allowed_library_ids) = allowed_library_ids(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    if let Some(response) =
        validate_library_scope(database_file, &allowed_library_ids, library_id).await
    {
        return response;
    }

    let libraries = match load_libraries(database_file).await {
        Ok(libraries) => libraries,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS libraries: {error}") })),
            )
                .into_response();
        }
    };
    let selected_library =
        library_id.and_then(|id| libraries.iter().find(|library| library.id == id));

    let restrictions = opds_restrictions(&headers);

    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    let browse_base_path = format!("/opds/v2/libraries{library_segment}/browse");
    let self_href = app_absolute_url(&headers, browse_base_path.as_str());
    let query = uri.query().unwrap_or_default();
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .clamp(1, 100);
    let publishers = query
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next().unwrap_or_default();
            let value = parts.next().unwrap_or_default();
            if key == "publisher" && !value.is_empty() {
                Some(percent_decode(value))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let mut navigation = vec![
        opds_subsection_navigation_link(
            &headers,
            "Recommended",
            format!("/opds/v2/libraries{library_segment}").as_str(),
        ),
        opds_subsection_navigation_link(
            &headers,
            "Browse",
            format!("/opds/v2/libraries{library_segment}/browse").as_str(),
        ),
    ];
    let has_collections = has_visible_collections_for_scope(
        database_file,
        &allowed_library_ids,
        restrictions.as_ref(),
        library_id,
    )
    .await;
    if has_collections {
        navigation.push(opds_subsection_navigation_link(
            &headers,
            "Collections",
            format!("/opds/v2/libraries{library_segment}/collections").as_str(),
        ));
    }
    let has_readlists = has_visible_readlists_for_scope(
        database_file,
        &allowed_library_ids,
        restrictions.as_ref(),
        library_id,
    )
    .await;
    if has_readlists {
        navigation.push(opds_subsection_navigation_link(
            &headers,
            "Read lists",
            format!("/opds/v2/libraries{library_segment}/readlists").as_str(),
        ));
    }

    let (series_navigation, total_series) = load_browse_series_navigation(
        &headers,
        database_file,
        &allowed_library_ids,
        library_id,
        publishers.as_slice(),
        page,
        size,
    )
    .await
    .unwrap_or_default();
    let publisher_navigation =
        load_browse_publisher_navigation(&headers, database_file, &allowed_library_ids, library_id)
            .await
            .unwrap_or_default();
    let mut groups = vec![json!({
        "metadata": { "title": "Series" },
        "navigation": series_navigation,
    })];
    if !publisher_navigation.is_empty() {
        groups.push(json!({
            "metadata": { "title": "Publisher" },
            "navigation": publisher_navigation,
        }));
    }
    let mut links = vec![
        json!({
            "rel": "self",
            "href": self_href,
            "type": "application/opds+json",
        }),
        json!({
            "rel": "start",
            "href": app_absolute_url(&headers, "/opds/v2/catalog"),
            "type": "application/opds+json",
        }),
        json!({
            "rel": "search",
            "href": app_absolute_url(&headers, "/opds/v2/search{?query}"),
            "type": "application/opds+json",
            "templated": true,
        }),
    ];
    if page > 0 {
        links.push(json!({
            "rel": "previous",
            "href": app_absolute_url(
                &headers,
                format!("{browse_base_path}?page={}", page.saturating_sub(1)).as_str(),
            ),
            "type": "application/opds+json",
        }));
    }
    if (page + 1) * size < total_series {
        links.push(json!({
            "rel": "next",
            "href": app_absolute_url(&headers, format!("{browse_base_path}?page={}", page + 1).as_str()),
            "type": "application/opds+json",
        }));
    }

    let modified = selected_library
        .as_ref()
        .map(|library| library.last_modified.as_str())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(opds_now_timestamp);

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(json!({
            "metadata": {
                "title": selected_library
                    .as_ref()
                    .map(|_| "Latest Books".to_string())
                    .unwrap_or_else(|| "All libraries".to_string()),
                "modified": modified,
                "itemsPerPage": size,
                "currentPage": page + 1,
                "numberOfItems": total_series,
            },
            "links": links,
            "navigation": navigation,
            "groups": groups,
        })),
    )
        .into_response()
}

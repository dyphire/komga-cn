use super::*;
use crate::identity_access::auth::{AuthUser, user_id};
use crate::state::{OpdsBookFeedEntry, OpdsSeriesEntry, OpdsState};
use axum::extract::State;

const RECOMMENDED_PAGE_SIZE: i64 = 100;

pub(crate) async fn opds_catalog(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
) -> Response {
    opds_v2_recommended(headers, &app, None, "/opds/v2/libraries".to_string(), &user).await
}

pub(crate) async fn opds_v2_libraries(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
) -> Response {
    opds_v2_recommended(headers, &app, None, "/opds/v2/libraries".to_string(), &user).await
}

pub(crate) async fn opds_v2_library(
    headers: HeaderMap,
    app: &OpdsState,
    library_id: &str,
    user: &AuthUser,
) -> Response {
    opds_v2_recommended(
        headers,
        app,
        Some(library_id),
        format!("/opds/v2/libraries/{library_id}"),
        user,
    )
    .await
}

async fn opds_v2_recommended(
    headers: HeaderMap,
    app: &OpdsState,
    library_id: Option<&str>,
    self_path: String,
    user: &AuthUser,
) -> Response {
    let allowed_library_ids = allowed_library_ids_for_user(user);

    let libraries = match load_libraries(app.opds_persisted.as_ref()).await {
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

    let user_id_value = user_id(user);

    let library_segment = selected_library
        .as_ref()
        .map(|library| format!("/{}", library.id))
        .unwrap_or_default();
    let recommended_path = format!("/opds/v2/libraries{library_segment}");
    let restrictions = opds_restrictions_for_user(user);

    let mut keep_reading = app
        .opds_catalog
        .load_keep_reading_books(user_id_value, library_id)
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
    let total_keep_reading = keep_reading.len();
    keep_reading.truncate(5);
    let keep_reading_publications = keep_reading
        .into_iter()
        .map(|book| opds_publication_for_feed_entry(&headers, &book))
        .collect::<Vec<_>>();

    let mut on_deck = app
        .opds_catalog
        .load_on_deck_books(user_id_value, library_id)
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
    let total_on_deck = on_deck.len();
    on_deck.truncate(5);
    let on_deck_publications = on_deck
        .into_iter()
        .map(|book| opds_publication_for_feed_entry(&headers, &book))
        .collect::<Vec<_>>();

    let (latest_books, total_latest_books) = load_visible_latest_books_for_catalog(
        app,
        &allowed_library_ids,
        restrictions.as_ref(),
        user_id_value,
        library_id,
    )
    .await
    .unwrap_or_default();
    let latest_books_publications = latest_books
        .into_iter()
        .map(|book| opds_publication_for_feed_entry(&headers, &book))
        .collect::<Vec<_>>();

    let (latest_series, total_latest_series) = load_visible_latest_series_for_catalog(
        app,
        &allowed_library_ids,
        restrictions.as_ref(),
        library_id,
    )
    .await
    .unwrap_or_default();
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
        app.opds_persisted.as_ref(),
        &allowed_library_ids,
        restrictions.as_ref(),
        library_id,
    )
    .await;
    let has_visible_readlists = has_visible_readlists_for_scope(
        app.opds_catalog.as_ref(),
        app.opds_persisted.as_ref(),
        &allowed_library_ids,
        restrictions.as_ref(),
        library_id,
    )
    .await;

    let mut navigation = vec![
        opds_subsection_navigation_link(&headers, "Recommended", recommended_path.as_str()),
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
                }],
                "navigation": libraries_navigation,
            }));
        }
    }
    if !keep_reading_publications.is_empty() {
        groups.push(json!({
            "metadata": recommended_group_metadata("Keep Reading", 5, total_keep_reading),
            "links": [{
                "title": "Keep Reading",
                "rel": "self",
                "href": app_absolute_url(&headers, format!("/opds/v2/libraries{library_segment}/keep-reading").as_str()),
                "type": "application/opds+json",
            }],
            "publications": keep_reading_publications,
        }));
    }
    if !on_deck_publications.is_empty() {
        groups.push(json!({
            "metadata": recommended_group_metadata("On Deck", 5, total_on_deck),
            "links": [{
                "title": "On Deck",
                "rel": "self",
                "href": app_absolute_url(&headers, format!("/opds/v2/libraries{library_segment}/on-deck").as_str()),
                "type": "application/opds+json",
            }],
            "publications": on_deck_publications,
        }));
    }
    if !latest_books_publications.is_empty() {
        groups.push(json!({
            "metadata": recommended_group_metadata("Latest Books", 5, total_latest_books),
            "links": [{
                "title": "Latest Books",
                "rel": "self",
                "href": app_absolute_url(&headers, format!("/opds/v2/libraries{library_segment}/books/latest").as_str()),
                "type": "application/opds+json",
            }],
            "publications": latest_books_publications,
        }));
    }
    if !latest_series_navigation.is_empty() {
        groups.push(json!({
            "metadata": recommended_group_metadata("Latest Series", 5, total_latest_series),
            "links": [{
                "title": "Latest Series",
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
        .map(normalize_opds_updated)
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
                },
                {
                    "title": "Home",
                    "rel": "start",
                    "href": app_absolute_url(&headers, "/opds/v2/catalog"),
                    "type": "application/opds+json",
                },
                {
                    "title": "Search",
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

async fn load_visible_latest_books_for_catalog(
    app: &OpdsState,
    allowed_library_ids: &Option<std::collections::HashSet<String>>,
    restrictions: Option<&super::super::types::OpdsRestrictions>,
    user_id: &str,
    library_id: Option<&str>,
) -> Result<(Vec<OpdsBookFeedEntry>, usize), String> {
    let mut offset = 0;
    let mut visible = Vec::new();
    let mut total = 0;

    loop {
        let page = app
            .opds_catalog
            .load_latest_books_paged(
                allowed_library_ids.as_ref(),
                Some(user_id),
                library_id,
                offset,
                RECOMMENDED_PAGE_SIZE,
            )
            .await?;
        if page.is_empty() {
            break;
        }

        let page_len = page.len();
        for book in page {
            if library_visible(allowed_library_ids, &book.library_id)
                && content_allowed_by_restrictions(
                    restrictions,
                    book.age_rating,
                    &book.sharing_labels,
                )
            {
                total += 1;
                if visible.len() < 5 {
                    visible.push(book);
                }
            }
        }

        if page_len < RECOMMENDED_PAGE_SIZE as usize {
            break;
        }
        offset += page_len as i64;
    }

    Ok((visible, total))
}

async fn load_visible_latest_series_for_catalog(
    app: &OpdsState,
    allowed_library_ids: &Option<std::collections::HashSet<String>>,
    restrictions: Option<&super::super::types::OpdsRestrictions>,
    library_id: Option<&str>,
) -> Result<(Vec<OpdsSeriesEntry>, usize), String> {
    let mut offset = 0;
    let mut visible = Vec::new();
    let mut total = 0;

    loop {
        let page = app
            .opds_catalog
            .load_latest_series_paged(
                allowed_library_ids.as_ref(),
                library_id,
                offset,
                RECOMMENDED_PAGE_SIZE,
            )
            .await?;
        if page.is_empty() {
            break;
        }

        let page_len = page.len();
        for series in page {
            if !series.one_shot
                && library_visible(allowed_library_ids, &series.library_id)
                && content_allowed_by_restrictions(
                    restrictions,
                    series.age_rating,
                    &series.sharing_labels,
                )
            {
                total += 1;
                if visible.len() < 5 {
                    visible.push(series);
                }
            }
        }

        if page_len < RECOMMENDED_PAGE_SIZE as usize {
            break;
        }
        offset += page_len as i64;
    }

    Ok((visible, total))
}

fn recommended_group_metadata(
    title: &str,
    items_per_page: usize,
    number_of_items: usize,
) -> serde_json::Value {
    json!({
        "title": title,
        "itemsPerPage": items_per_page,
        "currentPage": 1,
        "numberOfItems": number_of_items,
    })
}

pub(crate) async fn opds_v2_libraries_keep_reading(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    user: &AuthUser,
) -> Response {
    opds_v2_keep_reading_feed(headers, uri, app, None, user).await
}

pub(crate) async fn opds_v2_library_keep_reading(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    library_id: &str,
    user: &AuthUser,
) -> Response {
    opds_v2_keep_reading_feed(headers, uri, app, Some(library_id), user).await
}

pub(crate) async fn opds_v2_libraries_on_deck(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    user: &AuthUser,
) -> Response {
    opds_v2_on_deck_feed(headers, uri, app, None, user).await
}

pub(crate) async fn opds_v2_library_on_deck(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    library_id: &str,
    user: &AuthUser,
) -> Response {
    opds_v2_on_deck_feed(headers, uri, app, Some(library_id), user).await
}

pub(crate) async fn opds_v2_libraries_latest_books(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    user: &AuthUser,
) -> Response {
    opds_v2_latest_books_feed(headers, uri, app, None, user).await
}

pub(crate) async fn opds_v2_library_latest_books(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    library_id: &str,
    user: &AuthUser,
) -> Response {
    opds_v2_latest_books_feed(headers, uri, app, Some(library_id), user).await
}

pub(crate) async fn opds_v2_libraries_latest_series(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    user: &AuthUser,
) -> Response {
    opds_v2_latest_series_feed(headers, uri, app, None, user).await
}

pub(crate) async fn opds_v2_library_latest_series(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    library_id: &str,
    user: &AuthUser,
) -> Response {
    opds_v2_latest_series_feed(headers, uri, app, Some(library_id), user).await
}

pub(crate) async fn opds_v2_libraries_browse(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    user: &AuthUser,
) -> Response {
    opds_v2_library_browse(headers, uri, app, None, user).await
}

pub(crate) async fn opds_v2_library_browse(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    library_id: Option<&str>,
    user: &AuthUser,
) -> Response {
    let allowed_library_ids = allowed_library_ids_for_user(user);

    if let Some(response) = validate_library_scope(
        app.opds_persisted.as_ref(),
        &allowed_library_ids,
        library_id,
    )
    .await
    {
        return response;
    }

    let libraries = match load_libraries(app.opds_persisted.as_ref()).await {
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

    let restrictions = opds_restrictions_for_user(user);

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
        app.opds_persisted.as_ref(),
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
        app.opds_catalog.as_ref(),
        app.opds_persisted.as_ref(),
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
        app.opds_catalog.as_ref(),
        &headers,
        &allowed_library_ids,
        library_id,
        publishers.as_slice(),
        page,
        size,
    )
    .await
    .unwrap_or_default();
    let publisher_navigation = load_browse_publisher_navigation(
        app.opds_catalog.as_ref(),
        &headers,
        &allowed_library_ids,
        library_id,
    )
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
                    .map(|library| library.name.clone())
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

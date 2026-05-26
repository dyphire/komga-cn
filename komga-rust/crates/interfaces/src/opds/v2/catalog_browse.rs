use super::*;
use crate::identity_access::auth::AuthUser;
use crate::state::{
    OpdsFeedUserContext, OpdsLibraryScopeError, OpdsPersistedService, OpdsState,
    OpdsV2FeedCompositionService, OpdsV2FeedPageError, OpdsV2RecommendedGroup,
    OpdsV2RecommendedGroupContent, OpdsV2RecommendedPage,
};
use axum::extract::State;

pub(crate) async fn opds_catalog(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
) -> Response {
    opds_v2_recommended(headers, &app, None, &user).await
}

pub(crate) async fn opds_v2_libraries(
    State(app): State<OpdsState>,
    OpdsV2Authenticated(user): OpdsV2Authenticated,
    headers: HeaderMap,
) -> Response {
    opds_v2_recommended(headers, &app, None, &user).await
}

pub(crate) async fn opds_v2_library(
    headers: HeaderMap,
    app: &OpdsState,
    library_id: &str,
    user: &AuthUser,
) -> Response {
    opds_v2_recommended(headers, app, Some(library_id), user).await
}

async fn opds_v2_recommended(
    headers: HeaderMap,
    app: &OpdsState,
    library_id: Option<&str>,
    user: &AuthUser,
) -> Response {
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let service =
        OpdsV2FeedCompositionService::new(app.opds_catalog.as_ref(), app.opds_persisted.as_ref());
    let page = match service.recommended_page(&feed_user, library_id).await {
        Ok(page) => page,
        Err(error) => return opds_v2_recommended_error_response(error),
    };

    render_opds_v2_recommended(&headers, page)
}

fn render_opds_v2_recommended(headers: &HeaderMap, page: OpdsV2RecommendedPage) -> Response {
    let navigation = opds_v2_recommended_navigation(headers, &page);
    let groups = page
        .groups
        .into_iter()
        .map(|group| opds_v2_recommended_group(headers, group))
        .collect::<Vec<_>>();
    let modified = page
        .modified
        .as_deref()
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
                "title": page.title,
                "modified": modified,
            },
            "links": [
                {
                    "rel": "self",
                    "href": app_absolute_url(headers, page.self_path.as_str()),
                },
                {
                    "title": "Home",
                    "rel": "start",
                    "href": app_absolute_url(headers, "/opds/v2/catalog"),
                    "type": "application/opds+json",
                },
                {
                    "title": "Search",
                    "rel": "search",
                    "href": app_absolute_url(headers, "/opds/v2/search{?query}"),
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

fn opds_v2_recommended_navigation(
    headers: &HeaderMap,
    page: &OpdsV2RecommendedPage,
) -> Vec<serde_json::Value> {
    let mut navigation = vec![
        opds_subsection_navigation_link(headers, "Recommended", page.recommended_path.as_str()),
        opds_subsection_navigation_link(headers, "Browse", page.browse_path.as_str()),
    ];
    if page.has_visible_collections {
        navigation.push(opds_subsection_navigation_link(
            headers,
            "Collections",
            page.collections_path.as_str(),
        ));
    }
    if page.has_visible_readlists {
        navigation.push(opds_subsection_navigation_link(
            headers,
            "Read lists",
            page.readlists_path.as_str(),
        ));
    }
    navigation
}

fn opds_v2_recommended_group(
    headers: &HeaderMap,
    group: OpdsV2RecommendedGroup,
) -> serde_json::Value {
    match group.content {
        OpdsV2RecommendedGroupContent::Libraries(libraries) => {
            let navigation = libraries
                .into_iter()
                .map(|library| {
                    opds_navigation_link(
                        headers,
                        library.name.as_str(),
                        format!("/opds/v2/libraries/{}", library.id).as_str(),
                    )
                })
                .collect::<Vec<_>>();
            json!({
                "metadata": { "title": group.title },
                "links": [{
                    "rel": "self",
                    "href": app_absolute_url(headers, group.self_path.as_str()),
                }],
                "navigation": navigation,
            })
        }
        OpdsV2RecommendedGroupContent::Publications(books) => {
            let publications = books
                .iter()
                .map(|book| opds_publication_for_feed_entry(headers, book))
                .collect::<Vec<_>>();
            json!({
                "metadata": recommended_group_metadata(group.title.as_str(), 5, group.total),
                "links": [{
                    "title": group.title,
                    "rel": "self",
                    "href": app_absolute_url(headers, group.self_path.as_str()),
                    "type": "application/opds+json",
                }],
                "publications": publications,
            })
        }
        OpdsV2RecommendedGroupContent::Navigation(series) => {
            let navigation = series
                .into_iter()
                .map(|series| {
                    json!({
                        "title": series.title,
                        "href": app_absolute_url(headers, format!("/opds/v2/series/{}", series.id).as_str()),
                        "type": "application/opds+json",
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "metadata": recommended_group_metadata(group.title.as_str(), 5, group.total),
                "links": [{
                    "title": group.title,
                    "rel": "self",
                    "href": app_absolute_url(headers, group.self_path.as_str()),
                    "type": "application/opds+json",
                }],
                "navigation": navigation,
            })
        }
    }
}

fn opds_v2_recommended_error_response(error: OpdsV2FeedPageError) -> Response {
    match error {
        OpdsV2FeedPageError::LibraryScope(OpdsLibraryScopeError::NotFound) => {
            StatusCode::NOT_FOUND.into_response()
        }
        OpdsV2FeedPageError::LibraryScope(OpdsLibraryScopeError::Forbidden) => {
            StatusCode::FORBIDDEN.into_response()
        }
        OpdsV2FeedPageError::LibraryScope(OpdsLibraryScopeError::Load(error))
        | OpdsV2FeedPageError::Load(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("load OPDS libraries: {error}") })),
        )
            .into_response(),
    }
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

    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let persisted_service = OpdsPersistedService::new(app.opds_persisted.as_ref());

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
    let has_collections = persisted_service
        .has_visible_collections_for_scope(&feed_user, library_id)
        .await;
    if has_collections {
        navigation.push(opds_subsection_navigation_link(
            &headers,
            "Collections",
            format!("/opds/v2/libraries{library_segment}/collections").as_str(),
        ));
    }
    let has_readlists = persisted_service
        .has_visible_readlists_for_scope(&feed_user, library_id)
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

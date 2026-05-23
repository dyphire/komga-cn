use axum::Json;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::identity_access::auth::AuthUser;
use crate::request_urls::app_absolute_url;
use crate::state::{OpdsFeedService, OpdsFeedUserContext, OpdsPersistedService, OpdsState};

use super::feeds::{
    normalize_opds_updated, opds_navigation_response_with_paging, opds_publication_for_feed_entry,
    opds_publications_response_with_paging, opds_subsection_navigation_link, paginate_vec,
    parse_page_size,
};
use super::persisted::{
    allowed_library_ids_for_user, load_libraries, load_library, validate_library_scope,
};

pub(super) async fn opds_v2_keep_reading_feed(
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

    let selected_library = if let Some(id) = library_id {
        match load_library(app.opds_persisted.as_ref(), id).await {
            Ok(library) => library,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("load OPDS keep-reading library scope: {error}") })),
                )
                    .into_response();
            }
        }
    } else {
        None
    };
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let feed_service = OpdsFeedService::new(app.opds_catalog.as_ref());

    let page_result = match feed_service
        .keep_reading_page(&feed_user, None, page, size)
        .await
    {
        Ok(page_result) => page_result,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS keep-reading books: {error}") })),
            )
                .into_response();
        }
    };

    let publications = page_result
        .books
        .iter()
        .map(|book| opds_publication_for_feed_entry(&headers, book))
        .collect::<Vec<_>>();

    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    let self_path = format!("/opds/v2/libraries{library_segment}/keep-reading");
    let title = selected_library
        .as_ref()
        .map(|library| format!("{} - Keep Reading", library.name))
        .unwrap_or_else(|| "All libraries - Keep Reading".to_string());
    opds_publications_response_with_paging(
        &headers,
        title.as_str(),
        self_path.as_str(),
        selected_library
            .as_ref()
            .map(|library| library.last_modified.as_str()),
        publications,
        page,
        size,
        page_result.total_visible_books,
    )
}

pub(super) async fn opds_v2_on_deck_feed(
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

    let selected_library = if let Some(id) = library_id {
        match load_library(app.opds_persisted.as_ref(), id).await {
            Ok(library) => library,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("load OPDS on-deck library scope: {error}") })),
                )
                    .into_response();
            }
        }
    } else {
        None
    };

    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let feed_service = OpdsFeedService::new(app.opds_catalog.as_ref());
    let page_result = match feed_service
        .on_deck_page(&feed_user, library_id, page, size)
        .await
    {
        Ok(page_result) => page_result,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS on-deck books: {error}") })),
            )
                .into_response();
        }
    };

    let publications = page_result
        .books
        .iter()
        .map(|book| opds_publication_for_feed_entry(&headers, book))
        .collect::<Vec<_>>();

    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    let self_path = format!("/opds/v2/libraries{library_segment}/on-deck");
    let title = selected_library
        .as_ref()
        .map(|library| format!("{} - On Deck", library.name))
        .unwrap_or_else(|| "All libraries - On Deck".to_string());
    opds_publications_response_with_paging(
        &headers,
        title.as_str(),
        self_path.as_str(),
        selected_library
            .as_ref()
            .map(|library| library.last_modified.as_str()),
        publications,
        page,
        size,
        page_result.total_visible_books,
    )
}

pub(super) async fn opds_v2_latest_books_feed(
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

    let selected_library = if let Some(id) = library_id {
        match load_library(app.opds_persisted.as_ref(), id).await {
            Ok(library) => library,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("load OPDS latest-books library scope: {error}") })),
                )
                    .into_response();
            }
        }
    } else {
        None
    };
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let feed_service = OpdsFeedService::new(app.opds_catalog.as_ref());

    let page_result = match feed_service
        .latest_books_page(&feed_user, None, page, size)
        .await
    {
        Ok(page_result) => page_result,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS latest books: {error}") })),
            )
                .into_response();
        }
    };

    let publications = page_result
        .books
        .iter()
        .map(|book| opds_publication_for_feed_entry(&headers, book))
        .collect::<Vec<_>>();

    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    let self_path = format!("/opds/v2/libraries{library_segment}/books/latest");
    let title = selected_library
        .as_ref()
        .map(|library| format!("{} - Latest Books", library.name))
        .unwrap_or_else(|| "All libraries - Latest Books".to_string());
    opds_publications_response_with_paging(
        &headers,
        title.as_str(),
        self_path.as_str(),
        selected_library
            .as_ref()
            .map(|library| library.last_modified.as_str()),
        publications,
        page,
        size,
        page_result.total_visible_books,
    )
}

pub(super) async fn opds_v2_latest_series_feed(
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

    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let feed_service = OpdsFeedService::new(app.opds_catalog.as_ref());
    let page_result = match feed_service
        .latest_series_page(&feed_user, library_id, page, size)
        .await
    {
        Ok(page_result) => page_result,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS latest series: {error}") })),
            )
                .into_response();
        }
    };

    let navigation = page_result
        .series
        .into_iter()
        .map(|series| {
            json!({
                "title": series.title,
                "href": app_absolute_url(&headers, format!("/opds/v2/series/{}", series.id).as_str()),
                "type": "application/opds+json",
            })
        })
        .collect::<Vec<_>>();

    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    let self_path = format!("/opds/v2/libraries{library_segment}/series/latest");
    let modified = selected_library
        .map(|library| library.last_modified.as_str())
        .filter(|value| !value.is_empty());
    let title = selected_library
        .map(|library| format!("{} - Latest Series", library.name))
        .unwrap_or_else(|| "All libraries - Latest Series".to_string());

    opds_navigation_response_with_paging(
        &headers,
        title.as_str(),
        self_path.as_str(),
        modified,
        navigation,
        page,
        size,
        page_result.total_visible_series,
    )
}

pub(super) async fn opds_v2_collections_feed(
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
    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let persisted_service = OpdsPersistedService::new(app.opds_persisted.as_ref());

    let collections = match persisted_service
        .all_collections(&feed_user, library_id, false)
        .await
    {
        Ok(collections) => collections,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS collections: {error}") })),
            )
                .into_response();
        }
    };

    let total_visible_collections = collections.len();
    let (paged_collections, has_next) = paginate_vec(collections, page, size);
    let collection_navigation = paged_collections
        .into_iter()
        .map(|collection| {
            json!({
                "title": collection.name,
                "href": app_absolute_url(&headers, format!("/opds/v2/collections/{}", collection.id).as_str()),
                "type": "application/opds+json",
            })
        })
        .collect::<Vec<_>>();

    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    let self_path = format!("/opds/v2/libraries{library_segment}/collections");
    let has_visible_collections = persisted_service
        .has_visible_collections_for_scope(&feed_user, library_id)
        .await;
    let has_visible_readlists = persisted_service
        .has_visible_readlists_for_scope(&feed_user, library_id)
        .await;

    let mut navigation = vec![
        json!({
            "title": "Recommended",
            "rel": "subsection",
            "href": app_absolute_url(&headers, format!("/opds/v2/libraries{library_segment}").as_str()),
            "type": "application/opds+json",
        }),
        json!({
            "title": "Browse",
            "rel": "subsection",
            "href": app_absolute_url(&headers, format!("/opds/v2/libraries{library_segment}/browse").as_str()),
            "type": "application/opds+json",
        }),
    ];
    if has_visible_collections {
        navigation.push(json!({
            "title": "Collections",
            "rel": "subsection",
            "href": app_absolute_url(&headers, format!("/opds/v2/libraries{library_segment}/collections").as_str()),
            "type": "application/opds+json",
        }));
    }
    if has_visible_readlists {
        navigation.push(json!({
            "title": "Read lists",
            "rel": "subsection",
            "href": app_absolute_url(&headers, format!("/opds/v2/libraries{library_segment}/readlists").as_str()),
            "type": "application/opds+json",
        }));
    }

    let modified = selected_library
        .map(|library| library.last_modified.as_str())
        .filter(|value| !value.is_empty())
        .map(normalize_opds_updated)
        .unwrap_or_else(super::feeds::opds_now_timestamp);

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
            "href": app_absolute_url(&headers, format!("{self_path}?page={}", page.saturating_sub(1)).as_str()),
        }));
    }
    if has_next {
        links.push(json!({
            "rel": "next",
            "href": app_absolute_url(&headers, format!("{self_path}?page={}", page + 1).as_str()),
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
                "title": selected_library
                    .as_ref()
                    .map(|library| format!("{} - Collections", library.name))
                    .unwrap_or_else(|| "All libraries - Collections".to_string()),
                "modified": modified,
                "itemsPerPage": size,
                "currentPage": page + 1,
                "numberOfItems": total_visible_collections,
            },
            "links": links,
            "navigation": navigation,
            "groups": [
                {
                    "metadata": {
                        "title": "Collections"
                    },
                    "navigation": collection_navigation,
                }
            ],
        })),
    )
        .into_response()
}

pub(super) async fn opds_v2_readlists_feed(
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

    let selected_library = if let Some(id) = library_id {
        match load_library(app.opds_persisted.as_ref(), id).await {
            Ok(library) => library,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("load OPDS library readlists scope: {error}") })),
                )
                    .into_response();
            }
        }
    } else {
        None
    };

    let (page, size) = parse_page_size(uri.query().unwrap_or_default());
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let persisted_service = OpdsPersistedService::new(app.opds_persisted.as_ref());
    let readlists = match persisted_service
        .all_readlists(&feed_user, library_id)
        .await
    {
        Ok(readlists) => readlists,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS readlists: {error}") })),
            )
                .into_response();
        }
    };

    let total_readlists = readlists.len();
    let readlist_navigation = readlists
        .into_iter()
        .skip(page.saturating_mul(size))
        .take(size)
        .map(|readlist| {
            json!({
                "title": readlist.name,
                "href": app_absolute_url(&headers, format!("/opds/v2/readlists/{}", readlist.id).as_str()),
                "type": "application/opds+json",
            })
        })
        .collect::<Vec<_>>();

    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    let has_visible_collections = persisted_service
        .has_visible_collections_for_scope(&feed_user, library_id)
        .await;
    let has_visible_readlists = persisted_service
        .has_visible_readlists_for_scope(&feed_user, library_id)
        .await;
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

    let modified = selected_library
        .as_ref()
        .map(|library| library.last_modified.as_str())
        .filter(|value| !value.is_empty())
        .map(normalize_opds_updated)
        .unwrap_or_else(super::feeds::opds_now_timestamp);
    let self_path = format!("/opds/v2/libraries{library_segment}/readlists");
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
            "href": app_absolute_url(&headers, format!("{self_path}?page={}", page.saturating_sub(1)).as_str()),
        }));
    }
    if page.saturating_add(1).saturating_mul(size) < total_readlists {
        links.push(json!({
            "rel": "next",
            "href": app_absolute_url(&headers, format!("{self_path}?page={}", page + 1).as_str()),
        }));
    }

    let readlists_group = json!({
        "metadata": {
            "title": "Read Lists",
        },
        "navigation": readlist_navigation,
    });

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
                    .map(|library| format!("{} - Read Lists", library.name))
                    .unwrap_or_else(|| "All libraries - Read Lists".to_string()),
                "modified": modified,
                "itemsPerPage": size,
                "currentPage": page + 1,
                "numberOfItems": total_readlists,
            },
            "links": links,
            "navigation": navigation,
            "groups": [readlists_group],
        })),
    )
        .into_response()
}

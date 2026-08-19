use axum::Json;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use komga_application::opds::{
    OpdsFeedUserContext, OpdsLibraryScopeError, OpdsPersistedService, OpdsV2FeedCompositionService,
    OpdsV2FeedContent, OpdsV2FeedKind, OpdsV2FeedPage, OpdsV2FeedPageError,
};

use crate::contracts::opds::{
    OpdsV2FeedMetadataDto, OpdsV2GroupDto, OpdsV2GroupMetadataDto, OpdsV2GroupedFeedDto,
    OpdsV2LinkDto, OpdsV2NavigationGroupDto,
};
use crate::helpers::internal_error_response;
use crate::request_urls::app_absolute_url;
use crate::state::OpdsState;
use komga_application::identity_access::AuthUser;

use super::feeds::{
    OpdsV2PagedFeed, opds_navigation_link, opds_navigation_response_with_paging,
    opds_publication_for_feed_entry, opds_publications_response_with_paging,
    opds_subsection_navigation_link, opds_v2_updated, paginate_vec, parse_page_size,
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
    opds_v2_feed(
        headers,
        uri,
        app,
        library_id,
        user,
        OpdsV2FeedKind::KeepReading,
    )
    .await
}

pub(super) async fn opds_v2_on_deck_feed(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    library_id: Option<&str>,
    user: &AuthUser,
) -> Response {
    opds_v2_feed(headers, uri, app, library_id, user, OpdsV2FeedKind::OnDeck).await
}

pub(super) async fn opds_v2_latest_books_feed(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    library_id: Option<&str>,
    user: &AuthUser,
) -> Response {
    opds_v2_feed(
        headers,
        uri,
        app,
        library_id,
        user,
        OpdsV2FeedKind::LatestBooks,
    )
    .await
}

pub(super) async fn opds_v2_latest_series_feed(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    library_id: Option<&str>,
    user: &AuthUser,
) -> Response {
    opds_v2_feed(
        headers,
        uri,
        app,
        library_id,
        user,
        OpdsV2FeedKind::LatestSeries,
    )
    .await
}

async fn opds_v2_feed(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    library_id: Option<&str>,
    user: &AuthUser,
    kind: OpdsV2FeedKind,
) -> Response {
    let page_request = parse_page_size(uri.query().unwrap_or_default());
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let service = OpdsV2FeedCompositionService::new(
        app.opds_feed_catalog.as_ref(),
        app.opds_feed_persisted.as_ref(),
    );

    match service
        .feed_page(
            &feed_user,
            kind,
            library_id,
            page_request.page,
            page_request.size,
        )
        .await
    {
        Ok(page) => render_opds_v2_feed_page(&headers, page),
        Err(error) => opds_v2_feed_error_response(kind, error),
    }
}

fn render_opds_v2_feed_page(headers: &HeaderMap, feed_page: OpdsV2FeedPage) -> Response {
    let OpdsV2FeedPage {
        title,
        kind,
        library_id,
        modified,
        page,
        size,
        total,
        content,
    } = feed_page;
    let modified = modified.as_deref().filter(|value| !value.is_empty());
    let self_path = opds_v2_feed_path(kind, library_id.as_deref());

    match content {
        OpdsV2FeedContent::Publications(books) => {
            let publications = books
                .iter()
                .map(|book| opds_publication_for_feed_entry(headers, book))
                .collect::<Vec<_>>();
            opds_publications_response_with_paging(
                OpdsV2PagedFeed {
                    headers,
                    title: title.as_str(),
                    self_path: self_path.as_str(),
                    modified,
                    page,
                    size,
                    total,
                },
                publications,
            )
        }
        OpdsV2FeedContent::Navigation(series) => {
            let navigation = series
                .into_iter()
                .map(|series| OpdsV2LinkDto {
                    title: Some(series.title),
                    rel: None,
                    href: app_absolute_url(
                        headers,
                        format!("/opds/v2/series/{}", series.id).as_str(),
                    ),
                    media_type: Some("application/opds+json".to_string()),
                    templated: None,
                    properties: None,
                })
                .collect::<Vec<_>>();
            opds_navigation_response_with_paging(
                OpdsV2PagedFeed {
                    headers,
                    title: title.as_str(),
                    self_path: self_path.as_str(),
                    modified,
                    page,
                    size,
                    total,
                },
                navigation,
            )
        }
    }
}

pub(super) fn opds_v2_recommended_path(library_id: Option<&str>) -> String {
    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    format!("/opds/v2/libraries{library_segment}")
}

pub(super) fn opds_v2_feed_path(kind: OpdsV2FeedKind, library_id: Option<&str>) -> String {
    format!(
        "{}/{}",
        opds_v2_recommended_path(library_id),
        opds_v2_feed_path_suffix(kind)
    )
}

pub(super) fn opds_v2_browse_path(library_id: Option<&str>) -> String {
    format!("{}/browse", opds_v2_recommended_path(library_id))
}

pub(super) fn opds_v2_collections_path(library_id: Option<&str>) -> String {
    format!("{}/collections", opds_v2_recommended_path(library_id))
}

pub(super) fn opds_v2_readlists_path(library_id: Option<&str>) -> String {
    format!("{}/readlists", opds_v2_recommended_path(library_id))
}

fn opds_v2_feed_path_suffix(kind: OpdsV2FeedKind) -> &'static str {
    match kind {
        OpdsV2FeedKind::KeepReading => "keep-reading",
        OpdsV2FeedKind::OnDeck => "on-deck",
        OpdsV2FeedKind::LatestBooks => "books/latest",
        OpdsV2FeedKind::LatestSeries => "series/latest",
    }
}

fn opds_v2_feed_error_response(kind: OpdsV2FeedKind, error: OpdsV2FeedPageError) -> Response {
    match error {
        OpdsV2FeedPageError::LibraryScope(OpdsLibraryScopeError::NotFound) => {
            StatusCode::NOT_FOUND.into_response()
        }
        OpdsV2FeedPageError::LibraryScope(OpdsLibraryScopeError::Forbidden) => {
            StatusCode::FORBIDDEN.into_response()
        }
        OpdsV2FeedPageError::LibraryScope(OpdsLibraryScopeError::Load(error)) => {
            internal_error_response(format!(
                "load OPDS {} library scope: {error:#}",
                kind.error_label()
            ))
        }
        OpdsV2FeedPageError::Load(error) => {
            internal_error_response(format!("load OPDS {}: {error:#}", kind.error_label()))
        }
    }
}

fn opds_v2_load_error(context: &str, error: impl std::fmt::Display + std::fmt::Debug) -> Response {
    internal_error_response(format!("load OPDS {context}: {error:#}"))
}

trait OpdsV2FeedKindErrorLabel {
    fn error_label(self) -> &'static str;
}

impl OpdsV2FeedKindErrorLabel for OpdsV2FeedKind {
    fn error_label(self) -> &'static str {
        match self {
            Self::KeepReading => "keep-reading books",
            Self::OnDeck => "on-deck books",
            Self::LatestBooks => "latest books",
            Self::LatestSeries => "latest series",
        }
    }
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
        app.opds_library_persisted.as_ref(),
        &allowed_library_ids,
        library_id,
    )
    .await
    {
        return response;
    }

    let libraries = match load_libraries(app.opds_library_persisted.as_ref()).await {
        Ok(libraries) => libraries,
        Err(error) => return internal_error_response(format!("load OPDS libraries: {error:#}")),
    };
    let selected_library =
        library_id.and_then(|id| libraries.iter().find(|library| library.id == id));
    let page_request = parse_page_size(uri.query().unwrap_or_default());
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let persisted_service = OpdsPersistedService::new(app.opds_feed_persisted.as_ref());

    let collections = match persisted_service
        .all_collections(&feed_user, library_id, false)
        .await
    {
        Ok(collections) => collections,
        Err(error) => return internal_error_response(format!("load OPDS collections: {error:#}")),
    };

    let total_visible_collections = collections.len();
    let collections_page = paginate_vec(collections, page_request);
    let collection_navigation = collections_page
        .items
        .into_iter()
        .map(|collection| {
            opds_navigation_link(
                &headers,
                collection.name.as_str(),
                format!("/opds/v2/collections/{}", collection.id).as_str(),
            )
        })
        .collect::<Vec<_>>();

    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    let self_path = format!("/opds/v2/libraries{library_segment}/collections");
    let has_visible_collections = match persisted_service
        .has_visible_collections_for_scope(&feed_user, library_id)
        .await
    {
        Ok(value) => value,
        Err(error) => return opds_v2_load_error("collections navigation", error),
    };
    let has_visible_readlists = match persisted_service
        .has_visible_readlists_for_scope(&feed_user, library_id)
        .await
    {
        Ok(value) => value,
        Err(error) => return opds_v2_load_error("readlists navigation", error),
    };

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

    let modified = opds_v2_updated(selected_library.map(|library| library.last_modified.as_str()));

    let mut links = vec![
        OpdsV2LinkDto {
            title: None,
            rel: Some("self".to_string()),
            href: app_absolute_url(&headers, self_path.as_str()),
            media_type: None,
            templated: None,
            properties: None,
        },
        OpdsV2LinkDto {
            title: Some("Home".to_string()),
            rel: Some("start".to_string()),
            href: app_absolute_url(&headers, "/opds/v2/catalog"),
            media_type: Some("application/opds+json".to_string()),
            templated: None,
            properties: None,
        },
        OpdsV2LinkDto {
            title: Some("Search".to_string()),
            rel: Some("search".to_string()),
            href: app_absolute_url(&headers, "/opds/v2/search{?query}"),
            media_type: Some("application/opds+json".to_string()),
            templated: Some(true),
            properties: None,
        },
    ];
    if page_request.page > 0 {
        links.push(OpdsV2LinkDto {
            title: None,
            rel: Some("previous".to_string()),
            href: app_absolute_url(
                &headers,
                format!("{self_path}?page={}", page_request.page.saturating_sub(1)).as_str(),
            ),
            media_type: None,
            templated: None,
            properties: None,
        });
    }
    if collections_page.has_next {
        links.push(OpdsV2LinkDto {
            title: None,
            rel: Some("next".to_string()),
            href: app_absolute_url(
                &headers,
                format!("{self_path}?page={}", page_request.page + 1).as_str(),
            ),
            media_type: None,
            templated: None,
            properties: None,
        });
    }

    let title = selected_library
        .as_ref()
        .map(|library| format!("{} - Collections", library.name))
        .unwrap_or_else(|| "All libraries - Collections".to_string());

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(OpdsV2GroupedFeedDto {
            metadata: OpdsV2FeedMetadataDto {
                title,
                modified,
                description: None,
                items_per_page: page_request.size,
                current_page: page_request.page + 1,
                number_of_items: total_visible_collections,
            },
            links,
            navigation,
            groups: vec![OpdsV2GroupDto::Navigation(OpdsV2NavigationGroupDto {
                metadata: OpdsV2GroupMetadataDto {
                    title: "Collections".to_string(),
                    items_per_page: None,
                    current_page: None,
                    number_of_items: None,
                },
                links: None,
                navigation: collection_navigation,
            })],
        }),
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
        app.opds_library_persisted.as_ref(),
        &allowed_library_ids,
        library_id,
    )
    .await
    {
        return response;
    }

    let selected_library = if let Some(id) = library_id {
        match load_library(app.opds_library_persisted.as_ref(), id).await {
            Ok(library) => library,
            Err(error) => {
                return internal_error_response(format!(
                    "load OPDS library readlists scope: {error:#}"
                ));
            }
        }
    } else {
        None
    };

    let page_request = parse_page_size(uri.query().unwrap_or_default());
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let persisted_service = OpdsPersistedService::new(app.opds_feed_persisted.as_ref());
    let readlists = match persisted_service
        .all_readlists(&feed_user, library_id)
        .await
    {
        Ok(readlists) => readlists,
        Err(error) => return internal_error_response(format!("load OPDS readlists: {error:#}")),
    };

    let total_readlists = readlists.len();
    let readlists_page = paginate_vec(readlists, page_request);
    let readlist_navigation = readlists_page
        .items
        .into_iter()
        .map(|readlist| {
            opds_navigation_link(
                &headers,
                readlist.name.as_str(),
                format!("/opds/v2/readlists/{}", readlist.id).as_str(),
            )
        })
        .collect::<Vec<_>>();

    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    let has_visible_collections = match persisted_service
        .has_visible_collections_for_scope(&feed_user, library_id)
        .await
    {
        Ok(value) => value,
        Err(error) => return opds_v2_load_error("collections navigation", error),
    };
    let has_visible_readlists = match persisted_service
        .has_visible_readlists_for_scope(&feed_user, library_id)
        .await
    {
        Ok(value) => value,
        Err(error) => return opds_v2_load_error("readlists navigation", error),
    };
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

    let modified = opds_v2_updated(
        selected_library
            .as_ref()
            .map(|library| library.last_modified.as_str()),
    );
    let self_path = format!("/opds/v2/libraries{library_segment}/readlists");
    let mut links = vec![
        OpdsV2LinkDto {
            title: None,
            rel: Some("self".to_string()),
            href: app_absolute_url(&headers, self_path.as_str()),
            media_type: None,
            templated: None,
            properties: None,
        },
        OpdsV2LinkDto {
            title: Some("Home".to_string()),
            rel: Some("start".to_string()),
            href: app_absolute_url(&headers, "/opds/v2/catalog"),
            media_type: Some("application/opds+json".to_string()),
            templated: None,
            properties: None,
        },
        OpdsV2LinkDto {
            title: Some("Search".to_string()),
            rel: Some("search".to_string()),
            href: app_absolute_url(&headers, "/opds/v2/search{?query}"),
            media_type: Some("application/opds+json".to_string()),
            templated: Some(true),
            properties: None,
        },
    ];
    if page_request.page > 0 {
        links.push(OpdsV2LinkDto {
            title: None,
            rel: Some("previous".to_string()),
            href: app_absolute_url(
                &headers,
                format!("{self_path}?page={}", page_request.page.saturating_sub(1)).as_str(),
            ),
            media_type: None,
            templated: None,
            properties: None,
        });
    }
    if readlists_page.has_next {
        links.push(OpdsV2LinkDto {
            title: None,
            rel: Some("next".to_string()),
            href: app_absolute_url(
                &headers,
                format!("{self_path}?page={}", page_request.page + 1).as_str(),
            ),
            media_type: None,
            templated: None,
            properties: None,
        });
    }

    let title = selected_library
        .as_ref()
        .map(|library| format!("{} - Read Lists", library.name))
        .unwrap_or_else(|| "All libraries - Read Lists".to_string());

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(OpdsV2GroupedFeedDto {
            metadata: OpdsV2FeedMetadataDto {
                title,
                modified,
                description: None,
                items_per_page: page_request.size,
                current_page: page_request.page + 1,
                number_of_items: total_readlists,
            },
            links,
            navigation,
            groups: vec![OpdsV2GroupDto::Navigation(OpdsV2NavigationGroupDto {
                metadata: OpdsV2GroupMetadataDto {
                    title: "Read Lists".to_string(),
                    items_per_page: None,
                    current_page: None,
                    number_of_items: None,
                },
                links: None,
                navigation: readlist_navigation,
            })],
        }),
    )
        .into_response()
}

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use komga_application::identity_access::AuthUser;
use komga_application::opds::{
    OpdsFeedUserContext, OpdsLibraryScopeError, OpdsPersistedService, OpdsV2FeedCompositionService,
    OpdsV2FeedPageError, OpdsV2RecommendedGroup, OpdsV2RecommendedGroupContent,
    OpdsV2RecommendedGroupKind, OpdsV2RecommendedPage,
};

use super::super::feed_endpoints::{
    opds_v2_browse_path, opds_v2_collections_path, opds_v2_feed_path, opds_v2_keep_reading_feed,
    opds_v2_latest_books_feed, opds_v2_latest_series_feed, opds_v2_on_deck_feed,
    opds_v2_readlists_path, opds_v2_recommended_path,
};
use super::super::feeds::{
    opds_navigation_link, opds_publication_for_feed_entry, opds_subsection_navigation_link,
    opds_v2_updated, percent_decode, query_value,
};
use super::super::persisted::{
    allowed_library_ids_for_user, load_browse_publisher_navigation, load_browse_series_navigation,
    load_libraries, validate_library_scope,
};
use crate::contracts::opds::{
    OpdsV2FeedMetadataDto, OpdsV2GroupDto, OpdsV2GroupMetadataDto, OpdsV2GroupedFeedDto,
    OpdsV2LinkDto, OpdsV2NavigationGroupDto, OpdsV2PublicationGroupDto, OpdsV2RecommendedFeedDto,
    OpdsV2RecommendedMetadataDto,
};
use crate::helpers::internal_error_response;
use crate::opds_auth::OpdsV2Authenticated;
use crate::request_urls::app_absolute_url;
use crate::state::OpdsState;

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
    let service = OpdsV2FeedCompositionService::new(
        app.opds_feed_catalog.as_ref(),
        app.opds_feed_persisted.as_ref(),
    );
    let page = match service.recommended_page(&feed_user, library_id).await {
        Ok(page) => page,
        Err(error) => return opds_v2_recommended_error_response(error),
    };

    render_opds_v2_recommended(&headers, page)
}

fn render_opds_v2_recommended(headers: &HeaderMap, page: OpdsV2RecommendedPage) -> Response {
    let self_path = opds_v2_recommended_path(page.library_id.as_deref());
    let navigation = opds_v2_recommended_navigation(headers, &page);
    let groups = page
        .groups
        .into_iter()
        .map(|group| opds_v2_recommended_group(headers, page.library_id.as_deref(), group))
        .collect::<Vec<_>>();
    let modified = opds_v2_updated(page.modified.as_deref());
    let links = vec![
        OpdsV2LinkDto {
            title: None,
            rel: Some("self".to_string()),
            href: app_absolute_url(headers, self_path.as_str()),
            media_type: None,
            templated: None,
            properties: None,
        },
        OpdsV2LinkDto {
            title: Some("Home".to_string()),
            rel: Some("start".to_string()),
            href: app_absolute_url(headers, "/opds/v2/catalog"),
            media_type: Some("application/opds+json".to_string()),
            templated: None,
            properties: None,
        },
        OpdsV2LinkDto {
            title: Some("Search".to_string()),
            rel: Some("search".to_string()),
            href: app_absolute_url(headers, "/opds/v2/search{?query}"),
            media_type: Some("application/opds+json".to_string()),
            templated: Some(true),
            properties: None,
        },
    ];

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(OpdsV2RecommendedFeedDto {
            metadata: OpdsV2RecommendedMetadataDto {
                title: page.title,
                modified,
            },
            links,
            navigation,
            groups,
        }),
    )
        .into_response()
}

fn opds_v2_recommended_navigation(
    headers: &HeaderMap,
    page: &OpdsV2RecommendedPage,
) -> Vec<OpdsV2LinkDto> {
    let library_id = page.library_id.as_deref();
    let mut navigation = vec![
        opds_subsection_navigation_link(
            headers,
            "Recommended",
            opds_v2_recommended_path(library_id).as_str(),
        ),
        opds_subsection_navigation_link(
            headers,
            "Browse",
            opds_v2_browse_path(library_id).as_str(),
        ),
    ];
    if page.has_visible_collections {
        navigation.push(opds_subsection_navigation_link(
            headers,
            "Collections",
            opds_v2_collections_path(library_id).as_str(),
        ));
    }
    if page.has_visible_readlists {
        navigation.push(opds_subsection_navigation_link(
            headers,
            "Read lists",
            opds_v2_readlists_path(library_id).as_str(),
        ));
    }
    navigation
}

fn opds_v2_recommended_group(
    headers: &HeaderMap,
    library_id: Option<&str>,
    group: OpdsV2RecommendedGroup,
) -> OpdsV2GroupDto {
    let self_path = opds_v2_recommended_group_path(group.kind, library_id);
    let title = group.title;
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
            OpdsV2GroupDto::Navigation(OpdsV2NavigationGroupDto {
                metadata: OpdsV2GroupMetadataDto {
                    title,
                    items_per_page: None,
                    current_page: None,
                    number_of_items: None,
                },
                links: Some(vec![OpdsV2LinkDto {
                    title: None,
                    rel: Some("self".to_string()),
                    href: app_absolute_url(headers, self_path.as_str()),
                    media_type: None,
                    templated: None,
                    properties: None,
                }]),
                navigation,
            })
        }
        OpdsV2RecommendedGroupContent::Publications(books) => {
            let publications = books
                .iter()
                .map(|book| opds_publication_for_feed_entry(headers, book))
                .collect::<Vec<_>>();
            OpdsV2GroupDto::Publications(OpdsV2PublicationGroupDto {
                metadata: recommended_group_metadata(title.as_str(), 5, group.total),
                links: Some(vec![OpdsV2LinkDto {
                    title: Some(title),
                    rel: Some("self".to_string()),
                    href: app_absolute_url(headers, self_path.as_str()),
                    media_type: Some("application/opds+json".to_string()),
                    templated: None,
                    properties: None,
                }]),
                publications,
            })
        }
        OpdsV2RecommendedGroupContent::Navigation(series) => {
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
            OpdsV2GroupDto::Navigation(OpdsV2NavigationGroupDto {
                metadata: recommended_group_metadata(title.as_str(), 5, group.total),
                links: Some(vec![OpdsV2LinkDto {
                    title: Some(title),
                    rel: Some("self".to_string()),
                    href: app_absolute_url(headers, self_path.as_str()),
                    media_type: Some("application/opds+json".to_string()),
                    templated: None,
                    properties: None,
                }]),
                navigation,
            })
        }
    }
}

fn opds_v2_recommended_group_path(
    kind: OpdsV2RecommendedGroupKind,
    library_id: Option<&str>,
) -> String {
    match kind {
        OpdsV2RecommendedGroupKind::Libraries => opds_v2_recommended_path(None),
        OpdsV2RecommendedGroupKind::Feed(kind) => opds_v2_feed_path(kind, library_id),
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
        | OpdsV2FeedPageError::Load(error) => {
            internal_error_response(format!("load OPDS libraries: {error:#}"))
        }
    }
}

fn recommended_group_metadata(
    title: &str,
    items_per_page: usize,
    number_of_items: usize,
) -> OpdsV2GroupMetadataDto {
    OpdsV2GroupMetadataDto {
        title: title.to_string(),
        items_per_page: Some(items_per_page),
        current_page: Some(1),
        number_of_items: Some(number_of_items),
    }
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

    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let persisted_service = OpdsPersistedService::new(app.opds_feed_persisted.as_ref());

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
    let has_collections = match persisted_service
        .has_visible_collections_for_scope(&feed_user, library_id)
        .await
    {
        Ok(value) => value,
        Err(error) => return opds_v2_browse_load_error("collections navigation", error),
    };
    if has_collections {
        navigation.push(opds_subsection_navigation_link(
            &headers,
            "Collections",
            format!("/opds/v2/libraries{library_segment}/collections").as_str(),
        ));
    }
    let has_readlists = match persisted_service
        .has_visible_readlists_for_scope(&feed_user, library_id)
        .await
    {
        Ok(value) => value,
        Err(error) => return opds_v2_browse_load_error("readlists navigation", error),
    };
    if has_readlists {
        navigation.push(opds_subsection_navigation_link(
            &headers,
            "Read lists",
            format!("/opds/v2/libraries{library_segment}/readlists").as_str(),
        ));
    }

    let series_navigation_page = match load_browse_series_navigation(
        app.opds_browse_catalog.as_ref(),
        &headers,
        &allowed_library_ids,
        library_id,
        publishers.as_slice(),
        page,
        size,
    )
    .await
    {
        Ok(page) => page,
        Err(error) => return opds_v2_browse_load_error("series navigation", error),
    };
    let publisher_navigation = match load_browse_publisher_navigation(
        app.opds_browse_catalog.as_ref(),
        &headers,
        &allowed_library_ids,
        library_id,
    )
    .await
    {
        Ok(entries) => entries,
        Err(error) => return opds_v2_browse_load_error("publisher navigation", error),
    };
    let mut groups = vec![OpdsV2GroupDto::Navigation(OpdsV2NavigationGroupDto {
        metadata: OpdsV2GroupMetadataDto {
            title: "Series".to_string(),
            items_per_page: None,
            current_page: None,
            number_of_items: None,
        },
        links: None,
        navigation: series_navigation_page.entries,
    })];
    if !publisher_navigation.is_empty() {
        groups.push(OpdsV2GroupDto::Navigation(OpdsV2NavigationGroupDto {
            metadata: OpdsV2GroupMetadataDto {
                title: "Publisher".to_string(),
                items_per_page: None,
                current_page: None,
                number_of_items: None,
            },
            links: None,
            navigation: publisher_navigation,
        }));
    }
    let mut links = vec![
        OpdsV2LinkDto {
            title: None,
            rel: Some("self".to_string()),
            href: self_href,
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
    if page > 0 {
        links.push(OpdsV2LinkDto {
            title: None,
            rel: Some("previous".to_string()),
            href: app_absolute_url(
                &headers,
                format!("{browse_base_path}?page={}", page.saturating_sub(1)).as_str(),
            ),
            media_type: Some("application/opds+json".to_string()),
            templated: None,
            properties: None,
        });
    }
    if (page + 1) * size < series_navigation_page.total_count {
        links.push(OpdsV2LinkDto {
            title: None,
            rel: Some("next".to_string()),
            href: app_absolute_url(
                &headers,
                format!("{browse_base_path}?page={}", page + 1).as_str(),
            ),
            media_type: Some("application/opds+json".to_string()),
            templated: None,
            properties: None,
        });
    }

    let modified = opds_v2_updated(
        selected_library
            .as_ref()
            .map(|library| library.last_modified.as_str()),
    );

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(OpdsV2GroupedFeedDto {
            metadata: OpdsV2FeedMetadataDto {
                title: selected_library
                    .as_ref()
                    .map(|library| library.name.clone())
                    .unwrap_or_else(|| "All libraries".to_string()),
                modified,
                description: None,
                items_per_page: size,
                current_page: page + 1,
                number_of_items: series_navigation_page.total_count,
            },
            links,
            navigation,
            groups,
        }),
    )
        .into_response()
}

fn opds_v2_browse_load_error(
    context: &str,
    error: impl std::fmt::Display + std::fmt::Debug,
) -> Response {
    internal_error_response(format!("load OPDS browse {context}: {error:#}"))
}

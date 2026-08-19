use axum::Json;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use komga_application::identity_access::{AuthUser, user_id};
use komga_application::opds::{
    OpdsBookFeedEntry, OpdsFeedUserContext, OpdsPersistedService, OpdsSeriesAccessError,
    PersistedSeriesBookRecord,
};

use super::super::feed_endpoints::{opds_v2_collections_feed, opds_v2_readlists_feed};
use super::super::feeds::{
    OpdsV2PagedFeed, opds_navigation_link, opds_navigation_response_with_paging,
    opds_publication_for_feed_entry, opds_publications_response_with_paging, opds_v2_updated,
    paginate_vec, parse_page_size, percent_decode, query_escape,
};
use super::super::persisted::load_series_tags;
use crate::contracts::opds::{
    OpdsV2FacetDto, OpdsV2FeedMetadataDto, OpdsV2GroupDto, OpdsV2GroupMetadataDto, OpdsV2LinkDto,
    OpdsV2NavigationGroupDto, OpdsV2PublicationFacetFeedDto, OpdsV2PublicationGroupDto,
    OpdsV2RecommendedMetadataDto, OpdsV2SearchFeedDto,
};
use crate::helpers::spring_error_response;
use crate::request_urls::app_absolute_url;
use crate::state::OpdsState;

pub(crate) async fn opds_v2_libraries_collections(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    user: &AuthUser,
) -> Response {
    opds_v2_collections_feed(headers, uri, app, None, user).await
}

pub(crate) async fn opds_v2_library_collections(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    library_id: &str,
    user: &AuthUser,
) -> Response {
    opds_v2_collections_feed(headers, uri, app, Some(library_id), user).await
}

pub(crate) async fn opds_v2_collection(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    collection_id: &str,
    user: &AuthUser,
) -> Response {
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let persisted_service =
        OpdsPersistedService::new(app.opds_collection_detail_persisted.as_ref());
    let Some(detail) = (match persisted_service
        .collection_detail(&feed_user, collection_id)
        .await
    {
        Ok(detail) => detail,
        Err(error) => {
            return spring_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("load OPDS collection: {error:#}"),
            );
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let collection = detail.collection;
    let total_filtered_series = detail.series.len();
    let page_request = parse_page_size(uri.query().unwrap_or_default());
    let series_page = paginate_vec(detail.series, page_request);
    let navigation = series_page
        .items
        .into_iter()
        .map(|series| {
            opds_navigation_link(
                &headers,
                series.title.as_str(),
                format!("/opds/v2/series/{}", series.id).as_str(),
            )
        })
        .collect::<Vec<_>>();

    opds_navigation_response_with_paging(
        OpdsV2PagedFeed {
            headers: &headers,
            title: collection.name.as_str(),
            self_path: format!("/opds/v2/collections/{collection_id}").as_str(),
            modified: Some(collection.last_modified.as_str()),
            page: page_request.page,
            size: page_request.size,
            total: total_filtered_series,
        },
        navigation,
    )
}

pub(crate) async fn opds_v2_libraries_readlists(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    user: &AuthUser,
) -> Response {
    opds_v2_readlists_feed(headers, uri, app, None, user).await
}

pub(crate) async fn opds_v2_library_readlists(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    library_id: &str,
    user: &AuthUser,
) -> Response {
    opds_v2_readlists_feed(headers, uri, app, Some(library_id), user).await
}

pub(crate) async fn opds_v2_series(
    headers: HeaderMap,
    uri: Uri,
    app: &OpdsState,
    series_id: &str,
    user: &AuthUser,
) -> Response {
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let persisted_service = OpdsPersistedService::new(app.opds_series_persisted.as_ref());
    let series = match persisted_service
        .visible_series(&feed_user, series_id)
        .await
    {
        Ok(series) => series,
        Err(OpdsSeriesAccessError::NotFound) => return StatusCode::NOT_FOUND.into_response(),
        Err(OpdsSeriesAccessError::Forbidden) => return StatusCode::FORBIDDEN.into_response(),
        Err(OpdsSeriesAccessError::Load(error)) => {
            return spring_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("load OPDS series: {error:#}"),
            );
        }
    };
    let current_user_id = user_id(user).to_string();

    let tag = uri.query().and_then(|raw| {
        raw.split('&').find_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            (key == "tag").then_some(percent_decode(&value.replace('+', " ")))
        })
    });
    let page_request = parse_page_size(uri.query().unwrap_or_default());

    let visible_books = match persisted_service
        .series_books_page(&feed_user, &series.id, &current_user_id, 0, i64::MAX)
        .await
    {
        Ok(books) => books,
        Err(error) => {
            return spring_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("load OPDS series books: {error:#}"),
            );
        }
    };

    let series_tags = match load_series_tags(app.opds_series_persisted.as_ref(), &series.id).await {
        Ok(tags) => tags,
        Err(error) => {
            return spring_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("load OPDS series tags: {error:#}"),
            );
        }
    };

    let tag_links = series_tags
        .into_iter()
        .map(|tag_value| OpdsV2LinkDto {
            title: Some(tag_value.clone()),
            rel: (tag.as_deref() == Some(tag_value.as_str())).then(|| "self".to_string()),
            href: app_absolute_url(
                &headers,
                format!(
                    "/opds/v2/series/{series_id}?tag={}",
                    query_escape(tag_value.as_str())
                )
                .as_str(),
            ),
            media_type: Some("application/opds+json".to_string()),
            templated: None,
            properties: None,
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
    let books_page = paginate_vec(filtered_books, page_request);
    let publications = books_page
        .items
        .into_iter()
        .map(|book| opds_publication_for_feed_entry(&headers, &series_book_feed_entry(book)))
        .collect::<Vec<_>>();

    let self_path = format!("/opds/v2/series/{series_id}");
    let page_path = if let Some(selected_tag) = tag.as_deref() {
        format!(
            "{self_path}?tag={}&size={}",
            query_escape(selected_tag),
            page_request.size
        )
    } else {
        format!("{self_path}?size={}", page_request.size)
    };
    let modified = opds_v2_updated(Some(series.last_modified.as_str()));

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
                series_page_link_path(page_path.as_str(), page_request.page.saturating_sub(1))
                    .as_str(),
            ),
            media_type: None,
            templated: None,
            properties: None,
        });
    }
    if books_page.has_next {
        links.push(OpdsV2LinkDto {
            title: None,
            rel: Some("next".to_string()),
            href: app_absolute_url(
                &headers,
                series_page_link_path(page_path.as_str(), page_request.page + 1).as_str(),
            ),
            media_type: None,
            templated: None,
            properties: None,
        });
    }

    let facets = (!tag_links.is_empty()).then(|| {
        vec![OpdsV2FacetDto {
            metadata: OpdsV2GroupMetadataDto {
                title: "Tag".to_string(),
                items_per_page: None,
                current_page: None,
                number_of_items: None,
            },
            links: tag_links,
        }]
    });

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds+json"),
        )],
        Json(OpdsV2PublicationFacetFeedDto {
            metadata: OpdsV2FeedMetadataDto {
                title: series.title,
                modified,
                description: Some(series.summary),
                items_per_page: page_request.size,
                current_page: page_request.page + 1,
                number_of_items: total_filtered_books,
            },
            links,
            publications,
            facets,
        }),
    )
        .into_response()
}

fn series_book_feed_entry(book: PersistedSeriesBookRecord) -> OpdsBookFeedEntry {
    book.into()
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
    app: &OpdsState,
    readlist_id: &str,
    user: &AuthUser,
) -> Response {
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let persisted_service = OpdsPersistedService::new(app.opds_readlist_detail_persisted.as_ref());
    let Some(detail) = (match persisted_service
        .readlist_detail(&feed_user, readlist_id)
        .await
    {
        Ok(detail) => detail,
        Err(error) => {
            return spring_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("load OPDS readlist: {error:#}"),
            );
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let readlist = detail.readlist;
    let total_visible_books = detail.books.len();
    let page_request = parse_page_size(uri.query().unwrap_or_default());
    let books_page = paginate_vec(detail.books, page_request);
    let publications = books_page
        .items
        .into_iter()
        .map(|book| {
            let entry = OpdsBookFeedEntry::from(book);
            opds_publication_for_feed_entry(&headers, &entry)
        })
        .collect::<Vec<_>>();

    opds_publications_response_with_paging(
        OpdsV2PagedFeed {
            headers: &headers,
            title: readlist.name.as_str(),
            self_path: format!("/opds/v2/readlists/{readlist_id}").as_str(),
            modified: Some(readlist.last_modified.as_str()),
            page: page_request.page,
            size: page_request.size,
            total: total_visible_books,
        },
        publications,
    )
}

pub(crate) async fn opds_v2_search(
    headers: HeaderMap,
    app: &OpdsState,
    query: Option<&str>,
    user: &AuthUser,
) -> Response {
    let search_query = query.unwrap_or_default().trim();
    let feed_user = OpdsFeedUserContext::from_auth_user(user);
    let persisted_service = OpdsPersistedService::new(app.opds_search_persisted.as_ref());
    let results = match persisted_service
        .unified_search(&feed_user, search_query)
        .await
    {
        Ok(results) => results,
        Err(error) => {
            return spring_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("load OPDS search results: {error:#}"),
            );
        }
    };

    let series_navigation = results
        .series
        .into_iter()
        .map(|item| {
            opds_navigation_link(
                &headers,
                item.title.as_str(),
                format!("/opds/v2/series/{}", item.id).as_str(),
            )
        })
        .collect::<Vec<_>>();

    let book_publications = results
        .books
        .into_iter()
        .map(|item| {
            let entry = OpdsBookFeedEntry::from(item);
            opds_publication_for_feed_entry(&headers, &entry)
        })
        .collect::<Vec<_>>();

    let collections_navigation = results
        .collections
        .into_iter()
        .map(|item| {
            opds_navigation_link(
                &headers,
                item.name.as_str(),
                format!("/opds/v2/collections/{}", item.id).as_str(),
            )
        })
        .collect::<Vec<_>>();

    let readlist_navigation = results
        .readlists
        .into_iter()
        .map(|item| {
            opds_navigation_link(
                &headers,
                item.name.as_str(),
                format!("/opds/v2/readlists/{}", item.id).as_str(),
            )
        })
        .collect::<Vec<_>>();

    let mut groups = Vec::<OpdsV2GroupDto>::new();
    if !series_navigation.is_empty() {
        groups.push(OpdsV2GroupDto::Navigation(OpdsV2NavigationGroupDto {
            metadata: OpdsV2GroupMetadataDto {
                title: "Series".to_string(),
                items_per_page: None,
                current_page: None,
                number_of_items: None,
            },
            links: None,
            navigation: series_navigation,
        }));
    }
    if !book_publications.is_empty() {
        groups.push(OpdsV2GroupDto::Publications(OpdsV2PublicationGroupDto {
            metadata: OpdsV2GroupMetadataDto {
                title: "Books".to_string(),
                items_per_page: None,
                current_page: None,
                number_of_items: None,
            },
            links: None,
            publications: book_publications,
        }));
    }
    if !collections_navigation.is_empty() {
        groups.push(OpdsV2GroupDto::Navigation(OpdsV2NavigationGroupDto {
            metadata: OpdsV2GroupMetadataDto {
                title: "Collections".to_string(),
                items_per_page: None,
                current_page: None,
                number_of_items: None,
            },
            links: None,
            navigation: collections_navigation,
        }));
    }
    if !readlist_navigation.is_empty() {
        groups.push(OpdsV2GroupDto::Navigation(OpdsV2NavigationGroupDto {
            metadata: OpdsV2GroupMetadataDto {
                title: "Read Lists".to_string(),
                items_per_page: None,
                current_page: None,
                number_of_items: None,
            },
            links: None,
            navigation: readlist_navigation,
        }));
    }

    let links = vec![
        OpdsV2LinkDto {
            title: None,
            rel: Some("start".to_string()),
            href: app_absolute_url(&headers, "/opds/v2/catalog"),
            media_type: Some("application/opds+json".to_string()),
            templated: None,
            properties: None,
        },
        OpdsV2LinkDto {
            title: None,
            rel: Some("search".to_string()),
            href: app_absolute_url(&headers, "/opds/v2/search{?query}"),
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
        Json(OpdsV2SearchFeedDto {
            metadata: OpdsV2RecommendedMetadataDto {
                title: "Search results".to_string(),
                modified: opds_v2_updated(None),
            },
            links,
            groups,
        }),
    )
        .into_response()
}

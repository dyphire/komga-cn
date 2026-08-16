use axum::Json;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use komga_application::identity_access::{AuthUser, user_id};
use komga_application::opds::{
    OpdsBookFeedEntry, OpdsFeedUserContext, OpdsPersistedService, OpdsSeriesAccessError,
    PersistedSeriesBookRecord,
};
use serde_json::{Value, json};

use super::super::feed_endpoints::{opds_v2_collections_feed, opds_v2_readlists_feed};
use super::super::feeds::{
    OpdsV2PagedFeed, normalize_opds_updated, opds_navigation_link,
    opds_navigation_response_with_paging, opds_now_timestamp, opds_publication_for_feed_entry,
    opds_publications_response_with_paging, paginate_vec, parse_page_size, percent_decode,
    query_escape,
};
use super::super::persisted::load_series_tags;
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
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS collection: {error:#}") })),
            )
                .into_response();
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
        if page_request.page > 0 {
            let previous_path = if self_path.contains('?') {
                format!("{self_path}&page={}", page_request.page.saturating_sub(1))
            } else {
                format!("{self_path}?page={}", page_request.page.saturating_sub(1))
            };
            links.push(json!({
                "rel": "previous",
                "href": app_absolute_url(&headers, previous_path.as_str()),
            }));
        }
        if series_page.has_next {
            let next_path = if self_path.contains('?') {
                format!("{self_path}&page={}", page_request.page + 1)
            } else {
                format!("{self_path}?page={}", page_request.page + 1)
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
                    "itemsPerPage": page_request.size,
                    "currentPage": page_request.page + 1,
                    "numberOfItems": total_filtered_series,
                },
                "links": links,
                "navigation": [],
            })),
        )
            .into_response();
    }

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
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS series: {error:#}") })),
            )
                .into_response();
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
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS series books: {error:#}") })),
            )
                .into_response();
        }
    };

    let series_tags = match load_series_tags(app.opds_series_persisted.as_ref(), &series.id).await {
        Ok(tags) => tags,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS series tags: {error:#}") })),
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
    if page_request.page > 0 {
        links.push(json!({
            "rel": "previous",
            "href": app_absolute_url(&headers, series_page_link_path(page_path.as_str(), page_request.page.saturating_sub(1)).as_str()),
        }));
    }
    if books_page.has_next {
        links.push(json!({
            "rel": "next",
            "href": app_absolute_url(&headers, series_page_link_path(page_path.as_str(), page_request.page + 1).as_str()),
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
                "itemsPerPage": page_request.size,
                "currentPage": page_request.page + 1,
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
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS readlist: {error:#}") })),
            )
                .into_response();
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
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load OPDS search results: {error:#}") })),
            )
                .into_response();
        }
    };

    let series_navigation = results
        .series
        .into_iter()
        .map(|item| {
            json!({
                "title": item.title,
                "href": app_absolute_url(&headers, format!("/opds/v2/series/{}", item.id).as_str()),
                "type": "application/opds+json",
            })
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
            json!({
                "title": item.name,
                "href": app_absolute_url(&headers, format!("/opds/v2/collections/{}", item.id).as_str()),
                "type": "application/opds+json",
            })
        })
        .collect::<Vec<_>>();

    let readlist_navigation = results
        .readlists
        .into_iter()
        .map(|item| {
            json!({
                "title": item.name,
                "href": app_absolute_url(&headers, format!("/opds/v2/readlists/{}", item.id).as_str()),
                "type": "application/opds+json",
            })
        })
        .collect::<Vec<_>>();

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

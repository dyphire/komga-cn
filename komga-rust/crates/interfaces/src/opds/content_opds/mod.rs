use axum::extract::State;
use std::path::Path;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path as AxumPath, Query};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::identity_access::auth::{require_auth, resolved_auth_user, user_id};
use crate::media_assets;
use crate::request_urls::app_absolute_url;
use crate::state::HttpAppState;

mod auth_payload;
mod feed_endpoints;
mod feeds;
mod manifest;
mod persisted;
mod types;
mod v1;
mod v2;

use self::auth_payload::opds_catalog_unauthorized_response;
use self::feed_endpoints::*;
use self::feeds::*;
use self::persisted::*;

pub(crate) use self::auth_payload::opds_auth;
pub(crate) use self::manifest::{opds_manifest, opds_manifest_with_profile};

pub(crate) async fn opds_manifest_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    AxumPath(book_id): AxumPath<String>,
) -> Response {
    opds_manifest(headers, &app, &book_id).await
}

pub(crate) async fn opds_manifest_profile_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    AxumPath((book_id, manifest_profile)): AxumPath<(String, String)>,
) -> Response {
    opds_manifest_with_profile(headers, &app, &book_id, &manifest_profile).await
}

pub(crate) async fn opds_auth_route(headers: HeaderMap) -> Response {
    opds_auth(headers).await
}

pub(crate) async fn opds_catalog_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
) -> Response {
    v2::opds_catalog(headers, &app).await
}

pub(crate) async fn opds_v1_series_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if resolved_auth_user(&headers).is_none() {
        return v1::opds_v1_basic_unauthorized_response();
    }

    v1::opds_v1_series(headers, uri, &app).await
}

pub(crate) async fn opds_v1_catalog_route(headers: HeaderMap) -> Response {
    v1::opds_v1_catalog(headers).await
}

pub(crate) async fn opds_v1_search_route(headers: HeaderMap) -> Response {
    v1::opds_v1_search(headers).await
}

pub(crate) async fn opds_v1_on_deck_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if resolved_auth_user(&headers).is_none() {
        return v1::opds_v1_basic_unauthorized_response();
    }

    v1::opds_v1_on_deck(headers, uri, &app).await
}

pub(crate) async fn opds_v1_keep_reading_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if resolved_auth_user(&headers).is_none() {
        return v1::opds_v1_basic_unauthorized_response();
    }

    v1::opds_v1_keep_reading(headers, uri, &app).await
}

pub(crate) async fn opds_v1_series_latest_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    v1::opds_v1_series_latest(headers, uri, &app).await
}

pub(crate) async fn opds_v1_books_latest_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if resolved_auth_user(&headers).is_none() {
        return v1::opds_v1_basic_unauthorized_response();
    }

    v1::opds_v1_books_latest(headers, uri, &app).await
}

pub(crate) async fn opds_v1_libraries_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
) -> Response {
    v1::opds_v1_libraries(headers, &app).await
}

pub(crate) async fn opds_v1_collections_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if resolved_auth_user(&headers).is_none() {
        return v1::opds_v1_basic_unauthorized_response();
    }

    v1::opds_v1_collections(headers, uri, &app).await
}

pub(crate) async fn opds_v1_readlists_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if resolved_auth_user(&headers).is_none() {
        return v1::opds_v1_basic_unauthorized_response();
    }

    v1::opds_v1_readlists(headers, uri, &app).await
}

pub(crate) async fn opds_v1_publishers_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    v1::opds_v1_publishers(headers, uri, &app).await
}

pub(crate) async fn opds_v1_series_detail_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(series_id): AxumPath<String>,
) -> Response {
    if resolved_auth_user(&headers).is_none() {
        return v1::opds_v1_basic_unauthorized_response();
    }

    v1::opds_v1_series_detail(headers, uri, &app, &series_id).await
}

pub(crate) async fn opds_v1_library_detail_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    v1::opds_v1_library_detail(headers, uri, &app, &library_id).await
}

pub(crate) async fn opds_v1_collection_detail_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(collection_id): AxumPath<String>,
) -> Response {
    if resolved_auth_user(&headers).is_none() {
        return v1::opds_v1_basic_unauthorized_response();
    }

    v1::opds_v1_collection_detail(headers, uri, &app, &collection_id).await
}

pub(crate) async fn opds_v1_readlist_detail_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(readlist_id): AxumPath<String>,
) -> Response {
    if resolved_auth_user(&headers).is_none() {
        return v1::opds_v1_basic_unauthorized_response();
    }

    v1::opds_v1_readlist_detail(headers, uri, &app, &readlist_id).await
}

pub(crate) async fn opds_v1_book_file_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    AxumPath((book_id, _file_name)): AxumPath<(String, String)>,
) -> Response {
    media_assets::handlers::book_file(State(app), headers, AxumPath(book_id)).await
}

pub(crate) async fn opds_v1_book_thumbnail_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    AxumPath(book_id): AxumPath<String>,
) -> Response {
    if resolved_auth_user(&headers).is_none() {
        return v1::opds_v1_basic_unauthorized_response();
    }

    media_assets::handlers::book_thumbnail_opds(State(app), headers, AxumPath(book_id)).await
}

pub(crate) async fn opds_v1_book_thumbnail_small_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    AxumPath(book_id): AxumPath<String>,
) -> Response {
    if resolved_auth_user(&headers).is_none() {
        return v1::opds_v1_basic_unauthorized_response();
    }

    media_assets::handlers::book_thumbnail_opds_small(State(app), headers, AxumPath(book_id)).await
}

pub(crate) async fn opds_v2_book_file_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    AxumPath(book_id): AxumPath<String>,
) -> Response {
    if resolved_auth_user(&headers).is_none() {
        return opds_catalog_unauthorized_response(&headers);
    }

    media_assets::handlers::book_file(State(app), headers, AxumPath(book_id)).await
}

pub(crate) async fn opds_v2_book_file_with_suffix_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    AxumPath((book_id, file_name)): AxumPath<(String, String)>,
) -> Response {
    if resolved_auth_user(&headers).is_none() {
        return opds_catalog_unauthorized_response(&headers);
    }

    media_assets::handlers::book_file_with_suffix(
        State(app),
        headers,
        AxumPath((book_id, file_name)),
    )
    .await
}

pub(crate) async fn opds_v2_book_page_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Query(query): Query<media_assets::handlers::BookPageQuery>,
    AxumPath((book_id, page_number)): AxumPath<(String, u32)>,
) -> Response {
    if resolved_auth_user(&headers).is_none() {
        return opds_catalog_unauthorized_response(&headers);
    }

    media_assets::handlers::book_page_opds_v2(
        State(app),
        headers,
        Query(query),
        AxumPath((book_id, page_number)),
    )
    .await
}

pub(crate) async fn opds_v2_book_page_raw_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    AxumPath((book_id, page_number)): AxumPath<(String, i32)>,
) -> Response {
    if resolved_auth_user(&headers).is_none() {
        return opds_catalog_unauthorized_response(&headers);
    }

    media_assets::handlers::book_page_raw(State(app), headers, AxumPath((book_id, page_number)))
        .await
}

pub(crate) async fn opds_v2_book_thumbnail_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    AxumPath(book_id): AxumPath<String>,
) -> Response {
    if resolved_auth_user(&headers).is_none() {
        return opds_catalog_unauthorized_response(&headers);
    }

    media_assets::handlers::book_thumbnail_opds(State(app), headers, AxumPath(book_id)).await
}

pub(crate) async fn opds_v2_libraries_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
) -> Response {
    v2::opds_v2_libraries(headers, &app).await
}

pub(crate) async fn opds_v2_library_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    v2::opds_v2_library(headers, &app, &library_id).await
}

pub(crate) async fn opds_v2_library_readlists_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    v2::opds_v2_library_readlists(headers, uri, &app, &library_id).await
}

pub(crate) async fn opds_v2_libraries_readlists_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    v2::opds_v2_libraries_readlists(headers, uri, &app).await
}

pub(crate) async fn opds_v2_libraries_keep_reading_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    v2::opds_v2_libraries_keep_reading(headers, uri, &app).await
}

pub(crate) async fn opds_v2_library_keep_reading_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    v2::opds_v2_library_keep_reading(headers, uri, &app, &library_id).await
}

pub(crate) async fn opds_v2_libraries_on_deck_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    v2::opds_v2_libraries_on_deck(headers, uri, &app).await
}

pub(crate) async fn opds_v2_library_on_deck_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    v2::opds_v2_library_on_deck(headers, uri, &app, &library_id).await
}

pub(crate) async fn opds_v2_libraries_latest_books_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    v2::opds_v2_libraries_latest_books(headers, uri, &app).await
}

pub(crate) async fn opds_v2_library_latest_books_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    v2::opds_v2_library_latest_books(headers, uri, &app, &library_id).await
}

pub(crate) async fn opds_v2_libraries_latest_series_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    v2::opds_v2_libraries_latest_series(headers, uri, &app).await
}

pub(crate) async fn opds_v2_library_latest_series_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    v2::opds_v2_library_latest_series(headers, uri, &app, &library_id).await
}

pub(crate) async fn opds_v2_libraries_browse_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    v2::opds_v2_libraries_browse(headers, uri, &app).await
}

pub(crate) async fn opds_v2_library_browse_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    v2::opds_v2_library_browse(headers, uri, &app, Some(&library_id)).await
}

pub(crate) async fn opds_v2_libraries_collections_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    v2::opds_v2_libraries_collections(headers, uri, &app).await
}

pub(crate) async fn opds_v2_library_collections_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    v2::opds_v2_library_collections(headers, uri, &app, &library_id).await
}

pub(crate) async fn opds_v2_collection_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(collection_id): AxumPath<String>,
) -> Response {
    v2::opds_v2_collection(headers, uri, &app, &collection_id).await
}

pub(crate) async fn opds_v2_series_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(series_id): AxumPath<String>,
) -> Response {
    v2::opds_v2_series(headers, uri, &app, &series_id).await
}

pub(crate) async fn opds_v2_readlist_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(readlist_id): AxumPath<String>,
) -> Response {
    v2::opds_v2_readlist(headers, uri, &app, &readlist_id).await
}

pub(crate) async fn opds_v2_search_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let query = uri
        .query()
        .and_then(|raw| {
            raw.split('&').find_map(|pair| {
                let (key, value) = pair.split_once('=')?;
                (key == "query").then_some(value)
            })
        })
        .map(|value| percent_decode(&value.replace('+', " ")));

    v2::opds_v2_search(headers, &app, query.as_deref()).await
}

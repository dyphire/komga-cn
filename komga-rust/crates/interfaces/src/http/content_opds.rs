use std::path::Path;

use axum::Json;
use axum::extract::{Extension, Path as AxumPath};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::http::identity_access::auth::{require_auth, resolved_auth_user, user_id};
use crate::http::media_assets;
use crate::http::request_urls::app_absolute_url;
use crate::http::state::AuthDatabaseState;
use crate::http::state::RuntimeProfile;

#[path = "content_opds/auth_payload.rs"]
mod auth_payload;
#[path = "content_opds/feed_endpoints.rs"]
mod feed_endpoints;
#[path = "content_opds/feeds.rs"]
mod feeds;
#[path = "content_opds/manifest.rs"]
mod manifest;
#[path = "content_opds/persisted.rs"]
mod persisted;
#[path = "content_opds/types.rs"]
mod types;
#[path = "content_opds/v1.rs"]
mod v1;
#[path = "content_opds/v2.rs"]
mod v2;

use self::auth_payload::opds_catalog_unauthorized_response;
use self::feed_endpoints::*;
use self::feeds::*;
use self::persisted::*;
use self::types::*;

pub(crate) use self::auth_payload::opds_auth;
pub(crate) use self::manifest::{opds_manifest, opds_manifest_with_profile};
pub(crate) use self::v1::*;
pub(crate) use self::v2::*;

pub(crate) async fn opds_manifest_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    AxumPath(book_id): AxumPath<String>,
) -> Response {
    opds_manifest(headers, auth_db.database_file.as_path(), &book_id).await
}

pub(crate) async fn opds_manifest_profile_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    AxumPath((book_id, manifest_profile)): AxumPath<(String, String)>,
) -> Response {
    opds_manifest_with_profile(
        headers,
        auth_db.database_file.as_path(),
        &book_id,
        &manifest_profile,
    )
    .await
}

pub(crate) async fn opds_auth_route(headers: HeaderMap) -> Response {
    opds_auth(headers).await
}

pub(crate) async fn opds_catalog_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    opds_catalog(headers, auth_db.database_file.as_path()).await
}

pub(crate) async fn opds_v1_series_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    opds_v1_series(headers, uri, auth_db.database_file.as_path()).await
}

pub(crate) async fn opds_v1_catalog_route(headers: HeaderMap) -> Response {
    opds_v1_catalog(headers).await
}

pub(crate) async fn opds_v1_search_route(headers: HeaderMap) -> Response {
    opds_v1_search(headers).await
}

pub(crate) async fn opds_v1_on_deck_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    opds_v1_on_deck(headers, uri, auth_db.database_file.as_path()).await
}

pub(crate) async fn opds_v1_keep_reading_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    opds_v1_keep_reading(headers, uri, auth_db.database_file.as_path()).await
}

pub(crate) async fn opds_v1_series_latest_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    opds_v1_series_latest(headers, uri, auth_db.database_file.as_path()).await
}

pub(crate) async fn opds_v1_books_latest_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    opds_v1_books_latest(headers, uri, auth_db.database_file.as_path()).await
}

pub(crate) async fn opds_v1_libraries_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    opds_v1_libraries(headers, auth_db.database_file.as_path()).await
}

pub(crate) async fn opds_v1_collections_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    opds_v1_collections(headers, uri, auth_db.database_file.as_path()).await
}

pub(crate) async fn opds_v1_readlists_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    opds_v1_readlists(headers, uri, auth_db.database_file.as_path()).await
}

pub(crate) async fn opds_v1_publishers_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    opds_v1_publishers(headers, uri, auth_db.database_file.as_path()).await
}

pub(crate) async fn opds_v1_series_detail_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(series_id): AxumPath<String>,
) -> Response {
    opds_v1_series_detail(headers, uri, auth_db.database_file.as_path(), &series_id).await
}

pub(crate) async fn opds_v1_library_detail_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    opds_v1_library_detail(headers, uri, auth_db.database_file.as_path(), &library_id).await
}

pub(crate) async fn opds_v1_collection_detail_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(collection_id): AxumPath<String>,
) -> Response {
    opds_v1_collection_detail(
        headers,
        uri,
        auth_db.database_file.as_path(),
        &collection_id,
    )
    .await
}

pub(crate) async fn opds_v1_readlist_detail_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(readlist_id): AxumPath<String>,
) -> Response {
    opds_v1_readlist_detail(headers, uri, auth_db.database_file.as_path(), &readlist_id).await
}

pub(crate) async fn opds_v1_book_file_route(
    Extension(profile): Extension<RuntimeProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    AxumPath((book_id, _file_name)): AxumPath<(String, String)>,
) -> Response {
    media_assets::book_file(
        Extension(profile),
        Extension(auth_db),
        headers,
        AxumPath(book_id),
    )
    .await
}

pub(crate) async fn opds_v2_libraries_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    opds_v2_libraries(headers, auth_db.database_file.as_path()).await
}

pub(crate) async fn opds_v2_library_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    opds_v2_library(headers, auth_db.database_file.as_path(), &library_id).await
}

pub(crate) async fn opds_v2_library_readlists_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    opds_v2_library_readlists(headers, auth_db.database_file.as_path(), &library_id).await
}

pub(crate) async fn opds_v2_libraries_readlists_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    opds_v2_libraries_readlists(headers, auth_db.database_file.as_path()).await
}

pub(crate) async fn opds_v2_libraries_keep_reading_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    opds_v2_libraries_keep_reading(headers, auth_db.database_file.as_path()).await
}

pub(crate) async fn opds_v2_library_keep_reading_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    opds_v2_library_keep_reading(headers, auth_db.database_file.as_path(), &library_id).await
}

pub(crate) async fn opds_v2_libraries_on_deck_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    opds_v2_libraries_on_deck(headers, auth_db.database_file.as_path()).await
}

pub(crate) async fn opds_v2_library_on_deck_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    opds_v2_library_on_deck(headers, auth_db.database_file.as_path(), &library_id).await
}

pub(crate) async fn opds_v2_libraries_latest_books_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    opds_v2_libraries_latest_books(headers, auth_db.database_file.as_path()).await
}

pub(crate) async fn opds_v2_library_latest_books_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    opds_v2_library_latest_books(headers, auth_db.database_file.as_path(), &library_id).await
}

pub(crate) async fn opds_v2_libraries_latest_series_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    opds_v2_libraries_latest_series(headers, auth_db.database_file.as_path()).await
}

pub(crate) async fn opds_v2_library_latest_series_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    opds_v2_library_latest_series(headers, auth_db.database_file.as_path(), &library_id).await
}

pub(crate) async fn opds_v2_libraries_browse_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    opds_v2_libraries_browse(headers, uri, auth_db.database_file.as_path()).await
}

pub(crate) async fn opds_v2_library_browse_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    opds_v2_library_browse(
        headers,
        uri,
        auth_db.database_file.as_path(),
        Some(&library_id),
    )
    .await
}

pub(crate) async fn opds_v2_libraries_collections_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    opds_v2_libraries_collections(headers, auth_db.database_file.as_path()).await
}

pub(crate) async fn opds_v2_library_collections_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    AxumPath(library_id): AxumPath<String>,
) -> Response {
    opds_v2_library_collections(headers, auth_db.database_file.as_path(), &library_id).await
}

pub(crate) async fn opds_v2_collection_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    AxumPath(collection_id): AxumPath<String>,
) -> Response {
    opds_v2_collection(headers, auth_db.database_file.as_path(), &collection_id).await
}

pub(crate) async fn opds_v2_book_thumbnail_small_route(
    headers: HeaderMap,
    AxumPath(book_id): AxumPath<String>,
) -> Response {
    opds_v2_book_thumbnail_small(headers, &book_id).await
}

pub(crate) async fn opds_v2_series_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    AxumPath(series_id): AxumPath<String>,
) -> Response {
    opds_v2_series(headers, auth_db.database_file.as_path(), &series_id).await
}

pub(crate) async fn opds_v2_readlist_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    AxumPath(readlist_id): AxumPath<String>,
) -> Response {
    opds_v2_readlist(headers, auth_db.database_file.as_path(), &readlist_id).await
}

pub(crate) async fn opds_v2_search_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
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

    opds_v2_search(headers, auth_db.database_file.as_path(), query.as_deref()).await
}

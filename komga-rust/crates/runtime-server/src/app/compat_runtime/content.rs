use axum::extract::Extension;
use axum::http::{HeaderMap, Uri};
use axum::response::Response;

use crate::app::CompatProfile;
use crate::app::discovery_auth::DiscoveryAuthState;

#[path = "content_auth.rs"]
mod content_auth;
#[path = "content_java_live.rs"]
mod content_java_live;
#[path = "content_libraries.rs"]
mod content_libraries;
#[path = "content_opds.rs"]
mod content_opds;
#[path = "content/detail.rs"]
mod detail;
#[path = "content/discovery.rs"]
mod discovery;
#[path = "content/helpers.rs"]
mod helpers;
#[path = "content/media.rs"]
mod media;

pub(super) use detail::{
    book_detail, book_readlists, book_sibling_next, book_sibling_previous, readlist_detail,
    readlist_book_sibling_next, readlist_book_sibling_previous, readlist_books, readlists,
    series_collections, series_detail,
};
pub(super) use discovery::{books, books_latest, books_list, series, series_list};
pub(super) use helpers::{
    DiscoveryOwnershipRoute, DiscoveryShape, discovery_ownership_route, mark_native,
};
pub(super) use media::{
    book_file, book_page, book_page_thumbnail, book_pages, book_progression, book_progression_get,
    book_read_progress, book_read_progress_delete, book_read_progress_get, book_thumbnail,
};

pub(super) async fn libraries(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
) -> Response {
    content_libraries::response(profile, headers, auth_state).await
}

pub(super) async fn users_me(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    content_auth::users_me(headers, uri, auth_state).await
}

pub(super) async fn login_set_cookie(headers: HeaderMap) -> Response {
    content_auth::login_set_cookie(headers).await
}

pub(super) async fn opds_manifest(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
) -> Response {
    content_opds::opds_manifest(profile, headers).await
}

pub(super) async fn opds_auth(headers: HeaderMap) -> Response {
    content_opds::opds_auth(headers).await
}

pub(super) async fn opds_catalog(headers: HeaderMap) -> Response {
    content_opds::opds_catalog(headers).await
}

pub(super) async fn opds_v1_series(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
) -> Response {
    content_opds::opds_v1_series(profile, headers).await
}

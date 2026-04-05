use std::path::Path as FsPath;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path as AxumPath};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_application::discovery::{
    DiscoveryRequestValidation, bootstrap_series_id_for_runtime_shape, query_validation_mode,
    reject_bootstrap_shape_mismatch, requested_library_ids_for_runtime_shape,
};
use komga_domain::discovery::{DirectBrowseBooksListFamily, DiscoveryError};
use serde_json::{Value, json};

use crate::http::discovery_auth::DiscoveryAuthState;
use crate::http::identity_access::auth::{
    require_admin, require_auth, resolved_auth_user, user_id,
};

use super::super::{AuthDatabaseState, OperationalState};
use super::helpers::{
    books_page_payload, contains_legacy_search_input, contains_legacy_search_query,
    extract_full_text_search, mark_persisted_owned, mark_runtime_owned, query_bool, query_value,
    query_values, wants_persisted_marker,
};

#[path = "books.rs"]
mod books;
#[path = "detail.rs"]
mod detail;
#[path = "facets.rs"]
mod facets;
#[path = "series.rs"]
mod series;
#[path = "series_routes.rs"]
mod series_routes;

pub use books::{book_tags, books, books_duplicates, books_latest, books_list, books_ondeck};
pub use detail::{
    DiscoveryDetailAccessBackends, DiscoveryDetailBooksAccessBackend,
    DiscoveryDetailCollectionsAccessBackend, DiscoveryDetailReadlistsAccessBackend,
    DiscoveryDetailSeriesAccessBackend, ExistingSeriesMetadataRecord, PersistedBookAuthorRecord,
    PersistedBookDetailRecord, PersistedBookResourceRecord, PersistedBookSiblingDirectionRecord,
    PersistedCollectionAccessRecord, PersistedComicrackMatchCandidateRecord,
    PersistedReadProgressRecord, PersistedReadlistBookRecord, PersistedReadlistRecord,
    PersistedSeriesCollectionRecord, PersistedSeriesDetailRecord, PersistedSeriesResourceRecord,
    PersistedSeriesRestrictionRecord, SeriesAlternateTitleRecord, SeriesMetadataLinkRecord,
    SeriesSummaryRecord, book_detail, book_readlists, book_sibling_next, book_sibling_previous,
    collection_create, collection_delete, collection_detail, collection_series, collection_update,
    collections, install_discovery_detail_access_backends, readlist_book_sibling_next,
    readlist_book_sibling_previous, readlist_books, readlist_create, readlist_delete,
    readlist_detail, readlist_match_comicrack, readlist_update, readlists,
    resolve_book_id_for_persisted, resolve_series_id_for_persisted, series_collections,
    series_detail, series_metadata_update,
};
pub use facets::{
    age_ratings, authors, authors_names, authors_roles, authors_v2, genres, languages, publishers,
    series_release_dates, series_tags, sharing_labels, tags,
};
pub use persisted::{
    PersistedAuthorEntry, PersistedAuthorsScope, PersistedBookBrowseEntry,
    PersistedBookPosterSummary, PersistedBookSummary, PersistedBookTagsScope,
    PersistedDiscoveryAccessBackend, PersistedSeriesSummary, install_persisted_discovery_access,
};
pub use series::{
    series, series_alphabetical_groups, series_latest, series_list, series_new, series_updated,
};
pub use series_routes::{series_alphabetical_groups_deprecated, series_books};

#[path = "filters.rs"]
mod filters;
#[path = "persisted.rs"]
mod persisted;

use filters::*;
use persisted::*;

pub(super) async fn authors_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    authors(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn authors_names_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    authors_names(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn authors_roles_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    authors_roles(headers, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn genres_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    genres(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn tags_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    tags(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn series_tags_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    series_tags(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn languages_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    languages(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn publishers_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    publishers(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn age_ratings_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    age_ratings(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn sharing_labels_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    sharing_labels(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn series_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    series(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn series_new_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    series_new(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn series_updated_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    series_updated(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn series_release_dates_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    series_release_dates(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn series_latest_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    series_latest(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn series_detail_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    AxumPath(series_id): AxumPath<String>,
) -> Response {
    series_detail(
        headers,
        AxumPath(series_id),
        auth_state,
        auth_db.database_file.as_path(),
    )
    .await
}

pub(super) async fn series_collections_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    AxumPath(series_id): AxumPath<String>,
) -> Response {
    series_collections(
        headers,
        AxumPath(series_id),
        auth_state,
        auth_db.database_file.as_path(),
    )
    .await
}

pub(super) async fn series_metadata_update_route(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(operational): Extension<OperationalState>,
    headers: HeaderMap,
    AxumPath(series_id): AxumPath<String>,
    Json(body): Json<Value>,
) -> Response {
    series_metadata_update(
        headers,
        auth_db.database_file.as_path(),
        operational.runtime.lucene_data_directory.as_path(),
        AxumPath(series_id),
        body,
    )
    .await
}

pub(super) async fn series_alphabetical_groups_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    series_alphabetical_groups(headers, body, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn authors_v2_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    authors_v2(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

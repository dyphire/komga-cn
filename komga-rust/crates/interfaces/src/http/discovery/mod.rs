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

use crate::http::discovery_auth::state::DiscoveryAuthState;
use crate::http::identity_access::auth::{
    require_request_admin, require_request_auth, resolved_request_auth_user, user_id,
};

use super::helpers::{
    books_page_payload, extract_full_text_search, mark_runtime_owned, query_bool, query_value,
    query_values,
};
use crate::http::state::{AuthDatabaseState, OperationalState};

pub mod books;
pub mod detail;
mod facets;
mod filters;
pub mod persisted;
pub mod series;
mod series_routes;

pub(super) async fn authors_names_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    facets::authors_names(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn authors_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    facets::authors_deprecated_get(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn authors_roles_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
) -> Response {
    facets::authors_roles(headers, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn genres_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    facets::genres(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn tags_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    facets::tags(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn series_tags_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    facets::series_tags(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn languages_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    facets::languages(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn publishers_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    facets::publishers(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn age_ratings_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    facets::age_ratings(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn sharing_labels_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    facets::sharing_labels(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn series_new_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    series::series_new(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn series_updated_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    series::series_updated(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn series_release_dates_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    facets::series_release_dates(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn series_latest_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    series::series_latest(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn series_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    series::series_deprecated_get(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn books_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    books::books_deprecated_get(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

pub(super) async fn series_detail_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    AxumPath(series_id): AxumPath<String>,
) -> Response {
    detail::series_detail(
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
    detail::series_collections(
        headers,
        AxumPath(series_id),
        auth_state,
        auth_db.database_file.as_path(),
    )
    .await
}

pub(super) async fn series_books_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(series_id): AxumPath<String>,
) -> Response {
    books::series_books_deprecated(
        headers,
        uri,
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
    detail::series_metadata_update(
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
    series::series_alphabetical_groups(headers, body, auth_state, auth_db.database_file.as_path())
        .await
}

pub(super) async fn authors_v2_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    facets::authors_v2(headers, uri, auth_state, auth_db.database_file.as_path()).await
}

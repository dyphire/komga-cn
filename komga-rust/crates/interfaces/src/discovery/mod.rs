use axum::extract::State;
use std::path::Path as FsPath;
use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::Path as AxumPath;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_application::discovery::{
    DiscoveryRequestValidation, bootstrap_series_id_for_runtime_shape, query_validation_mode,
    reject_bootstrap_shape_mismatch, requested_library_ids_for_runtime_shape,
};
use komga_domain::discovery::{DirectBrowseBooksListFamily, DiscoveryError};
use serde_json::{Value, json};

use crate::identity_access::auth::{
    require_request_admin, require_request_auth, resolved_request_auth_user, user_id,
};

use super::helpers::{
    books_page_payload, extract_full_text_search, mark_runtime_owned, query_bool, query_value,
    query_values,
};
use crate::state::HttpAppState;

pub mod books;
pub mod detail;
mod facets;
mod filters;
pub mod persisted;
pub mod series;
mod series_routes;

pub(super) async fn authors_names_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    facets::authors_names(headers, uri, &app).await
}

pub(super) async fn authors_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    facets::authors_deprecated_get(headers, uri, &app).await
}

pub(super) async fn authors_roles_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
) -> Response {
    facets::authors_roles(headers, &app).await
}

pub(super) async fn genres_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    facets::genres(headers, uri, &app).await
}

pub(super) async fn tags_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    facets::tags(headers, uri, &app).await
}

pub(super) async fn series_tags_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    facets::series_tags(headers, uri, &app).await
}

pub(super) async fn languages_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    facets::languages(headers, uri, &app).await
}

pub(super) async fn publishers_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    facets::publishers(headers, uri, &app).await
}

pub(super) async fn age_ratings_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    facets::age_ratings(headers, uri, &app).await
}

pub(super) async fn sharing_labels_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    facets::sharing_labels(headers, uri, &app).await
}

pub(super) async fn series_new_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    series::series_new(headers, uri, &app).await
}

pub(super) async fn series_updated_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    series::series_updated(headers, uri, &app).await
}

pub(super) async fn series_release_dates_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    facets::series_release_dates(headers, uri, &app).await
}

pub(super) async fn series_latest_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    series::series_latest(headers, uri, &app).await
}

pub(super) async fn series_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    series::series_deprecated_get(headers, uri, &app).await
}

pub(super) async fn books_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    books::books_deprecated_get(headers, uri, &app).await
}

pub(super) async fn series_detail_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    AxumPath(series_id): AxumPath<String>,
) -> Response {
    detail::series_detail(headers, AxumPath(series_id), &app).await
}

pub(super) async fn series_collections_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    AxumPath(series_id): AxumPath<String>,
) -> Response {
    detail::series_collections(headers, AxumPath(series_id), &app).await
}

pub(super) async fn series_books_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(series_id): AxumPath<String>,
) -> Response {
    books::series_books_deprecated(State(app), headers, uri, AxumPath(series_id)).await
}

pub(super) async fn series_metadata_update_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    AxumPath(series_id): AxumPath<String>,
    Json(body): Json<Value>,
) -> Response {
    detail::series_metadata_update(headers, &app, AxumPath(series_id), body).await
}

pub(super) async fn series_alphabetical_groups_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    series::series_alphabetical_groups(headers, body, &app).await
}

pub(super) async fn authors_v2_route(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    facets::authors_v2(headers, uri, &app).await
}

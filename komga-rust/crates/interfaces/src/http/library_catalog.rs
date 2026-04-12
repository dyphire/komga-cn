use axum::Json;
use axum::extract::{Extension, Path};
use axum::http::{HeaderMap, Uri};
use axum::response::Response;
use serde_json::Value;

#[path = "library_catalog/handlers.rs"]
mod handlers;
#[path = "library_catalog/request_mapping.rs"]
mod request_mapping;
#[path = "library_catalog/response_mapping.rs"]
mod response_mapping;
#[path = "library_catalog/task_mapping.rs"]
mod task_mapping;

pub use handlers::*;

pub use crate::OperationalState;
pub use crate::http::helpers::{mark_runtime_owned, to_domain_query_context};

use crate::http::discovery_auth::DiscoveryAuthState;
use crate::http::state::AuthDatabaseState;
use crate::http::state::OperationalState as InterfaceOperationalState;
pub(super) async fn libraries_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(operational): Extension<InterfaceOperationalState>,
    headers: HeaderMap,
) -> Response {
    response(
        headers,
        auth_state,
        auth_db.database_file.as_path(),
        operational,
    )
    .await
}

pub(super) async fn library_detail_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(operational): Extension<InterfaceOperationalState>,
    headers: HeaderMap,
    Path(library_id): Path<String>,
) -> Response {
    library_detail(
        headers,
        auth_state,
        auth_db.database_file.as_path(),
        operational,
        Path(library_id),
    )
    .await
}

pub(super) async fn library_create_route(
    Extension(operational): Extension<InterfaceOperationalState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    library_create(headers, operational, body).await
}

pub(super) async fn library_update_route(
    Extension(operational): Extension<InterfaceOperationalState>,
    headers: HeaderMap,
    Path(library_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    library_update(headers, operational, Path(library_id), body).await
}

pub(super) async fn library_delete_route(
    Extension(operational): Extension<InterfaceOperationalState>,
    headers: HeaderMap,
    Path(library_id): Path<String>,
) -> Response {
    library_delete(headers, operational, Path(library_id)).await
}

pub(super) async fn library_scan_route(
    Extension(operational): Extension<InterfaceOperationalState>,
    headers: HeaderMap,
    uri: Uri,
    Path(library_id): Path<String>,
) -> Response {
    library_scan(headers, uri, operational, Path(library_id)).await
}

pub(super) async fn library_analyze_route(
    Extension(operational): Extension<InterfaceOperationalState>,
    headers: HeaderMap,
    Path(library_id): Path<String>,
) -> Response {
    library_analyze(headers, operational, Path(library_id)).await
}

pub(super) async fn library_metadata_refresh_route(
    Extension(operational): Extension<InterfaceOperationalState>,
    headers: HeaderMap,
    Path(library_id): Path<String>,
) -> Response {
    library_metadata_refresh(headers, operational, Path(library_id)).await
}

pub(super) async fn library_empty_trash_route(
    Extension(operational): Extension<InterfaceOperationalState>,
    headers: HeaderMap,
    Path(library_id): Path<String>,
) -> Response {
    library_empty_trash(headers, operational, Path(library_id)).await
}

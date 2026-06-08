use axum::Json;
use axum::extract::Path;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_application::library_catalog::{LibraryDetailAccess, LibraryRecord};
use komga_domain::discovery::DiscoveryError;
use serde_json::{Value, json};

use crate::discovery_auth::context::DiscoveryQueryContext;
use crate::helpers::to_domain_query_context;
use crate::identity_access::auth::{Admin, Authenticated};
use crate::state::LibraryCatalogState;

use super::request_mapping::{
    is_deep_scan_query, parse_create_library_change_set, parse_update_library_change_set,
};
use super::response_mapping::{libraries_payload, library_payload};
use super::task_mapping::LibraryCatalogCommands;

pub async fn libraries_route(
    State(app): State<LibraryCatalogState>,
    _: Authenticated,
    headers: HeaderMap,
) -> Response {
    let context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(&app.identity, &headers, None)
        .await
    {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    runtime_owned_libraries_response(context, &app).await
}

pub async fn library_detail_route(
    State(app): State<LibraryCatalogState>,
    _: Authenticated,
    headers: HeaderMap,
    Path(library_id): Path<String>,
) -> Response {
    let context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(&app.identity, &headers, None)
        .await
    {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    runtime_owned_library_detail_response(context, &app, &library_id).await
}

pub async fn library_create_route(
    State(app): State<LibraryCatalogState>,
    Admin(_admin): Admin,
    Json(body): Json<Value>,
) -> Response {
    let changes = match parse_create_library_change_set(&body) {
        Ok(changes) => changes,
        Err(response) => return response,
    };
    LibraryCatalogCommands::new(&app)
        .create_library(changes)
        .await
}

pub async fn library_update_route(
    State(app): State<LibraryCatalogState>,
    Admin(_admin): Admin,
    Path(library_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let changes = match parse_update_library_change_set(&body) {
        Ok(changes) => changes,
        Err(response) => return response,
    };
    LibraryCatalogCommands::new(&app)
        .update_library(&library_id, changes)
        .await
}

pub async fn library_delete_route(
    State(app): State<LibraryCatalogState>,
    Admin(_admin): Admin,
    Path(library_id): Path<String>,
) -> Response {
    LibraryCatalogCommands::new(&app)
        .delete_library(&library_id)
        .await
}

pub async fn library_scan_route(
    State(app): State<LibraryCatalogState>,
    Admin(_admin): Admin,
    uri: Uri,
    Path(library_id): Path<String>,
) -> Response {
    let deep_scan = uri.query().map(is_deep_scan_query).unwrap_or(false);
    LibraryCatalogCommands::new(&app)
        .scan_library(&library_id, deep_scan)
        .await
}

pub async fn library_analyze_route(
    State(app): State<LibraryCatalogState>,
    Admin(_admin): Admin,
    Path(library_id): Path<String>,
) -> Response {
    LibraryCatalogCommands::new(&app)
        .analyze_library(&library_id)
        .await
}

pub async fn library_metadata_refresh_route(
    State(app): State<LibraryCatalogState>,
    Admin(_admin): Admin,
    Path(library_id): Path<String>,
) -> Response {
    LibraryCatalogCommands::new(&app)
        .refresh_metadata(&library_id)
        .await
}

pub async fn library_empty_trash_route(
    State(app): State<LibraryCatalogState>,
    Admin(_admin): Admin,
    Path(library_id): Path<String>,
) -> Response {
    LibraryCatalogCommands::new(&app)
        .empty_trash(&library_id)
        .await
}

pub(super) fn bad_request_response(message: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))).into_response()
}

fn discovery_error_message(error: &DiscoveryError) -> String {
    match error {
        DiscoveryError::UnsupportedSemantics(details) => format!("{details:?}"),
        DiscoveryError::InvalidSemantics(message) | DiscoveryError::Persistence(message) => {
            message.clone()
        }
    }
}

async fn runtime_owned_libraries_response(
    context: DiscoveryQueryContext,
    app: &LibraryCatalogState,
) -> Response {
    match runtime_owned_libraries(context.clone(), app).await {
        Ok(libraries) => Json(libraries_payload(libraries, context.is_admin)).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": discovery_error_message(&error) })),
        )
            .into_response(),
    }
}

async fn runtime_owned_library_detail_response(
    context: DiscoveryQueryContext,
    app: &LibraryCatalogState,
    library_id: &str,
) -> Response {
    let domain_context = to_domain_query_context(context.clone());
    match app
        .library_catalog
        .library_detail_access(domain_context, library_id)
        .await
    {
        Ok(LibraryDetailAccess::Visible(library)) => {
            Json(library_payload(&library, context.is_admin)).into_response()
        }
        Ok(LibraryDetailAccess::Forbidden) => StatusCode::FORBIDDEN.into_response(),
        Ok(LibraryDetailAccess::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": discovery_error_message(&error) })),
        )
            .into_response(),
    }
}

async fn runtime_owned_libraries(
    context: DiscoveryQueryContext,
    app: &LibraryCatalogState,
) -> Result<Vec<LibraryRecord>, DiscoveryError> {
    let domain_context = to_domain_query_context(context);
    app.library_catalog.list_libraries(domain_context).await
}

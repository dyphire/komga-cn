use axum::Json;
use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_application::library_catalog::{LibraryCatalogMutationError, LibraryRecord};
use komga_domain::discovery::DiscoveryError;
use serde_json::{Value, json};

use super::OperationalState;
use crate::http::discovery_auth::{DiscoveryAuthState, DiscoveryQueryContext};
use crate::http::helpers::mark_runtime_owned;
use crate::http::helpers::to_domain_query_context;
use crate::http::identity_access::auth::{require_admin, require_auth};
use crate::http::library_catalog::request_mapping::{
    is_deep_scan_query, parse_create_library_change_set, parse_update_library_change_set,
};
use crate::http::library_catalog::response_mapping::{libraries_payload, library_payload};
use crate::http::library_catalog::task_mapping::{
    enqueue_task_records, enqueue_task_records_with_status,
};

pub async fn response(
    headers: HeaderMap,
    auth_state: DiscoveryAuthState,
    state: OperationalState,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let context = match auth_state.resolve_query_context(&headers, None) {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    runtime_owned_libraries_response(context, &state).await
}

pub async fn library_detail(
    headers: HeaderMap,
    auth_state: DiscoveryAuthState,
    state: OperationalState,
    Path(library_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let context = match auth_state.resolve_query_context(&headers, Some(&[library_id.clone()])) {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    runtime_owned_library_detail_response(context, &state, &library_id).await
}

pub async fn library_update(
    headers: HeaderMap,
    state: OperationalState,
    Path(library_id): Path<String>,
    body: Value,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let changes = match parse_update_library_change_set(&body) {
        Ok(changes) => changes,
        Err(response) => return response,
    };
    let update_library = state.library_catalog.update_library.clone();
    match update_library(library_id, changes).await {
        Ok(result) if result.task_records.is_empty() => StatusCode::NO_CONTENT.into_response(),
        Ok(result) => {
            enqueue_task_records_with_status(&state, result.task_records, StatusCode::NO_CONTENT)
        }
        Err(error) => mutation_error_response(error),
    }
}

pub async fn library_create(headers: HeaderMap, state: OperationalState, body: Value) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let changes = match parse_create_library_change_set(&body) {
        Ok(changes) => changes,
        Err(response) => return response,
    };
    let create_library = state.library_catalog.create_library.clone();
    match create_library(changes).await {
        Ok(result) => {
            let enqueue_response = enqueue_task_records(&state, result.task_records);
            if enqueue_response.status().is_server_error() {
                return enqueue_response;
            }
            Json(library_payload(&result.library, true)).into_response()
        }
        Err(error) => mutation_error_response(error),
    }
}

pub async fn library_delete(
    headers: HeaderMap,
    state: OperationalState,
    Path(library_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let delete_library = state.library_catalog.delete_library.clone();
    match delete_library(library_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => mutation_error_response(error),
    }
}

pub async fn library_scan(
    headers: HeaderMap,
    uri: Uri,
    state: OperationalState,
    Path(library_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let deep_scan = uri.query().map(is_deep_scan_query).unwrap_or(false);
    let scan_library = state.library_catalog.scan_library.clone();
    match scan_library(library_id, deep_scan).await {
        Ok(result) => enqueue_task_records(&state, result.task_records),
        Err(error) => mutation_error_response(error),
    }
}

pub async fn library_analyze(
    headers: HeaderMap,
    state: OperationalState,
    Path(library_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let analyze_library = state.library_catalog.analyze_library.clone();
    match analyze_library(library_id).await {
        Ok(result) => enqueue_task_records(&state, result.task_records),
        Err(error) => mutation_error_response(error),
    }
}

pub async fn library_metadata_refresh(
    headers: HeaderMap,
    state: OperationalState,
    Path(library_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let refresh_metadata = state.library_catalog.refresh_metadata.clone();
    match refresh_metadata(library_id).await {
        Ok(result) => enqueue_task_records(&state, result.task_records),
        Err(error) => mutation_error_response(error),
    }
}

pub async fn library_empty_trash(
    headers: HeaderMap,
    state: OperationalState,
    Path(library_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let empty_trash = state.library_catalog.empty_trash.clone();
    match empty_trash(library_id).await {
        Ok(result) => enqueue_task_records(&state, result.task_records),
        Err(error) => mutation_error_response(error),
    }
}

pub(super) fn bad_request_response(message: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))).into_response()
}

fn internal_error_response(error: impl std::fmt::Display) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": error.to_string() })),
    )
        .into_response()
}

fn mutation_error_response(error: LibraryCatalogMutationError) -> Response {
    match error {
        LibraryCatalogMutationError::NotFound => StatusCode::NOT_FOUND.into_response(),
        LibraryCatalogMutationError::Validation(message) => bad_request_response(&message),
        LibraryCatalogMutationError::Persistence(message) => internal_error_response(message),
    }
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
    state: &OperationalState,
) -> Response {
    match runtime_owned_libraries(context.clone(), state).await {
        Ok(libraries) => {
            let mut response = Json(libraries_payload(libraries, context.is_admin)).into_response();
            mark_runtime_owned(&mut response);
            response
        }
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": discovery_error_message(&error) })),
        )
            .into_response(),
    }
}

async fn runtime_owned_library_detail_response(
    context: DiscoveryQueryContext,
    state: &OperationalState,
    library_id: &str,
) -> Response {
    let domain_context = to_domain_query_context(context.clone());
    let get_library = state.library_catalog.get_library.clone();
    match get_library(domain_context, library_id.to_string()).await {
        Ok(Some(library)) => {
            let mut response = Json(library_payload(&library, context.is_admin)).into_response();
            mark_runtime_owned(&mut response);
            response
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": discovery_error_message(&error) })),
        )
            .into_response(),
    }
}

async fn runtime_owned_libraries(
    context: DiscoveryQueryContext,
    state: &OperationalState,
) -> Result<Vec<LibraryRecord>, DiscoveryError> {
    let domain_context = to_domain_query_context(context);
    let list_libraries = state.library_catalog.list_libraries.clone();
    list_libraries(domain_context).await
}

use axum::Json;
use axum::extract::{Extension, Path};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_application::library_catalog::{LibraryCatalogMutationError, LibraryRecord};
use komga_domain::discovery::DiscoveryError;
use serde_json::{Value, json};
use std::path::Path as FsPath;

use crate::http::discovery_auth::context::{
    DetailAccessDenial, DetailResourceContext, DiscoveryQueryContext,
};
use crate::http::discovery_auth::state::DiscoveryAuthState;
use crate::http::helpers::{detail_access_denial_response, mark_runtime_owned, to_domain_query_context};
use crate::http::identity_access::auth::{require_admin, require_request_auth};
use crate::http::state::AuthDatabaseState;
use crate::http::state::OperationalState;

use super::request_mapping::{is_deep_scan_query, parse_create_library_change_set, parse_update_library_change_set};
use super::response_mapping::{libraries_payload, library_payload};
use super::task_mapping::{enqueue_task_records, enqueue_task_records_with_status};

pub async fn libraries_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(operational): Extension<OperationalState>,
    headers: HeaderMap,
) -> Response {
    response(headers, auth_state, auth_db.database_file.as_path(), operational).await
}

pub async fn library_detail_route(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(operational): Extension<OperationalState>,
    headers: HeaderMap,
    path: Path<String>,
) -> Response {
    library_detail(
        headers,
        auth_state,
        auth_db.database_file.as_path(),
        operational,
        path,
    )
    .await
}

pub async fn library_create_route(
    Extension(operational): Extension<OperationalState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    library_create(headers, operational, body).await
}

pub async fn library_update_route(
    Extension(operational): Extension<OperationalState>,
    headers: HeaderMap,
    path: Path<String>,
    Json(body): Json<Value>,
) -> Response {
    library_update(headers, operational, path, body).await
}

pub async fn library_delete_route(
    Extension(operational): Extension<OperationalState>,
    headers: HeaderMap,
    path: Path<String>,
) -> Response {
    library_delete(headers, operational, path).await
}

pub async fn library_scan_route(
    Extension(operational): Extension<OperationalState>,
    headers: HeaderMap,
    uri: Uri,
    path: Path<String>,
) -> Response {
    library_scan(headers, uri, operational, path).await
}

pub async fn library_analyze_route(
    Extension(operational): Extension<OperationalState>,
    headers: HeaderMap,
    path: Path<String>,
) -> Response {
    library_analyze(headers, operational, path).await
}

pub async fn library_metadata_refresh_route(
    Extension(operational): Extension<OperationalState>,
    headers: HeaderMap,
    path: Path<String>,
) -> Response {
    library_metadata_refresh(headers, operational, path).await
}

pub async fn library_empty_trash_route(
    Extension(operational): Extension<OperationalState>,
    headers: HeaderMap,
    path: Path<String>,
) -> Response {
    library_empty_trash(headers, operational, path).await
}

pub async fn response(
    headers: HeaderMap,
    auth_state: DiscoveryAuthState,
    database_file: &FsPath,
    state: OperationalState,
) -> Response {
    if let Some(response) = require_request_auth(&headers, database_file).await {
        return response;
    }

    let context = match auth_state
        .resolve_query_context_with_persistence(&headers, None, database_file)
        .await
    {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    runtime_owned_libraries_response(context, &state).await
}

pub async fn library_detail(
    headers: HeaderMap,
    auth_state: DiscoveryAuthState,
    database_file: &FsPath,
    state: OperationalState,
    Path(library_id): Path<String>,
) -> Response {
    if let Some(response) = require_request_auth(&headers, database_file).await {
        return response;
    }

    let detail_context = DetailResourceContext {
        library_id: Some(library_id.clone()),
        content: None,
    };

    let context = match auth_state
        .resolve_detail_query_context_with_persistence(&headers, &detail_context, database_file)
        .await
    {
        Ok(context) => context,
        Err(DetailAccessDenial::Forbidden) => {
            return forbidden_library_detail_response(&state, &library_id).await;
        }
        Err(denial) => return detail_access_denial_response(denial),
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

async fn forbidden_library_detail_response(state: &OperationalState, library_id: &str) -> Response {
    match library_exists(state, library_id).await {
        Ok(true) => StatusCode::FORBIDDEN.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": discovery_error_message(&error) })),
        )
            .into_response(),
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

async fn library_exists(
    state: &OperationalState,
    library_id: &str,
) -> Result<bool, DiscoveryError> {
    Ok(
        runtime_owned_libraries(admin_discovery_query_context(), state)
            .await?
            .into_iter()
            .any(|library| library.id == library_id),
    )
}

fn admin_discovery_query_context() -> DiscoveryQueryContext {
    DiscoveryQueryContext {
        user_id: None,
        is_admin: true,
        authorized_library_ids: None,
        restrictions: None,
    }
}

#[cfg(test)]
mod tests {
    use super::admin_discovery_query_context;

    #[test]
    fn admin_discovery_query_context_has_no_filters() {
        let context = admin_discovery_query_context();

        assert!(context.is_admin);
        assert!(context.user_id.is_none());
        assert!(context.authorized_library_ids.is_none());
        assert!(context.restrictions.is_none());
    }
}

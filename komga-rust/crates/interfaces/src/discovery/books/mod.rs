mod duplicates;
mod feeds;
pub(in crate::discovery) mod list_query;
mod tags;

pub use duplicates::books_duplicates;
pub use feeds::{books_latest, books_ondeck};
pub use tags::book_tags;

use super::persisted::common_helpers::{internal_error_response, requested_query_values};
use super::persisted::library_mappings::remap_requested_library_ids_for_persisted;
use crate::discovery_auth::context::{DetailContentContext, DetailResourceContext};
use crate::helpers::{
    books_page_payload, detail_access_denial_response, mark_runtime_owned, to_domain_query_context,
};
use crate::identity_access::auth::Authenticated;
use crate::state::DiscoveryState;
use axum::Json;
use axum::body::Bytes;
use axum::extract::Path as AxumPath;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_domain::discovery::{DiscoveryError, PageEnvelope};
use serde_json::{Value, json};

fn empty_books_page_response(page: usize, size: usize, unpaged: bool, sorted: bool) -> Response {
    Json(books_page_payload(
        PageEnvelope {
            content: vec![],
            page,
            size,
            total_elements: 0,
            total_pages: 0,
        },
        false,
        !unpaged,
        sorted,
    ))
    .into_response()
}

pub async fn books_list(
    State(app): State<DiscoveryState>,
    _authenticated: Authenticated,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    let payload = if body.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    } else {
        match serde_json::from_slice::<Value>(&body) {
            Ok(Value::Object(object)) => Value::Object(object),
            Ok(_) | Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        }
    };

    let interfaces_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(&app.identity, &headers, None)
        .await
    {
        Some(ctx) => ctx,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let context = to_domain_query_context(interfaces_context);

    let resolved = match super::query::resolve_books_list_request(&uri, payload) {
        Ok(resolved) => resolved,
        Err(error) => return error.into_response(),
    };

    match app
        .discovery_browse
        .list_books(&context, resolved.request)
        .await
    {
        Ok(page) => {
            let mut response = Json(books_page_payload(
                page,
                context.is_admin,
                resolved.response.paged,
                resolved.response.sorted,
            ))
            .into_response();
            mark_runtime_owned(&mut response);
            response
        }
        Err(DiscoveryError::InvalidSemantics(e)) => {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response()
        }
        Err(e) => internal_error_response(format!("{e:?}")),
    }
}

pub(crate) async fn books_deprecated_get(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let app = &app;
    let query = uri.query().unwrap_or_default();
    let requested_library_ids = requested_query_values(query, "library_id");
    let library_ids = remap_requested_library_ids_for_persisted(
        app.discovery_search.as_ref(),
        requested_library_ids.as_ref(),
    )
    .await;
    let requested_non_empty_library_ids = requested_library_ids
        .as_ref()
        .is_some_and(|values| !values.is_empty());
    let empty_page_on_unmapped_library = requested_non_empty_library_ids && library_ids.is_none();
    let auth_library_ids = library_ids.clone();
    let resolved = match super::query::resolve_deprecated_books_request(
        &uri,
        library_ids,
        empty_page_on_unmapped_library,
    ) {
        Ok(resolved) => resolved,
        Err(error) => return error.into_response(),
    };
    if resolved.response.empty_page_on_unmapped_library {
        return empty_books_page_response(
            resolved.request.page.page,
            resolved.request.page.size,
            resolved.request.page.unpaged,
            resolved.response.sorted,
        );
    }

    let interfaces_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(
            &app.identity,
            &headers,
            auth_library_ids.as_deref(),
        )
        .await
    {
        Some(ctx) => ctx,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let context = to_domain_query_context(interfaces_context);

    match app
        .discovery_browse
        .list_books(&context, resolved.request)
        .await
    {
        Ok(page) => {
            let mut response = Json(books_page_payload(
                page,
                context.is_admin,
                resolved.response.paged,
                resolved.response.sorted,
            ))
            .into_response();
            mark_runtime_owned(&mut response);
            response
        }
        Err(DiscoveryError::InvalidSemantics(e)) => {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response()
        }
        Err(e) => internal_error_response(format!("{e:?}")),
    }
}

pub async fn series_books_deprecated(
    State(app): State<DiscoveryState>,
    _authenticated: Authenticated,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(series_id): AxumPath<String>,
) -> Response {
    let resolved_series_id = super::detail::resolve_series_id_for_persisted(&app, &series_id).await;
    let Some(resource) =
        (match super::detail::load_persisted_series_resource(&app, &resolved_series_id).await {
            Ok(resource) => resource,
            Err(error) => return internal_error_response(error),
        })
    else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating,
            sharing_labels: resource.sharing_labels,
        }),
    };

    if let Err(denial) = app
        .discovery_auth
        .resolve_detail_query_context_with_persistence(&app.identity, &headers, &detail_context)
        .await
    {
        return detail_access_denial_response(denial);
    }

    let resolved = match super::query::resolve_series_books_request(&resolved_series_id, &uri) {
        Ok(resolved) => resolved,
        Err(error) => return error.into_response(),
    };

    let interfaces_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(&app.identity, &headers, None)
        .await
    {
        Some(ctx) => ctx,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let context = to_domain_query_context(interfaces_context);

    match app
        .discovery_browse
        .list_books(&context, resolved.request)
        .await
    {
        Ok(page) => {
            let mut response = Json(books_page_payload(
                page,
                context.is_admin,
                resolved.response.paged,
                resolved.response.sorted,
            ))
            .into_response();
            mark_runtime_owned(&mut response);
            response
        }
        Err(DiscoveryError::InvalidSemantics(e)) => {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response()
        }
        Err(e) => internal_error_response(format!("{e:?}")),
    }
}

mod duplicates;
mod feeds;
mod list_query;
mod tags;

pub use duplicates::books_duplicates;
pub use feeds::{books_latest, books_ondeck};
pub use tags::book_tags;

use super::persisted::common_helpers::{
    decode_query_component, internal_error_response, requested_query_values,
};
use super::persisted::library_mappings::remap_requested_library_ids_for_persisted;
use super::*;
use crate::discovery_auth::context::{DetailContentContext, DetailResourceContext};
use crate::helpers::{detail_access_denial_response, to_domain_query_context};
use axum::extract::State;
use komga_application::discovery::BooksBrowseQuery;
use komga_domain::discovery::{DiscoveryError, PageEnvelope};
use list_query::{
    build_legacy_books_filter, legacy_series_books_book_filter,
    legacy_series_books_sort_from_query, normalize_release_date_date_time,
    parse_book_filter_from_json, parse_book_sorts_from_json, parse_book_sorts_from_json_values,
};
use std::sync::Arc;

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
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    if let Some(response) = require_request_auth(&*app.services.runtime_identity, &headers).await {
        return response;
    }

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
        .resolve_query_context_with_persistence(&*app.services.runtime_identity, &headers, None)
        .await
    {
        Some(ctx) => ctx,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let context = to_domain_query_context(interfaces_context);

    let filter = match parse_book_filter_from_json(payload.get("condition")) {
        Ok(f) => f,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("invalid book filter: {e:?}") })),
            )
                .into_response();
        }
    };

    let search = extract_full_text_search(&payload);
    let has_search = search
        .as_ref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let query = uri.query().unwrap_or_default();
    let query_sort_values = query_values(query, "sort")
        .into_iter()
        .map(decode_query_component)
        .collect::<Vec<_>>();
    let sort = if query_sort_values.is_empty() {
        parse_book_sorts_from_json(payload.get("sort"), has_search)
    } else {
        parse_book_sorts_from_json_values(&query_sort_values, has_search)
    };
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .or_else(|| {
            payload
                .get("page")
                .and_then(|value| value.as_u64())
                .map(|value| value as usize)
        })
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .or_else(|| {
            payload
                .get("size")
                .and_then(|value| value.as_u64())
                .map(|value| value as usize)
        })
        .unwrap_or(20)
        .max(1);
    let unpaged = query_value(query, "unpaged")
        .map(|_| query_bool(query, "unpaged"))
        .or_else(|| payload.get("unpaged").and_then(|value| value.as_bool()))
        .unwrap_or(false);

    let sorted = !sort.is_empty();

    match app
        .services
        .discovery_list
        .list_books(
            &context,
            BooksBrowseQuery {
                filter,
                sort,
                search,
                page,
                size,
                unpaged,
            },
        )
        .await
    {
        Ok(page) => {
            let mut response =
                Json(books_page_payload(page, context.is_admin, !unpaged, sorted)).into_response();
            mark_runtime_owned(&mut response);
            response
        }
        Err(DiscoveryError::InvalidSemantics(e)) => {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response()
        }
        Err(e) => internal_error_response(format!("{e:?}")),
    }
}

pub(super) async fn books_deprecated_get(
    headers: HeaderMap,
    uri: Uri,
    app: &HttpAppState,
) -> Response {
    if let Some(response) = require_request_auth(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    let query = uri.query().unwrap_or_default();
    let requested_library_ids = requested_query_values(query, "library_id");
    let library_ids = remap_requested_library_ids_for_persisted(
        app.services.discovery_library_mapping.as_ref(),
        requested_library_ids.as_ref(),
    )
    .await;
    let tags = requested_query_values(query, "tag");
    let read_statuses = requested_query_values(query, "read_status");
    let media_statuses = requested_query_values(query, "media_status").map(|values| {
        values
            .into_iter()
            .map(|value| value.to_ascii_lowercase())
            .collect()
    });
    let released_after = match query_value(query, "released_after") {
        Some(value) => {
            let decoded = decode_query_component(value);
            let Some(normalized) = normalize_release_date_date_time(&decoded) else {
                return StatusCode::BAD_REQUEST.into_response();
            };
            Some(normalized)
        }
        None => None,
    };

    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query, "unpaged");
    let search = requested_query_values(query, "search")
        .and_then(|values| values.into_iter().next())
        .filter(|value| !value.trim().is_empty());
    let sorted = !query_values(query, "sort").is_empty() || search.is_some();
    let requested_non_empty_library_ids = requested_library_ids
        .as_ref()
        .is_some_and(|values| !values.is_empty());
    if requested_non_empty_library_ids && library_ids.is_none() {
        return empty_books_page_response(page, size, unpaged, sorted);
    }

    let interfaces_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(
            &*app.services.runtime_identity,
            &headers,
            library_ids.as_deref(),
        )
        .await
    {
        Some(ctx) => ctx,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let context = to_domain_query_context(interfaces_context);

    let filter = build_legacy_books_filter(
        library_ids,
        tags,
        read_statuses,
        media_statuses,
        released_after,
    );

    match app
        .services
        .discovery_list
        .list_books(
            &context,
            BooksBrowseQuery {
                filter,
                sort: vec![],
                search,
                page,
                size,
                unpaged,
            },
        )
        .await
    {
        Ok(page) => {
            let mut response =
                Json(books_page_payload(page, context.is_admin, !unpaged, sorted)).into_response();
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
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(series_id): AxumPath<String>,
) -> Response {
    if let Some(response) = require_request_auth(&*app.services.runtime_identity, &headers).await {
        return response;
    }

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
        .resolve_detail_query_context_with_persistence(
            &*app.services.runtime_identity,
            &headers,
            &detail_context,
        )
        .await
    {
        return detail_access_denial_response(denial);
    }

    let filter = match legacy_series_books_book_filter(&resolved_series_id, &uri) {
        Ok(filter) => filter,
        Err(status) => return status.into_response(),
    };

    let query = uri.query().unwrap_or_default();
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query, "unpaged");
    let sort = legacy_series_books_sort_from_query(&uri);

    let interfaces_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(&*app.services.runtime_identity, &headers, None)
        .await
    {
        Some(ctx) => ctx,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let context = to_domain_query_context(interfaces_context);

    match app
        .services
        .discovery_list
        .list_books(
            &context,
            BooksBrowseQuery {
                filter,
                sort,
                search: None,
                page,
                size,
                unpaged,
            },
        )
        .await
    {
        Ok(page) => {
            let sorted = !query_values(query, "sort").is_empty();
            let mut response =
                Json(books_page_payload(page, context.is_admin, !unpaged, sorted)).into_response();
            mark_runtime_owned(&mut response);
            response
        }
        Err(DiscoveryError::InvalidSemantics(e)) => {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response()
        }
        Err(e) => internal_error_response(format!("{e:?}")),
    }
}

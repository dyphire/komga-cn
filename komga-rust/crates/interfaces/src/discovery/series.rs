mod payload;

pub(in crate::discovery) use payload::series_read_model_page_payload;

use super::persisted::common_helpers::{internal_error_response, requested_query_values};
use crate::helpers::to_domain_query_context;
use crate::identity_access::auth::Authenticated;
use crate::state::DiscoveryState;
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_application::discovery::{SeriesAlphabeticalGroup, SeriesAlphabeticalGroupsRequest};
use komga_domain::discovery::{DiscoveryError, PageEnvelope, SeriesSort};
use serde_json::{Value, json};

fn normalize_kotlin_unpaged_page_shape<T>(mut page: PageEnvelope<T>) -> PageEnvelope<T> {
    let normalized_size = page.total_elements.max(20);
    page.page = 0;
    page.size = normalized_size;
    page.total_pages = if page.total_elements == 0 {
        0
    } else {
        ((page.total_elements - 1) / normalized_size) + 1
    };
    page
}

async fn series_feed(
    app: &DiscoveryState,
    headers: HeaderMap,
    uri: Uri,
    sorts: Vec<SeriesSort>,
    exclude_newly_added: bool,
    kotlin_unpaged_page_shape: bool,
) -> Response {
    let query = uri.query().unwrap_or_default();
    let requested_library_ids = requested_query_values(query, "library_id");
    let resolved = match super::query::resolve_series_feed_request(
        &uri,
        sorts,
        exclude_newly_added,
        kotlin_unpaged_page_shape,
    ) {
        Ok(resolved) => resolved,
        Err(error) => return error.into_response(),
    };
    let interfaces_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(
            &app.identity,
            &headers,
            requested_library_ids.as_deref(),
        )
        .await
    {
        Ok(Some(context)) => context,
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let context = to_domain_query_context(interfaces_context);

    match app
        .discovery_browse
        .list_series(&context, resolved.request)
        .await
    {
        Ok(page) => {
            let page = if resolved.response.kotlin_unpaged_shape {
                normalize_kotlin_unpaged_page_shape(page)
            } else {
                page
            };
            Json(series_read_model_page_payload(
                page,
                resolved.response.paged,
                resolved.response.sorted,
            ))
            .into_response()
        }
        Err(e) => internal_error_response(format!("{e:?}")),
    }
}

pub(crate) async fn series_latest(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    series_feed(
        &app,
        headers,
        uri,
        vec![SeriesSort::LastModifiedDateDesc],
        false,
        false,
    )
    .await
}

pub(crate) async fn series_deprecated_get(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let app = &app;
    let query = uri.query().unwrap_or_default();
    let requested_library_ids = requested_query_values(query, "library_id");
    let resolved = match super::query::resolve_deprecated_series_request(&uri) {
        Ok(resolved) => resolved,
        Err(error) => return error.into_response(),
    };
    let interfaces_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(
            &app.identity,
            &headers,
            requested_library_ids.as_deref(),
        )
        .await
    {
        Ok(Some(context)) => context,
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let context = to_domain_query_context(interfaces_context);

    match app
        .discovery_browse
        .list_series(&context, resolved.request)
        .await
    {
        Ok(page) => Json(series_read_model_page_payload(
            page,
            resolved.response.paged,
            resolved.response.sorted,
        ))
        .into_response(),
        Err(e) => internal_error_response(format!("{e:?}")),
    }
}

pub(crate) async fn series_alphabetical_groups(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let app = &app;
    let resolved = match super::query::resolve_series_alphabetical_groups_request(body) {
        Ok(resolved) => resolved,
        Err(error) => return error.into_response(),
    };

    series_alphabetical_groups_response(app, headers, None, resolved.request).await
}

pub(crate) async fn series_alphabetical_groups_deprecated_get(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let query = uri.query().unwrap_or_default();
    let requested_library_ids = requested_query_values(query, "library_id");
    let resolved = match super::query::resolve_deprecated_series_request(&uri) {
        Ok(resolved) => resolved,
        Err(error) => return error.into_response(),
    };
    let request = SeriesAlphabeticalGroupsRequest {
        filter: resolved.request.filter,
        search: resolved.request.search,
    };

    series_alphabetical_groups_response(&app, headers, requested_library_ids, request).await
}

async fn series_alphabetical_groups_response(
    app: &DiscoveryState,
    headers: HeaderMap,
    requested_library_ids: Option<Vec<String>>,
    request: SeriesAlphabeticalGroupsRequest,
) -> Response {
    let interfaces_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(
            &app.identity,
            &headers,
            requested_library_ids.as_deref(),
        )
        .await
    {
        Ok(Some(context)) => context,
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let context = to_domain_query_context(interfaces_context);

    match app
        .discovery_browse
        .list_series_alphabetical_groups(&context, request)
        .await
    {
        Ok(groups) => Json(Value::Array(
            groups
                .into_iter()
                .map(series_alphabetical_group_payload)
                .collect(),
        ))
        .into_response(),
        Err(e) => internal_error_response(format!("{e:?}")),
    }
}

fn series_alphabetical_group_payload(group: SeriesAlphabeticalGroup) -> Value {
    json!({
        "group": group.group,
        "count": group.count,
    })
}

pub(crate) async fn series_list(
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
        Ok(Some(ctx)) => ctx,
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let context = to_domain_query_context(interfaces_context);

    let resolved = match super::query::resolve_series_list_request(&uri, payload) {
        Ok(resolved) => resolved,
        Err(error) => return error.into_response(),
    };

    match app
        .discovery_browse
        .list_series(&context, resolved.request)
        .await
    {
        Ok(page) => Json(series_read_model_page_payload(
            page,
            resolved.response.paged,
            resolved.response.sorted,
        ))
        .into_response(),
        Err(DiscoveryError::InvalidSemantics(e)) => {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response()
        }
        Err(e) => internal_error_response(format!("{e:?}")),
    }
}

pub(crate) async fn series_new(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    series_feed(
        &app,
        headers,
        uri,
        vec![SeriesSort::CreatedDateDesc],
        false,
        false,
    )
    .await
}

pub(crate) async fn series_updated(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    series_feed(
        &app,
        headers,
        uri,
        vec![SeriesSort::LastModifiedDateDesc],
        true,
        true,
    )
    .await
}

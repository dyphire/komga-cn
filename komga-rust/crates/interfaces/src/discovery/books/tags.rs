use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_application::discovery::BookTagScope;
use serde_json::json;

use crate::helpers::{query_value, query_values, to_domain_query_context};
use crate::identity_access::auth::Authenticated;
use crate::state::DiscoveryState;

use super::super::persisted::common_helpers::{decode_query_component, internal_error_response};

pub async fn book_tags(
    State(app): State<DiscoveryState>,
    _authenticated: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let query = uri.query().unwrap_or_default();
    let library_ids = query_values(query, "library_id")
        .into_iter()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(decode_query_component)
        .collect::<Vec<_>>();
    let series_scope = query_value(query, "series_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);
    let readlist_scope = query_value(query, "readlist_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);
    let scoped_by_resource = series_scope.is_some() || readlist_scope.is_some();
    let interfaces_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(
            &*app.identity.service,
            &headers,
            if scoped_by_resource || library_ids.is_empty() {
                None
            } else {
                Some(library_ids.as_slice())
            },
        )
        .await
    {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let context = to_domain_query_context(interfaces_context);

    let scope = series_scope
        .map(BookTagScope::Series)
        .or_else(|| readlist_scope.map(BookTagScope::ReadList))
        .or_else(|| {
            context
                .authorized_library_ids
                .clone()
                .filter(|ids| !ids.is_empty())
                .map(|ids| {
                    BookTagScope::Libraries(
                        ids.into_iter().map(|id| id.as_str().to_string()).collect(),
                    )
                })
        })
        .or(Some(BookTagScope::All));
    let service_library_ids = if scoped_by_resource {
        context.authorized_library_ids.as_ref().map(|ids| {
            ids.iter()
                .map(|id| id.as_str().to_string())
                .collect::<Vec<_>>()
        })
    } else if library_ids.is_empty() {
        None
    } else {
        Some(library_ids)
    };

    match app
        .discovery_list
        .list_book_tags(&context, scope, service_library_ids)
        .await
    {
        Ok(tags) => Json(json!(tags)).into_response(),
        Err(error) => internal_error_response(format!("{error:?}")),
    }
}

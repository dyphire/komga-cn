use super::persisted::authors_queries::authors_v2_page_payload;
use super::persisted::common_helpers::{
    decode_query_component, discovery_error_response, internal_error_response,
};
use crate::discovery_auth::context::DiscoveryQueryContext;
use crate::discovery_auth::state::DiscoveryAuthState;
use crate::helpers::{query_bool, query_value, query_values, to_domain_query_context};
use crate::identity_access::auth::Authenticated;
use crate::state::DiscoveryState;
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_application::discovery::{FacetKind, FacetScope, PersistedAuthorsScope};
use serde_json::json;

fn decoded_library_ids(query: &str) -> Vec<String> {
    query_values(query, "library_id")
        .into_iter()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(decode_query_component)
        .collect()
}

fn decoded_collection_id(query: &str) -> Option<String> {
    query_value(query, "collection_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component)
}

#[allow(clippy::result_large_err)]
async fn resolve_query_context_or_unauthorized(
    identity: &crate::state::IdentityState,
    auth_state: &DiscoveryAuthState,
    headers: &HeaderMap,
    requested_library_ids: Option<&[String]>,
) -> Result<DiscoveryQueryContext, Response> {
    match auth_state
        .resolve_query_context_with_persistence(identity, headers, requested_library_ids)
        .await
    {
        Ok(Some(context)) => Ok(context),
        Ok(None) => Err(StatusCode::UNAUTHORIZED.into_response()),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR.into_response()),
    }
}

struct CollectionFacetScope {
    context: DiscoveryQueryContext,
    collection_id: Option<String>,
}

#[allow(clippy::result_large_err)]
async fn resolve_collection_facet_scope(
    identity: &crate::state::IdentityState,
    auth_state: &DiscoveryAuthState,
    headers: &HeaderMap,
    query: &str,
) -> Result<CollectionFacetScope, Response> {
    let library_ids = decoded_library_ids(query);
    let requested_library_ids = (!library_ids.is_empty()).then_some(library_ids.as_slice());
    let context =
        resolve_query_context_or_unauthorized(identity, auth_state, headers, requested_library_ids)
            .await?;

    Ok(CollectionFacetScope {
        context,
        collection_id: if library_ids.is_empty() {
            decoded_collection_id(query)
        } else {
            None
        },
    })
}

pub(crate) async fn authors_names(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let app = &app;
    let search = query_value(uri.query().unwrap_or_default(), "search")
        .map(decode_query_component)
        .unwrap_or_default();
    let context = match resolve_query_context_or_unauthorized(
        &app.identity,
        &app.discovery_auth,
        &headers,
        None,
    )
    .await
    {
        Ok(context) => context,
        Err(response) => return response,
    };

    match app
        .author_facets
        .load_author_names(&search, context.authorized_library_ids.as_deref())
        .await
    {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(crate) async fn authors_roles(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
) -> Response {
    let app = &app;
    let context = match resolve_query_context_or_unauthorized(
        &app.identity,
        &app.discovery_auth,
        &headers,
        None,
    )
    .await
    {
        Ok(context) => context,
        Err(response) => return response,
    };

    match app
        .author_facets
        .load_author_roles(context.authorized_library_ids.as_deref())
        .await
    {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(crate) async fn authors_deprecated_get(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let app = &app;
    let query = uri.query().unwrap_or_default();
    let search = query_value(query, "search")
        .map(decode_query_component)
        .unwrap_or_default();
    let library_id = query_value(query, "library_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);
    let collection_id = decoded_collection_id(query);
    let series_id = query_value(query, "series_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);
    let context = match resolve_query_context_or_unauthorized(
        &app.identity,
        &app.discovery_auth,
        &headers,
        library_id.as_ref().map(std::slice::from_ref),
    )
    .await
    {
        Ok(context) => context,
        Err(response) => return response,
    };

    let scope = if let Some(library_id) = library_id {
        PersistedAuthorsScope::Libraries(vec![library_id])
    } else if let Some(collection_id) = collection_id {
        PersistedAuthorsScope::Collection(collection_id)
    } else if let Some(series_id) = series_id {
        PersistedAuthorsScope::Series(series_id)
    } else {
        PersistedAuthorsScope::All
    };

    let mut authors = match app
        .author_facets
        .load_authors_by_scope(scope, context.authorized_library_ids.as_deref())
        .await
    {
        Ok(values) => values,
        Err(error) => return internal_error_response(error),
    };

    if !search.is_empty() {
        let search = search.to_ascii_lowercase();
        authors.retain(|author| author.name.to_ascii_lowercase().contains(&search));
    }

    Json(json!(authors)).into_response()
}

pub(crate) async fn authors_v2(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let app = &app;
    let query = uri.query().unwrap_or_default();
    let search = query_value(query, "search")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);
    let role = query_value(query, "role")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);
    let library_ids = query_values(query, "library_id")
        .into_iter()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(decode_query_component)
        .collect::<Vec<_>>();
    let collection_id = query_value(query, "collection_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);
    let series_id = query_value(query, "series_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);
    let readlist_id = query_value(query, "readlist_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query, "unpaged");
    let context = match resolve_query_context_or_unauthorized(
        &app.identity,
        &app.discovery_auth,
        &headers,
        (!library_ids.is_empty()).then_some(library_ids.as_slice()),
    )
    .await
    {
        Ok(context) => context,
        Err(response) => return response,
    };

    let scope = if !library_ids.is_empty() {
        PersistedAuthorsScope::Libraries(library_ids)
    } else if let Some(collection_id) = collection_id {
        PersistedAuthorsScope::Collection(collection_id)
    } else if let Some(series_id) = series_id {
        PersistedAuthorsScope::Series(series_id)
    } else if let Some(readlist_id) = readlist_id {
        PersistedAuthorsScope::ReadList(readlist_id)
    } else {
        PersistedAuthorsScope::All
    };

    let mut authors = match app
        .author_facets
        .load_authors_by_scope(scope, context.authorized_library_ids.as_deref())
        .await
    {
        Ok(values) => values,
        Err(error) => return internal_error_response(error),
    };

    if let Some(role) = role {
        let role = role.to_ascii_lowercase();
        authors.retain(|author| author.role.to_ascii_lowercase() == role);
    }

    if let Some(search) = search {
        let search = search.to_ascii_lowercase();
        authors.retain(|author| author.name.to_ascii_lowercase().contains(&search));
    }

    Json(authors_v2_page_payload(authors, page, size, unpaged)).into_response()
}

async fn collection_facet_handler(
    app: &DiscoveryState,
    headers: &HeaderMap,
    uri: &Uri,
    kind: FacetKind,
) -> Response {
    let query = uri.query().unwrap_or_default();
    let scope =
        match resolve_collection_facet_scope(&app.identity, &app.discovery_auth, headers, query)
            .await
        {
            Ok(scope) => scope,
            Err(response) => return response,
        };

    let domain_context = to_domain_query_context(scope.context.clone());
    let facet_scope = FacetScope {
        library_ids: scope.context.authorized_library_ids,
        collection_id: scope.collection_id,
    };
    match app
        .discovery_facets
        .list_facet_values(&domain_context, kind, facet_scope)
        .await
    {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => discovery_error_response(error),
    }
}

pub(crate) async fn genres(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    collection_facet_handler(&app, &headers, &uri, FacetKind::Genres).await
}

pub(crate) async fn tags(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    collection_facet_handler(&app, &headers, &uri, FacetKind::Tags).await
}

pub(crate) async fn series_tags(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    collection_facet_handler(&app, &headers, &uri, FacetKind::SeriesTags).await
}

pub(crate) async fn languages(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    collection_facet_handler(&app, &headers, &uri, FacetKind::Languages).await
}

pub(crate) async fn publishers(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    collection_facet_handler(&app, &headers, &uri, FacetKind::Publishers).await
}

pub(crate) async fn age_ratings(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    collection_facet_handler(&app, &headers, &uri, FacetKind::AgeRatings).await
}

pub(crate) async fn sharing_labels(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    collection_facet_handler(&app, &headers, &uri, FacetKind::SharingLabels).await
}

pub(crate) async fn series_release_dates(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    collection_facet_handler(&app, &headers, &uri, FacetKind::SeriesReleaseDates).await
}

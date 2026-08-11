use super::persisted::authors_queries::{authors_v2_page_payload, paged_values_payload};
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
use komga_application::discovery::{
    FacetKind, FacetScope, PersistedAuthorEntry, PersistedAuthorsScope, ReferentialTagsInclude,
    ReferentialTagsScope,
};
use serde_json::json;

fn decoded_library_ids(query: &str) -> Vec<String> {
    query_values(query, "library_id")
        .into_iter()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(decode_query_component)
        .collect()
}

fn decoded_ids(query: &str, name: &str) -> Vec<String> {
    query_values(query, name)
        .into_iter()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(decode_query_component)
        .collect()
}

fn decoded_collection_id(query: &str) -> Option<String> {
    decoded_ids(query, "collection_id").into_iter().next()
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
    collection_ids: Option<Vec<String>>,
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
        collection_ids: if library_ids.is_empty() {
            let collection_ids = decoded_ids(query, "collection_id");
            (!collection_ids.is_empty()).then_some(collection_ids)
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
        PersistedAuthorsScope::Collections(vec![collection_id])
    } else if let Some(series_id) = series_id {
        PersistedAuthorsScope::Series(vec![series_id])
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
    let query = uri.query().unwrap_or_default();
    let (authors, page, size, unpaged) = match scoped_authors_v2(&app, &headers, query).await {
        Ok(result) => result,
        Err(response) => return response,
    };

    Json(authors_v2_page_payload(authors, page, size, unpaged)).into_response()
}

async fn scoped_authors_v2(
    app: &DiscoveryState,
    headers: &HeaderMap,
    query: &str,
) -> Result<(Vec<PersistedAuthorEntry>, usize, usize, bool), Response> {
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
    let collection_ids = decoded_ids(query, "collection_id");
    let series_ids = decoded_ids(query, "series_id");
    let readlist_ids = decoded_ids(query, "readlist_id");
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
        headers,
        (!library_ids.is_empty()).then_some(library_ids.as_slice()),
    )
    .await
    {
        Ok(context) => context,
        Err(response) => return Err(response),
    };

    let scope = if !library_ids.is_empty() {
        PersistedAuthorsScope::Libraries(library_ids)
    } else if !collection_ids.is_empty() {
        PersistedAuthorsScope::Collections(collection_ids)
    } else if !series_ids.is_empty() {
        PersistedAuthorsScope::Series(series_ids)
    } else if !readlist_ids.is_empty() {
        PersistedAuthorsScope::ReadLists(readlist_ids)
    } else {
        PersistedAuthorsScope::All
    };

    let mut authors = match app
        .author_facets
        .load_authors_by_scope(scope, context.authorized_library_ids.as_deref())
        .await
    {
        Ok(values) => values,
        Err(error) => return Err(internal_error_response(error)),
    };

    if let Some(role) = role {
        let role = role.to_ascii_lowercase();
        authors.retain(|author| author.role.to_ascii_lowercase() == role);
    }

    if let Some(search) = search {
        let search = search.to_ascii_lowercase();
        authors.retain(|author| author.name.to_ascii_lowercase().contains(&search));
    }

    Ok((authors, page, size, unpaged))
}

async fn author_values_v2_handler(
    app: &DiscoveryState,
    headers: &HeaderMap,
    uri: &Uri,
    names: bool,
) -> Response {
    let query = uri.query().unwrap_or_default();
    let (authors, page, size, unpaged) = match scoped_authors_v2(app, headers, query).await {
        Ok(result) => result,
        Err(response) => return response,
    };
    let mut values = authors
        .into_iter()
        .map(|author| if names { author.name } else { author.role })
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();

    Json(paged_values_payload(
        values.into_iter().map(|value| json!(value)).collect(),
        page,
        size,
        unpaged,
    ))
    .into_response()
}

pub(crate) async fn authors_names_v2(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    author_values_v2_handler(&app, &headers, &uri, true).await
}

pub(crate) async fn authors_roles_v2(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    author_values_v2_handler(&app, &headers, &uri, false).await
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
        collection_ids: scope.collection_ids,
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

async fn scalar_facet_v2_handler(
    app: &DiscoveryState,
    headers: &HeaderMap,
    uri: &Uri,
    kind: FacetKind,
    numeric: bool,
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
        collection_ids: scope.collection_ids,
    };
    let mut values = match app
        .discovery_facets
        .list_facet_values(&domain_context, kind, facet_scope)
        .await
    {
        Ok(values) => values,
        Err(error) => return discovery_error_response(error),
    };
    if let Some(search) = query_value(query, "search")
        .filter(|value| !value.is_empty())
        .map(decode_query_component)
    {
        let search = search.to_ascii_lowercase();
        values.retain(|value| value.to_ascii_lowercase().contains(&search));
    }

    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let content = if numeric {
        values
            .into_iter()
            .filter_map(|value| value.parse::<i64>().ok())
            .map(|value| json!(value))
            .collect()
    } else {
        values.into_iter().map(|value| json!(value)).collect()
    };

    Json(paged_values_payload(
        content,
        page,
        size,
        query_bool(query, "unpaged"),
    ))
    .into_response()
}

async fn tags_v2_handler(app: &DiscoveryState, headers: &HeaderMap, uri: &Uri) -> Response {
    let query = uri.query().unwrap_or_default();
    let library_ids = decoded_library_ids(query);
    let collection_ids = decoded_ids(query, "collection_id");
    let series_ids = decoded_ids(query, "series_id");
    let readlist_ids = decoded_ids(query, "readlist_id");
    let requested_library_ids = (!library_ids.is_empty()).then_some(library_ids.as_slice());
    let context = match resolve_query_context_or_unauthorized(
        &app.identity,
        &app.discovery_auth,
        headers,
        requested_library_ids,
    )
    .await
    {
        Ok(context) => context,
        Err(response) => return response,
    };
    let scope = if !library_ids.is_empty() {
        ReferentialTagsScope::Libraries(library_ids)
    } else if !collection_ids.is_empty() {
        ReferentialTagsScope::Collections(collection_ids)
    } else if !series_ids.is_empty() {
        ReferentialTagsScope::Series(series_ids)
    } else if !readlist_ids.is_empty() {
        ReferentialTagsScope::ReadLists(readlist_ids)
    } else {
        ReferentialTagsScope::All
    };
    let include = match query_value(query, "include")
        .unwrap_or("BOTH")
        .to_ascii_uppercase()
        .as_str()
    {
        "SERIES" => ReferentialTagsInclude::Series,
        "BOOK" => ReferentialTagsInclude::Book,
        "BOTH" => ReferentialTagsInclude::Both,
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
    let domain_context = to_domain_query_context(context.clone());
    let mut values = match app
        .discovery_facets
        .list_referential_tags(
            &domain_context,
            scope,
            include,
            context.authorized_library_ids,
        )
        .await
    {
        Ok(values) => values,
        Err(error) => return discovery_error_response(error),
    };
    if let Some(search) = query_value(query, "search")
        .filter(|value| !value.is_empty())
        .map(decode_query_component)
    {
        let search = search.to_ascii_lowercase();
        values.retain(|value| value.to_ascii_lowercase().contains(&search));
    }

    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    Json(paged_values_payload(
        values.into_iter().map(|value| json!(value)).collect(),
        page,
        size,
        query_bool(query, "unpaged"),
    ))
    .into_response()
}

macro_rules! scalar_facet_v2 {
    ($name:ident, $kind:expr, $numeric:expr) => {
        pub(crate) async fn $name(
            State(app): State<DiscoveryState>,
            _: Authenticated,
            headers: HeaderMap,
            uri: Uri,
        ) -> Response {
            scalar_facet_v2_handler(&app, &headers, &uri, $kind, $numeric).await
        }
    };
}

scalar_facet_v2!(genres_v2, FacetKind::Genres, false);
scalar_facet_v2!(sharing_labels_v2, FacetKind::SharingLabels, false);
scalar_facet_v2!(languages_v2, FacetKind::Languages, false);
scalar_facet_v2!(publishers_v2, FacetKind::Publishers, false);
pub(crate) async fn tags_v2(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    tags_v2_handler(&app, &headers, &uri).await
}
scalar_facet_v2!(
    series_release_years_v2,
    FacetKind::SeriesReleaseDates,
    false
);
scalar_facet_v2!(age_ratings_v2, FacetKind::AgeRatings, true);

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

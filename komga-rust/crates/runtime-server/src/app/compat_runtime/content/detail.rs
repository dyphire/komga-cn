use axum::Json;
use axum::extract::{Extension, Path};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_application::discovery::{
    BookDetailQuery, BookReadlistsQuery, BookSiblingQuery, DiscoveryQueries,
    DiscoveryQueryRepository, ReadListBooksQuery, ReadListDetailQuery, SeriesCollectionsQuery,
    SeriesDetailQuery,
};
use komga_domain::discovery::{
    BookDetailReadModel, CollectionReadModel, DiscoveryError, PageEnvelope, ReadListReadModel,
    SeriesDetailReadModel,
};
use komga_persistence::discovery::{
    BookRow, CollectionRow, ReadListRow, ReadProgressRow, SeriesRow, SqliteDiscoveryAdapter,
};
use serde_json::{Map, Value, json};

use crate::app::CompatProfile;
use crate::app::discovery_auth::{DetailContentContext, DetailResourceContext, DiscoveryAuthState};
use crate::app::placeholder_auth::{require_auth, resolved_auth_user};

use super::content_java_live;
use super::helpers::{
    apply_non_native_diagnostics, books_page_payload, detail_access_denial_response, mark_native,
    mark_non_native, query_bool, query_value, query_values, restricted_book_url,
    to_domain_query_context,
};

pub(in crate::app::compat_runtime) async fn series_detail(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        let user = resolved_auth_user(&headers)
            .expect("authorized series detail request should resolve user");
        let path = format!("/api/v1/series/{series_id}");
        return match content_java_live::fetch_json(user, &path, "series detail").await {
            Ok(series) => Json(series).into_response(),
            Err(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": message })),
            )
                .into_response(),
        };
    }

    let mut adapter = SqliteDiscoveryAdapter::default();
    seed_series_detail_data(&mut adapter);
    let queries = DiscoveryQueries::new(adapter);

    let Some(resource) = (match queries.resolve_series_resource(&series_id).await {
        Ok(resource) => resource,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("series resource lookup failed: {error:?}") })),
            )
                .into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id.clone()),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating,
            sharing_labels: resource.labels,
        }),
    };

    let detail_query_context =
        match auth_state.resolve_detail_query_context(&headers, &detail_context) {
            Ok(context) => context,
            Err(denial) => return detail_access_denial_response(denial),
        };
    let is_admin = detail_query_context.is_admin;
    let oneshot_query_excluded = query_bool(uri.query().unwrap_or_default(), "oneshot");

    let domain_context = to_domain_query_context(detail_query_context);
    let query = SeriesDetailQuery { series_id };

    match queries.get_series_detail(&domain_context, query).await {
        Ok(Some(series)) => {
            let mut payload = series_detail_payload(&series, is_admin);

            if oneshot_query_excluded {
                apply_non_native_diagnostics(
                    &mut payload,
                    &DiscoveryError::NonNativeRequestShape(
                        komga_domain::discovery::NonNativeRequestShape::UnsupportedSeriesFilter(
                            "oneshot-query-parameter".to_string(),
                        ),
                    ),
                );

                let mut response = Json(payload).into_response();
                mark_non_native(&mut response);
                response
            } else {
                Json(payload).into_response()
            }
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("series detail query failed: {error:?}") })),
        )
            .into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn series_collections(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        let user = resolved_auth_user(&headers)
            .expect("authorized series collections request should resolve user");
        let path = format!("/api/v1/series/{series_id}/collections");
        return match content_java_live::fetch_json(user, &path, "series collections").await {
            Ok(collections) => Json(collections).into_response(),
            Err(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": message })),
            )
                .into_response(),
        };
    }

    let mut adapter = SqliteDiscoveryAdapter::default();
    seed_series_detail_data(&mut adapter);
    let queries = DiscoveryQueries::new(adapter);

    let Some(resource) = (match queries.resolve_series_resource(&series_id).await {
        Ok(resource) => resource,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("series resource lookup failed: {error:?}") })),
            )
                .into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id.clone()),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating,
            sharing_labels: resource.labels,
        }),
    };

    let detail_query_context =
        match auth_state.resolve_detail_query_context(&headers, &detail_context) {
            Ok(context) => context,
            Err(denial) => return detail_access_denial_response(denial),
        };

    let domain_context = to_domain_query_context(detail_query_context);
    let query = SeriesCollectionsQuery { series_id };

    match queries.list_series_collections(&domain_context, query).await {
        Ok(collections) => Json(series_collections_payload(&collections)).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": format!("series collections query failed: {error:?}"),
            })),
        )
            .into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn book_detail(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        let user = resolved_auth_user(&headers)
            .expect("authorized book detail request should resolve user");
        let path = format!("/api/v1/books/{book_id}");
        return match content_java_live::fetch_json(user, &path, "book detail").await {
            Ok(book) => Json(book).into_response(),
            Err(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": message })),
            )
                .into_response(),
        };
    }

    let mut adapter = SqliteDiscoveryAdapter::default();
    seed_series_detail_data(&mut adapter);
    let queries = DiscoveryQueries::new(adapter);

    let Some(resource) = (match queries.resolve_book_resource(&book_id).await {
        Ok(resource) => resource,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("book resource lookup failed: {error:?}") })),
            )
                .into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id.clone()),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating,
            sharing_labels: resource.labels,
        }),
    };

    let detail_query_context =
        match auth_state.resolve_detail_query_context(&headers, &detail_context) {
            Ok(context) => context,
            Err(denial) => return detail_access_denial_response(denial),
        };
    let is_admin = detail_query_context.is_admin;

    let domain_context = to_domain_query_context(detail_query_context);
    let query = BookDetailQuery { book_id };

    match queries.get_book_detail(&domain_context, query).await {
        Ok(Some(book)) => Json(book_detail_payload(&book, is_admin)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("book detail query failed: {error:?}") })),
        )
            .into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn book_sibling_previous(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        let user = resolved_auth_user(&headers)
            .expect("authorized sibling previous request should resolve user");
        let path = format!("/api/v1/books/{book_id}/previous");
        return match content_java_live::fetch_json(user, &path, "book previous").await {
            Ok(book) => Json(book).into_response(),
            Err(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": message })),
            )
                .into_response(),
        };
    }

    let mut adapter = SqliteDiscoveryAdapter::default();
    seed_series_detail_data(&mut adapter);
    let queries = DiscoveryQueries::new(adapter);

    let Some(resource) = (match queries.resolve_book_resource(&book_id).await {
        Ok(resource) => resource,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("book resource lookup failed: {error:?}") })),
            )
                .into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id.clone()),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating,
            sharing_labels: resource.labels,
        }),
    };

    let detail_query_context =
        match auth_state.resolve_detail_query_context(&headers, &detail_context) {
            Ok(context) => context,
            Err(denial) => return detail_access_denial_response(denial),
        };
    let is_admin = detail_query_context.is_admin;

    let domain_context = to_domain_query_context(detail_query_context);

    match queries
        .get_book_sibling_previous(&domain_context, BookSiblingQuery { book_id })
        .await
    {
        Ok(Some(book)) => Json(book_detail_payload(&book, is_admin)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("book previous query failed: {error:?}") })),
        )
            .into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn book_sibling_next(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        let user = resolved_auth_user(&headers)
            .expect("authorized sibling next request should resolve user");
        let path = format!("/api/v1/books/{book_id}/next");
        return match content_java_live::fetch_json(user, &path, "book next").await {
            Ok(book) => Json(book).into_response(),
            Err(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": message })),
            )
                .into_response(),
        };
    }

    let mut adapter = SqliteDiscoveryAdapter::default();
    seed_series_detail_data(&mut adapter);
    let queries = DiscoveryQueries::new(adapter);

    let Some(resource) = (match queries.resolve_book_resource(&book_id).await {
        Ok(resource) => resource,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("book resource lookup failed: {error:?}") })),
            )
                .into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id.clone()),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating,
            sharing_labels: resource.labels,
        }),
    };

    let detail_query_context =
        match auth_state.resolve_detail_query_context(&headers, &detail_context) {
            Ok(context) => context,
            Err(denial) => return detail_access_denial_response(denial),
        };
    let is_admin = detail_query_context.is_admin;

    let domain_context = to_domain_query_context(detail_query_context);

    match queries
        .get_book_sibling_next(&domain_context, BookSiblingQuery { book_id })
        .await
    {
        Ok(Some(book)) => Json(book_detail_payload(&book, is_admin)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("book next query failed: {error:?}") })),
        )
            .into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn book_readlists(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        let user = resolved_auth_user(&headers)
            .expect("authorized book readlists request should resolve user");
        let path = format!("/api/v1/books/{book_id}/readlists");
        return match content_java_live::fetch_json(user, &path, "book readlists").await {
            Ok(readlists) => Json(readlists).into_response(),
            Err(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": message })),
            )
                .into_response(),
        };
    }

    let mut adapter = SqliteDiscoveryAdapter::default();
    seed_series_detail_data(&mut adapter);
    let queries = DiscoveryQueries::new(adapter);

    let Some(resource) = (match queries.resolve_book_resource(&book_id).await {
        Ok(resource) => resource,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("book resource lookup failed: {error:?}") })),
            )
                .into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id.clone()),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating,
            sharing_labels: resource.labels,
        }),
    };

    let detail_query_context =
        match auth_state.resolve_detail_query_context(&headers, &detail_context) {
            Ok(context) => context,
            Err(denial) => return detail_access_denial_response(denial),
        };

    let domain_context = to_domain_query_context(detail_query_context);

    match queries
        .list_book_readlists(&domain_context, BookReadlistsQuery { book_id })
        .await
    {
        Ok(readlists) => Json(readlists_payload(&readlists)).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("book readlists query failed: {error:?}") })),
        )
            .into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn readlist_books(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        let user = resolved_auth_user(&headers)
            .expect("authorized readlist books request should resolve user");
        let path = uri
            .path_and_query()
            .map_or(uri.path(), |value| value.as_str());
        return match content_java_live::fetch_json(user, path, "readlist books").await {
            Ok(books) => Json(books).into_response(),
            Err(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": message })),
            )
                .into_response(),
        };
    }

    let query_string = uri.query().unwrap_or_default();
    let page = query_value(query_string, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query_string, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query_string, "unpaged");
    let library_ids = {
        let values = query_values(query_string, "library_id")
            .into_iter()
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        (!values.is_empty()).then_some(values)
    };

    let context = match auth_state.resolve_query_context(&headers, library_ids.as_deref()) {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    let mut adapter = SqliteDiscoveryAdapter::default();
    seed_series_detail_data(&mut adapter);

    let is_admin = context.is_admin;
    let queries = DiscoveryQueries::new(adapter);
    let domain_context = to_domain_query_context(context);
    let query = ReadListBooksQuery {
        readlist_id: readlist_id.clone(),
        page,
        size,
        unpaged,
        library_ids,
    };

    match queries.list_readlist_books(&domain_context, query).await {
        Ok(page) => {
            let mut response = Json(books_page_payload(page, is_admin, !unpaged)).into_response();
            mark_native(&mut response);
            response
        }
        Err(DiscoveryError::NonNativeRequestShape(details)) => {
            non_native_readlist_books_response(
                DiscoveryError::NonNativeRequestShape(details),
                &queries,
                &domain_context,
                &readlist_id,
                is_admin,
            )
            .await
        }
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("readlist books query failed: {error:?}") })),
        )
            .into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn readlist_detail(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        let user = resolved_auth_user(&headers)
            .expect("authorized readlist detail request should resolve user");
        let path = format!("/api/v1/readlists/{readlist_id}");
        return match content_java_live::fetch_json(user, &path, "readlist detail").await {
            Ok(readlist) => Json(readlist).into_response(),
            Err(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": message })),
            )
                .into_response(),
        };
    }

    let context = match auth_state.resolve_query_context(&headers, None) {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    let mut adapter = SqliteDiscoveryAdapter::default();
    seed_series_detail_data(&mut adapter);
    let queries = DiscoveryQueries::new(adapter);
    let domain_context = to_domain_query_context(context);

    match queries
        .get_readlist_detail(&domain_context, ReadListDetailQuery { readlist_id })
        .await
    {
        Ok(Some(readlist)) => Json(readlist_payload(&readlist)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": format!("readlist detail query failed: {error:?}"),
            })),
        )
            .into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn readlist_book_sibling_previous(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path((readlist_id, book_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        let user = resolved_auth_user(&headers)
            .expect("authorized readlist sibling previous request should resolve user");
        let path = format!("/api/v1/readlists/{readlist_id}/books/{book_id}/previous");
        return match content_java_live::fetch_json(user, &path, "readlist book previous").await {
            Ok(book) => Json(book).into_response(),
            Err(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": message })),
            )
                .into_response(),
        };
    }

    let mut adapter = SqliteDiscoveryAdapter::default();
    seed_series_detail_data(&mut adapter);

    let Some(resource) = (match adapter.resolve_book_resource(&book_id).await {
        Ok(resource) => resource,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("book resource lookup failed: {error:?}") })),
            )
                .into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id.clone()),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating,
            sharing_labels: resource.labels,
        }),
    };

    let detail_query_context =
        match auth_state.resolve_detail_query_context(&headers, &detail_context) {
            Ok(context) => context,
            Err(denial) => return detail_access_denial_response(denial),
        };
    let is_admin = detail_query_context.is_admin;

    let domain_context = to_domain_query_context(detail_query_context);

    match adapter
        .get_readlist_book_sibling_previous(&domain_context, &readlist_id, &book_id)
        .await
    {
        Ok(Some(book)) => Json(book_detail_payload(&book, is_admin)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("readlist book previous query failed: {error:?}") })),
        )
            .into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn readlist_book_sibling_next(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path((readlist_id, book_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        let user = resolved_auth_user(&headers)
            .expect("authorized readlist sibling next request should resolve user");
        let path = format!("/api/v1/readlists/{readlist_id}/books/{book_id}/next");
        return match content_java_live::fetch_json(user, &path, "readlist book next").await {
            Ok(book) => Json(book).into_response(),
            Err(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": message })),
            )
                .into_response(),
        };
    }

    let mut adapter = SqliteDiscoveryAdapter::default();
    seed_series_detail_data(&mut adapter);

    let Some(resource) = (match adapter.resolve_book_resource(&book_id).await {
        Ok(resource) => resource,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("book resource lookup failed: {error:?}") })),
            )
                .into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id.clone()),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating,
            sharing_labels: resource.labels,
        }),
    };

    let detail_query_context =
        match auth_state.resolve_detail_query_context(&headers, &detail_context) {
            Ok(context) => context,
            Err(denial) => return detail_access_denial_response(denial),
        };
    let is_admin = detail_query_context.is_admin;

    let domain_context = to_domain_query_context(detail_query_context);

    match adapter
        .get_readlist_book_sibling_next(&domain_context, &readlist_id, &book_id)
        .await
    {
        Ok(Some(book)) => Json(book_detail_payload(&book, is_admin)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("readlist book next query failed: {error:?}") })),
        )
            .into_response(),
    }
}

async fn non_native_readlist_books_response(
    error: DiscoveryError,
    queries: &DiscoveryQueries<SqliteDiscoveryAdapter>,
    domain_context: &komga_domain::discovery::DiscoveryQueryContext,
    readlist_id: &str,
    is_admin: bool,
) -> Response {
    let mut payload =
        compat_readlist_books_payload(queries, domain_context, readlist_id, is_admin).await;
    apply_non_native_diagnostics(&mut payload, &error);

    let mut response = Json(payload).into_response();
    mark_non_native(&mut response);
    response
}

async fn compat_readlist_books_payload(
    queries: &DiscoveryQueries<SqliteDiscoveryAdapter>,
    domain_context: &komga_domain::discovery::DiscoveryQueryContext,
    readlist_id: &str,
    is_admin: bool,
) -> Value {
    match queries
        .list_readlist_books(
            domain_context,
            ReadListBooksQuery {
                readlist_id: readlist_id.to_string(),
                page: 0,
                size: 20,
                unpaged: true,
                library_ids: None,
            },
        )
        .await
    {
        Ok(page) => books_page_payload(page, is_admin, false),
        Err(_) => books_page_payload(
            PageEnvelope::from_slice(Vec::new(), 0, 1, 0),
            is_admin,
            false,
        ),
    }
}

fn seed_series_detail_data(adapter: &mut SqliteDiscoveryAdapter) {
    adapter.insert_series(
        SeriesRow::new("series-1", "1", "series")
            .with_url("/library/1/series")
            .with_labels(["safe"])
            .with_genres(["fantasy"])
            .with_tags(["featured"])
            .with_language("en")
            .with_publisher("komga")
            .with_age_rating(16)
            .with_release_date("2024-01-01")
            .with_status("ONGOING")
            .with_complete(true)
            .with_read_status("READ")
            .with_authors(["alice"]),
    );
    adapter.insert_series(
        SeriesRow::new("series-2", "1", "restricted")
            .with_url("/library/1/restricted")
            .with_labels(["adult"])
            .with_age_rating(18)
            .with_status("ONGOING")
            .with_read_status("UNREAD"),
    );
    adapter.insert_series(
        SeriesRow::new("series-oneshot", "1", "oneshot")
            .with_url("/library/1/oneshot")
            .with_labels(["safe"])
            .with_oneshot(true)
            .with_status("ENDED")
            .with_read_status("UNREAD"),
    );

    adapter.insert_book(
        BookRow::new("book-0", "series-1", "1", "book-0.cbr")
            .with_url("/library1/book-0.cbr")
            .with_last_modified("2023-12-01T03:04:05Z")
            .with_size_bytes(111)
            .with_media("READY", "application/zip", 1)
            .with_media_profile("PROFILE-1")
            .with_number_sort(1)
            .with_read_status("UNREAD")
            .with_release_date("2023-12-01")
            .with_tags(["safe"])
            .with_authors(["alice"]),
    );
    adapter.insert_book(
        BookRow::new("book-1", "series-1", "1", "book.cbr")
            .with_url("/library1/book.cbr")
            .with_last_modified("2024-01-01T03:04:05Z")
            .with_size_bytes(222)
            .with_media("READY", "application/zip", 1)
            .with_media_profile("PROFILE-1")
            .with_number_sort(2)
            .with_read_status("READ")
            .with_release_date("2024-01-01")
            .with_tags(["safe"])
            .with_authors(["alice"]),
    );
    adapter.insert_book(
        BookRow::new("book-3", "series-1", "1", "book-3.cbr")
            .with_url("/library1/book-3.cbr")
            .with_last_modified("2024-02-01T03:04:05Z")
            .with_size_bytes(333)
            .with_media("READY", "application/zip", 1)
            .with_media_profile("PROFILE-1")
            .with_number_sort(10)
            .with_read_status("UNREAD")
            .with_release_date("2024-02-01")
            .with_tags(["safe"])
            .with_authors(["alice"]),
    );
    adapter.insert_book(
        BookRow::new("book-2", "series-2", "1", "restricted-book.cbz")
            .with_url("/library1/restricted-book.cbz")
            .with_last_modified("2024-01-03T03:04:05Z")
            .with_media("READY", "application/vnd.comicbook+zip", 1)
            .with_media_profile("PROFILE-2")
            .with_read_status("UNREAD")
            .with_release_date("2023-01-01")
            .with_tags(["adult"])
            .with_authors(["bob"]),
    );
    adapter.insert_book(
        BookRow::new("book-oneshot", "series-oneshot", "1", "oneshot-book.cbz")
            .with_url("/library1/oneshot-book.cbz")
            .with_last_modified("2024-02-01T03:04:05Z")
            .with_size_bytes(150)
            .with_media("READY", "application/vnd.comicbook+zip", 1)
            .with_media_profile("PROFILE-ONESHOT")
            .with_number_sort(1)
            .with_read_status("UNREAD")
            .with_release_date("2024-02-01")
            .with_tags(["safe"])
            .with_authors(["alice"]),
    );

    adapter.insert_collection(
        CollectionRow::new("collection-1", "Collection 1")
            .with_ordered(true)
            .with_series_ids(["series-1", "series-2"]),
    );
    adapter.insert_read_list(
        ReadListRow::new("readlist-1", "ReadList 1")
            .with_summary("Visible readlist")
            .with_book_ids(["book-1"]),
    );
    adapter.insert_read_list(
        ReadListRow::new("readlist-2", "ReadList 2")
            .with_summary("Mixed visibility readlist")
            .with_book_ids(["book-1", "book-2"]),
    );
    adapter.insert_read_list(
        ReadListRow::new("readlist-3", "ReadList 3")
            .with_summary("Restricted-only readlist")
            .with_book_ids(["book-2"]),
    );

    adapter.insert_read_progress(
        ReadProgressRow::new("book-1", "0PV32486S7X3J", 7, false)
            .with_read_date("2024-01-04T03:04:05Z")
            .with_created("2024-01-04T03:04:05Z")
            .with_last_modified("2024-01-04T03:04:05Z")
            .with_device("device-android", "Android"),
    );
}

fn book_detail_payload(book: &BookDetailReadModel, is_admin: bool) -> Value {
    let url = restricted_book_url(&book.url, is_admin);
    let media_profile = media_profile_for_media_type(&book.media_type);

    json!({
        "id": book.id,
        "seriesId": book.series_id,
        "seriesTitle": book.series_title,
        "libraryId": book.library_id,
        "name": book.name,
        "url": url,
        "number": book.number,
        "created": book.created,
        "lastModified": book.last_modified,
        "fileLastModified": book.file_last_modified,
        "sizeBytes": book.size_bytes,
        "size": format_size_bytes(book.size_bytes),
        "media": {
            "status": book.media_status,
            "mediaType": book.media_type,
            "pagesCount": book.media_pages_count,
            "comment": book.media_comment,
            "epubDivinaCompatible": false,
            "epubIsKepub": false,
            "mediaProfile": media_profile
        },
        "metadata": {
            "title": book.metadata_title,
            "titleLock": false,
            "summary": book.metadata_summary,
            "summaryLock": false,
            "number": book.metadata_number,
            "numberLock": false,
            "numberSort": book.metadata_number_sort,
            "numberSortLock": false,
            "releaseDate": book.metadata_release_date,
            "releaseDateLock": false,
            "authors": book.metadata_authors.iter().map(|name| json!({ "name": name, "role": "writer" })).collect::<Vec<_>>(),
            "authorsLock": false,
            "tags": book.metadata_tags,
            "tagsLock": false,
            "isbn": book.metadata_isbn,
            "isbnLock": false,
            "links": [],
            "linksLock": false,
            "created": book.metadata_created,
            "lastModified": book.metadata_last_modified
        },
        "readProgress": book.read_progress.as_ref().map_or(Value::Null, |progress| json!({
            "page": progress.page,
            "completed": progress.completed,
            "readDate": progress.read_date,
            "created": progress.created,
            "lastModified": progress.last_modified,
            "deviceId": progress.device_id,
            "deviceName": progress.device_name,
        })),
        "deleted": book.deleted,
        "fileHash": book.file_hash,
        "oneshot": book.oneshot
    })
}

fn format_size_bytes(size_bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];

    if size_bytes < 1024 {
        return format!("{size_bytes} B");
    }

    let mut size = size_bytes as f64;
    let mut unit_index = 0usize;
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if (size - size.round()).abs() < 0.05 {
        format!("{} {}", size.round() as u64, UNITS[unit_index])
    } else {
        format!("{size:.1} {}", UNITS[unit_index])
    }
}

fn media_profile_for_media_type(media_type: &str) -> &'static str {
    match media_type {
        "application/zip"
        | "application/x-rar-compressed"
        | "application/x-rar-compressed; version=4"
        | "application/x-rar-compressed; version=5" => "DIVINA",
        "application/epub+zip" => "EPUB",
        "application/pdf" => "PDF",
        _ => "",
    }
}

fn series_detail_payload(series: &SeriesDetailReadModel, is_admin: bool) -> Value {
    let url = if is_admin {
        series.url.clone()
    } else {
        String::new()
    };

    let mut metadata = Map::new();
    metadata.insert("status".to_string(), Value::String(series.status.clone()));
    metadata.insert("statusLock".to_string(), Value::Bool(false));
    metadata.insert("title".to_string(), Value::String(series.title.clone()));
    metadata.insert("titleLock".to_string(), Value::Bool(false));
    metadata.insert("titleSort".to_string(), Value::String(series.title.clone()));
    metadata.insert("titleSortLock".to_string(), Value::Bool(false));
    metadata.insert("summary".to_string(), Value::String(series.summary.clone()));
    metadata.insert("summaryLock".to_string(), Value::Bool(false));
    metadata.insert(
        "readingDirection".to_string(),
        Value::String(series.reading_direction.clone()),
    );
    metadata.insert("readingDirectionLock".to_string(), Value::Bool(false));
    metadata.insert(
        "publisher".to_string(),
        Value::String(series.publisher.clone()),
    );
    metadata.insert("publisherLock".to_string(), Value::Bool(false));
    metadata.insert(
        "ageRating".to_string(),
        series
            .age_rating
            .map_or(Value::Null, |it| Value::Number(it.into())),
    );
    metadata.insert("ageRatingLock".to_string(), Value::Bool(false));
    metadata.insert(
        "language".to_string(),
        Value::String(series.language.clone()),
    );
    metadata.insert("languageLock".to_string(), Value::Bool(false));
    metadata.insert(
        "genres".to_string(),
        Value::Array(series.genres.iter().cloned().map(Value::String).collect()),
    );
    metadata.insert("genresLock".to_string(), Value::Bool(false));
    metadata.insert(
        "tags".to_string(),
        Value::Array(series.tags.iter().cloned().map(Value::String).collect()),
    );
    metadata.insert("tagsLock".to_string(), Value::Bool(false));
    metadata.insert(
        "totalBookCount".to_string(),
        series
            .total_book_count
            .map_or(Value::Null, |it| Value::Number(it.into())),
    );
    metadata.insert("totalBookCountLock".to_string(), Value::Bool(false));
    metadata.insert(
        "sharingLabels".to_string(),
        Value::Array(
            series
                .sharing_labels
                .iter()
                .cloned()
                .map(Value::String)
                .collect(),
        ),
    );
    metadata.insert("sharingLabelsLock".to_string(), Value::Bool(false));
    metadata.insert("links".to_string(), Value::Array(vec![]));
    metadata.insert("linksLock".to_string(), Value::Bool(false));
    metadata.insert(
        "alternateTitles".to_string(),
        Value::Array(
            series
                .alternate_titles
                .iter()
                .cloned()
                .map(Value::String)
                .collect(),
        ),
    );
    metadata.insert("alternateTitlesLock".to_string(), Value::Bool(false));
    metadata.insert(
        "created".to_string(),
        Value::String(series.metadata_created.clone()),
    );
    metadata.insert(
        "lastModified".to_string(),
        Value::String(series.metadata_last_modified.clone()),
    );

    let mut books_metadata = Map::new();
    books_metadata.insert("authors".to_string(), Value::Array(vec![]));
    books_metadata.insert(
        "tags".to_string(),
        Value::Array(
            series
                .books_metadata_tags
                .iter()
                .cloned()
                .map(Value::String)
                .collect(),
        ),
    );
    books_metadata.insert(
        "releaseDate".to_string(),
        series
            .books_metadata_release_date
            .clone()
            .map_or(Value::Null, Value::String),
    );
    books_metadata.insert(
        "summary".to_string(),
        Value::String(series.books_metadata_summary.clone()),
    );
    books_metadata.insert(
        "summaryNumber".to_string(),
        Value::String(series.books_metadata_summary_number.clone()),
    );
    books_metadata.insert(
        "created".to_string(),
        Value::String(series.books_metadata_created.clone()),
    );
    books_metadata.insert(
        "lastModified".to_string(),
        Value::String(series.books_metadata_last_modified.clone()),
    );

    let mut payload = Map::new();
    payload.insert("id".to_string(), Value::String(series.id.clone()));
    payload.insert(
        "libraryId".to_string(),
        Value::String(series.library_id.clone()),
    );
    payload.insert("name".to_string(), Value::String(series.title.clone()));
    payload.insert("url".to_string(), Value::String(url));
    payload.insert("created".to_string(), Value::String(series.created.clone()));
    payload.insert(
        "lastModified".to_string(),
        Value::String(series.last_modified.clone()),
    );
    payload.insert(
        "fileLastModified".to_string(),
        Value::String(series.file_last_modified.clone()),
    );
    payload.insert(
        "booksCount".to_string(),
        Value::Number(series.books_count.into()),
    );
    payload.insert(
        "booksReadCount".to_string(),
        Value::Number(series.books_read_count.into()),
    );
    payload.insert(
        "booksUnreadCount".to_string(),
        Value::Number(series.books_unread_count.into()),
    );
    payload.insert(
        "booksInProgressCount".to_string(),
        Value::Number(series.books_in_progress_count.into()),
    );
    payload.insert("metadata".to_string(), Value::Object(metadata));
    payload.insert("booksMetadata".to_string(), Value::Object(books_metadata));
    payload.insert("deleted".to_string(), Value::Bool(series.deleted));
    payload.insert("oneshot".to_string(), Value::Bool(series.oneshot));

    Value::Object(payload)
}

fn series_collections_payload(collections: &[CollectionReadModel]) -> Value {
    Value::Array(collections.iter().map(collection_payload).collect())
}

fn readlists_payload(readlists: &[ReadListReadModel]) -> Value {
    Value::Array(readlists.iter().map(readlist_payload).collect())
}

fn collection_payload(collection: &CollectionReadModel) -> Value {
    json!({
        "id": collection.id,
        "name": collection.name,
        "ordered": collection.ordered,
        "seriesIds": collection.series_ids,
        "createdDate": collection.created_date,
        "lastModifiedDate": collection.last_modified_date,
        "filtered": collection.filtered,
    })
}

fn readlist_payload(readlist: &ReadListReadModel) -> Value {
    json!({
        "id": readlist.id,
        "name": readlist.name,
        "summary": readlist.summary,
        "ordered": readlist.ordered,
        "bookIds": readlist.book_ids,
        "createdDate": readlist.created_date,
        "lastModifiedDate": readlist.last_modified_date,
        "filtered": readlist.filtered,
    })
}

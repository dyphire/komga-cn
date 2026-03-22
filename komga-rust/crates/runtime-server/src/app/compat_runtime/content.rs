use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use komga_application::discovery::{
    BookDetailQuery, BookReadlistsQuery, BookSiblingQuery, BooksLatestQuery, BooksListQuery,
    DiscoveryQueries, SeriesCollectionsQuery, SeriesDetailQuery, SeriesListQuery,
};
use komga_domain::discovery::{
    AgeRestrictionKind as DomainAgeRestrictionKind, DiscoveryError,
    BookDetailReadModel, BookReadModel, CollectionReadModel, DirectBrowseBooksListFamily,
    DiscoveryQueryContext as DomainDiscoveryQueryContext, NonNativeRequestShape, PageEnvelope,
    QueryRestrictions as DomainQueryRestrictions, ReadListReadModel, SeriesDetailReadModel,
    SeriesReadModel,
};
use komga_persistence::discovery::{
    BookRow, CollectionRow, ReadListRow, ReadProgressRow, SeriesRow, SqliteDiscoveryAdapter,
};
use serde_json::{Map, Value, json};

use crate::app::CompatProfile;
use crate::app::discovery_auth::{
    AgeRestrictionKind, DetailAccessDenial, DetailContentContext, DetailResourceContext,
    DiscoveryAuthState, DiscoveryQueryContext, QueryRestrictions,
};
use crate::app::placeholder_auth::{
    require_auth, resolved_auth_user, resolved_token,
};
use crate::app::snapshots::{
    book_pages_json, books_latest_json, snapshot_json,
};

#[path = "content_libraries.rs"]
mod content_libraries;
#[path = "content_auth.rs"]
mod content_auth;
#[path = "content_opds.rs"]
mod content_opds;
#[path = "content_java_live.rs"]
mod content_java_live;

use super::{
    CACHE_CONTROL_PRIVATE, LAST_MODIFIED, PAGE_BODY, PDF_BODY, ReadProgress, ReadProgressState,
    SEARCH_OWNERSHIP_HEADER, SHADOW_JAVA_WRITER_MARKER, THUMBNAIL_BODY, THUMBNAIL_ETAG,
};

const NATIVE_OWNERSHIP_MARKER: &str = "native-rust-owned";

#[derive(Clone, Copy)]
enum DiscoveryShape {
    Libraries,
    SeriesList,
    BooksList,
    BooksLatest,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum DiscoveryOwnershipRoute {
    NativeOwned,
    LegacyCompat,
}

fn discovery_ownership_route(
    profile: CompatProfile,
    headers: &HeaderMap,
    _shape: DiscoveryShape,
) -> DiscoveryOwnershipRoute {
    if profile == CompatProfile::SnapshotAligned && has_native_ownership_marker(headers) {
        DiscoveryOwnershipRoute::NativeOwned
    } else {
        DiscoveryOwnershipRoute::LegacyCompat
    }
}

fn has_native_ownership_marker(headers: &HeaderMap) -> bool {
    headers
        .get(SEARCH_OWNERSHIP_HEADER)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == NATIVE_OWNERSHIP_MARKER)
}

pub(super) async fn libraries(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
) -> Response {
    content_libraries::response(profile, headers, auth_state).await
}

pub(super) async fn series(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        let user =
            resolved_auth_user(&headers).expect("authorized series request should resolve user");
        let path = uri.path_and_query().map_or(uri.path(), |value| value.as_str());
        return match content_java_live::fetch_json(user, path, "series").await {
            Ok(series) => Json(series).into_response(),
            Err(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": message })),
            )
                .into_response(),
        };
    }

    Json(series_json_for_request(profile, &uri, None)).into_response()
}

pub(super) async fn series_detail(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        let user =
            resolved_auth_user(&headers).expect("authorized series detail request should resolve user");
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

    let Some(resource) = (match queries.resolve_series_resource(&series_id) {
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

    let detail_query_context = match auth_state.resolve_detail_query_context(&headers, &detail_context)
    {
        Ok(context) => context,
        Err(denial) => return detail_access_denial_response(denial),
    };
    let is_admin = detail_query_context.is_admin;

    let domain_context = to_domain_query_context(detail_query_context);
    let query = SeriesDetailQuery { series_id };

    match queries.get_series_detail(&domain_context, query) {
        Ok(Some(series)) => Json(series_detail_payload(&series, is_admin)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("series detail query failed: {error:?}") })),
        )
            .into_response(),
    }
}

pub(super) async fn series_collections(
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

    let Some(resource) = (match queries.resolve_series_resource(&series_id) {
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

    let detail_query_context = match auth_state.resolve_detail_query_context(&headers, &detail_context)
    {
        Ok(context) => context,
        Err(denial) => return detail_access_denial_response(denial),
    };

    let domain_context = to_domain_query_context(detail_query_context);
    let query = SeriesCollectionsQuery { series_id };

    match queries.list_series_collections(&domain_context, query) {
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

pub(super) async fn series_list(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let payload = serde_json::from_slice::<Value>(&body).ok();
    let full_text_search = payload.as_ref().and_then(extract_full_text_search);

    if discovery_ownership_route(profile, &headers, DiscoveryShape::SeriesList)
        == DiscoveryOwnershipRoute::NativeOwned
        && let Some(native_response) = native_owned_series_list_response(
            &headers,
            &uri,
            payload.as_ref(),
            full_text_search.clone(),
            &auth_state,
        )
    {
        return native_response;
    }

    let mut response = Json(series_json_for_request(profile, &uri, full_text_search)).into_response();

    if wants_shadow_marker(&headers, payload.as_ref()) {
        mark_non_native(&mut response);
    }

    response
}

pub(super) async fn books(
    Extension(profile): Extension<CompatProfile>,
    Extension(state): Extension<ReadProgressState>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let token = resolved_token(&headers);
    let mut books = if profile == CompatProfile::JavaLiveLocaldb {
        let user = resolved_auth_user(&headers).expect("authorized books request should resolve user");
        match content_java_live::fetch_json(user, "/api/v1/books", "books").await {
            Ok(books) => books,
            Err(message) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": message })),
                )
                    .into_response();
            }
        }
    } else {
        snapshot_json("books-list.json", profile)
    };
    let read_progress = state
        .progress_by_token
        .lock()
        .expect("read-progress state lock should not be poisoned")
        .get(&token)
        .and_then(|books| books.get("book-1"))
        .cloned();

    overlay_book_read_progress(&mut books, read_progress);

    Json(books).into_response()
}

pub(super) async fn books_list(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let payload = serde_json::from_slice::<Value>(&body).ok();
    let full_text_search = payload.as_ref().and_then(extract_full_text_search);

    if discovery_ownership_route(profile, &headers, DiscoveryShape::BooksList)
        == DiscoveryOwnershipRoute::NativeOwned
        && let Some(native_response) = native_owned_books_list_response(
            &headers,
            &uri,
            payload.as_ref(),
            full_text_search.clone(),
            &auth_state,
        )
    {
        return native_response;
    }

    let mut response = Json(books_json_for_request(profile, &uri, full_text_search)).into_response();

    if wants_shadow_marker(&headers, payload.as_ref()) {
        mark_non_native(&mut response);
    }

    response
}

pub(super) async fn books_latest(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(state): Extension<ReadProgressState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if discovery_ownership_route(profile, &headers, DiscoveryShape::BooksLatest)
        == DiscoveryOwnershipRoute::NativeOwned
        && let Some(native_response) = native_owned_books_latest_response(&headers, &uri, &auth_state)
    {
        return native_response;
    }

    let token = resolved_token(&headers);
    let read_progress = state
        .progress_by_token
        .lock()
        .expect("read-progress state lock should not be poisoned")
        .get(&token)
        .and_then(|books| books.get("book-1"))
        .cloned();

    let mut books = if profile == CompatProfile::JavaLiveLocaldb {
        let user =
            resolved_auth_user(&headers).expect("authorized books-latest request should resolve user");
        let path = uri.path_and_query().map_or(uri.path(), |value| value.as_str());
        match content_java_live::fetch_json(user, path, "books latest").await {
            Ok(books) => books,
            Err(message) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": message })),
                )
                    .into_response();
            }
        }
    } else {
        books_latest_json(profile)
    };

    overlay_book_read_progress(&mut books, read_progress);

    Json(books).into_response()
}

pub(super) async fn book_detail(
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

    let Some(resource) = (match queries.resolve_book_resource(&book_id) {
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

    let detail_query_context = match auth_state.resolve_detail_query_context(&headers, &detail_context)
    {
        Ok(context) => context,
        Err(denial) => return detail_access_denial_response(denial),
    };
    let is_admin = detail_query_context.is_admin;

    let domain_context = to_domain_query_context(detail_query_context);
    let query = BookDetailQuery { book_id };

    match queries.get_book_detail(&domain_context, query) {
        Ok(Some(book)) => Json(book_detail_payload(&book, is_admin)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("book detail query failed: {error:?}") })),
        )
            .into_response(),
    }
}

pub(super) async fn book_sibling_previous(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        let user =
            resolved_auth_user(&headers).expect("authorized sibling previous request should resolve user");
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

    let Some(resource) = (match queries.resolve_book_resource(&book_id) {
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

    let detail_query_context = match auth_state.resolve_detail_query_context(&headers, &detail_context)
    {
        Ok(context) => context,
        Err(denial) => return detail_access_denial_response(denial),
    };
    let is_admin = detail_query_context.is_admin;

    let domain_context = to_domain_query_context(detail_query_context);

    match queries.get_book_sibling_previous(&domain_context, BookSiblingQuery { book_id }) {
        Ok(Some(book)) => Json(book_detail_payload(&book, is_admin)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("book previous query failed: {error:?}") })),
        )
            .into_response(),
    }
}

pub(super) async fn book_sibling_next(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        let user = resolved_auth_user(&headers).expect("authorized sibling next request should resolve user");
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

    let Some(resource) = (match queries.resolve_book_resource(&book_id) {
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

    let detail_query_context = match auth_state.resolve_detail_query_context(&headers, &detail_context)
    {
        Ok(context) => context,
        Err(denial) => return detail_access_denial_response(denial),
    };
    let is_admin = detail_query_context.is_admin;

    let domain_context = to_domain_query_context(detail_query_context);

    match queries.get_book_sibling_next(&domain_context, BookSiblingQuery { book_id }) {
        Ok(Some(book)) => Json(book_detail_payload(&book, is_admin)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("book next query failed: {error:?}") })),
        )
            .into_response(),
    }
}

pub(super) async fn book_readlists(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        let user =
            resolved_auth_user(&headers).expect("authorized book readlists request should resolve user");
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

    let Some(resource) = (match queries.resolve_book_resource(&book_id) {
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

    let detail_query_context = match auth_state.resolve_detail_query_context(&headers, &detail_context)
    {
        Ok(context) => context,
        Err(denial) => return detail_access_denial_response(denial),
    };

    let domain_context = to_domain_query_context(detail_query_context);

    match queries.list_book_readlists(&domain_context, BookReadlistsQuery { book_id }) {
        Ok(readlists) => Json(readlists_payload(&readlists)).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("book readlists query failed: {error:?}") })),
        )
            .into_response(),
    }
}

pub(super) async fn users_me(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    content_auth::users_me(headers, uri, auth_state).await
}

pub(super) async fn login_set_cookie(headers: HeaderMap) -> Response {
    content_auth::login_set_cookie(headers).await
}

pub(super) async fn book_page(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if headers
        .get(header::IF_MODIFIED_SINCE)
        .and_then(|value| value.to_str().ok())
        == Some(LAST_MODIFIED)
    {
        return non_native_response((
            StatusCode::NOT_MODIFIED,
            [
                (header::LAST_MODIFIED, LAST_MODIFIED),
                (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
            ],
        )
            .into_response());
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        return non_native_response((
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::LAST_MODIFIED, LAST_MODIFIED),
                (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
                (
                    header::CONTENT_DISPOSITION,
                    "inline; filename=\"=?UTF-8?Q?book.cbr-1.png?=\"; filename*=UTF-8''book.cbr-1.png",
                ),
            ],
            PAGE_BODY,
        )
            .into_response());
    }

    non_native_response((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/pdf"),
            (header::LAST_MODIFIED, LAST_MODIFIED),
            (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
            (
                header::CONTENT_DISPOSITION,
                "inline; filename=\"=?UTF-8?Q?book.pdf-1.pdf?=\"; filename*=UTF-8''book.pdf-1.pdf",
            ),
        ],
        PDF_BODY,
    )
        .into_response())
}

pub(super) async fn book_page_thumbnail(
    Extension(_profile): Extension<CompatProfile>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if headers
        .get(header::IF_MODIFIED_SINCE)
        .and_then(|value| value.to_str().ok())
        == Some(LAST_MODIFIED)
    {
        return non_native_response((
            StatusCode::NOT_MODIFIED,
            [
                (header::LAST_MODIFIED, LAST_MODIFIED),
                (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
            ],
        )
            .into_response());
    }

    non_native_response((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/jpeg"),
            (header::CACHE_CONTROL, CACHE_CONTROL_PRIVATE),
            (header::LAST_MODIFIED, LAST_MODIFIED),
            (header::ETAG, THUMBNAIL_ETAG),
        ],
        THUMBNAIL_BODY,
    )
        .into_response())
}

pub(super) async fn book_thumbnail(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        return non_native_response(StatusCode::NOT_FOUND.into_response());
    }

    non_native_response(StatusCode::NOT_FOUND.into_response())
}

pub(super) async fn book_pages(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        let user =
            resolved_auth_user(&headers).expect("authorized book pages request should resolve user");
        return match content_java_live::fetch_json(user, "/api/v1/books/book-1/pages", "book pages")
            .await
        {
            Ok(pages) => non_native_response(Json(pages).into_response()),
            Err(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": message })),
            )
                .into_response(),
        };
    }

    non_native_response(Json(book_pages_json(profile)).into_response())
}

pub(super) async fn book_file(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        return non_native_response((
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/zip"),
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"=?UTF-8?Q?book.cbr?=\"; filename*=UTF-8''book.cbr",
                ),
            ],
            PAGE_BODY,
        )
            .into_response());
    }

    non_native_response((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/pdf"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"=?UTF-8?Q?book.pdf?=\"; filename*=UTF-8''book.pdf",
            ),
        ],
        PDF_BODY,
    )
        .into_response())
}

pub(super) async fn book_read_progress(
    Extension(_profile): Extension<CompatProfile>,
    Extension(state): Extension<ReadProgressState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    body: Bytes,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if book_id != "book-1" {
        return non_native_response(StatusCode::NOT_FOUND.into_response());
    }

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return non_native_response(invalid_read_progress_payload());
    };

    let token = resolved_token(&headers);

    if payload.get("completed").and_then(|value| value.as_bool()) == Some(true) {
        set_read_progress(&state, token, book_id, 10, true);
        return non_native_response(StatusCode::NO_CONTENT.into_response());
    }

    if let Some(page) = payload.get("page").and_then(|value| value.as_u64())
        && (1..=10).contains(&page)
    {
        set_read_progress(&state, token, book_id, page, false);
        return non_native_response(StatusCode::NO_CONTENT.into_response());
    }

    non_native_response(invalid_read_progress_payload())
}

pub(super) async fn book_read_progress_get(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let path = format!("/api/v1/books/{book_id}/read-progress");

    if profile == CompatProfile::JavaLiveLocaldb {
        let user =
            resolved_auth_user(&headers).expect("authorized read-progress GET should resolve user");
        return match content_java_live::fetch_text_response(user, &path, "book read-progress")
            .await
        {
            Ok(response) => non_native_response(response),
            Err(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": message })),
            )
                .into_response(),
        };
    }

    non_native_response(method_not_allowed_json_response(&path))
}

pub(super) async fn book_read_progress_delete(
    Extension(state): Extension<ReadProgressState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if book_id != "book-1" {
        return non_native_response(StatusCode::NOT_FOUND.into_response());
    }

    let token = resolved_token(&headers);
    let mut all_progress = state
        .progress_by_token
        .lock()
        .expect("read-progress state lock should not be poisoned");

    if let Some(user_progress) = all_progress.get_mut(&token) {
        user_progress.remove(&book_id);
    }

    non_native_response(StatusCode::NO_CONTENT.into_response())
}

pub(super) async fn book_progression(
    headers: HeaderMap,
    Path(book_id): Path<String>,
    body: Bytes,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if book_id != "book-1" {
        return non_native_response(StatusCode::NOT_FOUND.into_response());
    }

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return non_native_response(invalid_progression_payload());
    };

    let progression = payload
        .get("locator")
        .and_then(|value| value.get("locations"))
        .and_then(|value| value.get("progression"))
        .and_then(|value| value.as_f64());

    if progression.is_some() {
        non_native_response(StatusCode::NO_CONTENT.into_response())
    } else {
        non_native_response(invalid_progression_payload())
    }
}

pub(super) async fn book_progression_get(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if book_id != "book-1" {
        return non_native_response(StatusCode::NOT_FOUND.into_response());
    }

    let path = format!("/api/v1/books/{book_id}/progression");

    if profile == CompatProfile::JavaLiveLocaldb {
        let user =
            resolved_auth_user(&headers).expect("authorized progression GET should resolve user");
        return match content_java_live::fetch_text_response(user, &path, "book progression").await {
            Ok(response) => non_native_response(response),
            Err(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": message })),
            )
                .into_response(),
        };
    }

    non_native_response(StatusCode::NO_CONTENT.into_response())
}

pub(super) async fn opds_manifest(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
) -> Response {
    content_opds::opds_manifest(profile, headers).await
}

pub(super) async fn opds_auth(headers: HeaderMap) -> Response {
    content_opds::opds_auth(headers).await
}

pub(super) async fn opds_catalog(headers: HeaderMap) -> Response {
    content_opds::opds_catalog(headers).await
}

pub(super) async fn opds_v1_series(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
) -> Response {
    content_opds::opds_v1_series(profile, headers).await
}

fn series_json_for_request(profile: CompatProfile, uri: &Uri, full_text_search: Option<String>) -> Value {
    let mut series = snapshot_json("series-list.json", profile);
    let query = uri.query().unwrap_or_default();

    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let sort = query_value(query, "sort");
    let search_term = query_value(query, "search")
        .map(str::to_owned)
        .or(full_text_search);
    let search_regex = query_value(query, "search_regex").and_then(parse_search_regex);
    let has_author_filters = query_has_key(query, "authors") || query_has_key(query, "author");

    let mut filtered_content = series
        .pointer("/content")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();

    if has_author_filters {
        filtered_content.clear();
    }

    if let Some(term) = search_term {
        let normalized = term.to_ascii_lowercase();
        if !normalized.trim().is_empty() && !"series".contains(normalized.trim()) {
            filtered_content.clear();
        }
    }

    if let Some((pattern, field)) = search_regex {
        let candidate = if field == "title_sort" {
            "series"
        } else {
            "series"
        };
        if !matches_search_pattern(candidate, &pattern) {
            filtered_content.clear();
        }
    }

    let total_elements = filtered_content.len();
    let start = page.saturating_mul(size);
    let end = start.saturating_add(size).min(total_elements);
    let page_content = if start >= total_elements {
        Vec::new()
    } else {
        filtered_content[start..end].to_vec()
    };
    let number_of_elements = page_content.len();
    let total_pages = if total_elements == 0 {
        0
    } else {
        total_elements.div_ceil(size)
    };
    let first = page == 0;
    let last = total_pages == 0 || page + 1 >= total_pages;
    let empty = number_of_elements == 0;
    let sorted = sort.is_some();

    series["content"] = Value::Array(page_content);
    series["number"] = Value::Number((page as u64).into());
    series["size"] = Value::Number((size as u64).into());
    series["first"] = Value::Bool(first);
    series["last"] = Value::Bool(last);
    series["empty"] = Value::Bool(empty);
    series["numberOfElements"] = Value::Number((number_of_elements as u64).into());
    series["totalElements"] = Value::Number((total_elements as u64).into());
    series["totalPages"] = Value::Number((total_pages as u64).into());
    series["pageable"]["pageNumber"] = Value::Number((page as u64).into());
    series["pageable"]["pageSize"] = Value::Number((size as u64).into());
    series["pageable"]["offset"] = Value::Number((start as u64).into());
    series["sort"]["empty"] = Value::Bool(!sorted);
    series["sort"]["sorted"] = Value::Bool(sorted);
    series["sort"]["unsorted"] = Value::Bool(!sorted);
    series["pageable"]["sort"]["empty"] = Value::Bool(!sorted);
    series["pageable"]["sort"]["sorted"] = Value::Bool(sorted);
    series["pageable"]["sort"]["unsorted"] = Value::Bool(!sorted);

    series
}

fn books_json_for_request(profile: CompatProfile, uri: &Uri, full_text_search: Option<String>) -> Value {
    let mut books = snapshot_json("books-list.json", profile);
    let query = uri.query().unwrap_or_default();

    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let sort = query_values(query, "sort");

    let mut filtered_content = books
        .pointer("/content")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();

    if let Some(term) = full_text_search {
        let normalized = term.to_ascii_lowercase();
        if !normalized.trim().is_empty() {
            filtered_content.retain(|candidate| {
                candidate
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| name.to_ascii_lowercase().contains(normalized.trim()))
            });
        }
    }

    let total_elements = filtered_content.len();
    let start = page.saturating_mul(size);
    let end = start.saturating_add(size).min(total_elements);
    let page_content = if start >= total_elements {
        Vec::new()
    } else {
        filtered_content[start..end].to_vec()
    };
    let number_of_elements = page_content.len();
    let total_pages = if total_elements == 0 {
        0
    } else {
        total_elements.div_ceil(size)
    };
    let first = page == 0;
    let last = total_pages == 0 || page + 1 >= total_pages;
    let empty = number_of_elements == 0;
    let sorted = !sort.is_empty();

    books["content"] = Value::Array(page_content);
    books["number"] = Value::Number((page as u64).into());
    books["size"] = Value::Number((size as u64).into());
    books["first"] = Value::Bool(first);
    books["last"] = Value::Bool(last);
    books["empty"] = Value::Bool(empty);
    books["numberOfElements"] = Value::Number((number_of_elements as u64).into());
    books["totalElements"] = Value::Number((total_elements as u64).into());
    books["totalPages"] = Value::Number((total_pages as u64).into());
    books["pageable"]["pageNumber"] = Value::Number((page as u64).into());
    books["pageable"]["pageSize"] = Value::Number((size as u64).into());
    books["pageable"]["offset"] = Value::Number((start as u64).into());
    books["sort"]["empty"] = Value::Bool(!sorted);
    books["sort"]["sorted"] = Value::Bool(sorted);
    books["sort"]["unsorted"] = Value::Bool(!sorted);
    books["pageable"]["sort"]["empty"] = Value::Bool(!sorted);
    books["pageable"]["sort"]["sorted"] = Value::Bool(sorted);
    books["pageable"]["sort"]["unsorted"] = Value::Bool(!sorted);

    books
}

#[derive(Clone, Debug, Default)]
struct NativeSeriesFilters {
    library_ids: Option<Vec<String>>,
    deleted: Option<bool>,
    oneshot: Option<bool>,
    read_statuses: Option<Vec<String>>,
    genres: Option<Vec<String>>,
    tags: Option<Vec<String>>,
    languages: Option<Vec<String>>,
    publishers: Option<Vec<String>>,
    age_ratings: Option<Vec<u16>>,
    release_dates: Option<Vec<String>>,
    sharing_labels: Option<Vec<String>>,
    series_statuses: Option<Vec<String>>,
    complete: Option<bool>,
    authors: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default)]
struct NativeBooksFilters {
    direct_browse_family: Option<DirectBrowseBooksListFamily>,
    library_ids: Option<Vec<String>>,
    series_ids: Option<Vec<String>>,
    deleted: Option<bool>,
    oneshot: Option<bool>,
    tags: Option<Vec<String>>,
    read_statuses: Option<Vec<String>>,
    media_profiles: Option<Vec<String>>,
    media_statuses: Option<Vec<String>>,
    authors: Option<Vec<String>>,
    release_dates: Option<Vec<String>>,
}

fn native_owned_books_list_response(
    headers: &HeaderMap,
    uri: &Uri,
    payload: Option<&Value>,
    full_text_search: Option<String>,
    auth_state: &DiscoveryAuthState,
) -> Option<Response> {
    let sorts = query_values(uri.query().unwrap_or_default(), "sort")
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let page = query_value(uri.query().unwrap_or_default(), "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(uri.query().unwrap_or_default(), "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(uri.query().unwrap_or_default(), "unpaged");

    let filters = match parse_native_books_filters(payload.and_then(|value| value.get("condition"))) {
        Ok(filters) => filters,
        Err(error) => {
            return Some(non_native_books_list_response(
                error,
                uri,
                full_text_search,
                payload,
            ));
        }
    };

    let context = match auth_state.resolve_query_context(headers, filters.library_ids.as_deref()) {
        Some(context) => context,
        None => return Some(StatusCode::UNAUTHORIZED.into_response()),
    };

    let mut adapter = SqliteDiscoveryAdapter::default();
    seed_books_discovery_data(&mut adapter);

    let is_admin = context.is_admin;
    let queries = DiscoveryQueries::new(adapter);
    let domain_context = to_domain_query_context(context);
    let query = BooksListQuery {
        page,
        size,
        unpaged,
        direct_browse_family: filters.direct_browse_family,
        library_ids: filters.library_ids,
        series_ids: filters.series_ids,
        deleted: filters.deleted,
        oneshot: filters.oneshot,
        tags: filters.tags,
        read_statuses: filters.read_statuses,
        media_profiles: filters.media_profiles,
        media_statuses: filters.media_statuses,
        authors: filters.authors,
        release_dates: filters.release_dates,
        sort: sorts,
        search: full_text_search,
    };
    let is_direct_browse = query.direct_browse_family.is_some();
    let fallback_search = query.search.clone();

    let result = if is_direct_browse {
        queries.list_books_direct_browse(&domain_context, query)
    } else {
        queries.list_books(&domain_context, query)
    };

    match result {
        Ok(page) => {
            let mut response = Json(books_page_payload(page, is_admin, true)).into_response();
            mark_native(&mut response);
            Some(response)
        }
        Err(DiscoveryError::NonNativeRequestShape(details)) => {
            Some(non_native_books_list_response(
                DiscoveryError::NonNativeRequestShape(details),
                uri,
                fallback_search,
                payload,
            ))
        }
        Err(error) => Some(
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("native books list failed: {error:?}") })),
            )
                .into_response(),
        ),
    }
}

fn native_owned_books_latest_response(
    headers: &HeaderMap,
    uri: &Uri,
    auth_state: &DiscoveryAuthState,
) -> Option<Response> {
    let sorts = query_values(uri.query().unwrap_or_default(), "sort");
    if !sorts.is_empty() {
        return Some(non_native_books_latest_response(
            DiscoveryError::NonNativeRequestShape(NonNativeRequestShape::UnsupportedBookSort(
                sorts[0].to_string(),
            )),
            uri,
        ));
    }

    let page = query_value(uri.query().unwrap_or_default(), "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(uri.query().unwrap_or_default(), "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(uri.query().unwrap_or_default(), "unpaged");

    let context = match auth_state.resolve_query_context(headers, None) {
        Some(context) => context,
        None => return Some(StatusCode::UNAUTHORIZED.into_response()),
    };

    let mut adapter = SqliteDiscoveryAdapter::default();
    seed_books_discovery_data(&mut adapter);

    let is_admin = context.is_admin;
    let queries = DiscoveryQueries::new(adapter);
    let domain_context = to_domain_query_context(context);
    let query = BooksLatestQuery {
        page,
        size,
        unpaged,
        library_ids: None,
    };

    match queries.list_books_latest(&domain_context, query) {
        Ok(page) => {
            let mut response = Json(books_page_payload(page, is_admin, !unpaged)).into_response();
            mark_native(&mut response);
            Some(response)
        }
        Err(DiscoveryError::NonNativeRequestShape(details)) => Some(non_native_books_latest_response(
            DiscoveryError::NonNativeRequestShape(details),
            uri,
        )),
        Err(error) => Some(
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("native books latest failed: {error:?}") })),
            )
                .into_response(),
        ),
    }
}

fn native_owned_series_list_response(
    headers: &HeaderMap,
    uri: &Uri,
    payload: Option<&Value>,
    full_text_search: Option<String>,
    auth_state: &DiscoveryAuthState,
) -> Option<Response> {
    let sorts = query_values(uri.query().unwrap_or_default(), "sort")
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let page = query_value(uri.query().unwrap_or_default(), "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(uri.query().unwrap_or_default(), "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);

    let filters = match parse_native_series_filters(payload.and_then(|value| value.get("condition")))
    {
        Ok(filters) => filters,
        Err(error) => return Some(non_native_series_list_response(error, uri, full_text_search)),
    };

    let context = match auth_state.resolve_query_context(headers, filters.library_ids.as_deref()) {
        Some(context) => context,
        None => return Some(StatusCode::UNAUTHORIZED.into_response()),
    };

    let mut adapter = SqliteDiscoveryAdapter::default();
    seed_series_discovery_data(&mut adapter);

    let queries = DiscoveryQueries::new(adapter);
    let domain_context = to_domain_query_context(context);
    let query = SeriesListQuery {
        page,
        size,
        library_ids: filters.library_ids,
        deleted: filters.deleted,
        oneshot: filters.oneshot,
        read_statuses: filters.read_statuses,
        genres: filters.genres,
        tags: filters.tags,
        languages: filters.languages,
        publishers: filters.publishers,
        age_ratings: filters.age_ratings,
        release_dates: filters.release_dates,
        sharing_labels: filters.sharing_labels,
        series_statuses: filters.series_statuses,
        complete: filters.complete,
        authors: filters.authors,
        sort: sorts,
        search: full_text_search,
    };
    let fallback_search = query.search.clone();

    match queries.list_series(&domain_context, query) {
        Ok(page) => {
            let mut response = Json(series_page_payload(page)).into_response();
            mark_native(&mut response);
            Some(response)
        }
        Err(DiscoveryError::NonNativeRequestShape(details)) => {
            Some(non_native_series_list_response(
                DiscoveryError::NonNativeRequestShape(details),
                uri,
                fallback_search,
            ))
        }
        Err(error) => Some(
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("native series list failed: {error:?}") })),
            )
                .into_response(),
        ),
    }
}

fn non_native_series_list_response(
    error: DiscoveryError,
    uri: &Uri,
    full_text_search: Option<String>,
) -> Response {
    let mut payload = series_json_for_request(
        CompatProfile::SnapshotAligned,
        uri,
        full_text_search,
    );
    apply_non_native_diagnostics(&mut payload, &error);

    let mut response = Json(payload).into_response();
    mark_non_native(&mut response);
    response
}

fn non_native_books_list_response(
    error: DiscoveryError,
    uri: &Uri,
    full_text_search: Option<String>,
    _request_payload: Option<&Value>,
) -> Response {
    let mut payload = books_json_for_request(
        CompatProfile::SnapshotAligned,
        uri,
        full_text_search,
    );
    apply_non_native_diagnostics(&mut payload, &error);

    let mut response = Json(payload).into_response();
    mark_non_native(&mut response);
    response
}

fn non_native_books_latest_response(error: DiscoveryError, uri: &Uri) -> Response {
    let mut payload = books_latest_json_for_request(CompatProfile::SnapshotAligned, uri);
    apply_non_native_diagnostics(&mut payload, &error);

    let mut response = Json(payload).into_response();
    mark_non_native(&mut response);
    response
}

fn apply_non_native_diagnostics(payload: &mut Value, error: &DiscoveryError) {
    let (reason, shape) = match error {
        DiscoveryError::NonNativeRequestShape(details) => {
            ("unsupported-request-shape", non_native_shape_label(details))
        }
        DiscoveryError::InvalidRequest(message) => ("invalid-request", message.clone()),
        DiscoveryError::Persistence(message) => ("persistence-error", message.clone()),
    };

    payload["_compat"] = json!({
        "discoveryOwnership": "non-native",
        "reason": reason,
        "shape": shape,
    });
}

fn non_native_shape_label(shape: &NonNativeRequestShape) -> String {
    match shape {
        NonNativeRequestShape::UnsupportedSeriesSort(value) => {
            format!("UnsupportedSeriesSort({value})")
        }
        NonNativeRequestShape::UnsupportedSeriesFilter(value) => {
            format!("UnsupportedSeriesFilter({value})")
        }
        NonNativeRequestShape::UnsupportedBookSort(value) => {
            format!("UnsupportedBookSort({value})")
        }
        NonNativeRequestShape::UnsupportedBookFilter(value) => {
            format!("UnsupportedBookFilter({value})")
        }
    }
}

fn parse_native_series_filters(condition: Option<&Value>) -> Result<NativeSeriesFilters, DiscoveryError> {
    let Some(condition) = condition else {
        return Ok(NativeSeriesFilters::default());
    };

    let Some(condition_type) = condition.get("type").and_then(Value::as_str) else {
        return Err(DiscoveryError::InvalidRequest(
            "series condition missing type".to_string(),
        ));
    };

    match condition_type {
        "LibraryId" => parse_library_id_filter(condition),
        "Deleted" => parse_deleted_filter(condition),
        "OneShot" => parse_oneshot_filter(condition),
        "ReadStatus" => parse_series_read_status_filter(condition),
        "Genre" => parse_series_genre_filter(condition),
        "Tag" => parse_series_tag_filter(condition),
        "Language" => parse_series_language_filter(condition),
        "Publisher" => parse_series_publisher_filter(condition),
        "AgeRating" => parse_series_age_rating_filter(condition),
        "ReleaseDate" => parse_series_release_date_filter(condition),
        "SharingLabel" => parse_series_sharing_label_filter(condition),
        "SeriesStatus" => parse_series_status_filter(condition),
        "Complete" => parse_series_complete_filter(condition),
        "Author" => parse_series_author_filter(condition),
        "AllOfSeries" => parse_composite_filters(condition, true),
        "AnyOfSeries" => parse_composite_filters(condition, false),
        unsupported => Err(DiscoveryError::NonNativeRequestShape(
            NonNativeRequestShape::UnsupportedSeriesFilter(unsupported.to_string()),
        )),
    }
}

fn parse_native_books_filters(condition: Option<&Value>) -> Result<NativeBooksFilters, DiscoveryError> {
    let Some(condition) = condition else {
        return Ok(NativeBooksFilters::default());
    };

    let Some(condition_type) = condition.get("type").and_then(Value::as_str) else {
        return Err(DiscoveryError::InvalidRequest(
            "books condition missing type".to_string(),
        ));
    };

    match condition_type {
        "LibraryId" => parse_books_library_id_filter(condition),
        "SeriesId" => parse_books_series_id_filter(condition),
        "Deleted" => parse_books_deleted_filter(condition),
        "OneShot" => parse_books_oneshot_filter(condition),
        "Tag" => parse_books_tag_filter(condition),
        "ReadStatus" => parse_books_read_status_filter(condition),
        "MediaProfile" => parse_books_media_profile_filter(condition),
        "MediaStatus" => parse_books_media_status_filter(condition),
        "Author" => parse_books_author_filter(condition),
        "ReleaseDate" => parse_books_release_date_filter(condition),
        "AllOfBook" => parse_books_composite_filters(condition, true),
        "AnyOfBook" => parse_books_composite_filters(condition, false),
        unsupported => Err(DiscoveryError::NonNativeRequestShape(
            NonNativeRequestShape::UnsupportedBookFilter(unsupported.to_string()),
        )),
    }
}

fn parse_books_library_id_filter(condition: &Value) -> Result<NativeBooksFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" {
        return Err(DiscoveryError::NonNativeRequestShape(
            NonNativeRequestShape::UnsupportedBookFilter("LibraryId.operator".to_string()),
        ));
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Err(DiscoveryError::InvalidRequest(
            "books library filter value missing".to_string(),
        ));
    };

    Ok(NativeBooksFilters {
        library_ids: Some(vec![value.to_string()]),
        ..NativeBooksFilters::default()
    })
}

fn parse_books_series_id_filter(condition: &Value) -> Result<NativeBooksFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" {
        return Err(DiscoveryError::NonNativeRequestShape(
            NonNativeRequestShape::UnsupportedBookFilter("SeriesId.operator".to_string()),
        ));
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Err(DiscoveryError::InvalidRequest(
            "books series filter value missing".to_string(),
        ));
    };

    Ok(NativeBooksFilters {
        direct_browse_family: Some(DirectBrowseBooksListFamily::BrowseBookSiblingsUnpaged),
        series_ids: Some(vec![value.to_string()]),
        ..NativeBooksFilters::default()
    })
}

fn parse_books_deleted_filter(condition: &Value) -> Result<NativeBooksFilters, DiscoveryError> {
    let Some(operator) = condition.get("operator").and_then(Value::as_str) else {
        return Err(DiscoveryError::InvalidRequest(
            "books deleted filter missing operator".to_string(),
        ));
    };

    let deleted = match operator.to_ascii_lowercase().as_str() {
        "istrue" => true,
        "isfalse" => false,
        _ => {
            return Err(DiscoveryError::NonNativeRequestShape(
                NonNativeRequestShape::UnsupportedBookFilter("Deleted.operator".to_string()),
            ));
        }
    };

    Ok(NativeBooksFilters {
        deleted: Some(deleted),
        ..NativeBooksFilters::default()
    })
}

fn parse_books_oneshot_filter(condition: &Value) -> Result<NativeBooksFilters, DiscoveryError> {
    let Some(operator) = condition.get("operator").and_then(Value::as_str) else {
        return Err(DiscoveryError::InvalidRequest(
            "books oneshot filter missing operator".to_string(),
        ));
    };

    let oneshot = match operator.to_ascii_lowercase().as_str() {
        "istrue" => true,
        "isfalse" => false,
        _ => {
            return Err(DiscoveryError::NonNativeRequestShape(
                NonNativeRequestShape::UnsupportedBookFilter("OneShot.operator".to_string()),
            ));
        }
    };

    Ok(NativeBooksFilters {
        oneshot: Some(oneshot),
        ..NativeBooksFilters::default()
    })
}

fn parse_books_tag_filter(condition: &Value) -> Result<NativeBooksFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" {
        return Err(DiscoveryError::NonNativeRequestShape(
            NonNativeRequestShape::UnsupportedBookFilter("Tag.operator".to_string()),
        ));
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Err(DiscoveryError::InvalidRequest(
            "books tag filter value missing".to_string(),
        ));
    };

    Ok(NativeBooksFilters {
        tags: Some(vec![value.to_ascii_lowercase()]),
        ..NativeBooksFilters::default()
    })
}

fn parse_books_read_status_filter(condition: &Value) -> Result<NativeBooksFilters, DiscoveryError> {
    parse_books_string_filter(condition, "ReadStatus", "is", |value| NativeBooksFilters {
        read_statuses: Some(vec![value]),
        ..NativeBooksFilters::default()
    })
}

fn parse_books_media_profile_filter(condition: &Value) -> Result<NativeBooksFilters, DiscoveryError> {
    parse_books_string_filter(condition, "MediaProfile", "is", |value| NativeBooksFilters {
        media_profiles: Some(vec![value]),
        ..NativeBooksFilters::default()
    })
}

fn parse_books_media_status_filter(condition: &Value) -> Result<NativeBooksFilters, DiscoveryError> {
    parse_books_string_filter(condition, "MediaStatus", "is", |value| NativeBooksFilters {
        media_statuses: Some(vec![value]),
        ..NativeBooksFilters::default()
    })
}

fn parse_books_author_filter(condition: &Value) -> Result<NativeBooksFilters, DiscoveryError> {
    parse_books_string_filter(condition, "Author", "contains_or_is", |value| NativeBooksFilters {
        authors: Some(vec![value]),
        ..NativeBooksFilters::default()
    })
}

fn parse_books_release_date_filter(condition: &Value) -> Result<NativeBooksFilters, DiscoveryError> {
    parse_books_string_filter(condition, "ReleaseDate", "is", |value| NativeBooksFilters {
        release_dates: Some(vec![value]),
        ..NativeBooksFilters::default()
    })
}

fn parse_series_string_filter<F>(
    condition: &Value,
    filter_name: &str,
    expected_operator: &str,
    build: F,
) -> Result<NativeSeriesFilters, DiscoveryError>
where
    F: Fn(String) -> NativeSeriesFilters,
{
    ensure_series_operator(condition, filter_name, expected_operator)?;
    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Err(DiscoveryError::InvalidRequest(format!(
            "series {filter_name} filter value missing"
        )));
    };

    Ok(build(value.to_ascii_lowercase()))
}

fn parse_books_string_filter<F>(
    condition: &Value,
    filter_name: &str,
    expected_operator: &str,
    build: F,
) -> Result<NativeBooksFilters, DiscoveryError>
where
    F: Fn(String) -> NativeBooksFilters,
{
    ensure_books_operator(condition, filter_name, expected_operator)?;
    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Err(DiscoveryError::InvalidRequest(format!(
            "books {filter_name} filter value missing"
        )));
    };

    Ok(build(value.to_ascii_lowercase()))
}

fn ensure_series_operator(
    condition: &Value,
    filter_name: &str,
    expected_operator: &str,
) -> Result<(), DiscoveryError> {
    let Some(operator) = condition.get("operator").and_then(Value::as_str) else {
        return Err(DiscoveryError::InvalidRequest(format!(
            "series {filter_name} filter missing operator"
        )));
    };
    let op = operator.to_ascii_lowercase();
    let is_supported = if expected_operator == "contains_or_is" {
        op == "contains" || op == "is"
    } else {
        op == expected_operator
    };

    if is_supported {
        Ok(())
    } else {
        Err(DiscoveryError::NonNativeRequestShape(
            NonNativeRequestShape::UnsupportedSeriesFilter(format!("{filter_name}.operator")),
        ))
    }
}

fn ensure_books_operator(
    condition: &Value,
    filter_name: &str,
    expected_operator: &str,
) -> Result<(), DiscoveryError> {
    let Some(operator) = condition.get("operator").and_then(Value::as_str) else {
        return Err(DiscoveryError::InvalidRequest(format!(
            "books {filter_name} filter missing operator"
        )));
    };
    let op = operator.to_ascii_lowercase();
    let is_supported = if expected_operator == "contains_or_is" {
        op == "contains" || op == "is"
    } else {
        op == expected_operator
    };

    if is_supported {
        Ok(())
    } else {
        Err(DiscoveryError::NonNativeRequestShape(
            NonNativeRequestShape::UnsupportedBookFilter(format!("{filter_name}.operator")),
        ))
    }
}

fn parse_books_composite_filters(condition: &Value, all_of: bool) -> Result<NativeBooksFilters, DiscoveryError> {
    let Some(children) = condition.get("conditions").and_then(Value::as_array) else {
        return Err(DiscoveryError::InvalidRequest(
            "books composite filter missing conditions".to_string(),
        ));
    };

    let mut aggregate = NativeBooksFilters::default();
    let mut child_count = 0usize;
    let mut series_leaf_count = 0usize;
    let mut library_groups: Vec<Vec<String>> = vec![];
    let mut series_groups: Vec<Vec<String>> = vec![];
    let mut tag_groups: Vec<Vec<String>> = vec![];
    let mut read_status_groups: Vec<Vec<String>> = vec![];
    let mut media_profile_groups: Vec<Vec<String>> = vec![];
    let mut media_status_groups: Vec<Vec<String>> = vec![];
    let mut author_groups: Vec<Vec<String>> = vec![];
    let mut release_date_groups: Vec<Vec<String>> = vec![];

    for child in children {
        child_count += 1;
        let parsed = parse_native_books_filters(Some(child))?;
        let is_series_leaf = parsed.series_ids.is_some()
            && parsed.library_ids.is_none()
            && parsed.deleted.is_none()
            && parsed.oneshot.is_none()
            && parsed.tags.is_none()
            && parsed.read_statuses.is_none()
            && parsed.media_profiles.is_none()
            && parsed.media_statuses.is_none()
            && parsed.authors.is_none()
            && parsed.release_dates.is_none();
        if is_series_leaf {
            series_leaf_count += 1;
        }

        if let Some(ids) = parsed.library_ids {
            library_groups.push(ids);
        }
        if let Some(ids) = parsed.series_ids {
            series_groups.push(ids);
        }
        if let Some(tags) = parsed.tags {
            tag_groups.push(tags);
        }
        if let Some(read_statuses) = parsed.read_statuses {
            read_status_groups.push(read_statuses);
        }
        if let Some(media_profiles) = parsed.media_profiles {
            media_profile_groups.push(media_profiles);
        }
        if let Some(media_statuses) = parsed.media_statuses {
            media_status_groups.push(media_statuses);
        }
        if let Some(authors) = parsed.authors {
            author_groups.push(authors);
        }
        if let Some(release_dates) = parsed.release_dates {
            release_date_groups.push(release_dates);
        }

        aggregate.deleted = merge_boolean_filter(aggregate.deleted, parsed.deleted)?;
        aggregate.oneshot = merge_boolean_filter(aggregate.oneshot, parsed.oneshot)?;
    }

    aggregate.library_ids = merge_string_groups(library_groups, all_of);
    aggregate.series_ids = merge_string_groups(series_groups, all_of);
    aggregate.tags = merge_string_groups(tag_groups, all_of);
    aggregate.read_statuses = merge_string_groups(read_status_groups, all_of);
    aggregate.media_profiles = merge_string_groups(media_profile_groups, all_of);
    aggregate.media_statuses = merge_string_groups(media_status_groups, all_of);
    aggregate.authors = merge_string_groups(author_groups, all_of);
    aggregate.release_dates = merge_string_groups(release_date_groups, all_of);
    aggregate.direct_browse_family = if all_of && child_count == 1 && series_leaf_count == 1 {
        Some(DirectBrowseBooksListFamily::BrowseSeriesPaged)
    } else {
        None
    };

    Ok(aggregate)
}

fn merge_string_groups(groups: Vec<Vec<String>>, all_of: bool) -> Option<Vec<String>> {
    if groups.is_empty() {
        return None;
    }

    if all_of {
        let mut intersection = groups[0].clone();
        for group in groups.iter().skip(1) {
            intersection.retain(|candidate| group.contains(candidate));
        }
        Some(intersection)
    } else {
        let mut union = vec![];
        for group in groups {
            for candidate in group {
                if !union.contains(&candidate) {
                    union.push(candidate);
                }
            }
        }
        Some(union)
    }
}

fn merge_u16_groups(groups: Vec<Vec<u16>>, all_of: bool) -> Option<Vec<u16>> {
    if groups.is_empty() {
        return None;
    }

    if all_of {
        let mut intersection = groups[0].clone();
        for group in groups.iter().skip(1) {
            intersection.retain(|candidate| group.contains(candidate));
        }
        Some(intersection)
    } else {
        let mut union = vec![];
        for group in groups {
            for candidate in group {
                if !union.contains(&candidate) {
                    union.push(candidate);
                }
            }
        }
        Some(union)
    }
}

fn parse_library_id_filter(condition: &Value) -> Result<NativeSeriesFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" {
        return Err(DiscoveryError::NonNativeRequestShape(
            NonNativeRequestShape::UnsupportedSeriesFilter("LibraryId.operator".to_string()),
        ));
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Err(DiscoveryError::InvalidRequest(
            "series library filter value missing".to_string(),
        ));
    };

    Ok(NativeSeriesFilters {
        library_ids: Some(vec![value.to_string()]),
        ..NativeSeriesFilters::default()
    })
}

fn parse_deleted_filter(condition: &Value) -> Result<NativeSeriesFilters, DiscoveryError> {
    let Some(operator) = condition.get("operator").and_then(Value::as_str) else {
        return Err(DiscoveryError::InvalidRequest(
            "series deleted filter missing operator".to_string(),
        ));
    };

    let deleted = match operator.to_ascii_lowercase().as_str() {
        "istrue" => true,
        "isfalse" => false,
        _ => {
            return Err(DiscoveryError::NonNativeRequestShape(
                NonNativeRequestShape::UnsupportedSeriesFilter("Deleted.operator".to_string()),
            ));
        }
    };

    Ok(NativeSeriesFilters {
        deleted: Some(deleted),
        ..NativeSeriesFilters::default()
    })
}

fn parse_oneshot_filter(condition: &Value) -> Result<NativeSeriesFilters, DiscoveryError> {
    let Some(operator) = condition.get("operator").and_then(Value::as_str) else {
        return Err(DiscoveryError::InvalidRequest(
            "series oneshot filter missing operator".to_string(),
        ));
    };

    let oneshot = match operator.to_ascii_lowercase().as_str() {
        "istrue" => true,
        "isfalse" => false,
        _ => {
            return Err(DiscoveryError::NonNativeRequestShape(
                NonNativeRequestShape::UnsupportedSeriesFilter("OneShot.operator".to_string()),
            ));
        }
    };

    Ok(NativeSeriesFilters {
        oneshot: Some(oneshot),
        ..NativeSeriesFilters::default()
    })
}

fn parse_series_read_status_filter(condition: &Value) -> Result<NativeSeriesFilters, DiscoveryError> {
    parse_series_string_filter(condition, "ReadStatus", "is", |value| NativeSeriesFilters {
        read_statuses: Some(vec![value]),
        ..NativeSeriesFilters::default()
    })
}

fn parse_series_genre_filter(condition: &Value) -> Result<NativeSeriesFilters, DiscoveryError> {
    parse_series_string_filter(condition, "Genre", "contains_or_is", |value| NativeSeriesFilters {
        genres: Some(vec![value]),
        ..NativeSeriesFilters::default()
    })
}

fn parse_series_tag_filter(condition: &Value) -> Result<NativeSeriesFilters, DiscoveryError> {
    parse_series_string_filter(condition, "Tag", "contains_or_is", |value| NativeSeriesFilters {
        tags: Some(vec![value]),
        ..NativeSeriesFilters::default()
    })
}

fn parse_series_language_filter(condition: &Value) -> Result<NativeSeriesFilters, DiscoveryError> {
    parse_series_string_filter(condition, "Language", "is", |value| NativeSeriesFilters {
        languages: Some(vec![value]),
        ..NativeSeriesFilters::default()
    })
}

fn parse_series_publisher_filter(condition: &Value) -> Result<NativeSeriesFilters, DiscoveryError> {
    parse_series_string_filter(condition, "Publisher", "is", |value| NativeSeriesFilters {
        publishers: Some(vec![value]),
        ..NativeSeriesFilters::default()
    })
}

fn parse_series_age_rating_filter(condition: &Value) -> Result<NativeSeriesFilters, DiscoveryError> {
    ensure_series_operator(condition, "AgeRating", "is")?;
    let Some(value) = condition.get("value") else {
        return Err(DiscoveryError::InvalidRequest(
            "series age rating filter value missing".to_string(),
        ));
    };

    let parsed = if let Some(number) = value.as_u64() {
        number as u16
    } else if let Some(raw) = value.as_str() {
        raw.parse::<u16>().map_err(|_| {
            DiscoveryError::InvalidRequest("series age rating value invalid".to_string())
        })?
    } else {
        return Err(DiscoveryError::InvalidRequest(
            "series age rating value invalid".to_string(),
        ));
    };

    Ok(NativeSeriesFilters {
        age_ratings: Some(vec![parsed]),
        ..NativeSeriesFilters::default()
    })
}

fn parse_series_release_date_filter(condition: &Value) -> Result<NativeSeriesFilters, DiscoveryError> {
    parse_series_string_filter(condition, "ReleaseDate", "is", |value| NativeSeriesFilters {
        release_dates: Some(vec![value]),
        ..NativeSeriesFilters::default()
    })
}

fn parse_series_sharing_label_filter(condition: &Value) -> Result<NativeSeriesFilters, DiscoveryError> {
    parse_series_string_filter(
        condition,
        "SharingLabel",
        "contains_or_is",
        |value| NativeSeriesFilters {
            sharing_labels: Some(vec![value]),
            ..NativeSeriesFilters::default()
        },
    )
}

fn parse_series_status_filter(condition: &Value) -> Result<NativeSeriesFilters, DiscoveryError> {
    parse_series_string_filter(condition, "SeriesStatus", "is", |value| NativeSeriesFilters {
        series_statuses: Some(vec![value]),
        ..NativeSeriesFilters::default()
    })
}

fn parse_series_complete_filter(condition: &Value) -> Result<NativeSeriesFilters, DiscoveryError> {
    let Some(operator) = condition.get("operator").and_then(Value::as_str) else {
        return Err(DiscoveryError::InvalidRequest(
            "series complete filter missing operator".to_string(),
        ));
    };

    let complete = match operator.to_ascii_lowercase().as_str() {
        "istrue" => true,
        "isfalse" => false,
        _ => {
            return Err(DiscoveryError::NonNativeRequestShape(
                NonNativeRequestShape::UnsupportedSeriesFilter("Complete.operator".to_string()),
            ));
        }
    };

    Ok(NativeSeriesFilters {
        complete: Some(complete),
        ..NativeSeriesFilters::default()
    })
}

fn parse_series_author_filter(condition: &Value) -> Result<NativeSeriesFilters, DiscoveryError> {
    parse_series_string_filter(condition, "Author", "contains_or_is", |value| NativeSeriesFilters {
        authors: Some(vec![value]),
        ..NativeSeriesFilters::default()
    })
}

fn parse_composite_filters(condition: &Value, all_of: bool) -> Result<NativeSeriesFilters, DiscoveryError> {
    let Some(children) = condition.get("conditions").and_then(Value::as_array) else {
        return Err(DiscoveryError::InvalidRequest(
            "series composite filter missing conditions".to_string(),
        ));
    };

    let mut aggregate = NativeSeriesFilters::default();
    let mut library_groups: Vec<Vec<String>> = vec![];
    let mut read_status_groups: Vec<Vec<String>> = vec![];
    let mut genre_groups: Vec<Vec<String>> = vec![];
    let mut tag_groups: Vec<Vec<String>> = vec![];
    let mut language_groups: Vec<Vec<String>> = vec![];
    let mut publisher_groups: Vec<Vec<String>> = vec![];
    let mut release_date_groups: Vec<Vec<String>> = vec![];
    let mut sharing_label_groups: Vec<Vec<String>> = vec![];
    let mut series_status_groups: Vec<Vec<String>> = vec![];
    let mut author_groups: Vec<Vec<String>> = vec![];
    let mut age_rating_groups: Vec<Vec<u16>> = vec![];

    for child in children {
        let parsed = parse_native_series_filters(Some(child))?;
        if let Some(ids) = parsed.library_ids {
            library_groups.push(ids);
        }
        if let Some(read_statuses) = parsed.read_statuses {
            read_status_groups.push(read_statuses);
        }
        if let Some(genres) = parsed.genres {
            genre_groups.push(genres);
        }
        if let Some(tags) = parsed.tags {
            tag_groups.push(tags);
        }
        if let Some(languages) = parsed.languages {
            language_groups.push(languages);
        }
        if let Some(publishers) = parsed.publishers {
            publisher_groups.push(publishers);
        }
        if let Some(age_ratings) = parsed.age_ratings {
            age_rating_groups.push(age_ratings);
        }
        if let Some(release_dates) = parsed.release_dates {
            release_date_groups.push(release_dates);
        }
        if let Some(sharing_labels) = parsed.sharing_labels {
            sharing_label_groups.push(sharing_labels);
        }
        if let Some(series_statuses) = parsed.series_statuses {
            series_status_groups.push(series_statuses);
        }
        if let Some(authors) = parsed.authors {
            author_groups.push(authors);
        }

        aggregate.deleted = merge_boolean_filter(aggregate.deleted, parsed.deleted)?;
        aggregate.oneshot = merge_boolean_filter(aggregate.oneshot, parsed.oneshot)?;
        aggregate.complete = merge_boolean_filter(aggregate.complete, parsed.complete)?;
    }

    aggregate.library_ids = if library_groups.is_empty() {
        None
    } else if all_of {
        let mut intersection = library_groups[0].clone();
        for group in library_groups.iter().skip(1) {
            intersection.retain(|candidate| group.contains(candidate));
        }
        Some(intersection)
    } else {
        let mut union = vec![];
        for group in library_groups {
            for candidate in group {
                if !union.contains(&candidate) {
                    union.push(candidate);
                }
            }
        }
        Some(union)
    };

    aggregate.read_statuses = merge_string_groups(read_status_groups, all_of);
    aggregate.genres = merge_string_groups(genre_groups, all_of);
    aggregate.tags = merge_string_groups(tag_groups, all_of);
    aggregate.languages = merge_string_groups(language_groups, all_of);
    aggregate.publishers = merge_string_groups(publisher_groups, all_of);
    aggregate.age_ratings = merge_u16_groups(age_rating_groups, all_of);
    aggregate.release_dates = merge_string_groups(release_date_groups, all_of);
    aggregate.sharing_labels = merge_string_groups(sharing_label_groups, all_of);
    aggregate.series_statuses = merge_string_groups(series_status_groups, all_of);
    aggregate.authors = merge_string_groups(author_groups, all_of);

    Ok(aggregate)
}

fn merge_boolean_filter(left: Option<bool>, right: Option<bool>) -> Result<Option<bool>, DiscoveryError> {
    match (left, right) {
        (Some(a), Some(b)) if a == b => Ok(Some(a)),
        (Some(_), Some(_)) => Err(DiscoveryError::NonNativeRequestShape(
            NonNativeRequestShape::UnsupportedSeriesFilter("composite boolean mismatch".to_string()),
        )),
        (Some(value), None) | (None, Some(value)) => Ok(Some(value)),
        (None, None) => Ok(None),
    }
}

fn to_domain_query_context(context: DiscoveryQueryContext) -> DomainDiscoveryQueryContext {
    DomainDiscoveryQueryContext {
        user_id: context.user_id,
        is_admin: context.is_admin,
        authorized_library_ids: context.authorized_library_ids,
        restrictions: context.restrictions.map(to_domain_restrictions),
    }
}

fn to_domain_restrictions(restrictions: QueryRestrictions) -> DomainQueryRestrictions {
    DomainQueryRestrictions {
        age: restrictions.age,
        age_restriction: restrictions.age_restriction.map(to_domain_age_restriction_kind),
        labels_allow: restrictions.labels_allow,
        labels_exclude: restrictions.labels_exclude,
    }
}

fn to_domain_age_restriction_kind(kind: AgeRestrictionKind) -> DomainAgeRestrictionKind {
    match kind {
        AgeRestrictionKind::AllowOnly => DomainAgeRestrictionKind::AllowOnly,
        AgeRestrictionKind::Exclude => DomainAgeRestrictionKind::Exclude,
    }
}

fn seed_series_discovery_data(adapter: &mut SqliteDiscoveryAdapter) {
    adapter.insert_series(
        SeriesRow::new("series-1", "1", "series")
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
}

fn seed_books_discovery_data(adapter: &mut SqliteDiscoveryAdapter) {
    adapter.insert_series(SeriesRow::new("series-1", "1", "series").with_labels(["safe"]));
    adapter.insert_series(
        SeriesRow::new("series-2", "1", "restricted").with_labels(["adult"]),
    );

    adapter.insert_book(
        BookRow::new("book-1", "series-1", "1", "book.cbr")
            .with_url("/library1/book.cbr")
            .with_last_modified("2024-01-01T03:04:05Z")
            .with_media("READY", "application/zip", 1)
            .with_media_profile("PROFILE-1")
            .with_number_sort(1)
            .with_read_status("READ")
            .with_release_date("2024-01-01")
            .with_tags(["safe"])
            .with_authors(["alice"]),
    );
    adapter.insert_book(
        BookRow::new("book-2", "series-2", "1", "restricted-book.cbz")
            .with_url("/library1/restricted-book.cbz")
            .with_last_modified("2024-01-03T03:04:05Z")
            .with_media("READY", "application/vnd.comicbook+zip", 1)
            .with_media_profile("PROFILE-2")
            .with_number_sort(2)
            .with_read_status("UNREAD")
            .with_release_date("2023-01-01")
            .with_tags(["adult"])
            .with_authors(["bob"]),
    );
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

    adapter.insert_read_progress(
        ReadProgressRow::new("book-1", "0PV32486S7X3J", 7, false)
            .with_read_date("2024-01-04T03:04:05Z")
            .with_created("2024-01-04T03:04:05Z")
            .with_last_modified("2024-01-04T03:04:05Z")
            .with_device("device-android", "Android"),
    );
}

fn series_page_payload(page: PageEnvelope<SeriesReadModel>) -> Value {
    let content = page
        .content
        .iter()
        .map(series_payload)
        .collect::<Vec<_>>();
    let number_of_elements = content.len();
    let first = page.page == 0;
    let last = page.total_pages == 0 || page.page + 1 >= page.total_pages;
    let offset = page.page.saturating_mul(page.size);

    json!({
        "content": content,
        "pageable": {
            "pageNumber": page.page,
            "pageSize": page.size,
            "sort": {
                "empty": false,
                "sorted": true,
                "unsorted": false
            },
            "offset": offset,
            "paged": true,
            "unpaged": false
        },
        "last": last,
        "totalElements": page.total_elements,
        "totalPages": page.total_pages,
        "first": first,
        "size": page.size,
        "number": page.page,
        "sort": {
            "empty": false,
            "sorted": true,
            "unsorted": false
        },
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0
    })
}

fn books_page_payload(page: PageEnvelope<BookReadModel>, is_admin: bool, paged: bool) -> Value {
    let content = page
        .content
        .iter()
        .map(|book| book_payload(book, is_admin))
        .collect::<Vec<_>>();
    let number_of_elements = content.len();
    let first = page.page == 0;
    let last = page.total_pages == 0 || page.page + 1 >= page.total_pages;
    let offset = if paged {
        page.page.saturating_mul(page.size)
    } else {
        0
    };

    json!({
        "content": content,
        "pageable": {
            "pageNumber": page.page,
            "pageSize": page.size,
            "sort": {
                "empty": false,
                "sorted": true,
                "unsorted": false
            },
            "offset": offset,
            "paged": paged,
            "unpaged": !paged
        },
        "last": last,
        "totalElements": page.total_elements,
        "totalPages": page.total_pages,
        "first": first,
        "size": page.size,
        "number": page.page,
        "sort": {
            "empty": false,
            "sorted": true,
            "unsorted": false
        },
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0
    })
}

fn books_latest_json_for_request(profile: CompatProfile, uri: &Uri) -> Value {
    let mut books = books_latest_json(profile);
    let query = uri.query().unwrap_or_default();

    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query, "unpaged");

    let mut filtered_content = books
        .pointer("/content")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();

    let total_elements = filtered_content.len();
    let (page_content, number, response_size, total_pages, first, last, offset, paged) = if unpaged {
        let response_size = total_elements.max(1);
        let total_pages = if total_elements == 0 { 0 } else { 1 };
        let number = 0;
        let first = true;
        let last = true;
        let offset = 0;
        (
            filtered_content,
            number,
            response_size,
            total_pages,
            first,
            last,
            offset,
            false,
        )
    } else {
        let start = page.saturating_mul(size);
        let end = start.saturating_add(size).min(total_elements);
        let page_content = if start >= total_elements {
            Vec::new()
        } else {
            filtered_content.drain(start..end).collect()
        };
        let total_pages = if total_elements == 0 {
            0
        } else {
            total_elements.div_ceil(size)
        };
        let first = page == 0;
        let last = total_pages == 0 || page + 1 >= total_pages;
        (
            page_content,
            page,
            size,
            total_pages,
            first,
            last,
            start,
            true,
        )
    };
    let number_of_elements = page_content.len();

    books["content"] = Value::Array(page_content);
    books["number"] = Value::Number((number as u64).into());
    books["size"] = Value::Number((response_size as u64).into());
    books["first"] = Value::Bool(first);
    books["last"] = Value::Bool(last);
    books["empty"] = Value::Bool(number_of_elements == 0);
    books["numberOfElements"] = Value::Number((number_of_elements as u64).into());
    books["totalElements"] = Value::Number((total_elements as u64).into());
    books["totalPages"] = Value::Number((total_pages as u64).into());
    books["pageable"]["pageNumber"] = Value::Number((number as u64).into());
    books["pageable"]["pageSize"] = Value::Number((response_size as u64).into());
    books["pageable"]["offset"] = Value::Number((offset as u64).into());
    books["pageable"]["paged"] = Value::Bool(paged);
    books["pageable"]["unpaged"] = Value::Bool(!paged);

    books
}

fn book_payload(book: &BookReadModel, is_admin: bool) -> Value {
    let url = restricted_book_url(&book.url, is_admin);

    json!({
        "id": book.id,
        "seriesId": book.series_id,
        "seriesTitle": book.series_title,
        "libraryId": book.library_id,
        "name": book.title,
        "url": url,
        "number": 1,
        "created": book.created,
        "lastModified": book.last_modified,
        "fileLastModified": book.file_last_modified,
        "sizeBytes": book.size_bytes,
        "size": "0 B",
        "media": {
            "status": book.media_status,
            "mediaType": book.media_type,
            "pagesCount": book.media_pages_count,
            "comment": "",
            "epubDivinaCompatible": false,
            "epubIsKepub": false,
            "mediaProfile": ""
        },
        "metadata": {
            "title": book.title,
            "titleLock": false,
            "summary": "",
            "summaryLock": false,
            "number": "1",
            "numberLock": false,
            "numberSort": 1.0,
            "numberSortLock": false,
            "releaseDate": book.metadata_release_date,
            "releaseDateLock": false,
            "authors": [],
            "authorsLock": false,
            "tags": [],
            "tagsLock": false,
            "isbn": "",
            "isbnLock": false,
            "links": [],
            "linksLock": false,
            "created": book.created,
            "lastModified": book.last_modified
        },
        "readProgress": Value::Null,
        "deleted": book.deleted,
        "fileHash": "",
        "oneshot": book.oneshot
    })
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

fn restricted_book_url(url: &str, is_admin: bool) -> String {
    if is_admin {
        return url.to_string();
    }

    url.rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or_default()
        .to_string()
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
        series.age_rating.map_or(Value::Null, |it| Value::Number(it.into())),
    );
    metadata.insert("ageRatingLock".to_string(), Value::Bool(false));
    metadata.insert(
        "language".to_string(),
        Value::String(series.language.clone()),
    );
    metadata.insert("languageLock".to_string(), Value::Bool(false));
    metadata.insert(
        "genres".to_string(),
        Value::Array(
            series
                .genres
                .iter()
                .cloned()
                .map(Value::String)
                .collect(),
        ),
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

fn series_payload(series: &SeriesReadModel) -> Value {
    let metadata = json!({
        "status": "ONGOING",
        "statusLock": false,
        "title": series.title,
        "titleLock": false,
        "titleSort": series.title,
        "titleSortLock": false,
        "summary": "",
        "summaryLock": false,
        "readingDirection": "",
        "readingDirectionLock": false,
        "publisher": "",
        "publisherLock": false,
        "ageRating": null,
        "ageRatingLock": false,
        "language": "",
        "languageLock": false,
        "genres": [],
        "genresLock": false,
        "tags": [],
        "tagsLock": false,
        "totalBookCount": null,
        "totalBookCountLock": false,
        "sharingLabels": series.labels,
        "sharingLabelsLock": false,
        "links": [],
        "linksLock": false,
        "alternateTitles": [],
        "alternateTitlesLock": false,
        "created": "2026-01-01T00:00:00Z",
        "lastModified": "2026-01-01T00:00:00Z"
    });

    let books_metadata = json!({
        "authors": [],
        "tags": [],
        "releaseDate": null,
        "summary": "",
        "summaryNumber": "",
        "created": "2026-01-01T00:00:00Z",
        "lastModified": "2026-01-01T00:00:00Z"
    });

    json!({
        "id": series.id,
        "libraryId": series.library_id,
        "name": series.title,
        "url": "",
        "created": "2026-01-01T00:00:00Z",
        "lastModified": "2026-01-01T00:00:00Z",
        "fileLastModified": "2024-01-02T03:04:05Z",
        "booksCount": 0,
        "booksReadCount": 0,
        "booksUnreadCount": 0,
        "booksInProgressCount": 0,
        "metadata": metadata,
        "booksMetadata": books_metadata,
        "deleted": false,
        "oneshot": false
    })
}

fn extract_full_text_search(payload: &Value) -> Option<String> {
    payload
        .get("fullTextSearch")
        .and_then(|value| value.as_str())
        .map(str::to_owned)
}

fn wants_shadow_marker(headers: &HeaderMap, payload: Option<&Value>) -> bool {
    let ownership = payload
        .and_then(|payload| payload.get("ownership"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_ascii_lowercase());

    let requested_shadow_marker = headers
        .get(SEARCH_OWNERSHIP_HEADER)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == SHADOW_JAVA_WRITER_MARKER);

    let is_shadow_ownership = ownership
        .as_deref()
        .is_some_and(|value| value == "shadow");

    is_shadow_ownership || requested_shadow_marker
}

fn mark_non_native(response: &mut Response) {
    response.headers_mut().insert(
        HeaderName::from_static(SEARCH_OWNERSHIP_HEADER),
        HeaderValue::from_static(SHADOW_JAVA_WRITER_MARKER),
    );
}

fn non_native_response(mut response: Response) -> Response {
    mark_non_native(&mut response);
    response
}

fn mark_native(response: &mut Response) {
    response.headers_mut().insert(
        HeaderName::from_static(SEARCH_OWNERSHIP_HEADER),
        HeaderValue::from_static(NATIVE_OWNERSHIP_MARKER),
    );
}

fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let name = parts.next().unwrap_or_default();
        if name != key {
            return None;
        }
        Some(parts.next().unwrap_or_default())
    })
}

fn query_values<'a>(query: &'a str, key: &str) -> Vec<&'a str> {
    query
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let name = parts.next().unwrap_or_default();
            if name != key {
                return None;
            }
            Some(parts.next().unwrap_or_default())
        })
        .collect()
}

fn query_bool(query: &str, key: &str) -> bool {
    query_value(query, key)
        .map(|value| value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn query_has_key(query: &str, key: &str) -> bool {
    query
        .split('&')
        .any(|pair| pair.split('=').next().unwrap_or_default() == key)
}

fn parse_search_regex(value: &str) -> Option<(String, String)> {
    let mut parts = value.splitn(2, ',');
    let pattern = parts.next()?.trim();
    let field = parts.next()?.trim().to_ascii_lowercase();
    if pattern.is_empty() || (field != "title" && field != "title_sort") {
        return None;
    }
    Some((pattern.to_string(), field))
}

fn matches_search_pattern(candidate: &str, pattern: &str) -> bool {
    let text = candidate.to_ascii_lowercase();
    let mut expected = pattern.to_ascii_lowercase();

    let anchored_start = expected.starts_with('^');
    let anchored_end = expected.ends_with('$');
    if anchored_start {
        expected.remove(0);
    }
    if anchored_end {
        expected.pop();
    }

    if anchored_start && anchored_end {
        text == expected
    } else if anchored_start {
        text.starts_with(&expected)
    } else if anchored_end {
        text.ends_with(&expected)
    } else {
        text.contains(&expected)
    }
}

fn detail_access_denial_response(denial: DetailAccessDenial) -> Response {
    match denial {
        DetailAccessDenial::Unauthorized => StatusCode::UNAUTHORIZED.into_response(),
        DetailAccessDenial::Forbidden => StatusCode::FORBIDDEN.into_response(),
        DetailAccessDenial::NotFound => StatusCode::NOT_FOUND.into_response(),
    }
}

fn invalid_read_progress_payload() -> Response {
    (
        StatusCode::BAD_REQUEST,
        [(header::CONTENT_TYPE, "application/json")],
        Json(json!({
            "error": "invalid read progress payload",
        })),
    )
        .into_response()
}

fn invalid_progression_payload() -> Response {
    (
        StatusCode::BAD_REQUEST,
        [(header::CONTENT_TYPE, "application/json")],
        Json(json!({
            "error": "invalid progression payload",
        })),
    )
        .into_response()
}

fn method_not_allowed_json_response(path: &str) -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        [(header::CONTENT_TYPE, "application/json")],
        Json(json!({
            "error": "Method Not Allowed",
            "message": "Method 'GET' is not supported.",
            "path": path,
            "status": 405,
            "timestamp": "1970-01-01T00:00:00.000+00:00",
            "trace": "org.springframework.web.HttpRequestMethodNotSupportedException: Request method 'GET' is not supported",
        })),
    )
        .into_response()
}

fn set_read_progress(
    state: &ReadProgressState,
    token: String,
    book_id: String,
    page: u64,
    completed: bool,
) {
    let mut all_progress = state
        .progress_by_token
        .lock()
        .expect("read-progress state lock should not be poisoned");

    let user_progress = all_progress.entry(token).or_default();
    user_progress.insert(book_id, ReadProgress { page, completed });
}

fn overlay_book_read_progress(books: &mut Value, read_progress: Option<ReadProgress>) {
    if let Some(slot) = books.pointer_mut("/content/0/readProgress") {
        *slot = match read_progress {
            Some(read_progress) => json!({
                "page": read_progress.page,
                "completed": read_progress.completed,
            }),
            None => Value::Null,
        };
    }
}

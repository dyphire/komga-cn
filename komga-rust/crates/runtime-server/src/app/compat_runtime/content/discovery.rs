use axum::Json;
use axum::body::Bytes;
use axum::extract::Extension;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_application::discovery::{
    BooksLatestQuery, BooksListQuery, DiscoveryQueries, SeriesDetailQuery, SeriesListQuery,
};
use komga_domain::discovery::{
    DirectBrowseBooksListFamily, DiscoveryError, NonNativeRequestShape, PageEnvelope,
    SeriesReadModel,
};
use komga_persistence::discovery::{
    BookRow, SeriesRow, SqlxRuntimeDiscoveryAdapter, SqlxRuntimeDiscoveryStore,
};
use serde_json::{Value, json};

use crate::app::CompatProfile;
use crate::app::discovery_auth::DiscoveryAuthState;
use crate::app::placeholder_auth::{require_auth, resolved_auth_user, resolved_token};
use crate::app::snapshots::{books_latest_json, snapshot_json};

use super::super::ReadProgressState;
use super::content_java_live;
use super::helpers::{
    DiscoveryOwnershipRoute, DiscoveryShape, apply_non_native_diagnostics, books_page_payload,
    discovery_ownership_route, extract_full_text_search, mark_native, mark_non_native,
    matches_search_pattern, overlay_book_read_progress, parse_search_regex, query_bool,
    query_has_key, query_value, query_values, to_domain_query_context, wants_shadow_marker,
};

pub(in crate::app::compat_runtime) async fn series(
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
        let path = uri
            .path_and_query()
            .map_or(uri.path(), |value| value.as_str());
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

pub(in crate::app::compat_runtime) async fn series_list(
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
        .await
    {
        return native_response;
    }

    let mut response =
        Json(series_json_for_request(profile, &uri, full_text_search)).into_response();

    if wants_shadow_marker(&headers, payload.as_ref()) {
        mark_non_native(&mut response);
    }

    response
}

pub(in crate::app::compat_runtime) async fn books(
    Extension(profile): Extension<CompatProfile>,
    Extension(state): Extension<ReadProgressState>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let token = resolved_token(&headers);
    let mut books = if profile == CompatProfile::JavaLiveLocaldb {
        let user =
            resolved_auth_user(&headers).expect("authorized books request should resolve user");
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

pub(in crate::app::compat_runtime) async fn books_list(
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
    let is_exact_oneshot_bootstrap = exact_oneshot_bootstrap_series_id(payload.as_ref()).is_some();

    if (discovery_ownership_route(profile, &headers, DiscoveryShape::BooksList)
        == DiscoveryOwnershipRoute::NativeOwned
        || is_exact_oneshot_bootstrap)
        && let Some(native_response) = native_owned_books_list_response(
            &headers,
            &uri,
            payload.as_ref(),
            full_text_search.clone(),
            &auth_state,
        )
        .await
    {
        return native_response;
    }

    let mut response =
        Json(books_json_for_request(profile, &uri, full_text_search)).into_response();

    if wants_shadow_marker(&headers, payload.as_ref()) {
        mark_non_native(&mut response);
    }

    response
}

pub(in crate::app::compat_runtime) async fn books_latest(
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
        && let Some(native_response) =
            native_owned_books_latest_response(&headers, &uri, &auth_state).await
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
        let user = resolved_auth_user(&headers)
            .expect("authorized books-latest request should resolve user");
        let path = uri
            .path_and_query()
            .map_or(uri.path(), |value| value.as_str());
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

fn series_json_for_request(
    profile: CompatProfile,
    uri: &Uri,
    full_text_search: Option<String>,
) -> Value {
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

fn books_json_for_request(
    profile: CompatProfile,
    uri: &Uri,
    full_text_search: Option<String>,
) -> Value {
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

async fn native_owned_books_list_response(
    headers: &HeaderMap,
    uri: &Uri,
    payload: Option<&Value>,
    full_text_search: Option<String>,
    auth_state: &DiscoveryAuthState,
) -> Option<Response> {
    let query_string = uri.query().unwrap_or_default();
    let sorts = query_values(query_string, "sort")
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let page = query_value(query_string, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query_string, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query_string, "unpaged");
    let oneshot_bootstrap_series_id = exact_oneshot_bootstrap_series_id(payload);

    if oneshot_bootstrap_series_id.is_some() && !query_string.trim().is_empty() {
        return Some(non_native_books_list_response(
            DiscoveryError::NonNativeRequestShape(NonNativeRequestShape::UnsupportedBookFilter(
                "oneshot-bootstrap.query-params".to_string(),
            )),
            uri,
            full_text_search,
            payload,
        ));
    }

    let mut filters =
        match parse_native_books_filters(payload.and_then(|value| value.get("condition"))) {
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

    if let Some(series_id) = oneshot_bootstrap_series_id.clone() {
        filters.direct_browse_family = Some(DirectBrowseBooksListFamily::BrowseOneshotBootstrap);
        filters.series_ids = Some(vec![series_id]);
    }

    let is_admin = context.is_admin;
    let domain_context = to_domain_query_context(context);
    let fallback_search = full_text_search.clone();

    let result = with_seeded_books_discovery_queries(|queries| async move {
        if let Some(series_id) = oneshot_bootstrap_series_id.clone() {
            let visible_series = queries
                .get_series_detail(
                    &domain_context,
                    SeriesDetailQuery {
                        series_id: series_id.clone(),
                    },
                )
                .await?;

            if visible_series
                .as_ref()
                .map(|series| !series.oneshot)
                .unwrap_or(false)
            {
                return Err(DiscoveryError::NonNativeRequestShape(
                    NonNativeRequestShape::UnsupportedBookFilter(
                        "oneshot-bootstrap.series-not-oneshot".to_string(),
                    ),
                ));
            }

            let visible_books = queries
                .list_books(
                    &domain_context,
                    BooksListQuery {
                        page: 0,
                        size: 20,
                        unpaged: true,
                        direct_browse_family: None,
                        library_ids: None,
                        series_ids: Some(vec![series_id]),
                        deleted: None,
                        oneshot: None,
                        tags: None,
                        read_statuses: None,
                        media_profiles: None,
                        media_statuses: None,
                        authors: None,
                        release_dates: None,
                        sort: vec![],
                        search: None,
                    },
                )
                .await?;

            if visible_series.is_none() || visible_books.total_elements != 1 {
                return Err(DiscoveryError::NonNativeRequestShape(
                    NonNativeRequestShape::UnsupportedBookFilter(
                        "oneshot-bootstrap.visible-single-book".to_string(),
                    ),
                ));
            }
        }

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

        let page = if is_direct_browse {
            queries.list_books_direct_browse(&domain_context, query).await?
        } else {
            queries.list_books(&domain_context, query).await?
        };

        Ok(page)
    })
    .await;

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

async fn native_owned_books_latest_response(
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

    let is_admin = context.is_admin;
    let domain_context = to_domain_query_context(context);
    let query = BooksLatestQuery {
        page,
        size,
        unpaged,
        library_ids: None,
    };

    match with_seeded_books_discovery_queries(|queries| async move {
        queries.list_books_latest(&domain_context, query).await
    })
    .await
    {
        Ok(page) => {
            let mut response = Json(books_page_payload(page, is_admin, !unpaged)).into_response();
            mark_native(&mut response);
            Some(response)
        }
        Err(DiscoveryError::NonNativeRequestShape(details)) => Some(
            non_native_books_latest_response(DiscoveryError::NonNativeRequestShape(details), uri),
        ),
        Err(error) => Some(
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("native books latest failed: {error:?}") })),
            )
                .into_response(),
        ),
    }
}

async fn native_owned_series_list_response(
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

    let filters =
        match parse_native_series_filters(payload.and_then(|value| value.get("condition"))) {
            Ok(filters) => filters,
            Err(error) => {
                return Some(non_native_series_list_response(
                    error,
                    uri,
                    full_text_search,
                ));
            }
        };

    let context = match auth_state.resolve_query_context(headers, filters.library_ids.as_deref()) {
        Some(context) => context,
        None => return Some(StatusCode::UNAUTHORIZED.into_response()),
    };

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

    match with_seeded_series_discovery_queries(|queries| async move {
        queries.list_series(&domain_context, query).await
    })
    .await
    {
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
    let mut payload =
        series_json_for_request(CompatProfile::SnapshotAligned, uri, full_text_search);
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
    let mut payload = books_json_for_request(CompatProfile::SnapshotAligned, uri, full_text_search);
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

async fn with_seeded_books_discovery_queries<T, F, Fut>(
    operation: F,
) -> Result<T, DiscoveryError>
where
    F: FnOnce(DiscoveryQueries<SqlxRuntimeDiscoveryAdapter>) -> Fut,
    Fut: std::future::Future<Output = Result<T, DiscoveryError>>,
{
    let store = SqlxRuntimeDiscoveryStore::new("compat-runtime-books").await?;
    let seed_result = seed_books_discovery_data(&store).await;
    if let Err(error) = seed_result {
        store.cleanup().await;
        return Err(error);
    }

    let result = operation(DiscoveryQueries::new(store.adapter())).await;
    store.cleanup().await;
    result
}

async fn with_seeded_series_discovery_queries<T, F, Fut>(
    operation: F,
) -> Result<T, DiscoveryError>
where
    F: FnOnce(DiscoveryQueries<SqlxRuntimeDiscoveryAdapter>) -> Fut,
    Fut: std::future::Future<Output = Result<T, DiscoveryError>>,
{
    let store = SqlxRuntimeDiscoveryStore::new("compat-runtime-series").await?;
    let seed_result = seed_series_discovery_data(&store).await;
    if let Err(error) = seed_result {
        store.cleanup().await;
        return Err(error);
    }

    let result = operation(DiscoveryQueries::new(store.adapter())).await;
    store.cleanup().await;
    result
}

fn parse_native_series_filters(
    condition: Option<&Value>,
) -> Result<NativeSeriesFilters, DiscoveryError> {
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

fn parse_native_books_filters(
    condition: Option<&Value>,
) -> Result<NativeBooksFilters, DiscoveryError> {
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

fn exact_oneshot_bootstrap_series_id(payload: Option<&Value>) -> Option<String> {
    let payload = payload?.as_object()?;
    if payload.len() != 1 {
        return None;
    }

    let condition = payload.get("condition")?.as_object()?;
    if condition.len() != 3 {
        return None;
    }

    if condition.get("type").and_then(Value::as_str) != Some("SeriesId") {
        return None;
    }

    if !condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .eq_ignore_ascii_case("is")
    {
        return None;
    }

    condition
        .get("value")
        .and_then(Value::as_str)
        .map(str::to_string)
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

fn parse_books_media_profile_filter(
    condition: &Value,
) -> Result<NativeBooksFilters, DiscoveryError> {
    parse_books_string_filter(condition, "MediaProfile", "is", |value| {
        NativeBooksFilters {
            media_profiles: Some(vec![value]),
            ..NativeBooksFilters::default()
        }
    })
}

fn parse_books_media_status_filter(
    condition: &Value,
) -> Result<NativeBooksFilters, DiscoveryError> {
    parse_books_string_filter(condition, "MediaStatus", "is", |value| NativeBooksFilters {
        media_statuses: Some(vec![value]),
        ..NativeBooksFilters::default()
    })
}

fn parse_books_author_filter(condition: &Value) -> Result<NativeBooksFilters, DiscoveryError> {
    parse_books_string_filter(condition, "Author", "contains_or_is", |value| {
        NativeBooksFilters {
            authors: Some(vec![value]),
            ..NativeBooksFilters::default()
        }
    })
}

fn parse_books_release_date_filter(
    condition: &Value,
) -> Result<NativeBooksFilters, DiscoveryError> {
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

fn parse_books_composite_filters(
    condition: &Value,
    all_of: bool,
) -> Result<NativeBooksFilters, DiscoveryError> {
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

fn parse_series_read_status_filter(
    condition: &Value,
) -> Result<NativeSeriesFilters, DiscoveryError> {
    parse_series_string_filter(condition, "ReadStatus", "is", |value| NativeSeriesFilters {
        read_statuses: Some(vec![value]),
        ..NativeSeriesFilters::default()
    })
}

fn parse_series_genre_filter(condition: &Value) -> Result<NativeSeriesFilters, DiscoveryError> {
    parse_series_string_filter(condition, "Genre", "contains_or_is", |value| {
        NativeSeriesFilters {
            genres: Some(vec![value]),
            ..NativeSeriesFilters::default()
        }
    })
}

fn parse_series_tag_filter(condition: &Value) -> Result<NativeSeriesFilters, DiscoveryError> {
    parse_series_string_filter(condition, "Tag", "contains_or_is", |value| {
        NativeSeriesFilters {
            tags: Some(vec![value]),
            ..NativeSeriesFilters::default()
        }
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

fn parse_series_age_rating_filter(
    condition: &Value,
) -> Result<NativeSeriesFilters, DiscoveryError> {
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

fn parse_series_release_date_filter(
    condition: &Value,
) -> Result<NativeSeriesFilters, DiscoveryError> {
    parse_series_string_filter(condition, "ReleaseDate", "is", |value| {
        NativeSeriesFilters {
            release_dates: Some(vec![value]),
            ..NativeSeriesFilters::default()
        }
    })
}

fn parse_series_sharing_label_filter(
    condition: &Value,
) -> Result<NativeSeriesFilters, DiscoveryError> {
    parse_series_string_filter(condition, "SharingLabel", "contains_or_is", |value| {
        NativeSeriesFilters {
            sharing_labels: Some(vec![value]),
            ..NativeSeriesFilters::default()
        }
    })
}

fn parse_series_status_filter(condition: &Value) -> Result<NativeSeriesFilters, DiscoveryError> {
    parse_series_string_filter(condition, "SeriesStatus", "is", |value| {
        NativeSeriesFilters {
            series_statuses: Some(vec![value]),
            ..NativeSeriesFilters::default()
        }
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
    parse_series_string_filter(condition, "Author", "contains_or_is", |value| {
        NativeSeriesFilters {
            authors: Some(vec![value]),
            ..NativeSeriesFilters::default()
        }
    })
}

fn parse_composite_filters(
    condition: &Value,
    all_of: bool,
) -> Result<NativeSeriesFilters, DiscoveryError> {
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

fn merge_boolean_filter(
    left: Option<bool>,
    right: Option<bool>,
) -> Result<Option<bool>, DiscoveryError> {
    match (left, right) {
        (Some(a), Some(b)) if a == b => Ok(Some(a)),
        (Some(_), Some(_)) => Err(DiscoveryError::NonNativeRequestShape(
            NonNativeRequestShape::UnsupportedSeriesFilter(
                "composite boolean mismatch".to_string(),
            ),
        )),
        (Some(value), None) | (None, Some(value)) => Ok(Some(value)),
        (None, None) => Ok(None),
    }
}

async fn seed_series_discovery_data(store: &SqlxRuntimeDiscoveryStore) -> Result<(), DiscoveryError> {
    store
        .insert_series(
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
        )
        .await
}

async fn seed_books_discovery_data(store: &SqlxRuntimeDiscoveryStore) -> Result<(), DiscoveryError> {
    store
        .insert_series(SeriesRow::new("series-1", "1", "series").with_labels(["safe"]))
        .await?;
    store
        .insert_series(SeriesRow::new("series-2", "1", "restricted").with_labels(["adult"]))
        .await?;
    store
        .insert_series(
        SeriesRow::new("series-oneshot", "1", "oneshot")
            .with_labels(["safe"])
            .with_oneshot(true),
        )
        .await?;
    store
        .insert_series(
        SeriesRow::new("series-oneshot-multi", "1", "oneshot-multi")
            .with_labels(["safe"])
            .with_oneshot(true),
        )
        .await?;
    store
        .insert_series(
        SeriesRow::new("series-oneshot-restricted", "1", "oneshot-restricted")
            .with_labels(["adult"])
            .with_oneshot(true),
        )
        .await?;

    store
        .insert_book(
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
        )
        .await?;
    store
        .insert_book(
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
        )
        .await?;
    store
        .insert_book(
        BookRow::new("book-oneshot", "series-oneshot", "1", "oneshot-book.cbz")
            .with_url("/library1/oneshot-book.cbz")
            .with_last_modified("2024-02-01T03:04:05Z")
            .with_media("READY", "application/vnd.comicbook+zip", 1)
            .with_media_profile("PROFILE-ONESHOT")
            .with_number_sort(1)
            .with_read_status("UNREAD")
            .with_release_date("2024-02-01")
            .with_tags(["safe"])
            .with_authors(["alice"]),
        )
        .await?;
    store
        .insert_book(
        BookRow::new(
            "book-oneshot-multi-1",
            "series-oneshot-multi",
            "1",
            "oneshot-multi-1.cbz",
        )
        .with_url("/library1/oneshot-multi-1.cbz")
        .with_last_modified("2024-02-02T03:04:05Z")
        .with_media("READY", "application/vnd.comicbook+zip", 1)
        .with_media_profile("PROFILE-ONESHOT")
        .with_number_sort(1)
        .with_read_status("UNREAD")
        .with_release_date("2024-02-02")
        .with_tags(["safe"])
        .with_authors(["alice"]),
        )
        .await?;
    store
        .insert_book(
        BookRow::new(
            "book-oneshot-multi-2",
            "series-oneshot-multi",
            "1",
            "oneshot-multi-2.cbz",
        )
        .with_url("/library1/oneshot-multi-2.cbz")
        .with_last_modified("2024-02-03T03:04:05Z")
        .with_media("READY", "application/vnd.comicbook+zip", 1)
        .with_media_profile("PROFILE-ONESHOT")
        .with_number_sort(2)
        .with_read_status("UNREAD")
        .with_release_date("2024-02-03")
        .with_tags(["safe"])
        .with_authors(["alice"]),
        )
        .await?;
    store
        .insert_book(
        BookRow::new(
            "book-oneshot-restricted",
            "series-oneshot-restricted",
            "1",
            "oneshot-restricted.cbz",
        )
        .with_url("/library1/oneshot-restricted.cbz")
        .with_last_modified("2024-02-04T03:04:05Z")
        .with_media("READY", "application/vnd.comicbook+zip", 1)
        .with_media_profile("PROFILE-ONESHOT")
        .with_number_sort(1)
        .with_read_status("UNREAD")
        .with_release_date("2024-02-04")
        .with_tags(["adult"])
        .with_authors(["bob"]),
        )
        .await?;

    Ok(())
}

fn series_page_payload(page: PageEnvelope<SeriesReadModel>) -> Value {
    let content = page.content.iter().map(series_payload).collect::<Vec<_>>();
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
    let (page_content, number, response_size, total_pages, first, last, offset, paged) = if unpaged
    {
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

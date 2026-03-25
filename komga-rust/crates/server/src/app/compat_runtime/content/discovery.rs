use std::collections::{BTreeMap, BTreeSet};
use std::path::Path as FsPath;

use axum::Json;
use axum::body::Bytes;
use axum::extract::Extension;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_domain::discovery::{
    BookReadModel, DirectBrowseBooksListFamily, DiscoveryError, NonNativeRequestShape,
    PageEnvelope, SeriesReadModel,
};
use komga_persistence::sqlite::connect_pool;
use serde_json::{Value, json};
use sqlx::{QueryBuilder, Row, Sqlite};

use crate::app::CompatProfile;
use crate::app::discovery_auth::{DiscoveryAuthState, DiscoveryQueryContext};
use crate::app::placeholder_auth::{require_auth, resolved_auth_user, user_id};

use super::super::{AuthDatabaseState, ReadProgressState};
use super::helpers::{
    DiscoveryOwnershipRoute, DiscoveryShape, books_page_payload, discovery_ownership_route,
    extract_full_text_search, mark_native, mark_non_native, matches_search_pattern,
    parse_search_regex, query_bool, query_has_key, query_value, query_values, wants_shadow_marker,
};

pub(in crate::app::compat_runtime) async fn series(
    profile: CompatProfile,
    headers: HeaderMap,
    uri: Uri,
    auth_state: DiscoveryAuthState,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return removed_runtime_fallback_response(
            "GET /api/v1/series",
            "persisted/domain-backed data unavailable",
        );
    }

    let query = uri.query().unwrap_or_default();
    let requested_library_ids = requested_query_values(query, "library_id");
    let library_ids =
        remap_legacy_library_ids_for_persisted(database_file, requested_library_ids.as_ref())
            .await
            .or(requested_library_ids);
    let collection_ids = requested_query_values(query, "collection_id");
    let search = query_value(query, "search").map(decode_query_component);
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query, "unpaged");
    let search_regex = query_value(query, "search_regex").and_then(parse_search_regex);
    let has_author_filters = query_has_key(query, "authors") || query_has_key(query, "author");

    let context = match auth_state.resolve_query_context(&headers, library_ids.as_deref()) {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    let series_page = match load_persisted_series_page(
        database_file,
        &context,
        PersistedSeriesBrowseQuery {
            library_ids,
            collection_ids,
            search,
            search_regex,
            has_author_filters,
            page,
            size,
            unpaged,
            sort_mode: PersistedSeriesSortMode::TitleAsc,
        },
    )
    .await
    {
        Ok(page) => page,
        Err(error) => return internal_error_response(error),
    };

    let mut response = Json(series_page_payload(series_page)).into_response();
    if wants_shadow_marker(&headers, None) {
        mark_non_native(&mut response);
    } else if discovery_ownership_route(profile, &headers, DiscoveryShape::SeriesList)
        == DiscoveryOwnershipRoute::NativeOwned
    {
        mark_native(&mut response);
    }

    response
}

pub(in crate::app::compat_runtime) async fn series_latest(
    headers: HeaderMap,
    uri: Uri,
    auth_state: DiscoveryAuthState,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let query = uri.query().unwrap_or_default();
    let library_ids = requested_query_values(query, "library_id");
    let context = match auth_state.resolve_query_context(&headers, library_ids.as_deref()) {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query, "unpaged");

    match load_persisted_series_page(
        database_file,
        &context,
        PersistedSeriesBrowseQuery {
            library_ids,
            collection_ids: None,
            search: None,
            search_regex: None,
            has_author_filters: false,
            page,
            size,
            unpaged,
            sort_mode: PersistedSeriesSortMode::Latest,
        },
    )
    .await
    {
        Ok(page) => Json(series_page_payload(page)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn series_alphabetical_groups(
    headers: HeaderMap,
    body: Value,
    auth_state: DiscoveryAuthState,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let filters = match parse_native_series_filters(body.get("condition")) {
        Ok(filters) => filters,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("invalid series alphabetical-groups request: {error:?}") })),
            )
                .into_response();
        }
    };

    let filters = NativeSeriesFilters {
        library_ids: remap_legacy_library_ids_for_persisted(
            database_file,
            filters.library_ids.as_ref(),
        )
        .await,
        ..filters
    };

    let context = match auth_state.resolve_query_context(&headers, None) {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    match load_persisted_alphabetical_groups(database_file, &context, filters.library_ids).await {
        Ok(groups) => Json(Value::Array(groups)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn series_list(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_db): Extension<super::super::AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let ownership_route = discovery_ownership_route(profile, &headers, DiscoveryShape::SeriesList);
    let payload = serde_json::from_slice::<Value>(&body).ok();
    let full_text_search = payload.as_ref().and_then(extract_full_text_search);

    if auth_db.database_file.exists()
        && let Some(mut native_response) = native_owned_series_list_response(
            &headers,
            &uri,
            payload.as_ref(),
            full_text_search.clone(),
            &auth_state,
            auth_db.database_file.as_path(),
            ownership_route == DiscoveryOwnershipRoute::NativeOwned,
        )
        .await
    {
        if ownership_route != DiscoveryOwnershipRoute::NativeOwned {
            native_response
                .headers_mut()
                .remove("x-komga-compat-search-ownership");
        }
        return native_response;
    }

    let _ = profile;
    let _ = full_text_search;

    removed_runtime_fallback_response(
        "POST /api/v1/series/list",
        "persisted/domain-backed data unavailable",
    )
}

pub(in crate::app::compat_runtime) async fn books(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_db): Extension<super::super::AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(state): Extension<ReadProgressState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if auth_db.database_file.exists() {
        let query = uri.query().unwrap_or_default();
        let requested_library_ids = requested_query_values(query, "library_id");
        let library_ids = remap_legacy_library_ids_for_persisted(
            auth_db.database_file.as_path(),
            requested_library_ids.as_ref(),
        )
        .await
        .or(requested_library_ids);

        let context = match auth_state.resolve_query_context(&headers, library_ids.as_deref()) {
            Some(context) => context,
            None => return StatusCode::UNAUTHORIZED.into_response(),
        };

        let page = query_value(query, "page")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        let size = query_value(query, "size")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(20)
            .max(1);
        let unpaged = query_bool(query, "unpaged");
        let sort_values = query_values(query, "sort")
            .into_iter()
            .map(decode_query_component)
            .collect::<Vec<_>>();
        let search = query_value(query, "search").map(decode_query_component);

        if let Some(sort_mode) = parse_persisted_books_sort_mode(&sort_values) {
            match load_persisted_books_page(
                auth_db.database_file.as_path(),
                &context,
                PersistedBooksBrowseQuery {
                    library_ids,
                    search,
                    page,
                    size,
                    unpaged,
                    sort_mode,
                },
            )
            .await
            {
                Ok(page) => {
                    return Json(books_page_payload(page, context.is_admin, !unpaged))
                        .into_response();
                }
                Err(error) => return internal_error_response(error),
            }
        }
    }

    let _ = profile;
    let _ = state;

    removed_runtime_fallback_response(
        "GET /api/v1/books",
        "persisted/domain-backed data unavailable",
    )
}

pub(in crate::app::compat_runtime) async fn books_list(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_db): Extension<super::super::AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let ownership_route = discovery_ownership_route(profile, &headers, DiscoveryShape::BooksList);
    let payload = serde_json::from_slice::<Value>(&body).ok();
    let full_text_search = payload.as_ref().and_then(extract_full_text_search);
    let is_exact_oneshot_bootstrap = exact_oneshot_bootstrap_series_id(payload.as_ref()).is_some();

    if (auth_db.database_file.exists()
        || ownership_route == DiscoveryOwnershipRoute::NativeOwned
        || is_exact_oneshot_bootstrap)
        && let Some(mut native_response) = native_owned_books_list_response(
            &headers,
            &uri,
            payload.as_ref(),
            full_text_search.clone(),
            &auth_state,
            auth_db.database_file.as_path(),
            ownership_route == DiscoveryOwnershipRoute::NativeOwned,
        )
        .await
    {
        if ownership_route != DiscoveryOwnershipRoute::NativeOwned {
            native_response
                .headers_mut()
                .remove("x-komga-compat-search-ownership");
        }
        return native_response;
    }

    let _ = profile;
    let _ = full_text_search;

    removed_runtime_fallback_response(
        "POST /api/v1/books/list",
        "persisted/domain-backed data unavailable",
    )
}

pub(in crate::app::compat_runtime) async fn books_latest(
    Extension(profile): Extension<CompatProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    Extension(state): Extension<ReadProgressState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let ownership_route = discovery_ownership_route(profile, &headers, DiscoveryShape::BooksLatest);
    let query = uri.query().unwrap_or_default();
    let sorts = query_values(query, "sort");

    if auth_db.database_file.exists() && sorts.is_empty() {
        let requested_library_ids = requested_query_values(query, "library_id");
        let library_ids = remap_legacy_library_ids_for_persisted(
            auth_db.database_file.as_path(),
            requested_library_ids.as_ref(),
        )
        .await
        .or(requested_library_ids);

        let context = match auth_state.resolve_query_context(&headers, library_ids.as_deref()) {
            Some(context) => context,
            None => return StatusCode::UNAUTHORIZED.into_response(),
        };

        let page = query_value(query, "page")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        let size = query_value(query, "size")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(20)
            .max(1);
        let unpaged = query_bool(query, "unpaged");

        match load_persisted_books_page(
            auth_db.database_file.as_path(),
            &context,
            PersistedBooksBrowseQuery {
                library_ids,
                search: None,
                page,
                size,
                unpaged,
                sort_mode: PersistedBooksSortMode::LastModifiedDateDesc,
            },
        )
        .await
        {
            Ok(page) => {
                return Json(books_page_payload(page, context.is_admin, !unpaged)).into_response();
            }
            Err(error) => return internal_error_response(error),
        }
    }

    if (auth_db.database_file.exists() || ownership_route == DiscoveryOwnershipRoute::NativeOwned)
        && let Some(mut native_response) = native_owned_books_latest_response(
            &headers,
            &uri,
            &auth_state,
            auth_db.database_file.as_path(),
        )
        .await
    {
        if ownership_route != DiscoveryOwnershipRoute::NativeOwned {
            native_response
                .headers_mut()
                .remove("x-komga-compat-search-ownership");
        }
        return native_response;
    }

    let _ = profile;
    let _ = state;

    removed_runtime_fallback_response(
        "GET /api/v1/books/latest",
        "persisted/domain-backed data unavailable",
    )
}

pub(in crate::app::compat_runtime) async fn books_ondeck(
    Extension(auth_db): Extension<super::super::AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !auth_db.database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let user_id = user_id(&user).to_string();

    match load_persisted_ondeck_books(auth_db.database_file.as_path(), &user_id).await {
        Ok(entries) => {
            let mut response = Json(books_page_for_entries(entries, &uri)).into_response();
            mark_native(&mut response);
            response
        }
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn books_duplicates(
    Extension(auth_db): Extension<super::super::AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !auth_db.database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    match load_persisted_duplicate_books(auth_db.database_file.as_path()).await {
        Ok(entries) => {
            let mut response = Json(books_page_for_entries(entries, &uri)).into_response();
            mark_native(&mut response);
            response
        }
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn authors(
    headers: HeaderMap,
    uri: Uri,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return Json(json!([])).into_response();
    }

    let library_id = query_value(uri.query().unwrap_or_default(), "library_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);

    match load_persisted_authors(database_file, library_id.as_deref()).await {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn authors_names(
    headers: HeaderMap,
    uri: Uri,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return Json(json!([])).into_response();
    }

    let search = query_value(uri.query().unwrap_or_default(), "search")
        .map(decode_query_component)
        .unwrap_or_default();

    match load_persisted_author_names(database_file, &search).await {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn authors_roles(
    headers: HeaderMap,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return Json(json!([])).into_response();
    }

    match load_persisted_author_roles(database_file).await {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn authors_v2(
    headers: HeaderMap,
    uri: Uri,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return Json(authors_v2_page_payload(vec![], 0, 20, false)).into_response();
    }

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

    let mut authors = match load_persisted_authors_by_scope(database_file, &scope).await {
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

pub(in crate::app::compat_runtime) async fn genres(
    headers: HeaderMap,
    uri: Uri,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return Json(json!([])).into_response();
    }

    let library_id = query_value(uri.query().unwrap_or_default(), "library_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);

    match load_persisted_genres(database_file, library_id.as_deref()).await {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn tags(
    headers: HeaderMap,
    uri: Uri,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return Json(json!([])).into_response();
    }

    let library_id = query_value(uri.query().unwrap_or_default(), "library_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);

    match load_persisted_tags(database_file, library_id.as_deref()).await {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn series_tags(
    headers: HeaderMap,
    uri: Uri,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return Json(json!([])).into_response();
    }

    let query = uri.query().unwrap_or_default();
    let library_id = query_value(query, "library_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);
    let collection_id = query_value(query, "collection_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);

    match load_persisted_series_tags(
        database_file,
        library_id.as_deref(),
        collection_id.as_deref(),
    )
    .await
    {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn languages(
    headers: HeaderMap,
    uri: Uri,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return Json(json!([])).into_response();
    }

    let library_id = query_value(uri.query().unwrap_or_default(), "library_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);

    match load_persisted_languages(database_file, library_id.as_deref()).await {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn publishers(
    headers: HeaderMap,
    uri: Uri,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return Json(json!([])).into_response();
    }

    let library_id = query_value(uri.query().unwrap_or_default(), "library_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);

    match load_persisted_publishers(database_file, library_id.as_deref()).await {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn age_ratings(
    headers: HeaderMap,
    uri: Uri,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return Json(json!([])).into_response();
    }

    let library_id = query_value(uri.query().unwrap_or_default(), "library_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);

    match load_persisted_age_ratings(database_file, library_id.as_deref()).await {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn sharing_labels(
    headers: HeaderMap,
    uri: Uri,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return Json(json!([])).into_response();
    }

    let library_id = query_value(uri.query().unwrap_or_default(), "library_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);

    match load_persisted_sharing_labels(database_file, library_id.as_deref()).await {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn series_release_dates(
    headers: HeaderMap,
    uri: Uri,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return Json(json!([])).into_response();
    }

    let library_id = query_value(uri.query().unwrap_or_default(), "library_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);

    match load_persisted_series_release_dates(database_file, library_id.as_deref()).await {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn series_new(
    headers: HeaderMap,
    uri: Uri,
    auth_state: DiscoveryAuthState,
    database_file: &FsPath,
) -> Response {
    series_latest(headers, uri, auth_state, database_file).await
}

pub(in crate::app::compat_runtime) async fn series_updated(
    headers: HeaderMap,
    uri: Uri,
    auth_state: DiscoveryAuthState,
    database_file: &FsPath,
) -> Response {
    series_latest(headers, uri, auth_state, database_file).await
}

pub(in crate::app::compat_runtime) async fn book_tags(
    Extension(auth_db): Extension<super::super::AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !auth_db.database_file.exists() {
        return Json(json!([])).into_response();
    }

    let query = uri.query().unwrap_or_default();
    let scope = query_value(query, "series_id")
        .filter(|value| !value.is_empty())
        .map(|value| PersistedBookTagsScope::Series(decode_query_component(value)))
        .or_else(|| {
            query_value(query, "library_id")
                .filter(|value| !value.is_empty())
                .map(|value| PersistedBookTagsScope::Library(decode_query_component(value)))
        });

    match load_persisted_book_tags(auth_db.database_file.as_path(), scope.as_ref()).await {
        Ok(tags) => Json(json!(tags)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

#[derive(Clone)]
struct PersistedBookBrowseEntry {
    id: String,
    library_id: String,
    name: String,
    title: String,
}

enum PersistedBookTagsScope {
    Series(String),
    Library(String),
}

enum PersistedAuthorsScope {
    All,
    Libraries(Vec<String>),
    Collection(String),
    Series(String),
    ReadList(String),
}

#[derive(Clone, serde::Serialize)]
struct PersistedAuthorEntry {
    name: String,
    role: String,
}

fn books_page_for_entries(entries: Vec<PersistedBookBrowseEntry>, uri: &Uri) -> Value {
    let query = uri.query().unwrap_or_default();
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let requested_size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query, "unpaged");

    let total_elements = entries.len();
    let page_size = if unpaged {
        total_elements.max(1)
    } else {
        requested_size
    };
    let offset = if unpaged {
        0
    } else {
        page.saturating_mul(page_size)
    };

    let page_entries = if unpaged {
        entries
    } else if offset >= total_elements {
        vec![]
    } else {
        entries.into_iter().skip(offset).take(page_size).collect()
    };

    let content = page_entries
        .into_iter()
        .map(|entry| {
            json!({
                "id": entry.id,
                "libraryId": entry.library_id,
                "name": entry.name,
                "metadata": {
                    "title": entry.title,
                },
            })
        })
        .collect::<Vec<_>>();

    let number_of_elements = content.len();
    let total_pages = if total_elements == 0 {
        0
    } else if unpaged {
        1
    } else {
        total_elements.div_ceil(page_size)
    };
    let number = if unpaged { 0 } else { page };
    let first = number == 0;
    let last = total_pages == 0 || number + 1 >= total_pages;
    let empty = number_of_elements == 0;

    json!({
        "content": content,
        "number": number,
        "size": page_size,
        "first": first,
        "last": last,
        "empty": empty,
        "numberOfElements": number_of_elements,
        "totalElements": total_elements,
        "totalPages": total_pages,
        "sort": {
            "empty": true,
            "sorted": false,
            "unsorted": true,
        },
        "pageable": {
            "pageNumber": number,
            "pageSize": page_size,
            "offset": if unpaged { 0 } else { offset },
            "sort": {
                "empty": true,
                "sorted": false,
                "unsorted": true,
            },
            "paged": !unpaged,
            "unpaged": unpaged,
        },
    })
}

async fn load_persisted_ondeck_books(
    database_file: &FsPath,
    user_id: &str,
) -> Result<Vec<PersistedBookBrowseEntry>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open books ondeck db: {error}"))?;

    let rows = sqlx::query(
        "SELECT b.ID, b.LIBRARY_ID, b.NAME, COALESCE(bm.TITLE, b.NAME) AS TITLE, b.SERIES_ID, b.NUMBER FROM BOOK b JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID WHERE b.SERIES_ID IN (SELECT DISTINCT b_done.SERIES_ID FROM BOOK b_done JOIN READ_PROGRESS rp_done ON rp_done.BOOK_ID = b_done.ID WHERE rp_done.USER_ID = ? AND rp_done.COMPLETED = 1) AND b.SERIES_ID NOT IN (SELECT DISTINCT b_prog.SERIES_ID FROM BOOK b_prog JOIN READ_PROGRESS rp_prog ON rp_prog.BOOK_ID = b_prog.ID WHERE rp_prog.USER_ID = ? AND rp_prog.COMPLETED = 0) AND NOT EXISTS (SELECT 1 FROM READ_PROGRESS rp_seen WHERE rp_seen.BOOK_ID = b.ID AND rp_seen.USER_ID = ? AND rp_seen.COMPLETED = 1) ORDER BY b.SERIES_ID ASC, b.NUMBER ASC",
    )
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted books ondeck: {error}"))?;

    let mut first_per_series = BTreeMap::<String, PersistedBookBrowseEntry>::new();
    for row in rows {
        let series_id = row.get::<String, _>("SERIES_ID");
        first_per_series
            .entry(series_id)
            .or_insert_with(|| PersistedBookBrowseEntry {
                id: row.get::<String, _>("ID"),
                library_id: row.get::<String, _>("LIBRARY_ID"),
                name: row.get::<String, _>("NAME"),
                title: row.get::<String, _>("TITLE"),
            });
    }

    pool.close().await;
    Ok(first_per_series.into_values().collect())
}

async fn load_persisted_duplicate_books(
    database_file: &FsPath,
) -> Result<Vec<PersistedBookBrowseEntry>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open books duplicates db: {error}"))?;

    let rows = sqlx::query(
        "SELECT b.ID, b.LIBRARY_ID, b.NAME, COALESCE(bm.TITLE, b.NAME) AS TITLE FROM BOOK b JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID WHERE b.FILE_HASH IS NOT NULL AND TRIM(b.FILE_HASH) != '' AND b.FILE_HASH IN (SELECT FILE_HASH FROM BOOK WHERE FILE_HASH IS NOT NULL AND TRIM(FILE_HASH) != '' GROUP BY FILE_HASH HAVING COUNT(*) > 1) ORDER BY b.FILE_HASH ASC, b.NUMBER ASC, b.ID ASC",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted books duplicates: {error}"))?;

    let entries = rows
        .into_iter()
        .map(|row| PersistedBookBrowseEntry {
            id: row.get::<String, _>("ID"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            name: row.get::<String, _>("NAME"),
            title: row.get::<String, _>("TITLE"),
        })
        .collect::<Vec<_>>();

    pool.close().await;
    Ok(entries)
}

async fn load_persisted_book_tags(
    database_file: &FsPath,
    scope: Option<&PersistedBookTagsScope>,
) -> Result<Vec<String>, String> {
    let Some(scope) = scope else {
        return Ok(vec![]);
    };

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book tags db: {error}"))?;

    let rows = match scope {
        PersistedBookTagsScope::Series(series_id) => {
            sqlx::query(
                "SELECT bt.TAG
                 FROM BOOK_METADATA_TAG bt
                 JOIN BOOK b ON b.ID = bt.BOOK_ID
                 WHERE b.SERIES_ID = ?
                 ORDER BY lower(bt.TAG), bt.TAG, b.ID",
            )
            .bind(series_id)
            .fetch_all(&pool)
            .await
        }
        PersistedBookTagsScope::Library(library_id) => {
            sqlx::query(
                "SELECT bt.TAG
                 FROM BOOK_METADATA_TAG bt
                 JOIN BOOK b ON b.ID = bt.BOOK_ID
                 WHERE b.LIBRARY_ID = ?
                 ORDER BY lower(bt.TAG), bt.TAG, b.ID",
            )
            .bind(library_id)
            .fetch_all(&pool)
            .await
        }
    }
    .map_err(|error| format!("query persisted book tags: {error}"))?;

    let mut tags = Vec::with_capacity(rows.len());
    let mut seen = BTreeSet::new();
    for row in rows {
        let tag = row.get::<String, _>("TAG");
        if seen.insert(tag.clone()) {
            tags.push(tag);
        }
    }

    pool.close().await;
    Ok(tags)
}

async fn load_persisted_authors(
    database_file: &FsPath,
    library_id: Option<&str>,
) -> Result<Vec<PersistedAuthorEntry>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open authors db: {error}"))?;

    let rows = if let Some(library_id) = library_id {
        sqlx::query(
            "SELECT a.NAME, a.ROLE
             FROM BOOK_METADATA_AUTHOR a
             JOIN BOOK b ON b.ID = a.BOOK_ID
             WHERE b.LIBRARY_ID = ?
             ORDER BY lower(a.NAME), lower(a.ROLE), a.NAME, a.ROLE, b.ID",
        )
        .bind(library_id)
        .fetch_all(&pool)
        .await
    } else {
        sqlx::query(
            "SELECT a.NAME, a.ROLE
             FROM BOOK_METADATA_AUTHOR a
             JOIN BOOK b ON b.ID = a.BOOK_ID
             ORDER BY lower(a.NAME), lower(a.ROLE), a.NAME, a.ROLE, b.ID",
        )
        .fetch_all(&pool)
        .await
    }
    .map_err(|error| format!("query persisted authors: {error}"))?;

    let mut authors = Vec::with_capacity(rows.len());
    let mut seen = BTreeSet::new();
    for row in rows {
        let name = row.get::<String, _>("NAME");
        let role = row.get::<String, _>("ROLE");
        if seen.insert((name.clone(), role.clone())) {
            authors.push(PersistedAuthorEntry { name, role });
        }
    }

    pool.close().await;
    Ok(authors)
}

async fn load_persisted_author_names(
    database_file: &FsPath,
    search: &str,
) -> Result<Vec<String>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open author names db: {error}"))?;

    let rows = sqlx::query(
        "SELECT DISTINCT a.NAME
         FROM BOOK_METADATA_AUTHOR a
         JOIN BOOK b ON b.ID = a.BOOK_ID
         ORDER BY lower(a.NAME), a.NAME",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted author names: {error}"))?;

    let search = search.to_ascii_lowercase();
    let names = rows
        .into_iter()
        .map(|row| row.get::<String, _>("NAME"))
        .filter(|name| {
            if search.is_empty() {
                true
            } else {
                name.to_ascii_lowercase().contains(&search)
            }
        })
        .collect();

    pool.close().await;
    Ok(names)
}

async fn load_persisted_author_roles(database_file: &FsPath) -> Result<Vec<String>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open author roles db: {error}"))?;

    let rows = sqlx::query(
        "SELECT DISTINCT ROLE
         FROM BOOK_METADATA_AUTHOR
         ORDER BY lower(ROLE), ROLE",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted author roles: {error}"))?;

    let roles = rows
        .into_iter()
        .map(|row| row.get::<String, _>("ROLE"))
        .collect();

    pool.close().await;
    Ok(roles)
}

async fn load_persisted_authors_by_scope(
    database_file: &FsPath,
    scope: &PersistedAuthorsScope,
) -> Result<Vec<PersistedAuthorEntry>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open v2 authors db: {error}"))?;

    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT a.NAME, a.ROLE FROM BOOK_METADATA_AUTHOR a JOIN BOOK b ON b.ID = a.BOOK_ID",
    );

    match scope {
        PersistedAuthorsScope::All => {}
        PersistedAuthorsScope::Libraries(library_ids) => {
            query.push(" WHERE b.LIBRARY_ID IN (");
            let mut separated = query.separated(",");
            for library_id in library_ids {
                separated.push_bind(library_id);
            }
            separated.push_unseparated(")");
        }
        PersistedAuthorsScope::Collection(collection_id) => {
            query.push(" JOIN COLLECTION_SERIES cs ON cs.SERIES_ID = b.SERIES_ID WHERE cs.COLLECTION_ID = ");
            query.push_bind(collection_id);
        }
        PersistedAuthorsScope::Series(series_id) => {
            query.push(" WHERE b.SERIES_ID = ");
            query.push_bind(series_id);
        }
        PersistedAuthorsScope::ReadList(readlist_id) => {
            query.push(" JOIN READLIST_BOOK rb ON rb.BOOK_ID = b.ID WHERE rb.READLIST_ID = ");
            query.push_bind(readlist_id);
        }
    }

    query.push(" ORDER BY lower(a.NAME), lower(a.ROLE), a.NAME, a.ROLE, b.ID");

    let rows = query
        .build()
        .fetch_all(&pool)
        .await
        .map_err(|error| format!("query persisted v2 authors: {error}"))?;

    let mut authors = Vec::with_capacity(rows.len());
    let mut seen = BTreeSet::new();
    for row in rows {
        let name = row.get::<String, _>("NAME");
        let role = row.get::<String, _>("ROLE");
        if seen.insert((name.clone(), role.clone())) {
            authors.push(PersistedAuthorEntry { name, role });
        }
    }

    pool.close().await;
    Ok(authors)
}

async fn load_persisted_series_tags(
    database_file: &FsPath,
    library_id: Option<&str>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series tags db: {error}"))?;

    let rows = if let Some(library_id) = library_id {
        sqlx::query(
            "SELECT DISTINCT st.TAG
             FROM SERIES_METADATA_TAG st
             JOIN SERIES s ON s.ID = st.SERIES_ID
             WHERE s.LIBRARY_ID = ?
             ORDER BY lower(st.TAG), st.TAG",
        )
        .bind(library_id)
        .fetch_all(&pool)
        .await
    } else if let Some(collection_id) = collection_id {
        sqlx::query(
            "SELECT DISTINCT st.TAG
             FROM SERIES_METADATA_TAG st
             JOIN COLLECTION_SERIES cs ON cs.SERIES_ID = st.SERIES_ID
             WHERE cs.COLLECTION_ID = ?
             ORDER BY lower(st.TAG), st.TAG",
        )
        .bind(collection_id)
        .fetch_all(&pool)
        .await
    } else {
        sqlx::query(
            "SELECT DISTINCT TAG
             FROM SERIES_METADATA_TAG
             ORDER BY lower(TAG), TAG",
        )
        .fetch_all(&pool)
        .await
    }
    .map_err(|error| format!("query persisted series tags: {error}"))?;

    let tags = rows
        .into_iter()
        .map(|row| row.get::<String, _>("TAG"))
        .collect();

    pool.close().await;
    Ok(tags)
}

fn authors_v2_page_payload(
    authors: Vec<PersistedAuthorEntry>,
    page: usize,
    size: usize,
    unpaged: bool,
) -> Value {
    let total_elements = authors.len();
    let page_size = if unpaged {
        total_elements.max(1)
    } else {
        size.max(1)
    };
    let offset = if unpaged {
        0
    } else {
        page.saturating_mul(page_size)
    };

    let content = if unpaged {
        authors
    } else if offset >= total_elements {
        vec![]
    } else {
        authors.into_iter().skip(offset).take(page_size).collect()
    };

    let total_pages = if total_elements == 0 {
        0
    } else if unpaged {
        1
    } else {
        total_elements.div_ceil(page_size)
    };
    let number = if unpaged { 0 } else { page };
    let number_of_elements = content.len();
    let first = number == 0;
    let last = total_pages == 0 || number + 1 >= total_pages;

    json!({
        "content": content,
        "number": number,
        "size": page_size,
        "first": first,
        "last": last,
        "empty": number_of_elements == 0,
        "numberOfElements": number_of_elements,
        "totalElements": total_elements,
        "totalPages": total_pages,
        "sort": {
            "empty": true,
            "sorted": false,
            "unsorted": true,
        },
        "pageable": {
            "pageNumber": number,
            "pageSize": page_size,
            "offset": if unpaged { 0 } else { offset },
            "sort": {
                "empty": true,
                "sorted": false,
                "unsorted": true,
            },
            "paged": !unpaged,
            "unpaged": unpaged,
        },
    })
}

async fn load_persisted_genres(
    database_file: &FsPath,
    library_id: Option<&str>,
) -> Result<Vec<String>, String> {
    load_persisted_library_strings(
        database_file,
        library_id,
        "genres",
        "SELECT g.GENRE AS VALUE
         FROM SERIES_METADATA_GENRE g
         JOIN SERIES s ON s.ID = g.SERIES_ID
         WHERE s.LIBRARY_ID = ?
         ORDER BY lower(g.GENRE), g.GENRE, s.ID",
        "SELECT g.GENRE AS VALUE
         FROM SERIES_METADATA_GENRE g
         JOIN SERIES s ON s.ID = g.SERIES_ID
         ORDER BY lower(g.GENRE), g.GENRE, s.ID",
    )
    .await
}

async fn load_persisted_tags(
    database_file: &FsPath,
    library_id: Option<&str>,
) -> Result<Vec<String>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open tags db: {error}"))?;

    let rows = if let Some(library_id) = library_id {
        sqlx::query(
            "SELECT TAG FROM (
                 SELECT st.TAG AS TAG
                 FROM SERIES_METADATA_TAG st
                 JOIN SERIES s ON s.ID = st.SERIES_ID
                 WHERE s.LIBRARY_ID = ?
                 UNION
                 SELECT bt.TAG AS TAG
                 FROM BOOK_METADATA_TAG bt
                 JOIN BOOK b ON b.ID = bt.BOOK_ID
                 WHERE b.LIBRARY_ID = ?
             )
             ORDER BY lower(TAG), TAG",
        )
        .bind(library_id)
        .bind(library_id)
        .fetch_all(&pool)
        .await
    } else {
        sqlx::query(
            "SELECT TAG FROM (
                 SELECT st.TAG AS TAG
                 FROM SERIES_METADATA_TAG st
                 JOIN SERIES s ON s.ID = st.SERIES_ID
                 UNION
                 SELECT bt.TAG AS TAG
                 FROM BOOK_METADATA_TAG bt
                 JOIN BOOK b ON b.ID = bt.BOOK_ID
             )
             ORDER BY lower(TAG), TAG",
        )
        .fetch_all(&pool)
        .await
    }
    .map_err(|error| format!("query persisted tags: {error}"))?;

    let tags = rows
        .into_iter()
        .map(|row| row.get::<String, _>("TAG"))
        .collect();

    pool.close().await;
    Ok(tags)
}

async fn load_persisted_languages(
    database_file: &FsPath,
    library_id: Option<&str>,
) -> Result<Vec<String>, String> {
    load_persisted_library_strings(
        database_file,
        library_id,
        "languages",
        "SELECT DISTINCT sm.LANGUAGE AS VALUE
         FROM SERIES_METADATA sm
         JOIN SERIES s ON s.ID = sm.SERIES_ID
         WHERE s.LIBRARY_ID = ?
         ORDER BY lower(sm.LANGUAGE), sm.LANGUAGE",
        "SELECT DISTINCT sm.LANGUAGE AS VALUE
         FROM SERIES_METADATA sm
         JOIN SERIES s ON s.ID = sm.SERIES_ID
         ORDER BY lower(sm.LANGUAGE), sm.LANGUAGE",
    )
    .await
}

async fn load_persisted_publishers(
    database_file: &FsPath,
    library_id: Option<&str>,
) -> Result<Vec<String>, String> {
    load_persisted_library_strings(
        database_file,
        library_id,
        "publishers",
        "SELECT DISTINCT sm.PUBLISHER AS VALUE
         FROM SERIES_METADATA sm
         JOIN SERIES s ON s.ID = sm.SERIES_ID
         WHERE s.LIBRARY_ID = ?
         ORDER BY lower(sm.PUBLISHER), sm.PUBLISHER",
        "SELECT DISTINCT sm.PUBLISHER AS VALUE
         FROM SERIES_METADATA sm
         JOIN SERIES s ON s.ID = sm.SERIES_ID
         ORDER BY lower(sm.PUBLISHER), sm.PUBLISHER",
    )
    .await
}

async fn load_persisted_age_ratings(
    database_file: &FsPath,
    library_id: Option<&str>,
) -> Result<Vec<u16>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open age-ratings db: {error}"))?;

    let rows = if let Some(library_id) = library_id {
        sqlx::query(
            "SELECT DISTINCT sm.AGE_RATING AS VALUE
             FROM SERIES_METADATA sm
             JOIN SERIES s ON s.ID = sm.SERIES_ID
             WHERE s.LIBRARY_ID = ? AND sm.AGE_RATING IS NOT NULL
             ORDER BY sm.AGE_RATING",
        )
        .bind(library_id)
        .fetch_all(&pool)
        .await
    } else {
        sqlx::query(
            "SELECT DISTINCT sm.AGE_RATING AS VALUE
             FROM SERIES_METADATA sm
             JOIN SERIES s ON s.ID = sm.SERIES_ID
             WHERE sm.AGE_RATING IS NOT NULL
             ORDER BY sm.AGE_RATING",
        )
        .fetch_all(&pool)
        .await
    }
    .map_err(|error| format!("query persisted age-ratings: {error}"))?;

    let values = rows
        .into_iter()
        .filter_map(|row| row.get::<Option<i64>, _>("VALUE"))
        .map(|value| value.max(0) as u16)
        .collect();

    pool.close().await;
    Ok(values)
}

async fn load_persisted_sharing_labels(
    database_file: &FsPath,
    library_id: Option<&str>,
) -> Result<Vec<String>, String> {
    load_persisted_library_strings(
        database_file,
        library_id,
        "sharing-labels",
        "SELECT DISTINCT sms.LABEL AS VALUE
         FROM SERIES_METADATA_SHARING sms
         JOIN SERIES s ON s.ID = sms.SERIES_ID
         WHERE s.LIBRARY_ID = ?
         ORDER BY lower(sms.LABEL), sms.LABEL",
        "SELECT DISTINCT sms.LABEL AS VALUE
         FROM SERIES_METADATA_SHARING sms
         JOIN SERIES s ON s.ID = sms.SERIES_ID
         ORDER BY lower(sms.LABEL), sms.LABEL",
    )
    .await
}

async fn load_persisted_series_release_dates(
    database_file: &FsPath,
    library_id: Option<&str>,
) -> Result<Vec<String>, String> {
    load_persisted_library_strings(
        database_file,
        library_id,
        "series-release-dates",
        "SELECT DISTINCT bm.RELEASE_DATE AS VALUE
         FROM BOOK_METADATA bm
         JOIN BOOK b ON b.ID = bm.BOOK_ID
         WHERE b.LIBRARY_ID = ? AND bm.RELEASE_DATE IS NOT NULL AND bm.RELEASE_DATE <> ''
         ORDER BY bm.RELEASE_DATE",
        "SELECT DISTINCT bm.RELEASE_DATE AS VALUE
         FROM BOOK_METADATA bm
         JOIN BOOK b ON b.ID = bm.BOOK_ID
         WHERE bm.RELEASE_DATE IS NOT NULL AND bm.RELEASE_DATE <> ''
         ORDER BY bm.RELEASE_DATE",
    )
    .await
}

async fn load_persisted_library_strings(
    database_file: &FsPath,
    library_id: Option<&str>,
    label: &str,
    sql: &str,
    sql_all: &str,
) -> Result<Vec<String>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open {label} db: {error}"))?;

    let rows = if let Some(library_id) = library_id {
        sqlx::query(sql).bind(library_id).fetch_all(&pool).await
    } else {
        sqlx::query(sql_all).fetch_all(&pool).await
    }
    .map_err(|error| format!("query persisted {label}: {error}"))?;

    let values = rows
        .into_iter()
        .map(|row| row.get::<String, _>("VALUE"))
        .collect();

    pool.close().await;
    Ok(values)
}

#[derive(Clone)]
struct PersistedSeriesBrowseQuery {
    library_ids: Option<Vec<String>>,
    collection_ids: Option<Vec<String>>,
    search: Option<String>,
    search_regex: Option<(String, String)>,
    has_author_filters: bool,
    page: usize,
    size: usize,
    unpaged: bool,
    sort_mode: PersistedSeriesSortMode,
}

#[derive(Clone, Copy)]
enum PersistedSeriesSortMode {
    TitleAsc,
    Latest,
}

#[derive(Clone)]
struct PersistedSeriesSummary {
    id: String,
    library_id: String,
    title: String,
    title_sort: String,
    labels: Vec<String>,
    last_modified: String,
}

async fn load_persisted_series_page(
    database_file: &FsPath,
    context: &DiscoveryQueryContext,
    query: PersistedSeriesBrowseQuery,
) -> Result<PageEnvelope<SeriesReadModel>, String> {
    let mut series = load_persisted_series_summaries(database_file).await?;

    if let Some(allowed_ids) = context.authorized_library_ids.as_ref() {
        series.retain(|row| allowed_ids.iter().any(|id| id == row.library_id.as_str()));
    }
    if let Some(library_ids) = query.library_ids.as_ref() {
        series.retain(|row| library_ids.iter().any(|id| id == row.library_id.as_str()));
    }
    if query.has_author_filters {
        series.clear();
    }

    if let Some(collection_ids) = query.collection_ids.as_ref() {
        let memberships = load_collection_memberships(database_file).await?;
        series.retain(|row| {
            memberships
                .get(&row.id)
                .into_iter()
                .flatten()
                .any(|collection_id| collection_ids.iter().any(|id| id == collection_id))
        });
    }

    if let Some(search) = query.search.as_ref() {
        let normalized = search.trim().to_ascii_lowercase();
        if !normalized.is_empty() {
            series.retain(|row| {
                row.title.to_ascii_lowercase().contains(&normalized)
                    || row.title_sort.to_ascii_lowercase().contains(&normalized)
            });
        }
    }

    if let Some((pattern, field)) = query.search_regex.as_ref() {
        series.retain(|row| {
            let candidate = if field == "title_sort" {
                row.title_sort.as_str()
            } else {
                row.title.as_str()
            };
            matches_search_pattern(candidate, pattern)
        });
    }

    match query.sort_mode {
        PersistedSeriesSortMode::TitleAsc => {
            series.sort_by(|left, right| {
                left.title_sort
                    .to_ascii_lowercase()
                    .cmp(&right.title_sort.to_ascii_lowercase())
                    .then_with(|| left.id.cmp(&right.id))
            });
        }
        PersistedSeriesSortMode::Latest => {
            series.sort_by(|left, right| {
                right
                    .last_modified
                    .cmp(&left.last_modified)
                    .then_with(|| left.id.cmp(&right.id))
            });
        }
    }

    let total_elements = series.len();
    let content = if query.unpaged {
        series
    } else {
        let offset = query.page.saturating_mul(query.size);
        if offset >= total_elements {
            vec![]
        } else {
            series.into_iter().skip(offset).take(query.size).collect()
        }
    };
    let page = if query.unpaged { 0 } else { query.page };
    let page_size = if query.unpaged {
        total_elements.max(1)
    } else {
        query.size.max(1)
    };

    Ok(PageEnvelope::from_slice(
        content
            .into_iter()
            .map(|row| SeriesReadModel {
                id: row.id,
                library_id: row.library_id,
                title: row.title,
                labels: row.labels,
            })
            .collect(),
        page,
        page_size,
        total_elements,
    ))
}

async fn load_persisted_alphabetical_groups(
    database_file: &FsPath,
    context: &DiscoveryQueryContext,
    library_ids: Option<Vec<String>>,
) -> Result<Vec<Value>, String> {
    let page = load_persisted_series_page(
        database_file,
        context,
        PersistedSeriesBrowseQuery {
            library_ids,
            collection_ids: None,
            search: None,
            search_regex: None,
            has_author_filters: false,
            page: 0,
            size: usize::MAX,
            unpaged: true,
            sort_mode: PersistedSeriesSortMode::TitleAsc,
        },
    )
    .await?;

    let mut counts = BTreeMap::<String, i64>::new();
    for series in page.content {
        let group = first_group_key(&series.title);
        *counts.entry(group).or_insert(0) += 1;
    }

    Ok(counts
        .into_iter()
        .map(|(group, count)| json!({ "group": group, "count": count }))
        .collect())
}

async fn load_persisted_series_summaries(
    database_file: &FsPath,
) -> Result<Vec<PersistedSeriesSummary>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series db: {error}"))?;

    let rows = sqlx::query(
        "SELECT s.ID, s.LIBRARY_ID, s.LAST_MODIFIED_DATE, COALESCE(sm.TITLE, s.NAME) AS TITLE, COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) AS TITLE_SORT, COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS LABELS FROM SERIES s LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID GROUP BY s.ID, s.LIBRARY_ID, s.LAST_MODIFIED_DATE, sm.TITLE, sm.TITLE_SORT, s.NAME"
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted series summaries: {error}"))?;

    let summaries = rows
        .into_iter()
        .map(|row| PersistedSeriesSummary {
            id: row.get::<String, _>("ID"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            title: row.get::<String, _>("TITLE"),
            title_sort: row.get::<String, _>("TITLE_SORT"),
            labels: parse_csv_values(&row.get::<String, _>("LABELS")),
            last_modified: row.get::<String, _>("LAST_MODIFIED_DATE"),
        })
        .collect();

    pool.close().await;
    Ok(summaries)
}

async fn load_persisted_library_ids(database_file: &FsPath) -> Result<Vec<String>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open persisted browse-library db: {error}"))?;

    let rows = sqlx::query(
        "SELECT LIBRARY_ID AS ID FROM (
             SELECT DISTINCT LIBRARY_ID FROM SERIES WHERE DELETED_DATE IS NULL
             UNION
             SELECT DISTINCT LIBRARY_ID FROM BOOK WHERE DELETED_DATE IS NULL
         )
         ORDER BY ID COLLATE NOCASE ASC, ID ASC",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted browse-library ids: {error}"))?;

    let ids = rows
        .into_iter()
        .map(|row| row.get::<String, _>("ID"))
        .collect();

    pool.close().await;
    Ok(ids)
}

async fn remap_legacy_library_ids_for_persisted(
    database_file: &FsPath,
    requested: Option<&Vec<String>>,
) -> Option<Vec<String>> {
    let Some(requested) = requested else {
        return None;
    };

    if requested.is_empty() || !database_file.exists() {
        return None;
    }

    let persisted_ids = match load_persisted_library_ids(database_file).await {
        Ok(ids) => ids,
        Err(_) => return None,
    };

    if persisted_ids.is_empty() {
        return None;
    }

    let mut normalized = Vec::new();
    for value in requested {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }

        if persisted_ids.iter().any(|candidate| candidate == trimmed) {
            if !normalized.iter().any(|candidate| candidate == trimmed) {
                normalized.push(trimmed.to_string());
            }
            continue;
        }

        let Some(index) = trimmed.parse::<usize>().ok() else {
            continue;
        };
        if index == 0 {
            continue;
        }

        let Some(mapped) = persisted_ids.get(index - 1) else {
            continue;
        };
        if !normalized.iter().any(|candidate| candidate == mapped) {
            normalized.push(mapped.clone());
        }
    }

    (!normalized.is_empty()).then_some(normalized)
}

async fn load_collection_memberships(
    database_file: &FsPath,
) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series collection db: {error}"))?;

    let rows = sqlx::query("SELECT SERIES_ID, COLLECTION_ID FROM COLLECTION_SERIES")
        .fetch_all(&pool)
        .await
        .map_err(|error| format!("query series collection memberships: {error}"))?;

    let mut memberships = BTreeMap::<String, BTreeSet<String>>::new();
    for row in rows {
        memberships
            .entry(row.get::<String, _>("SERIES_ID"))
            .or_default()
            .insert(row.get::<String, _>("COLLECTION_ID"));
    }

    pool.close().await;
    Ok(memberships)
}

fn requested_query_values(query: &str, key: &str) -> Option<Vec<String>> {
    let values = query_values(query, key)
        .into_iter()
        .filter(|value| !value.is_empty())
        .map(decode_query_component)
        .collect::<Vec<_>>();
    (!values.is_empty()).then_some(values)
}

fn first_group_key(title: &str) -> String {
    title
        .trim()
        .chars()
        .next()
        .map(|ch| ch.to_ascii_uppercase().to_string())
        .unwrap_or_else(|| "#".to_string())
}

fn parse_csv_values(raw: &str) -> Vec<String> {
    if raw.trim().is_empty() {
        return vec![];
    }

    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn decode_query_component(value: &str) -> String {
    let mut decoded = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0usize;

    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let first = (bytes[index + 1] as char).to_digit(16);
                let second = (bytes[index + 2] as char).to_digit(16);

                if let (Some(first), Some(second)) = (first, second) {
                    decoded.push((first * 16 + second) as u8);
                    index += 3;
                } else {
                    decoded.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }

    String::from_utf8_lossy(&decoded).into_owned()
}

fn internal_error_response(error: String) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": error })),
    )
        .into_response()
}

fn removed_runtime_fallback_response(route: &str, detail: impl std::fmt::Display) -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "error": format!(
                "{route} requires persisted/domain-backed data; snapshot fallback removed ({detail})",
            ),
        })),
    )
        .into_response()
}

fn removed_discovery_fallback_response(route: &str, error: &DiscoveryError) -> Response {
    removed_runtime_fallback_response(route, format!("{error:?}"))
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

#[derive(Clone, Copy)]
enum PersistedBooksSortMode {
    TitleAsc,
    CreatedDateDesc,
    LastModifiedDateDesc,
    ReleaseDateDesc,
}

#[derive(Clone)]
struct PersistedBooksBrowseQuery {
    library_ids: Option<Vec<String>>,
    search: Option<String>,
    page: usize,
    size: usize,
    unpaged: bool,
    sort_mode: PersistedBooksSortMode,
}

#[derive(Clone)]
struct PersistedBookSummary {
    id: String,
    series_id: String,
    series_title: String,
    library_id: String,
    title: String,
    url: String,
    created: String,
    last_modified: String,
    file_last_modified: String,
    size_bytes: u64,
    media_status: String,
    media_type: String,
    media_pages_count: u32,
    metadata_release_date: Option<String>,
    deleted: bool,
    oneshot: bool,
    labels: Vec<String>,
}

async fn native_owned_persisted_books_page(
    database_file: &FsPath,
    context: &DiscoveryQueryContext,
    filters: &NativeBooksFilters,
    sorts: &[String],
    full_text_search: Option<String>,
    page: usize,
    size: usize,
    unpaged: bool,
    oneshot_bootstrap_series_id: Option<&str>,
) -> Option<Result<PageEnvelope<BookReadModel>, String>> {
    if !database_file.exists()
        || oneshot_bootstrap_series_id.is_some()
        || !native_books_filters_persisted_compatible(filters)
    {
        return None;
    }

    let sort_mode = parse_persisted_books_sort_mode(sorts)?;
    let has_persisted_rows = match persisted_books_exist(database_file).await {
        Ok(has_rows) => has_rows,
        Err(error) => return Some(Err(error)),
    };
    if !has_persisted_rows {
        return None;
    }

    Some(
        load_persisted_books_page(
            database_file,
            context,
            PersistedBooksBrowseQuery {
                library_ids: filters.library_ids.clone(),
                search: full_text_search,
                page,
                size,
                unpaged,
                sort_mode,
            },
        )
        .await,
    )
}

async fn native_owned_persisted_series_page(
    database_file: &FsPath,
    context: &DiscoveryQueryContext,
    filters: &NativeSeriesFilters,
    sorts: &[String],
    full_text_search: Option<String>,
    page: usize,
    size: usize,
) -> Option<Result<PageEnvelope<SeriesReadModel>, String>> {
    if !database_file.exists() || !native_series_filters_persisted_compatible(filters) {
        return None;
    }

    let sort_mode = parse_persisted_series_sort_mode(sorts)?;
    let has_persisted_rows = match persisted_series_exist(database_file).await {
        Ok(has_rows) => has_rows,
        Err(error) => return Some(Err(error)),
    };
    if !has_persisted_rows {
        return None;
    }

    Some(
        load_persisted_series_page(
            database_file,
            context,
            PersistedSeriesBrowseQuery {
                library_ids: filters.library_ids.clone(),
                collection_ids: None,
                search: full_text_search,
                search_regex: None,
                has_author_filters: false,
                page,
                size,
                unpaged: false,
                sort_mode,
            },
        )
        .await,
    )
}

fn parse_persisted_books_sort_mode(sorts: &[String]) -> Option<PersistedBooksSortMode> {
    if sorts.is_empty() {
        return Some(PersistedBooksSortMode::TitleAsc);
    }

    match sorts.first().map(|value| value.as_str()) {
        Some("metadata.title,asc") => Some(PersistedBooksSortMode::TitleAsc),
        Some("series,metadata.numberSort,asc") => Some(PersistedBooksSortMode::TitleAsc),
        Some("createdDate,desc") => Some(PersistedBooksSortMode::CreatedDateDesc),
        Some("lastModifiedDate,desc") => Some(PersistedBooksSortMode::LastModifiedDateDesc),
        Some("metadata.releaseDate,desc") => Some(PersistedBooksSortMode::ReleaseDateDesc),
        _ => None,
    }
}

fn parse_persisted_series_sort_mode(sorts: &[String]) -> Option<PersistedSeriesSortMode> {
    if sorts.is_empty() {
        return Some(PersistedSeriesSortMode::TitleAsc);
    }

    match sorts.first().map(|value| value.as_str()) {
        Some("metadata.titleSort,asc") => Some(PersistedSeriesSortMode::TitleAsc),
        Some("createdDate,desc")
        | Some("lastModifiedDate,desc")
        | Some("booksMetadata.releaseDate,desc") => Some(PersistedSeriesSortMode::Latest),
        _ => None,
    }
}

fn native_books_filters_persisted_compatible(filters: &NativeBooksFilters) -> bool {
    filters.direct_browse_family.is_none()
        && filters.series_ids.is_none()
        && filters.deleted.is_none()
        && filters.oneshot.is_none()
        && filters.tags.is_none()
        && filters.read_statuses.is_none()
        && filters.media_profiles.is_none()
        && filters.media_statuses.is_none()
        && filters.authors.is_none()
        && filters.release_dates.is_none()
}

fn native_series_filters_persisted_compatible(filters: &NativeSeriesFilters) -> bool {
    filters.deleted.is_none()
        && filters.oneshot.is_none()
        && filters.read_statuses.is_none()
        && filters.genres.is_none()
        && filters.tags.is_none()
        && filters.languages.is_none()
        && filters.publishers.is_none()
        && filters.age_ratings.is_none()
        && filters.release_dates.is_none()
        && filters.sharing_labels.is_none()
        && filters.series_statuses.is_none()
        && filters.complete.is_none()
        && filters.authors.is_none()
}

async fn persisted_books_exist(database_file: &FsPath) -> Result<bool, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open persisted books db: {error}"))?;
    let row = sqlx::query("SELECT COUNT(*) AS COUNT FROM BOOK WHERE DELETED_DATE IS NULL")
        .fetch_one(&pool)
        .await
        .map_err(|error| format!("query persisted books count: {error}"))?;
    pool.close().await;

    Ok(row.get::<i64, _>("COUNT") > 0)
}

async fn persisted_series_exist(database_file: &FsPath) -> Result<bool, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open persisted series db: {error}"))?;
    let row = sqlx::query("SELECT COUNT(*) AS COUNT FROM SERIES WHERE DELETED_DATE IS NULL")
        .fetch_one(&pool)
        .await
        .map_err(|error| format!("query persisted series count: {error}"))?;
    pool.close().await;

    Ok(row.get::<i64, _>("COUNT") > 0)
}

async fn load_persisted_books_page(
    database_file: &FsPath,
    context: &DiscoveryQueryContext,
    query: PersistedBooksBrowseQuery,
) -> Result<PageEnvelope<BookReadModel>, String> {
    let mut books = load_persisted_book_summaries(database_file).await?;

    if let Some(allowed_ids) = context.authorized_library_ids.as_ref() {
        books.retain(|row| allowed_ids.iter().any(|id| id == row.library_id.as_str()));
    }
    if let Some(library_ids) = query.library_ids.as_ref() {
        books.retain(|row| library_ids.iter().any(|id| id == row.library_id.as_str()));
    }

    if let Some(search) = query.search.as_ref() {
        let normalized = search.trim().to_ascii_lowercase();
        if !normalized.is_empty() {
            books.retain(|row| row.title.to_ascii_lowercase().contains(&normalized));
        }
    }

    match query.sort_mode {
        PersistedBooksSortMode::TitleAsc => {
            books.sort_by(|left, right| {
                left.title
                    .to_ascii_lowercase()
                    .cmp(&right.title.to_ascii_lowercase())
                    .then_with(|| left.id.cmp(&right.id))
            });
        }
        PersistedBooksSortMode::CreatedDateDesc => {
            books.sort_by(|left, right| {
                right
                    .created
                    .cmp(&left.created)
                    .then_with(|| left.id.cmp(&right.id))
            });
        }
        PersistedBooksSortMode::LastModifiedDateDesc => {
            books.sort_by(|left, right| {
                right
                    .last_modified
                    .cmp(&left.last_modified)
                    .then_with(|| left.id.cmp(&right.id))
            });
        }
        PersistedBooksSortMode::ReleaseDateDesc => {
            books.sort_by(|left, right| {
                right
                    .metadata_release_date
                    .cmp(&left.metadata_release_date)
                    .then_with(|| left.id.cmp(&right.id))
            });
        }
    }

    let total_elements = books.len();
    let content = if query.unpaged {
        books
    } else {
        let offset = query.page.saturating_mul(query.size);
        if offset >= total_elements {
            vec![]
        } else {
            books.into_iter().skip(offset).take(query.size).collect()
        }
    };
    let page = if query.unpaged { 0 } else { query.page };
    let page_size = if query.unpaged {
        total_elements.max(1)
    } else {
        query.size.max(1)
    };

    Ok(PageEnvelope::from_slice(
        content
            .into_iter()
            .map(|row| BookReadModel {
                id: row.id,
                series_id: row.series_id,
                series_title: row.series_title,
                library_id: row.library_id,
                title: row.title,
                url: row.url,
                created: row.created,
                last_modified: row.last_modified,
                file_last_modified: row.file_last_modified,
                size_bytes: row.size_bytes,
                media_status: row.media_status,
                media_type: row.media_type,
                media_pages_count: row.media_pages_count,
                metadata_release_date: row.metadata_release_date,
                deleted: row.deleted,
                oneshot: row.oneshot,
                labels: row.labels,
            })
            .collect(),
        page,
        page_size,
        total_elements,
    ))
}

async fn load_persisted_book_summaries(
    database_file: &FsPath,
) -> Result<Vec<PersistedBookSummary>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open books db: {error}"))?;

    let rows = sqlx::query(
        "SELECT b.ID, b.SERIES_ID, b.LIBRARY_ID, b.URL, b.CREATED_DATE, b.LAST_MODIFIED_DATE, CAST(b.FILE_LAST_MODIFIED AS TEXT) AS FILE_LAST_MODIFIED, b.FILE_SIZE, s.ONESHOT AS ONESHOT, b.DELETED_DATE, COALESCE(sm.TITLE, s.NAME) AS SERIES_TITLE, COALESCE(bm.TITLE, b.NAME) AS TITLE, bm.RELEASE_DATE AS RELEASE_DATE, COALESCE(m.STATUS, 'UNKNOWN') AS MEDIA_STATUS, COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE, COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT, COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS LABELS FROM BOOK b JOIN SERIES s ON s.ID = b.SERIES_ID LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID GROUP BY b.ID, b.SERIES_ID, b.LIBRARY_ID, b.URL, b.CREATED_DATE, b.LAST_MODIFIED_DATE, b.FILE_LAST_MODIFIED, b.FILE_SIZE, s.ONESHOT, b.DELETED_DATE, sm.TITLE, s.NAME, bm.TITLE, b.NAME, bm.RELEASE_DATE, m.STATUS, m.MEDIA_TYPE, m.PAGE_COUNT",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted book summaries: {error}"))?;

    let summaries = rows
        .into_iter()
        .map(|row| {
            let size_bytes = row.get::<i64, _>("FILE_SIZE").max(0) as u64;
            let page_count = row.get::<i64, _>("PAGE_COUNT").max(0) as u32;
            PersistedBookSummary {
                id: row.get::<String, _>("ID"),
                series_id: row.get::<String, _>("SERIES_ID"),
                series_title: row.get::<String, _>("SERIES_TITLE"),
                library_id: row.get::<String, _>("LIBRARY_ID"),
                title: row.get::<String, _>("TITLE"),
                url: row.get::<String, _>("URL"),
                created: row.get::<String, _>("CREATED_DATE"),
                last_modified: row.get::<String, _>("LAST_MODIFIED_DATE"),
                file_last_modified: row.get::<String, _>("FILE_LAST_MODIFIED"),
                size_bytes,
                media_status: row.get::<String, _>("MEDIA_STATUS"),
                media_type: row.get::<String, _>("MEDIA_TYPE"),
                media_pages_count: page_count,
                metadata_release_date: row.get::<Option<String>, _>("RELEASE_DATE"),
                deleted: row.get::<Option<String>, _>("DELETED_DATE").is_some(),
                oneshot: row.get::<bool, _>("ONESHOT"),
                labels: parse_csv_values(&row.get::<String, _>("LABELS")),
            }
        })
        .collect();

    pool.close().await;
    Ok(summaries)
}

async fn native_owned_books_list_response(
    headers: &HeaderMap,
    uri: &Uri,
    payload: Option<&Value>,
    full_text_search: Option<String>,
    auth_state: &DiscoveryAuthState,
    database_file: &FsPath,
    strict_native_shape: bool,
) -> Option<Response> {
    let query_string = uri.query().unwrap_or_default();
    let sorts = query_values(query_string, "sort")
        .into_iter()
        .map(decode_query_component)
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
        return Some(removed_discovery_fallback_response(
            "POST /api/v1/books/list",
            &DiscoveryError::NonNativeRequestShape(NonNativeRequestShape::UnsupportedBookFilter(
                "oneshot-bootstrap.query-params".to_string(),
            )),
        ));
    }

    let mut filters =
        match parse_native_books_filters(payload.and_then(|value| value.get("condition"))) {
            Ok(filters) => filters,
            Err(error) => {
                if strict_native_shape {
                    return Some(removed_discovery_fallback_response(
                        "POST /api/v1/books/list",
                        &error,
                    ));
                }
                legacy_webui_books_filters(payload)
            }
        };

    if !strict_native_shape {
        coerce_legacy_books_filters_for_persisted(&mut filters);
        filters.library_ids =
            remap_legacy_library_ids_for_persisted(database_file, filters.library_ids.as_ref())
                .await;
    }

    let requested_library_ids = strict_native_shape
        .then_some(filters.library_ids.as_deref())
        .flatten();
    let context = match auth_state.resolve_query_context(headers, requested_library_ids) {
        Some(context) => context,
        None => return Some(StatusCode::UNAUTHORIZED.into_response()),
    };

    if let Some(series_id) = oneshot_bootstrap_series_id.clone() {
        filters.direct_browse_family = Some(DirectBrowseBooksListFamily::BrowseOneshotBootstrap);
        filters.series_ids = Some(vec![series_id]);
    }

    let is_admin = context.is_admin;

    if let Some(persisted_page) = native_owned_persisted_books_page(
        database_file,
        &context,
        &filters,
        &sorts,
        full_text_search.clone(),
        page,
        size,
        unpaged,
        oneshot_bootstrap_series_id.as_deref(),
    )
    .await
    {
        match persisted_page {
            Ok(page) => {
                let mut response =
                    Json(books_page_payload(page, is_admin, !unpaged)).into_response();
                mark_native(&mut response);
                return Some(response);
            }
            Err(error) => {
                return Some(
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": format!("native books list failed: {error}") })),
                    )
                        .into_response(),
                );
            }
        }
    }

    let _ = uri;
    let _ = payload;
    let _ = full_text_search;
    let _ = is_admin;

    Some(removed_runtime_fallback_response(
        "POST /api/v1/books/list",
        "persisted/domain-backed data unavailable",
    ))
}

async fn native_owned_books_latest_response(
    headers: &HeaderMap,
    uri: &Uri,
    auth_state: &DiscoveryAuthState,
    database_file: &FsPath,
) -> Option<Response> {
    let sorts = query_values(uri.query().unwrap_or_default(), "sort");
    if !sorts.is_empty() {
        return Some(removed_discovery_fallback_response(
            "GET /api/v1/books/latest",
            &DiscoveryError::NonNativeRequestShape(NonNativeRequestShape::UnsupportedBookSort(
                sorts[0].to_string(),
            )),
        ));
    }

    if !database_file.exists() {
        return Some(removed_runtime_fallback_response(
            "GET /api/v1/books/latest",
            "persisted/domain-backed data unavailable",
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

    let _ = context;
    let _ = page;
    let _ = size;
    let _ = unpaged;
    let _ = uri;

    Some(removed_runtime_fallback_response(
        "GET /api/v1/books/latest",
        "persisted/domain-backed data unavailable",
    ))
}

async fn native_owned_series_list_response(
    headers: &HeaderMap,
    uri: &Uri,
    payload: Option<&Value>,
    full_text_search: Option<String>,
    auth_state: &DiscoveryAuthState,
    database_file: &FsPath,
    strict_native_shape: bool,
) -> Option<Response> {
    let sorts = query_values(uri.query().unwrap_or_default(), "sort")
        .into_iter()
        .map(decode_query_component)
        .collect::<Vec<_>>();
    let page = query_value(uri.query().unwrap_or_default(), "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(uri.query().unwrap_or_default(), "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);

    let mut filters =
        match parse_native_series_filters(payload.and_then(|value| value.get("condition"))) {
            Ok(filters) => filters,
            Err(error) => {
                if strict_native_shape {
                    return Some(removed_discovery_fallback_response(
                        "POST /api/v1/series/list",
                        &error,
                    ));
                }
                legacy_webui_series_filters(payload)
            }
        };

    if !strict_native_shape {
        coerce_legacy_series_filters_for_persisted(&mut filters);
        filters.library_ids =
            remap_legacy_library_ids_for_persisted(database_file, filters.library_ids.as_ref())
                .await;
    }

    let requested_library_ids = strict_native_shape
        .then_some(filters.library_ids.as_deref())
        .flatten();
    let context = match auth_state.resolve_query_context(headers, requested_library_ids) {
        Some(context) => context,
        None => return Some(StatusCode::UNAUTHORIZED.into_response()),
    };

    if let Some(persisted_page) = native_owned_persisted_series_page(
        database_file,
        &context,
        &filters,
        &sorts,
        full_text_search.clone(),
        page,
        size,
    )
    .await
    {
        match persisted_page {
            Ok(page) => {
                let mut response = Json(series_page_payload(page)).into_response();
                mark_native(&mut response);
                return Some(response);
            }
            Err(error) => {
                return Some(
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": format!("native series list failed: {error}") })),
                    )
                        .into_response(),
                );
            }
        }
    }

    let _ = uri;
    let _ = full_text_search;

    Some(removed_runtime_fallback_response(
        "POST /api/v1/series/list",
        "persisted/domain-backed data unavailable",
    ))
}

fn parse_native_series_filters(
    condition: Option<&Value>,
) -> Result<NativeSeriesFilters, DiscoveryError> {
    let Some(condition) = condition else {
        return Ok(NativeSeriesFilters::default());
    };

    if condition.get("type").and_then(Value::as_str).is_none() {
        let normalized = normalize_webui_series_condition(condition)?;
        return parse_native_series_filters(Some(&normalized));
    }

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

    if condition.get("type").and_then(Value::as_str).is_none() {
        let normalized = normalize_webui_books_condition(condition)?;
        return parse_native_books_filters(Some(&normalized));
    }

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

fn normalize_webui_series_condition(condition: &Value) -> Result<Value, DiscoveryError> {
    let Some(object) = condition.as_object() else {
        return Err(DiscoveryError::InvalidRequest(
            "series condition must be an object".to_string(),
        ));
    };

    if let Some(children) = object.get("allOf").and_then(Value::as_array) {
        let conditions = children
            .iter()
            .map(normalize_webui_series_condition)
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(json!({ "type": "AllOfSeries", "conditions": conditions }));
    }

    if let Some(children) = object.get("anyOf").and_then(Value::as_array) {
        let conditions = children
            .iter()
            .map(normalize_webui_series_condition)
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(json!({ "type": "AnyOfSeries", "conditions": conditions }));
    }

    map_webui_leaf_to_native(
        object,
        &[
            ("libraryId", "LibraryId"),
            ("deleted", "Deleted"),
            ("oneShot", "OneShot"),
            ("readStatus", "ReadStatus"),
            ("genre", "Genre"),
            ("tag", "Tag"),
            ("language", "Language"),
            ("publisher", "Publisher"),
            ("ageRating", "AgeRating"),
            ("releaseDate", "ReleaseDate"),
            ("sharingLabel", "SharingLabel"),
            ("seriesStatus", "SeriesStatus"),
            ("complete", "Complete"),
            ("author", "Author"),
        ],
        "series",
    )
}

fn normalize_webui_books_condition(condition: &Value) -> Result<Value, DiscoveryError> {
    let Some(object) = condition.as_object() else {
        return Err(DiscoveryError::InvalidRequest(
            "books condition must be an object".to_string(),
        ));
    };

    if let Some(children) = object.get("allOf").and_then(Value::as_array) {
        let conditions = children
            .iter()
            .map(normalize_webui_books_condition)
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(json!({ "type": "AllOfBook", "conditions": conditions }));
    }

    if let Some(children) = object.get("anyOf").and_then(Value::as_array) {
        let conditions = children
            .iter()
            .map(normalize_webui_books_condition)
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(json!({ "type": "AnyOfBook", "conditions": conditions }));
    }

    map_webui_leaf_to_native(
        object,
        &[
            ("libraryId", "LibraryId"),
            ("seriesId", "SeriesId"),
            ("deleted", "Deleted"),
            ("oneShot", "OneShot"),
            ("tag", "Tag"),
            ("readStatus", "ReadStatus"),
            ("mediaProfile", "MediaProfile"),
            ("mediaStatus", "MediaStatus"),
            ("author", "Author"),
            ("releaseDate", "ReleaseDate"),
        ],
        "books",
    )
}

fn map_webui_leaf_to_native(
    object: &serde_json::Map<String, Value>,
    mappings: &[(&str, &str)],
    label: &str,
) -> Result<Value, DiscoveryError> {
    for (webui_key, native_type) in mappings {
        if let Some(operator_shape) = object.get(*webui_key)
            && let Some(operator_map) = operator_shape.as_object()
        {
            let mut normalized = serde_json::Map::new();
            normalized.insert(
                "type".to_string(),
                Value::String((*native_type).to_string()),
            );
            for (key, value) in operator_map {
                normalized.insert(key.clone(), value.clone());
            }
            return Ok(Value::Object(normalized));
        }
    }

    let detail = format!("{label}.unsupported-webui-filter");
    let shape = if label == "books" {
        NonNativeRequestShape::UnsupportedBookFilter(detail)
    } else {
        NonNativeRequestShape::UnsupportedSeriesFilter(detail)
    };

    Err(DiscoveryError::NonNativeRequestShape(shape))
}

fn legacy_webui_series_filters(payload: Option<&Value>) -> NativeSeriesFilters {
    let mut filters = NativeSeriesFilters::default();
    let Some(condition) = payload.and_then(|value| value.get("condition")) else {
        return filters;
    };

    let mut library_ids = vec![];
    collect_webui_string_filter_values(condition, "libraryId", &mut library_ids);
    if !library_ids.is_empty() {
        library_ids.sort();
        library_ids.dedup();
        filters.library_ids = Some(library_ids);
    }

    filters
}

fn legacy_webui_books_filters(payload: Option<&Value>) -> NativeBooksFilters {
    let mut filters = NativeBooksFilters::default();
    let Some(condition) = payload.and_then(|value| value.get("condition")) else {
        return filters;
    };

    let mut library_ids = vec![];
    collect_webui_string_filter_values(condition, "libraryId", &mut library_ids);
    if !library_ids.is_empty() {
        library_ids.sort();
        library_ids.dedup();
        filters.library_ids = Some(library_ids);
    }

    let mut series_ids = vec![];
    collect_webui_string_filter_values(condition, "seriesId", &mut series_ids);
    if !series_ids.is_empty() {
        series_ids.sort();
        series_ids.dedup();
        filters.series_ids = Some(series_ids);
    }

    filters
}

fn coerce_legacy_series_filters_for_persisted(filters: &mut NativeSeriesFilters) {
    filters.deleted = None;
    filters.oneshot = None;
    filters.read_statuses = None;
    filters.genres = None;
    filters.tags = None;
    filters.languages = None;
    filters.publishers = None;
    filters.age_ratings = None;
    filters.release_dates = None;
    filters.sharing_labels = None;
    filters.series_statuses = None;
    filters.complete = None;
    filters.authors = None;
}

fn coerce_legacy_books_filters_for_persisted(filters: &mut NativeBooksFilters) {
    filters.direct_browse_family = None;
    filters.series_ids = None;
    filters.deleted = None;
    filters.oneshot = None;
    filters.tags = None;
    filters.read_statuses = None;
    filters.media_profiles = None;
    filters.media_statuses = None;
    filters.authors = None;
    filters.release_dates = None;
}

fn collect_webui_string_filter_values(condition: &Value, key: &str, output: &mut Vec<String>) {
    match condition {
        Value::Object(object) => {
            if let Some(filter) = object.get(key)
                && let Some(filter_object) = filter.as_object()
                && let Some(value) = filter_object.get("value").and_then(Value::as_str)
                && !value.is_empty()
            {
                output.push(value.to_string());
            }

            for nested in object.values() {
                collect_webui_string_filter_values(nested, key, output);
            }
        }
        Value::Array(values) => {
            for nested in values {
                collect_webui_string_filter_values(nested, key, output);
            }
        }
        _ => {}
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

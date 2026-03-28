use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path as FsPath;

use axum::Json;
use axum::body::Bytes;
use axum::extract::Extension;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_domain::discovery::{
    BookReadModel, DirectBrowseBooksListFamily, DiscoveryError, PageEnvelope,
};
use komga_persistence::sqlite::connect_pool;
use serde_json::{Value, json};
use sqlx::{QueryBuilder, Row, Sqlite};

use crate::app::CompatProfile;
use crate::app::discovery_auth::{DiscoveryAuthState, DiscoveryQueryContext};
use crate::app::runtime_auth::{require_auth, resolved_auth_user, user_id};

use super::super::{AuthDatabaseState, ReadProgressState};
use super::helpers::{
    DiscoveryOwnershipRoute, DiscoveryShape, books_page_payload, discovery_ownership_route,
    extract_full_text_search, mark_native, mark_non_native, matches_search_pattern,
    parse_search_regex, query_bool, query_value, query_values, wants_shadow_marker,
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
        return StatusCode::NOT_FOUND.into_response();
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
            titles: None,
            titles_excluded: None,
            titles_contains: None,
            titles_contains_excluded: None,
            titles_begins_with: None,
            titles_begins_with_excluded: None,
            titles_ends_with: None,
            titles_ends_with_excluded: None,
            title_sorts: None,
            title_sorts_excluded: None,
            title_sorts_contains: None,
            title_sorts_contains_excluded: None,
            title_sorts_begins_with: None,
            title_sorts_begins_with_excluded: None,
            title_sorts_ends_with: None,
            title_sorts_ends_with_excluded: None,
            deleted: None,
            oneshot: None,
            read_statuses: None,
            read_statuses_excluded: None,
            complete: None,
            genres: None,
            genres_excluded: None,
            genres_null: None,
            tags: None,
            tags_excluded: None,
            tags_null: None,
            languages: None,
            languages_excluded: None,
            publishers: None,
            publishers_excluded: None,
            age_ratings: None,
            age_ratings_excluded: None,
            age_ratings_null: None,
            age_rating_gt: None,
            age_rating_lt: None,
            sharing_labels: None,
            sharing_labels_excluded: None,
            sharing_labels_null: None,
            authors: None,
            authors_excluded: None,
            release_dates: None,
            release_dates_excluded: None,
            release_dates_null: None,
            release_date_gt: None,
            release_date_lt: None,
            release_date_begins_with: None,
            release_date_ends_with: None,
            release_date_contains_excluded: None,
            release_date_begins_with_excluded: None,
            release_date_ends_with_excluded: None,
            release_date_in_last_days: None,
            release_date_not_in_last_days: None,
            series_statuses: None,
            series_statuses_excluded: None,
            search,
            search_regex,
            page,
            size,
            unpaged,
            sort_modes: vec![PersistedSeriesSortMode::TitleAsc],
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
            titles: None,
            titles_excluded: None,
            titles_contains: None,
            titles_contains_excluded: None,
            titles_begins_with: None,
            titles_begins_with_excluded: None,
            titles_ends_with: None,
            titles_ends_with_excluded: None,
            title_sorts: None,
            title_sorts_excluded: None,
            title_sorts_contains: None,
            title_sorts_contains_excluded: None,
            title_sorts_begins_with: None,
            title_sorts_begins_with_excluded: None,
            title_sorts_ends_with: None,
            title_sorts_ends_with_excluded: None,
            deleted: None,
            oneshot: None,
            read_statuses: None,
            read_statuses_excluded: None,
            complete: None,
            genres: None,
            genres_excluded: None,
            genres_null: None,
            tags: None,
            tags_excluded: None,
            tags_null: None,
            languages: None,
            languages_excluded: None,
            publishers: None,
            publishers_excluded: None,
            age_ratings: None,
            age_ratings_excluded: None,
            age_ratings_null: None,
            age_rating_gt: None,
            age_rating_lt: None,
            sharing_labels: None,
            sharing_labels_excluded: None,
            sharing_labels_null: None,
            authors: None,
            authors_excluded: None,
            release_dates: None,
            release_dates_excluded: None,
            release_dates_null: None,
            release_date_gt: None,
            release_date_lt: None,
            release_date_begins_with: None,
            release_date_ends_with: None,
            release_date_contains_excluded: None,
            release_date_begins_with_excluded: None,
            release_date_ends_with_excluded: None,
            release_date_in_last_days: None,
            release_date_not_in_last_days: None,
            series_statuses: None,
            series_statuses_excluded: None,
            search: None,
            search_regex: None,
            page,
            size,
            unpaged,
            sort_modes: vec![PersistedSeriesSortMode::Latest],
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

    let full_text_search = extract_full_text_search(&body);
    let search_regex = extract_regex_search(&body);

    let context = match auth_state.resolve_query_context(&headers, None) {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    match load_persisted_alphabetical_groups(
        database_file,
        &context,
        filters,
        full_text_search,
        search_regex,
    )
    .await
    {
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

    if !auth_db.database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    invalid_native_series_list_response(DiscoveryError::InvalidRequest(
        "unsupported native series filter combination".to_string(),
    ))
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

    if !auth_db.database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
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

        let sort_modes = parse_persisted_books_sort_modes(&sort_values);
        match load_persisted_books_page(
            auth_db.database_file.as_path(),
            &context,
            PersistedBooksBrowseQuery {
                library_ids,
                series_ids: None,
                series_ids_excluded: None,
                read_list_ids: None,
                read_list_ids_excluded: None,
                titles: None,
                titles_excluded: None,
                titles_contains: None,
                titles_contains_excluded: None,
                titles_begins_with: None,
                titles_begins_with_excluded: None,
                titles_ends_with: None,
                titles_ends_with_excluded: None,
                deleted: None,
                oneshot: None,
                tags: None,
                tags_excluded: None,
                tags_null: None,
                media_profiles: None,
                media_profiles_excluded: None,
                authors: None,
                authors_excluded: None,
                poster_types: None,
                poster_types_excluded: None,
                poster_selected: None,
                poster_selected_excluded: None,
                media_statuses: None,
                media_statuses_excluded: None,
                read_statuses: None,
                read_statuses_excluded: None,
                release_dates: None,
                release_dates_excluded: None,
                release_dates_null: None,
                release_date_gt: None,
                release_date_lt: None,
                release_date_begins_with: None,
                release_date_ends_with: None,
                release_date_contains_excluded: None,
                release_date_begins_with_excluded: None,
                release_date_ends_with_excluded: None,
                release_date_in_last_days: None,
                release_date_not_in_last_days: None,
                number_sorts: None,
                number_sorts_excluded: None,
                number_sort_gt: None,
                number_sort_lt: None,
                search,
                page,
                size,
                unpaged,
                sort_modes,
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

    let _ = profile;
    let _ = state;

    empty_books_page_response(&uri, false)
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

    if !auth_db.database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
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

    if !auth_db.database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    invalid_native_books_list_response(DiscoveryError::InvalidRequest(
        "unsupported native books filter combination".to_string(),
    ))
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
                series_ids: None,
                series_ids_excluded: None,
                read_list_ids: None,
                read_list_ids_excluded: None,
                titles: None,
                titles_excluded: None,
                titles_contains: None,
                titles_contains_excluded: None,
                titles_begins_with: None,
                titles_begins_with_excluded: None,
                titles_ends_with: None,
                titles_ends_with_excluded: None,
                deleted: None,
                oneshot: None,
                tags: None,
                tags_excluded: None,
                tags_null: None,
                media_profiles: None,
                media_profiles_excluded: None,
                authors: None,
                authors_excluded: None,
                poster_types: None,
                poster_types_excluded: None,
                poster_selected: None,
                poster_selected_excluded: None,
                media_statuses: None,
                media_statuses_excluded: None,
                read_statuses: None,
                read_statuses_excluded: None,
                release_dates: None,
                release_dates_excluded: None,
                release_dates_null: None,
                release_date_gt: None,
                release_date_lt: None,
                release_date_begins_with: None,
                release_date_ends_with: None,
                release_date_contains_excluded: None,
                release_date_begins_with_excluded: None,
                release_date_ends_with_excluded: None,
                release_date_in_last_days: None,
                release_date_not_in_last_days: None,
                number_sorts: None,
                number_sorts_excluded: None,
                number_sort_gt: None,
                number_sort_lt: None,
                search: None,
                page,
                size,
                unpaged,
                sort_modes: vec![PersistedBooksSortMode::LastModifiedDateDesc],
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

    if !auth_db.database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    invalid_native_books_list_response(DiscoveryError::InvalidRequest(
        "unsupported native books latest filter combination".to_string(),
    ))
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
        return StatusCode::NOT_FOUND.into_response();
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
        return StatusCode::NOT_FOUND.into_response();
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
        return StatusCode::NOT_FOUND.into_response();
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
        return StatusCode::NOT_FOUND.into_response();
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
        return StatusCode::NOT_FOUND.into_response();
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
        return StatusCode::NOT_FOUND.into_response();
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
        return StatusCode::NOT_FOUND.into_response();
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
        return StatusCode::NOT_FOUND.into_response();
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
        return StatusCode::NOT_FOUND.into_response();
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
        return StatusCode::NOT_FOUND.into_response();
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
        return StatusCode::NOT_FOUND.into_response();
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
        return StatusCode::NOT_FOUND.into_response();
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
        return StatusCode::NOT_FOUND.into_response();
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
        "SELECT b.ID, b.LIBRARY_ID, b.NAME, COALESCE(bm.TITLE, b.NAME) AS TITLE, b.SERIES_ID, \
                b.NUMBER \
         FROM BOOK b \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         WHERE b.SERIES_ID IN (SELECT DISTINCT b_done.SERIES_ID \
         FROM BOOK b_done \
         JOIN READ_PROGRESS rp_done ON rp_done.BOOK_ID = b_done.ID \
         WHERE rp_done.USER_ID = ? \
         AND rp_done.COMPLETED = 1) \
         AND b.SERIES_ID NOT IN (SELECT DISTINCT b_prog.SERIES_ID \
         FROM BOOK b_prog \
         JOIN READ_PROGRESS rp_prog ON rp_prog.BOOK_ID = b_prog.ID \
         WHERE rp_prog.USER_ID = ? \
         AND rp_prog.COMPLETED = 0) \
         AND NOT EXISTS (SELECT 1 \
         FROM READ_PROGRESS rp_seen \
         WHERE rp_seen.BOOK_ID = b.ID \
         AND rp_seen.USER_ID = ? \
         AND rp_seen.COMPLETED = 1) \
         ORDER BY b.SERIES_ID ASC, b.NUMBER ASC",
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

    Ok(first_per_series.into_values().collect())
}

async fn load_persisted_duplicate_books(
    database_file: &FsPath,
) -> Result<Vec<PersistedBookBrowseEntry>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open books duplicates db: {error}"))?;

    let rows = sqlx::query(
        "SELECT b.ID, b.LIBRARY_ID, b.NAME, COALESCE(bm.TITLE, b.NAME) AS TITLE \
         FROM BOOK b \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         WHERE b.FILE_HASH IS NOT NULL \
         AND TRIM(b.FILE_HASH) != '' \
         AND b.FILE_HASH IN (SELECT FILE_HASH \
         FROM BOOK \
         WHERE FILE_HASH IS NOT NULL \
         AND TRIM(FILE_HASH) != '' \
         GROUP BY FILE_HASH \
         HAVING COUNT(*) > 1) \
         ORDER BY b.FILE_HASH ASC, b.NUMBER ASC, b.ID ASC",
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
                "SELECT bt.TAG \
                 FROM BOOK_METADATA_TAG bt \
                 JOIN BOOK b ON b.ID = bt.BOOK_ID \
                 WHERE b.SERIES_ID = ? \
                 ORDER BY lower(bt.TAG), bt.TAG, b.ID",
            )
            .bind(series_id)
            .fetch_all(&pool)
            .await
        }
        PersistedBookTagsScope::Library(library_id) => {
            sqlx::query(
                "SELECT bt.TAG \
                 FROM BOOK_METADATA_TAG bt \
                 JOIN BOOK b ON b.ID = bt.BOOK_ID \
                 WHERE b.LIBRARY_ID = ? \
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
            "SELECT a.NAME, a.ROLE \
             FROM BOOK_METADATA_AUTHOR a \
             JOIN BOOK b ON b.ID = a.BOOK_ID \
             WHERE b.LIBRARY_ID = ? \
             ORDER BY lower(a.NAME), lower(a.ROLE), a.NAME, a.ROLE, b.ID",
        )
        .bind(library_id)
        .fetch_all(&pool)
        .await
    } else {
        sqlx::query(
            "SELECT a.NAME, a.ROLE \
             FROM BOOK_METADATA_AUTHOR a \
             JOIN BOOK b ON b.ID = a.BOOK_ID \
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
        "SELECT DISTINCT a.NAME \
         FROM BOOK_METADATA_AUTHOR a \
         JOIN BOOK b ON b.ID = a.BOOK_ID \
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

    Ok(names)
}

async fn load_persisted_author_roles(database_file: &FsPath) -> Result<Vec<String>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open author roles db: {error}"))?;

    let rows = sqlx::query(
        "SELECT DISTINCT ROLE \
         FROM BOOK_METADATA_AUTHOR \
         ORDER BY lower(ROLE), ROLE",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted author roles: {error}"))?;

    let roles = rows
        .into_iter()
        .map(|row| row.get::<String, _>("ROLE"))
        .collect();

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
        "SELECT a.NAME, a.ROLE \
         FROM BOOK_METADATA_AUTHOR a \
         JOIN BOOK b ON b.ID = a.BOOK_ID",
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
            "SELECT DISTINCT st.TAG \
             FROM SERIES_METADATA_TAG st \
             JOIN SERIES s ON s.ID = st.SERIES_ID \
             WHERE s.LIBRARY_ID = ? \
             ORDER BY lower(st.TAG), st.TAG",
        )
        .bind(library_id)
        .fetch_all(&pool)
        .await
    } else if let Some(collection_id) = collection_id {
        sqlx::query(
            "SELECT DISTINCT st.TAG \
             FROM SERIES_METADATA_TAG st \
             JOIN COLLECTION_SERIES cs ON cs.SERIES_ID = st.SERIES_ID \
             WHERE cs.COLLECTION_ID = ? \
             ORDER BY lower(st.TAG), st.TAG",
        )
        .bind(collection_id)
        .fetch_all(&pool)
        .await
    } else {
        sqlx::query(
            "SELECT DISTINCT TAG \
             FROM SERIES_METADATA_TAG \
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
        "SELECT g.GENRE AS VALUE \
         FROM SERIES_METADATA_GENRE g \
         JOIN SERIES s ON s.ID = g.SERIES_ID \
         WHERE s.LIBRARY_ID = ? \
         ORDER BY lower(g.GENRE), g.GENRE, s.ID",
        "SELECT g.GENRE AS VALUE \
         FROM SERIES_METADATA_GENRE g \
         JOIN SERIES s ON s.ID = g.SERIES_ID \
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
            "SELECT TAG \
             FROM ( SELECT st.TAG AS TAG \
             FROM SERIES_METADATA_TAG st \
             JOIN SERIES s ON s.ID = st.SERIES_ID \
             WHERE s.LIBRARY_ID = ? \
             UNION SELECT bt.TAG AS TAG \
             FROM BOOK_METADATA_TAG bt \
             JOIN BOOK b ON b.ID = bt.BOOK_ID \
             WHERE b.LIBRARY_ID = ? ) \
             ORDER BY lower(TAG), TAG",
        )
        .bind(library_id)
        .bind(library_id)
        .fetch_all(&pool)
        .await
    } else {
        sqlx::query(
            "SELECT TAG \
             FROM ( SELECT st.TAG AS TAG \
             FROM SERIES_METADATA_TAG st \
             JOIN SERIES s ON s.ID = st.SERIES_ID \
             UNION SELECT bt.TAG AS TAG \
             FROM BOOK_METADATA_TAG bt \
             JOIN BOOK b ON b.ID = bt.BOOK_ID ) \
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
        "SELECT DISTINCT sm.LANGUAGE AS VALUE \
         FROM SERIES_METADATA sm \
         JOIN SERIES s ON s.ID = sm.SERIES_ID \
         WHERE s.LIBRARY_ID = ? \
         ORDER BY lower(sm.LANGUAGE), sm.LANGUAGE",
        "SELECT DISTINCT sm.LANGUAGE AS VALUE \
         FROM SERIES_METADATA sm \
         JOIN SERIES s ON s.ID = sm.SERIES_ID \
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
        "SELECT DISTINCT sm.PUBLISHER AS VALUE \
         FROM SERIES_METADATA sm \
         JOIN SERIES s ON s.ID = sm.SERIES_ID \
         WHERE s.LIBRARY_ID = ? \
         ORDER BY lower(sm.PUBLISHER), sm.PUBLISHER",
        "SELECT DISTINCT sm.PUBLISHER AS VALUE \
         FROM SERIES_METADATA sm \
         JOIN SERIES s ON s.ID = sm.SERIES_ID \
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
            "SELECT DISTINCT sm.AGE_RATING AS VALUE \
             FROM SERIES_METADATA sm \
             JOIN SERIES s ON s.ID = sm.SERIES_ID \
             WHERE s.LIBRARY_ID = ? \
             AND sm.AGE_RATING IS NOT NULL \
             ORDER BY sm.AGE_RATING",
        )
        .bind(library_id)
        .fetch_all(&pool)
        .await
    } else {
        sqlx::query(
            "SELECT DISTINCT sm.AGE_RATING AS VALUE \
             FROM SERIES_METADATA sm \
             JOIN SERIES s ON s.ID = sm.SERIES_ID \
             WHERE sm.AGE_RATING IS NOT NULL \
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
        "SELECT DISTINCT sms.LABEL AS VALUE \
         FROM SERIES_METADATA_SHARING sms \
         JOIN SERIES s ON s.ID = sms.SERIES_ID \
         WHERE s.LIBRARY_ID = ? \
         ORDER BY lower(sms.LABEL), sms.LABEL",
        "SELECT DISTINCT sms.LABEL AS VALUE \
         FROM SERIES_METADATA_SHARING sms \
         JOIN SERIES s ON s.ID = sms.SERIES_ID \
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
        "SELECT DISTINCT bm.RELEASE_DATE AS VALUE \
         FROM BOOK_METADATA bm \
         JOIN BOOK b ON b.ID = bm.BOOK_ID \
         WHERE b.LIBRARY_ID = ? \
         AND bm.RELEASE_DATE IS NOT NULL \
         AND bm.RELEASE_DATE <> '' \
         ORDER BY bm.RELEASE_DATE",
        "SELECT DISTINCT bm.RELEASE_DATE AS VALUE \
         FROM BOOK_METADATA bm \
         JOIN BOOK b ON b.ID = bm.BOOK_ID \
         WHERE bm.RELEASE_DATE IS NOT NULL \
         AND bm.RELEASE_DATE <> '' \
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

    Ok(values)
}

#[derive(Clone)]
struct PersistedSeriesBrowseQuery {
    library_ids: Option<Vec<String>>,
    collection_ids: Option<Vec<String>>,
    titles: Option<Vec<String>>,
    titles_excluded: Option<Vec<String>>,
    titles_contains: Option<Vec<String>>,
    titles_contains_excluded: Option<Vec<String>>,
    titles_begins_with: Option<Vec<String>>,
    titles_begins_with_excluded: Option<Vec<String>>,
    titles_ends_with: Option<Vec<String>>,
    titles_ends_with_excluded: Option<Vec<String>>,
    title_sorts: Option<Vec<String>>,
    title_sorts_excluded: Option<Vec<String>>,
    title_sorts_contains: Option<Vec<String>>,
    title_sorts_contains_excluded: Option<Vec<String>>,
    title_sorts_begins_with: Option<Vec<String>>,
    title_sorts_begins_with_excluded: Option<Vec<String>>,
    title_sorts_ends_with: Option<Vec<String>>,
    title_sorts_ends_with_excluded: Option<Vec<String>>,
    deleted: Option<bool>,
    oneshot: Option<bool>,
    read_statuses: Option<Vec<String>>,
    read_statuses_excluded: Option<Vec<String>>,
    complete: Option<bool>,
    genres: Option<Vec<String>>,
    genres_excluded: Option<Vec<String>>,
    genres_null: Option<bool>,
    tags: Option<Vec<String>>,
    tags_excluded: Option<Vec<String>>,
    tags_null: Option<bool>,
    languages: Option<Vec<String>>,
    languages_excluded: Option<Vec<String>>,
    publishers: Option<Vec<String>>,
    publishers_excluded: Option<Vec<String>>,
    age_ratings: Option<Vec<u16>>,
    age_ratings_excluded: Option<Vec<u16>>,
    age_ratings_null: Option<bool>,
    age_rating_gt: Option<u16>,
    age_rating_lt: Option<u16>,
    sharing_labels: Option<Vec<String>>,
    sharing_labels_excluded: Option<Vec<String>>,
    sharing_labels_null: Option<bool>,
    authors: Option<Vec<String>>,
    authors_excluded: Option<Vec<String>>,
    release_dates: Option<Vec<String>>,
    release_dates_excluded: Option<Vec<String>>,
    release_dates_null: Option<bool>,
    release_date_gt: Option<String>,
    release_date_lt: Option<String>,
    release_date_begins_with: Option<Vec<String>>,
    release_date_ends_with: Option<Vec<String>>,
    release_date_contains_excluded: Option<Vec<String>>,
    release_date_begins_with_excluded: Option<Vec<String>>,
    release_date_ends_with_excluded: Option<Vec<String>>,
    release_date_in_last_days: Option<i64>,
    release_date_not_in_last_days: Option<i64>,
    series_statuses: Option<Vec<String>>,
    series_statuses_excluded: Option<Vec<String>>,
    search: Option<String>,
    search_regex: Option<(String, String)>,
    page: usize,
    size: usize,
    unpaged: bool,
    sort_modes: Vec<PersistedSeriesSortMode>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    created: String,
    last_modified: String,
    file_last_modified: String,
    books_count: u64,
    books_read_count: u64,
    books_unread_count: u64,
    books_in_progress_count: u64,
    status: String,
    summary: String,
    reading_direction: String,
    publisher: String,
    age_rating: Option<u16>,
    language: String,
    genres: Vec<String>,
    tags: Vec<String>,
    alternate_titles: Vec<String>,
    metadata_created: String,
    metadata_last_modified: String,
    books_metadata_authors: Vec<String>,
    books_metadata_tags: Vec<String>,
    books_metadata_release_date: Option<String>,
    books_metadata_summary: String,
    books_metadata_summary_number: String,
    books_metadata_created: String,
    books_metadata_last_modified: String,
    deleted: bool,
    oneshot: bool,
}

async fn load_persisted_series_page(
    database_file: &FsPath,
    context: &DiscoveryQueryContext,
    query: PersistedSeriesBrowseQuery,
) -> Result<PageEnvelope<PersistedSeriesSummary>, String> {
    let mut series = load_persisted_series_summaries(database_file).await?;

    if let Some(allowed_ids) = context.authorized_library_ids.as_ref() {
        series.retain(|row| allowed_ids.iter().any(|id| id == row.library_id.as_str()));
    }
    if let Some(library_ids) = query.library_ids.as_ref() {
        series.retain(|row| library_ids.iter().any(|id| id == row.library_id.as_str()));
    }

    if let Some(titles) = query.titles.as_ref() {
        series.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            titles.iter().any(|value| normalized == *value)
        });
    }

    if let Some(titles_excluded) = query.titles_excluded.as_ref() {
        series.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            !titles_excluded.iter().any(|value| normalized == *value)
        });
    }

    if let Some(titles_contains) = query.titles_contains.as_ref() {
        series.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            titles_contains
                .iter()
                .any(|value| normalized.contains(value))
        });
    }

    if let Some(titles_contains_excluded) = query.titles_contains_excluded.as_ref() {
        series.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            !titles_contains_excluded
                .iter()
                .any(|value| normalized.contains(value))
        });
    }

    if let Some(titles_begins_with) = query.titles_begins_with.as_ref() {
        series.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            titles_begins_with
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(titles_begins_with_excluded) = query.titles_begins_with_excluded.as_ref() {
        series.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            !titles_begins_with_excluded
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(titles_ends_with) = query.titles_ends_with.as_ref() {
        series.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            titles_ends_with
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(titles_ends_with_excluded) = query.titles_ends_with_excluded.as_ref() {
        series.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            !titles_ends_with_excluded
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(title_sorts) = query.title_sorts.as_ref() {
        series.retain(|row| {
            let normalized = row.title_sort.to_ascii_lowercase();
            title_sorts.iter().any(|value| normalized == *value)
        });
    }

    if let Some(title_sorts_excluded) = query.title_sorts_excluded.as_ref() {
        series.retain(|row| {
            let normalized = row.title_sort.to_ascii_lowercase();
            !title_sorts_excluded
                .iter()
                .any(|value| normalized == *value)
        });
    }

    if let Some(title_sorts_contains) = query.title_sorts_contains.as_ref() {
        series.retain(|row| {
            let normalized = row.title_sort.to_ascii_lowercase();
            title_sorts_contains
                .iter()
                .any(|value| normalized.contains(value))
        });
    }

    if let Some(title_sorts_contains_excluded) = query.title_sorts_contains_excluded.as_ref() {
        series.retain(|row| {
            let normalized = row.title_sort.to_ascii_lowercase();
            !title_sorts_contains_excluded
                .iter()
                .any(|value| normalized.contains(value))
        });
    }

    if let Some(title_sorts_begins_with) = query.title_sorts_begins_with.as_ref() {
        series.retain(|row| {
            let normalized = row.title_sort.to_ascii_lowercase();
            title_sorts_begins_with
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(title_sorts_begins_with_excluded) = query.title_sorts_begins_with_excluded.as_ref()
    {
        series.retain(|row| {
            let normalized = row.title_sort.to_ascii_lowercase();
            !title_sorts_begins_with_excluded
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(title_sorts_ends_with) = query.title_sorts_ends_with.as_ref() {
        series.retain(|row| {
            let normalized = row.title_sort.to_ascii_lowercase();
            title_sorts_ends_with
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(title_sorts_ends_with_excluded) = query.title_sorts_ends_with_excluded.as_ref() {
        series.retain(|row| {
            let normalized = row.title_sort.to_ascii_lowercase();
            !title_sorts_ends_with_excluded
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(deleted) = query.deleted {
        series.retain(|row| row.deleted == deleted);
    }

    if let Some(oneshot) = query.oneshot {
        series.retain(|row| row.oneshot == oneshot);
    }

    if query.read_statuses.is_some() || query.read_statuses_excluded.is_some() {
        let Some(user_id) = context.user_id.as_deref() else {
            series.clear();
            let page = PageEnvelope::from_slice(vec![], query.page, query.size, 0);
            return Ok(page);
        };

        let read_progress = load_series_read_progress_counts(database_file, user_id).await?;

        if let Some(read_statuses) = query.read_statuses.as_ref() {
            series.retain(|row| {
                read_statuses.iter().any(|status| {
                    series_matches_read_status(
                        row,
                        read_progress.get(&row.id).copied(),
                        status.as_str(),
                    )
                })
            });
        }

        if let Some(read_statuses_excluded) = query.read_statuses_excluded.as_ref() {
            series.retain(|row| {
                !read_statuses_excluded.iter().any(|status| {
                    series_matches_read_status(
                        row,
                        read_progress.get(&row.id).copied(),
                        status.as_str(),
                    )
                })
            });
        }
    }

    if let Some(complete) = query.complete {
        let total_book_counts = load_series_total_book_counts(database_file).await?;
        series.retain(|row| {
            let Some(total_book_count) = total_book_counts.get(&row.id).copied() else {
                return false;
            };
            let total_book_count = total_book_count.max(0) as u64;
            if complete {
                total_book_count == row.books_count
            } else {
                total_book_count != row.books_count
            }
        });
    }

    if let Some(genres) = query.genres.as_ref() {
        series.retain(|row| {
            row.genres.iter().any(|genre| {
                let normalized = genre.to_ascii_lowercase();
                genres.iter().any(|value| normalized.contains(value))
            })
        });
    }

    if let Some(genres_excluded) = query.genres_excluded.as_ref() {
        series.retain(|row| {
            !row.genres.iter().any(|genre| {
                let normalized = genre.to_ascii_lowercase();
                genres_excluded
                    .iter()
                    .any(|value| normalized.contains(value))
            })
        });
    }

    if let Some(genres_null) = query.genres_null {
        series.retain(|row| row.genres.is_empty() == genres_null);
    }

    if let Some(tags) = query.tags.as_ref() {
        series.retain(|row| {
            row.tags.iter().any(|tag| {
                let normalized = tag.to_ascii_lowercase();
                tags.iter().any(|value| normalized.contains(value))
            })
        });
    }

    if let Some(tags_excluded) = query.tags_excluded.as_ref() {
        series.retain(|row| {
            !row.tags.iter().any(|tag| {
                let normalized = tag.to_ascii_lowercase();
                tags_excluded.iter().any(|value| normalized.contains(value))
            })
        });
    }

    if let Some(tags_null) = query.tags_null {
        series.retain(|row| row.tags.is_empty() == tags_null);
    }

    if let Some(languages) = query.languages.as_ref() {
        series.retain(|row| {
            languages
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&row.language))
        });
    }

    if let Some(languages_excluded) = query.languages_excluded.as_ref() {
        series.retain(|row| {
            !languages_excluded
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&row.language))
        });
    }

    if let Some(publishers) = query.publishers.as_ref() {
        series.retain(|row| {
            publishers
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&row.publisher))
        });
    }

    if let Some(publishers_excluded) = query.publishers_excluded.as_ref() {
        series.retain(|row| {
            !publishers_excluded
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&row.publisher))
        });
    }

    if let Some(age_ratings) = query.age_ratings.as_ref() {
        series.retain(|row| {
            row.age_rating
                .map(|rating| age_ratings.iter().any(|value| *value == rating))
                .unwrap_or(false)
        });
    }

    if let Some(age_ratings_excluded) = query.age_ratings_excluded.as_ref() {
        series.retain(|row| {
            row.age_rating
                .map(|rating| !age_ratings_excluded.iter().any(|value| *value == rating))
                .unwrap_or(true)
        });
    }

    if let Some(age_ratings_null) = query.age_ratings_null {
        series.retain(|row| row.age_rating.is_none() == age_ratings_null);
    }

    if let Some(age_rating_gt) = query.age_rating_gt {
        series.retain(|row| {
            row.age_rating
                .map(|rating| rating > age_rating_gt)
                .unwrap_or(false)
        });
    }

    if let Some(age_rating_lt) = query.age_rating_lt {
        series.retain(|row| {
            row.age_rating
                .map(|rating| rating < age_rating_lt)
                .unwrap_or(false)
        });
    }

    if let Some(sharing_labels) = query.sharing_labels.as_ref() {
        series.retain(|row| {
            row.labels.iter().any(|label| {
                let normalized = label.to_ascii_lowercase();
                sharing_labels
                    .iter()
                    .any(|value| normalized.contains(value))
            })
        });
    }

    if let Some(sharing_labels_excluded) = query.sharing_labels_excluded.as_ref() {
        series.retain(|row| {
            !row.labels.iter().any(|label| {
                let normalized = label.to_ascii_lowercase();
                sharing_labels_excluded
                    .iter()
                    .any(|value| normalized.contains(value))
            })
        });
    }

    if let Some(sharing_labels_null) = query.sharing_labels_null {
        series.retain(|row| row.labels.is_empty() == sharing_labels_null);
    }

    if let Some(authors) = query.authors.as_ref() {
        series.retain(|row| {
            row.books_metadata_authors.iter().any(|author| {
                let normalized = author.to_ascii_lowercase();
                authors
                    .iter()
                    .any(|value| author_value_matches(&normalized, value))
            })
        });
    }

    if let Some(authors_excluded) = query.authors_excluded.as_ref() {
        series.retain(|row| {
            !row.books_metadata_authors.iter().any(|author| {
                let normalized = author.to_ascii_lowercase();
                authors_excluded
                    .iter()
                    .any(|value| author_value_matches(&normalized, value))
            })
        });
    }

    if let Some(series_statuses) = query.series_statuses.as_ref() {
        series.retain(|row| {
            series_statuses
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&row.status))
        });
    }

    if let Some(series_statuses_excluded) = query.series_statuses_excluded.as_ref() {
        series.retain(|row| {
            !series_statuses_excluded
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&row.status))
        });
    }

    if let Some(release_dates) = query.release_dates.as_ref() {
        series.retain(|row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return false;
            };
            release_dates.iter().any(|value| value == release_date)
        });
    }

    if let Some(release_dates_excluded) = query.release_dates_excluded.as_ref() {
        series.retain(|row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return true;
            };
            !release_dates_excluded
                .iter()
                .any(|value| value == release_date)
        });
    }

    if let Some(release_dates_null) = query.release_dates_null {
        series.retain(|row| row.books_metadata_release_date.is_none() == release_dates_null);
    }

    if let Some(release_date_gt) = query.release_date_gt.as_ref() {
        series.retain(|row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return false;
            };
            release_date > release_date_gt
        });
    }

    if let Some(release_date_lt) = query.release_date_lt.as_ref() {
        series.retain(|row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return false;
            };
            release_date < release_date_lt
        });
    }

    if let Some(release_date_in_last_days) = query.release_date_in_last_days
        && let Some(cutoff) =
            persisted_utc_date_minus_days(database_file, release_date_in_last_days).await?
    {
        series.retain(|row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return false;
            };
            release_date > &cutoff
        });
    }

    if let Some(release_date_not_in_last_days) = query.release_date_not_in_last_days
        && let Some(cutoff) =
            persisted_utc_date_minus_days(database_file, release_date_not_in_last_days).await?
    {
        series.retain(|row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return false;
            };
            release_date < &cutoff
        });
    }

    if let Some(release_date_begins_with) = query.release_date_begins_with.as_ref() {
        series.retain(|row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return false;
            };
            let normalized = release_date.to_ascii_lowercase();
            release_date_begins_with
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(release_date_ends_with) = query.release_date_ends_with.as_ref() {
        series.retain(|row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return false;
            };
            let normalized = release_date.to_ascii_lowercase();
            release_date_ends_with
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(release_date_begins_with_excluded) =
        query.release_date_begins_with_excluded.as_ref()
    {
        series.retain(|row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return true;
            };
            let normalized = release_date.to_ascii_lowercase();
            !release_date_begins_with_excluded
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(release_date_ends_with_excluded) = query.release_date_ends_with_excluded.as_ref() {
        series.retain(|row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return true;
            };
            let normalized = release_date.to_ascii_lowercase();
            !release_date_ends_with_excluded
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(release_date_contains_excluded) = query.release_date_contains_excluded.as_ref() {
        series.retain(|row| {
            let Some(release_date) = row.books_metadata_release_date.as_ref() else {
                return true;
            };
            let normalized = release_date.to_ascii_lowercase();
            !release_date_contains_excluded
                .iter()
                .any(|value| normalized.contains(value))
        });
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

    series.sort_by(|left, right| {
        for sort_mode in &query.sort_modes {
            let ordering = match sort_mode {
                PersistedSeriesSortMode::TitleAsc => left
                    .title_sort
                    .to_ascii_lowercase()
                    .cmp(&right.title_sort.to_ascii_lowercase()),
                PersistedSeriesSortMode::Latest => right.last_modified.cmp(&left.last_modified),
            };
            if ordering != std::cmp::Ordering::Equal {
                return ordering;
            }
        }
        left.id.cmp(&right.id)
    });

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
        content,
        page,
        page_size,
        total_elements,
    ))
}

async fn load_persisted_alphabetical_groups(
    database_file: &FsPath,
    context: &DiscoveryQueryContext,
    filters: NativeSeriesFilters,
    full_text_search: Option<String>,
    search_regex: Option<(String, String)>,
) -> Result<Vec<Value>, String> {
    let page = load_persisted_series_page(
        database_file,
        context,
        PersistedSeriesBrowseQuery {
            library_ids: filters.library_ids,
            collection_ids: filters.collection_ids,
            titles: filters.titles,
            titles_excluded: filters.titles_excluded,
            titles_contains: filters.titles_contains,
            titles_contains_excluded: filters.titles_contains_excluded,
            titles_begins_with: filters.titles_begins_with,
            titles_begins_with_excluded: filters.titles_begins_with_excluded,
            titles_ends_with: filters.titles_ends_with,
            titles_ends_with_excluded: filters.titles_ends_with_excluded,
            title_sorts: filters.title_sorts,
            title_sorts_excluded: filters.title_sorts_excluded,
            title_sorts_contains: filters.title_sorts_contains,
            title_sorts_contains_excluded: filters.title_sorts_contains_excluded,
            title_sorts_begins_with: filters.title_sorts_begins_with,
            title_sorts_begins_with_excluded: filters.title_sorts_begins_with_excluded,
            title_sorts_ends_with: filters.title_sorts_ends_with,
            title_sorts_ends_with_excluded: filters.title_sorts_ends_with_excluded,
            deleted: filters.deleted,
            oneshot: filters.oneshot,
            read_statuses: filters.read_statuses,
            read_statuses_excluded: filters.read_statuses_excluded,
            complete: filters.complete,
            genres: filters.genres,
            genres_excluded: filters.genres_excluded,
            genres_null: filters.genres_null,
            tags: filters.tags,
            tags_excluded: filters.tags_excluded,
            tags_null: filters.tags_null,
            languages: filters.languages,
            languages_excluded: filters.languages_excluded,
            publishers: filters.publishers,
            publishers_excluded: filters.publishers_excluded,
            age_ratings: filters.age_ratings,
            age_ratings_excluded: filters.age_ratings_excluded,
            age_ratings_null: filters.age_ratings_null,
            age_rating_gt: filters.age_rating_gt,
            age_rating_lt: filters.age_rating_lt,
            sharing_labels: filters.sharing_labels,
            sharing_labels_excluded: filters.sharing_labels_excluded,
            sharing_labels_null: filters.sharing_labels_null,
            authors: filters.authors,
            authors_excluded: filters.authors_excluded,
            release_dates: filters.release_dates,
            release_dates_excluded: filters.release_dates_excluded,
            release_dates_null: filters.release_dates_null,
            release_date_gt: filters.release_date_gt,
            release_date_lt: filters.release_date_lt,
            release_date_begins_with: filters.release_date_begins_with,
            release_date_ends_with: filters.release_date_ends_with,
            release_date_contains_excluded: filters.release_date_contains_excluded,
            release_date_begins_with_excluded: filters.release_date_begins_with_excluded,
            release_date_ends_with_excluded: filters.release_date_ends_with_excluded,
            release_date_in_last_days: filters.release_date_in_last_days,
            release_date_not_in_last_days: filters.release_date_not_in_last_days,
            series_statuses: filters.series_statuses,
            series_statuses_excluded: filters.series_statuses_excluded,
            search: full_text_search,
            search_regex,
            page: 0,
            size: usize::MAX,
            unpaged: true,
            sort_modes: vec![PersistedSeriesSortMode::TitleAsc],
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
        r#"SELECT s.ID,
                  s.LIBRARY_ID,
                  s.CREATED_DATE,
                  s.LAST_MODIFIED_DATE,
                  CAST(s.FILE_LAST_MODIFIED AS TEXT) AS FILE_LAST_MODIFIED,
                  s.BOOK_COUNT,
                  s.DELETED_DATE,
                  s.ONESHOT,
                  COALESCE(sm.TITLE, s.NAME) AS TITLE,
                  COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) AS TITLE_SORT,
                  COALESCE(sm.STATUS, 'ONGOING') AS STATUS,
                  COALESCE(sm.SUMMARY, '') AS SUMMARY,
                  COALESCE(sm.READING_DIRECTION, '') AS READING_DIRECTION,
                  COALESCE(sm.PUBLISHER, '') AS PUBLISHER,
                  sm.AGE_RATING AS AGE_RATING,
                  sm.TOTAL_BOOK_COUNT AS TOTAL_BOOK_COUNT,
                  COALESCE(sm.LANGUAGE, '') AS LANGUAGE,
                  COALESCE(sm.CREATED_DATE, s.CREATED_DATE) AS METADATA_CREATED,
                  COALESCE(sm.LAST_MODIFIED_DATE, s.LAST_MODIFIED_DATE) AS METADATA_LAST_MODIFIED,
                  COALESCE(bma.RELEASE_DATE, NULL) AS BOOKS_METADATA_RELEASE_DATE,
                  COALESCE(bma.SUMMARY, '') AS BOOKS_METADATA_SUMMARY,
                  COALESCE(bma.SUMMARY_NUMBER, '') AS BOOKS_METADATA_SUMMARY_NUMBER,
                  COALESCE(bma.CREATED_DATE, s.CREATED_DATE) AS BOOKS_METADATA_CREATED,
                  COALESCE(bma.LAST_MODIFIED_DATE, s.LAST_MODIFIED_DATE) AS BOOKS_METADATA_LAST_MODIFIED,
                  COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS LABELS,
                  COALESCE(GROUP_CONCAT(DISTINCT smg.GENRE), '') AS GENRES,
                  COALESCE(GROUP_CONCAT(DISTINCT smt.TAG), '') AS TAGS,
                  COALESCE(GROUP_CONCAT(DISTINCT smat.TITLE), '') AS ALTERNATE_TITLES,
                  COALESCE(
                    GROUP_CONCAT(
                      DISTINCT CASE
                        WHEN bmaa.ROLE IS NULL OR bmaa.ROLE = '' THEN bmaa.NAME
                        ELSE bmaa.NAME || '::' || bmaa.ROLE
                      END
                    ),
                    ''
                  ) AS BOOKS_METADATA_AUTHORS,
                  COALESCE(GROUP_CONCAT(DISTINCT bmat.TAG), '') AS BOOKS_METADATA_TAGS
           FROM SERIES s
           LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
           LEFT JOIN BOOK_METADATA_AGGREGATION bma ON bma.SERIES_ID = s.ID
           LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
           LEFT JOIN SERIES_METADATA_GENRE smg ON smg.SERIES_ID = s.ID
           LEFT JOIN SERIES_METADATA_TAG smt ON smt.SERIES_ID = s.ID
           LEFT JOIN SERIES_METADATA_ALTERNATE_TITLE smat ON smat.SERIES_ID = s.ID
           LEFT JOIN BOOK_METADATA_AGGREGATION_AUTHOR bmaa ON bmaa.SERIES_ID = s.ID
           LEFT JOIN BOOK_METADATA_AGGREGATION_TAG bmat ON bmat.SERIES_ID = s.ID
           GROUP BY s.ID,
                    s.LIBRARY_ID,
                    s.CREATED_DATE,
                    s.LAST_MODIFIED_DATE,
                    s.FILE_LAST_MODIFIED,
                    s.BOOK_COUNT,
                    s.DELETED_DATE,
                    s.ONESHOT,
                    sm.TITLE,
                    sm.TITLE_SORT,
                    sm.STATUS,
                    sm.SUMMARY,
                    sm.READING_DIRECTION,
                    sm.PUBLISHER,
                    sm.AGE_RATING,
                    sm.TOTAL_BOOK_COUNT,
                    sm.LANGUAGE,
                    sm.CREATED_DATE,
                    sm.LAST_MODIFIED_DATE,
                    bma.RELEASE_DATE,
                    bma.SUMMARY,
                    bma.SUMMARY_NUMBER,
                    bma.CREATED_DATE,
                    bma.LAST_MODIFIED_DATE,
                    s.NAME"#,
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
            created: row.get::<String, _>("CREATED_DATE"),
            last_modified: row.get::<String, _>("LAST_MODIFIED_DATE"),
            file_last_modified: row.get::<String, _>("FILE_LAST_MODIFIED"),
            books_count: row.get::<i64, _>("BOOK_COUNT").max(0) as u64,
            books_read_count: 0,
            books_unread_count: row.get::<i64, _>("BOOK_COUNT").max(0) as u64,
            books_in_progress_count: 0,
            status: row.get::<String, _>("STATUS"),
            summary: row.get::<String, _>("SUMMARY"),
            reading_direction: row.get::<String, _>("READING_DIRECTION"),
            publisher: row.get::<String, _>("PUBLISHER"),
            age_rating: row
                .get::<Option<i64>, _>("AGE_RATING")
                .map(|value| value as u16),
            language: row.get::<String, _>("LANGUAGE"),
            genres: parse_csv_values(&row.get::<String, _>("GENRES")),
            tags: parse_csv_values(&row.get::<String, _>("TAGS")),
            alternate_titles: parse_csv_values(&row.get::<String, _>("ALTERNATE_TITLES")),
            metadata_created: row.get::<String, _>("METADATA_CREATED"),
            metadata_last_modified: row.get::<String, _>("METADATA_LAST_MODIFIED"),
            books_metadata_authors: parse_csv_values(
                &row.get::<String, _>("BOOKS_METADATA_AUTHORS"),
            ),
            books_metadata_tags: parse_csv_values(&row.get::<String, _>("BOOKS_METADATA_TAGS")),
            books_metadata_release_date: row
                .get::<Option<String>, _>("BOOKS_METADATA_RELEASE_DATE"),
            books_metadata_summary: row.get::<String, _>("BOOKS_METADATA_SUMMARY"),
            books_metadata_summary_number: row.get::<String, _>("BOOKS_METADATA_SUMMARY_NUMBER"),
            books_metadata_created: row.get::<String, _>("BOOKS_METADATA_CREATED"),
            books_metadata_last_modified: row.get::<String, _>("BOOKS_METADATA_LAST_MODIFIED"),
            deleted: row.get::<Option<String>, _>("DELETED_DATE").is_some(),
            oneshot: row
                .try_get::<bool, _>("ONESHOT")
                .ok()
                .or_else(|| {
                    row.try_get::<i64, _>("ONESHOT")
                        .ok()
                        .map(|value| value != 0)
                })
                .unwrap_or(false),
        })
        .collect();

    Ok(summaries)
}

async fn load_persisted_library_ids(database_file: &FsPath) -> Result<Vec<String>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open persisted browse-library db: {error}"))?;

    let rows = sqlx::query(
        "SELECT LIBRARY_ID AS ID \
         FROM ( SELECT DISTINCT LIBRARY_ID \
         FROM SERIES \
         WHERE DELETED_DATE IS NULL \
         UNION SELECT DISTINCT LIBRARY_ID \
         FROM BOOK \
         WHERE DELETED_DATE IS NULL ) \
         ORDER BY ID COLLATE NOCASE ASC, ID ASC",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted browse-library ids: {error}"))?;

    let ids = rows
        .into_iter()
        .map(|row| row.get::<String, _>("ID"))
        .collect();

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

    let rows = sqlx::query(
        "SELECT SERIES_ID, COLLECTION_ID \
                            FROM COLLECTION_SERIES",
    )
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

    Ok(memberships)
}

async fn load_readlist_memberships(
    database_file: &FsPath,
) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open readlist memberships db: {error}"))?;

    let rows = sqlx::query(
        "SELECT BOOK_ID, READLIST_ID \
                            FROM READLIST_BOOK",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query readlist memberships: {error}"))?;

    let mut memberships = BTreeMap::<String, BTreeSet<String>>::new();
    for row in rows {
        memberships
            .entry(row.get::<String, _>("BOOK_ID"))
            .or_default()
            .insert(row.get::<String, _>("READLIST_ID"));
    }

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

fn invalid_native_series_list_response(error: DiscoveryError) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "error": format!("invalid native series request: {error:?}"),
        })),
    )
        .into_response()
}

fn invalid_native_books_list_response(error: DiscoveryError) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "error": format!("invalid native books request: {error:?}"),
        })),
    )
        .into_response()
}

fn empty_books_page_response(uri: &Uri, is_admin: bool) -> Response {
    let query = uri.query().unwrap_or_default();
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query, "unpaged");
    let page_number = if unpaged { 0 } else { page };
    let page_size = if unpaged { 1 } else { size };

    let mut response = Json(books_page_payload(
        PageEnvelope::from_slice(vec![], page_number, page_size, 0),
        is_admin,
        !unpaged,
    ))
    .into_response();
    mark_non_native(&mut response);
    response
}

#[derive(Clone, Debug, Default)]
struct NativeSeriesFilters {
    library_ids: Option<Vec<String>>,
    collection_ids: Option<Vec<String>>,
    titles: Option<Vec<String>>,
    titles_excluded: Option<Vec<String>>,
    titles_contains: Option<Vec<String>>,
    titles_contains_excluded: Option<Vec<String>>,
    titles_begins_with: Option<Vec<String>>,
    titles_begins_with_excluded: Option<Vec<String>>,
    titles_ends_with: Option<Vec<String>>,
    titles_ends_with_excluded: Option<Vec<String>>,
    title_sorts: Option<Vec<String>>,
    title_sorts_excluded: Option<Vec<String>>,
    title_sorts_contains: Option<Vec<String>>,
    title_sorts_contains_excluded: Option<Vec<String>>,
    title_sorts_begins_with: Option<Vec<String>>,
    title_sorts_begins_with_excluded: Option<Vec<String>>,
    title_sorts_ends_with: Option<Vec<String>>,
    title_sorts_ends_with_excluded: Option<Vec<String>>,
    deleted: Option<bool>,
    oneshot: Option<bool>,
    read_statuses: Option<Vec<String>>,
    read_statuses_excluded: Option<Vec<String>>,
    genres: Option<Vec<String>>,
    genres_excluded: Option<Vec<String>>,
    genres_null: Option<bool>,
    tags: Option<Vec<String>>,
    tags_excluded: Option<Vec<String>>,
    tags_null: Option<bool>,
    languages: Option<Vec<String>>,
    languages_excluded: Option<Vec<String>>,
    publishers: Option<Vec<String>>,
    publishers_excluded: Option<Vec<String>>,
    age_ratings: Option<Vec<u16>>,
    age_ratings_excluded: Option<Vec<u16>>,
    age_ratings_null: Option<bool>,
    age_rating_gt: Option<u16>,
    age_rating_lt: Option<u16>,
    release_dates: Option<Vec<String>>,
    release_dates_excluded: Option<Vec<String>>,
    release_dates_null: Option<bool>,
    release_date_gt: Option<String>,
    release_date_lt: Option<String>,
    release_date_begins_with: Option<Vec<String>>,
    release_date_ends_with: Option<Vec<String>>,
    release_date_contains_excluded: Option<Vec<String>>,
    release_date_begins_with_excluded: Option<Vec<String>>,
    release_date_ends_with_excluded: Option<Vec<String>>,
    release_date_in_last_days: Option<i64>,
    release_date_not_in_last_days: Option<i64>,
    sharing_labels: Option<Vec<String>>,
    sharing_labels_excluded: Option<Vec<String>>,
    sharing_labels_null: Option<bool>,
    series_statuses: Option<Vec<String>>,
    series_statuses_excluded: Option<Vec<String>>,
    complete: Option<bool>,
    authors: Option<Vec<String>>,
    authors_excluded: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default)]
struct NativeBooksFilters {
    direct_browse_family: Option<DirectBrowseBooksListFamily>,
    library_ids: Option<Vec<String>>,
    series_ids: Option<Vec<String>>,
    series_ids_excluded: Option<Vec<String>>,
    read_list_ids: Option<Vec<String>>,
    read_list_ids_excluded: Option<Vec<String>>,
    titles: Option<Vec<String>>,
    titles_excluded: Option<Vec<String>>,
    titles_contains: Option<Vec<String>>,
    titles_contains_excluded: Option<Vec<String>>,
    titles_begins_with: Option<Vec<String>>,
    titles_begins_with_excluded: Option<Vec<String>>,
    titles_ends_with: Option<Vec<String>>,
    titles_ends_with_excluded: Option<Vec<String>>,
    deleted: Option<bool>,
    oneshot: Option<bool>,
    tags: Option<Vec<String>>,
    tags_excluded: Option<Vec<String>>,
    tags_null: Option<bool>,
    read_statuses: Option<Vec<String>>,
    read_statuses_excluded: Option<Vec<String>>,
    media_profiles: Option<Vec<String>>,
    media_profiles_excluded: Option<Vec<String>>,
    media_statuses: Option<Vec<String>>,
    media_statuses_excluded: Option<Vec<String>>,
    authors: Option<Vec<String>>,
    authors_excluded: Option<Vec<String>>,
    poster_types: Option<Vec<String>>,
    poster_types_excluded: Option<Vec<String>>,
    poster_selected: Option<bool>,
    poster_selected_excluded: Option<bool>,
    release_dates: Option<Vec<String>>,
    release_dates_excluded: Option<Vec<String>>,
    release_dates_null: Option<bool>,
    release_date_gt: Option<String>,
    release_date_lt: Option<String>,
    release_date_begins_with: Option<Vec<String>>,
    release_date_ends_with: Option<Vec<String>>,
    release_date_contains_excluded: Option<Vec<String>>,
    release_date_begins_with_excluded: Option<Vec<String>>,
    release_date_ends_with_excluded: Option<Vec<String>>,
    release_date_in_last_days: Option<i64>,
    release_date_not_in_last_days: Option<i64>,
    number_sorts: Option<Vec<f64>>,
    number_sorts_excluded: Option<Vec<f64>>,
    number_sort_gt: Option<f64>,
    number_sort_lt: Option<f64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PersistedBooksSortMode {
    TitleAsc,
    CreatedDateDesc,
    LastModifiedDateDesc,
    ReleaseDateDesc,
}

#[derive(Clone)]
struct PersistedBooksBrowseQuery {
    library_ids: Option<Vec<String>>,
    series_ids: Option<Vec<String>>,
    series_ids_excluded: Option<Vec<String>>,
    read_list_ids: Option<Vec<String>>,
    read_list_ids_excluded: Option<Vec<String>>,
    titles: Option<Vec<String>>,
    titles_excluded: Option<Vec<String>>,
    titles_contains: Option<Vec<String>>,
    titles_contains_excluded: Option<Vec<String>>,
    titles_begins_with: Option<Vec<String>>,
    titles_begins_with_excluded: Option<Vec<String>>,
    titles_ends_with: Option<Vec<String>>,
    titles_ends_with_excluded: Option<Vec<String>>,
    deleted: Option<bool>,
    oneshot: Option<bool>,
    tags: Option<Vec<String>>,
    tags_excluded: Option<Vec<String>>,
    tags_null: Option<bool>,
    media_profiles: Option<Vec<String>>,
    media_profiles_excluded: Option<Vec<String>>,
    authors: Option<Vec<String>>,
    authors_excluded: Option<Vec<String>>,
    poster_types: Option<Vec<String>>,
    poster_types_excluded: Option<Vec<String>>,
    poster_selected: Option<bool>,
    poster_selected_excluded: Option<bool>,
    media_statuses: Option<Vec<String>>,
    media_statuses_excluded: Option<Vec<String>>,
    read_statuses: Option<Vec<String>>,
    read_statuses_excluded: Option<Vec<String>>,
    release_dates: Option<Vec<String>>,
    release_dates_excluded: Option<Vec<String>>,
    release_dates_null: Option<bool>,
    release_date_gt: Option<String>,
    release_date_lt: Option<String>,
    release_date_begins_with: Option<Vec<String>>,
    release_date_ends_with: Option<Vec<String>>,
    release_date_contains_excluded: Option<Vec<String>>,
    release_date_begins_with_excluded: Option<Vec<String>>,
    release_date_ends_with_excluded: Option<Vec<String>>,
    release_date_in_last_days: Option<i64>,
    release_date_not_in_last_days: Option<i64>,
    number_sorts: Option<Vec<f64>>,
    number_sorts_excluded: Option<Vec<f64>>,
    number_sort_gt: Option<f64>,
    number_sort_lt: Option<f64>,
    search: Option<String>,
    page: usize,
    size: usize,
    unpaged: bool,
    sort_modes: Vec<PersistedBooksSortMode>,
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
    read_status: String,
    metadata_number_sort: Option<f64>,
    metadata_release_date: Option<String>,
    deleted: bool,
    oneshot: bool,
    labels: Vec<String>,
    metadata_tags: Vec<String>,
    metadata_authors: Vec<String>,
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

    let sort_modes = parse_persisted_books_sort_modes(sorts);
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
                series_ids: filters.series_ids.clone(),
                series_ids_excluded: filters.series_ids_excluded.clone(),
                read_list_ids: filters.read_list_ids.clone(),
                read_list_ids_excluded: filters.read_list_ids_excluded.clone(),
                titles: filters.titles.clone(),
                titles_excluded: filters.titles_excluded.clone(),
                titles_contains: filters.titles_contains.clone(),
                titles_contains_excluded: filters.titles_contains_excluded.clone(),
                titles_begins_with: filters.titles_begins_with.clone(),
                titles_begins_with_excluded: filters.titles_begins_with_excluded.clone(),
                titles_ends_with: filters.titles_ends_with.clone(),
                titles_ends_with_excluded: filters.titles_ends_with_excluded.clone(),
                deleted: filters.deleted,
                oneshot: filters.oneshot,
                tags: filters.tags.clone(),
                tags_excluded: filters.tags_excluded.clone(),
                tags_null: filters.tags_null,
                media_profiles: filters.media_profiles.clone(),
                media_profiles_excluded: filters.media_profiles_excluded.clone(),
                authors: filters.authors.clone(),
                authors_excluded: filters.authors_excluded.clone(),
                poster_types: filters.poster_types.clone(),
                poster_types_excluded: filters.poster_types_excluded.clone(),
                poster_selected: filters.poster_selected,
                poster_selected_excluded: filters.poster_selected_excluded,
                media_statuses: filters.media_statuses.clone(),
                media_statuses_excluded: filters.media_statuses_excluded.clone(),
                read_statuses: filters.read_statuses.clone(),
                read_statuses_excluded: filters.read_statuses_excluded.clone(),
                release_dates: filters.release_dates.clone(),
                release_dates_excluded: filters.release_dates_excluded.clone(),
                release_dates_null: filters.release_dates_null,
                release_date_gt: filters.release_date_gt.clone(),
                release_date_lt: filters.release_date_lt.clone(),
                release_date_begins_with: filters.release_date_begins_with.clone(),
                release_date_ends_with: filters.release_date_ends_with.clone(),
                release_date_contains_excluded: filters.release_date_contains_excluded.clone(),
                release_date_begins_with_excluded: filters
                    .release_date_begins_with_excluded
                    .clone(),
                release_date_ends_with_excluded: filters.release_date_ends_with_excluded.clone(),
                release_date_in_last_days: filters.release_date_in_last_days,
                release_date_not_in_last_days: filters.release_date_not_in_last_days,
                number_sorts: filters.number_sorts.clone(),
                number_sorts_excluded: filters.number_sorts_excluded.clone(),
                number_sort_gt: filters.number_sort_gt,
                number_sort_lt: filters.number_sort_lt,
                search: full_text_search,
                page,
                size,
                unpaged,
                sort_modes,
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
) -> Option<Result<PageEnvelope<PersistedSeriesSummary>, String>> {
    if !database_file.exists() || !native_series_filters_persisted_compatible(filters) {
        return None;
    }

    let sort_modes = parse_persisted_series_sort_modes(sorts);
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
                collection_ids: filters.collection_ids.clone(),
                titles: filters.titles.clone(),
                titles_excluded: filters.titles_excluded.clone(),
                titles_contains: filters.titles_contains.clone(),
                titles_contains_excluded: filters.titles_contains_excluded.clone(),
                titles_begins_with: filters.titles_begins_with.clone(),
                titles_begins_with_excluded: filters.titles_begins_with_excluded.clone(),
                titles_ends_with: filters.titles_ends_with.clone(),
                titles_ends_with_excluded: filters.titles_ends_with_excluded.clone(),
                title_sorts: filters.title_sorts.clone(),
                title_sorts_excluded: filters.title_sorts_excluded.clone(),
                title_sorts_contains: filters.title_sorts_contains.clone(),
                title_sorts_contains_excluded: filters.title_sorts_contains_excluded.clone(),
                title_sorts_begins_with: filters.title_sorts_begins_with.clone(),
                title_sorts_begins_with_excluded: filters.title_sorts_begins_with_excluded.clone(),
                title_sorts_ends_with: filters.title_sorts_ends_with.clone(),
                title_sorts_ends_with_excluded: filters.title_sorts_ends_with_excluded.clone(),
                deleted: filters.deleted,
                oneshot: filters.oneshot,
                read_statuses: filters.read_statuses.clone(),
                read_statuses_excluded: filters.read_statuses_excluded.clone(),
                complete: filters.complete,
                genres: filters.genres.clone(),
                genres_excluded: filters.genres_excluded.clone(),
                genres_null: filters.genres_null,
                tags: filters.tags.clone(),
                tags_excluded: filters.tags_excluded.clone(),
                tags_null: filters.tags_null,
                languages: filters.languages.clone(),
                languages_excluded: filters.languages_excluded.clone(),
                publishers: filters.publishers.clone(),
                publishers_excluded: filters.publishers_excluded.clone(),
                age_ratings: filters.age_ratings.clone(),
                age_ratings_excluded: filters.age_ratings_excluded.clone(),
                age_ratings_null: filters.age_ratings_null,
                age_rating_gt: filters.age_rating_gt,
                age_rating_lt: filters.age_rating_lt,
                sharing_labels: filters.sharing_labels.clone(),
                sharing_labels_excluded: filters.sharing_labels_excluded.clone(),
                sharing_labels_null: filters.sharing_labels_null,
                authors: filters.authors.clone(),
                authors_excluded: filters.authors_excluded.clone(),
                release_dates: filters.release_dates.clone(),
                release_dates_excluded: filters.release_dates_excluded.clone(),
                release_dates_null: filters.release_dates_null,
                release_date_gt: filters.release_date_gt.clone(),
                release_date_lt: filters.release_date_lt.clone(),
                release_date_begins_with: filters.release_date_begins_with.clone(),
                release_date_ends_with: filters.release_date_ends_with.clone(),
                release_date_contains_excluded: filters.release_date_contains_excluded.clone(),
                release_date_begins_with_excluded: filters
                    .release_date_begins_with_excluded
                    .clone(),
                release_date_ends_with_excluded: filters.release_date_ends_with_excluded.clone(),
                release_date_in_last_days: filters.release_date_in_last_days,
                release_date_not_in_last_days: filters.release_date_not_in_last_days,
                series_statuses: filters.series_statuses.clone(),
                series_statuses_excluded: filters.series_statuses_excluded.clone(),
                search: full_text_search,
                search_regex: None,
                page,
                size,
                unpaged: false,
                sort_modes,
            },
        )
        .await,
    )
}

fn parse_persisted_books_sort_modes(sorts: &[String]) -> Vec<PersistedBooksSortMode> {
    let mut modes = sorts
        .iter()
        .filter_map(|sort| match sort.as_str() {
            "metadata.title,asc" | "series,metadata.numberSort,asc" => {
                Some(PersistedBooksSortMode::TitleAsc)
            }
            "createdDate,desc" => Some(PersistedBooksSortMode::CreatedDateDesc),
            "lastModifiedDate,desc" => Some(PersistedBooksSortMode::LastModifiedDateDesc),
            "metadata.releaseDate,desc" => Some(PersistedBooksSortMode::ReleaseDateDesc),
            _ => None,
        })
        .collect::<Vec<_>>();
    modes.dedup();
    if modes.is_empty() {
        modes.push(PersistedBooksSortMode::TitleAsc);
    }
    modes
}

fn parse_persisted_series_sort_modes(sorts: &[String]) -> Vec<PersistedSeriesSortMode> {
    let mut modes = sorts
        .iter()
        .filter_map(|sort| match sort.as_str() {
            "metadata.titleSort,asc" => Some(PersistedSeriesSortMode::TitleAsc),
            "createdDate,desc" | "lastModifiedDate,desc" | "booksMetadata.releaseDate,desc" => {
                Some(PersistedSeriesSortMode::Latest)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    modes.dedup();
    if modes.is_empty() {
        modes.push(PersistedSeriesSortMode::TitleAsc);
    }
    modes
}

fn extract_regex_search(payload: &Value) -> Option<(String, String)> {
    let regex_search = payload
        .get("regexSearch")
        .or_else(|| payload.get("searchRegex"))?;
    let regex = regex_search
        .get("regex")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let field = regex_search
        .get("field")
        .and_then(Value::as_str)
        .map(|value| value.to_ascii_lowercase())
        .and_then(|value| match value.as_str() {
            "title" => Some("title".to_string()),
            "title_sort" => Some("title_sort".to_string()),
            _ => None,
        })
        .unwrap_or_else(|| "title".to_string());
    Some((regex, field))
}

fn media_profile_for_media_type(media_type: &str) -> &'static str {
    match media_type {
        "application/zip"
        | "application/x-rar-compressed"
        | "application/x-rar-compressed; version=4"
        | "application/x-rar-compressed; version=5" => "divina",
        "application/epub+zip" => "epub",
        "application/pdf" => "pdf",
        _ => "",
    }
}

fn native_books_filters_persisted_compatible(filters: &NativeBooksFilters) -> bool {
    let _ = filters;
    true
}

fn native_series_filters_persisted_compatible(filters: &NativeSeriesFilters) -> bool {
    let _ = filters;
    true
}

async fn persisted_books_exist(database_file: &FsPath) -> Result<bool, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open persisted books db: {error}"))?;
    let row = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
                           FROM BOOK \
                           WHERE DELETED_DATE IS NULL",
    )
    .fetch_one(&pool)
    .await
    .map_err(|error| format!("query persisted books count: {error}"))?;

    Ok(row.get::<i64, _>("COUNT") > 0)
}

async fn persisted_series_exist(database_file: &FsPath) -> Result<bool, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open persisted series db: {error}"))?;
    let row = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
                           FROM SERIES \
                           WHERE DELETED_DATE IS NULL",
    )
    .fetch_one(&pool)
    .await
    .map_err(|error| format!("query persisted series count: {error}"))?;

    Ok(row.get::<i64, _>("COUNT") > 0)
}

async fn persisted_utc_date_minus_days(
    database_file: &FsPath,
    days: i64,
) -> Result<Option<String>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open persisted date db: {error}"))?;

    let modifier = if days >= 0 {
        format!("-{days} days")
    } else {
        format!("+{} days", days.saturating_abs())
    };

    let row = sqlx::query("SELECT date('now', ?) AS CUTOFF")
        .bind(modifier)
        .fetch_one(&pool)
        .await
        .map_err(|error| format!("query persisted utc cutoff date: {error}"))?;

    let cutoff = row.get::<Option<String>, _>("CUTOFF");
    Ok(cutoff)
}

async fn load_series_read_progress_counts(
    database_file: &FsPath,
    user_id: &str,
) -> Result<HashMap<String, (i64, i64)>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series read-progress db: {error}"))?;

    let rows = sqlx::query(
        "SELECT SERIES_ID, READ_COUNT, IN_PROGRESS_COUNT \
         FROM READ_PROGRESS_SERIES \
         WHERE USER_ID = ?",
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query series read-progress counts: {error}"))?;

    let mut counts = HashMap::new();
    for row in rows {
        let series_id = row.get::<String, _>("SERIES_ID");
        let read_count = row.get::<i64, _>("READ_COUNT");
        let in_progress_count = row.get::<i64, _>("IN_PROGRESS_COUNT");
        counts.insert(series_id, (read_count, in_progress_count));
    }

    Ok(counts)
}

async fn load_series_total_book_counts(
    database_file: &FsPath,
) -> Result<HashMap<String, i64>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series metadata db: {error}"))?;

    let rows = sqlx::query(
        "SELECT SERIES_ID, TOTAL_BOOK_COUNT \
         FROM SERIES_METADATA \
         WHERE TOTAL_BOOK_COUNT IS NOT NULL",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query series total-book-counts: {error}"))?;

    let mut totals = HashMap::new();
    for row in rows {
        let series_id = row.get::<String, _>("SERIES_ID");
        let total_book_count = row.get::<i64, _>("TOTAL_BOOK_COUNT");
        totals.insert(series_id, total_book_count);
    }

    Ok(totals)
}

fn series_matches_read_status(
    row: &PersistedSeriesSummary,
    read_progress: Option<(i64, i64)>,
    status: &str,
) -> bool {
    match status.to_ascii_lowercase().as_str() {
        "unread" => read_progress.is_none(),
        "read" => read_progress
            .map(|(read_count, _)| read_count.max(0) as u64 == row.books_count)
            .unwrap_or(false),
        "in_progress" | "inprogress" => read_progress
            .map(|(read_count, _)| read_count.max(0) as u64 != row.books_count)
            .unwrap_or(false),
        _ => false,
    }
}

#[derive(Clone)]
struct PersistedBookPosterSummary {
    thumbnail_type: String,
    selected: bool,
}

fn poster_matches(
    poster: &PersistedBookPosterSummary,
    poster_types: Option<&Vec<String>>,
    poster_selected: Option<bool>,
) -> bool {
    let type_matches = poster_types
        .map(|values| {
            values
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&poster.thumbnail_type))
        })
        .unwrap_or(true);
    let selected_matches = poster_selected
        .map(|value| poster.selected == value)
        .unwrap_or(true);
    type_matches && selected_matches
}

fn author_value_matches(author: &str, expected: &str) -> bool {
    if let Some((expected_name, expected_role)) = expected.split_once("::") {
        let (author_name, author_role) = author
            .split_once("::")
            .map(|(name, role)| (name, Some(role)))
            .unwrap_or((author, None));

        if expected_name.is_empty() {
            return author_role
                .map(|role| role.eq_ignore_ascii_case(expected_role))
                .unwrap_or(false);
        }

        if expected_role.is_empty() {
            return author_name.eq_ignore_ascii_case(expected_name);
        }

        return author_name.eq_ignore_ascii_case(expected_name)
            && author_role
                .map(|role| role.eq_ignore_ascii_case(expected_role))
                .unwrap_or(false);
    }

    author.contains(expected)
}

async fn load_book_poster_summaries(
    database_file: &FsPath,
) -> Result<HashMap<String, Vec<PersistedBookPosterSummary>>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book poster db: {error}"))?;

    let rows = sqlx::query(
        "SELECT BOOK_ID, TYPE, SELECTED \
                            FROM THUMBNAIL_BOOK",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query book posters: {error}"))?;

    let mut posters: HashMap<String, Vec<PersistedBookPosterSummary>> = HashMap::new();
    for row in rows {
        let book_id = row.get::<String, _>("BOOK_ID");
        let poster = PersistedBookPosterSummary {
            thumbnail_type: row.get::<String, _>("TYPE"),
            selected: row.get::<i64, _>("SELECTED") != 0,
        };
        posters.entry(book_id).or_default().push(poster);
    }

    Ok(posters)
}

async fn load_persisted_books_page(
    database_file: &FsPath,
    context: &DiscoveryQueryContext,
    query: PersistedBooksBrowseQuery,
) -> Result<PageEnvelope<BookReadModel>, String> {
    let mut books =
        load_persisted_book_summaries(database_file, context.user_id.as_deref()).await?;

    if let Some(allowed_ids) = context.authorized_library_ids.as_ref() {
        books.retain(|row| allowed_ids.iter().any(|id| id == row.library_id.as_str()));
    }
    if let Some(library_ids) = query.library_ids.as_ref() {
        books.retain(|row| library_ids.iter().any(|id| id == row.library_id.as_str()));
    }

    if let Some(series_ids) = query.series_ids.as_ref() {
        books.retain(|row| series_ids.iter().any(|id| id == row.series_id.as_str()));
    }

    if let Some(series_ids_excluded) = query.series_ids_excluded.as_ref() {
        books.retain(|row| {
            !series_ids_excluded
                .iter()
                .any(|id| id == row.series_id.as_str())
        });
    }

    if let Some(read_list_ids) = query.read_list_ids.as_ref() {
        let memberships = load_readlist_memberships(database_file).await?;
        books.retain(|row| {
            memberships
                .get(&row.id)
                .into_iter()
                .flatten()
                .any(|read_list_id| read_list_ids.iter().any(|id| id == read_list_id))
        });
    }

    if let Some(read_list_ids_excluded) = query.read_list_ids_excluded.as_ref() {
        let memberships = load_readlist_memberships(database_file).await?;
        books.retain(|row| {
            !memberships
                .get(&row.id)
                .into_iter()
                .flatten()
                .any(|read_list_id| read_list_ids_excluded.iter().any(|id| id == read_list_id))
        });
    }

    if let Some(titles) = query.titles.as_ref() {
        books.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            titles.iter().any(|value| normalized == *value)
        });
    }

    if let Some(titles_excluded) = query.titles_excluded.as_ref() {
        books.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            !titles_excluded.iter().any(|value| normalized == *value)
        });
    }

    if let Some(titles_contains) = query.titles_contains.as_ref() {
        books.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            titles_contains
                .iter()
                .any(|value| normalized.contains(value))
        });
    }

    if let Some(titles_contains_excluded) = query.titles_contains_excluded.as_ref() {
        books.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            !titles_contains_excluded
                .iter()
                .any(|value| normalized.contains(value))
        });
    }

    if let Some(titles_begins_with) = query.titles_begins_with.as_ref() {
        books.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            titles_begins_with
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(titles_begins_with_excluded) = query.titles_begins_with_excluded.as_ref() {
        books.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            !titles_begins_with_excluded
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(titles_ends_with) = query.titles_ends_with.as_ref() {
        books.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            titles_ends_with
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(titles_ends_with_excluded) = query.titles_ends_with_excluded.as_ref() {
        books.retain(|row| {
            let normalized = row.title.to_ascii_lowercase();
            !titles_ends_with_excluded
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(tags) = query.tags.as_ref() {
        books.retain(|row| {
            row.metadata_tags.iter().any(|tag| {
                let normalized = tag.to_ascii_lowercase();
                tags.iter().any(|value| normalized == *value)
            })
        });
    }

    if let Some(tags_excluded) = query.tags_excluded.as_ref() {
        books.retain(|row| {
            !row.metadata_tags.iter().any(|tag| {
                let normalized = tag.to_ascii_lowercase();
                tags_excluded.iter().any(|value| normalized == *value)
            })
        });
    }

    if let Some(tags_null) = query.tags_null {
        books.retain(|row| row.metadata_tags.is_empty() == tags_null);
    }

    if let Some(authors) = query.authors.as_ref() {
        books.retain(|row| {
            row.metadata_authors.iter().any(|author| {
                let normalized = author.to_ascii_lowercase();
                authors
                    .iter()
                    .any(|value| author_value_matches(&normalized, value))
            })
        });
    }

    if let Some(authors_excluded) = query.authors_excluded.as_ref() {
        books.retain(|row| {
            !row.metadata_authors.iter().any(|author| {
                let normalized = author.to_ascii_lowercase();
                authors_excluded
                    .iter()
                    .any(|value| author_value_matches(&normalized, value))
            })
        });
    }

    if query.poster_types.is_some()
        || query.poster_types_excluded.is_some()
        || query.poster_selected.is_some()
        || query.poster_selected_excluded.is_some()
    {
        let posters = load_book_poster_summaries(database_file).await?;

        if query.poster_types.is_some() || query.poster_selected.is_some() {
            books.retain(|row| {
                posters.get(&row.id).into_iter().flatten().any(|poster| {
                    poster_matches(poster, query.poster_types.as_ref(), query.poster_selected)
                })
            });
        }

        if query.poster_types_excluded.is_some() || query.poster_selected_excluded.is_some() {
            books.retain(|row| {
                !posters.get(&row.id).into_iter().flatten().any(|poster| {
                    poster_matches(
                        poster,
                        query.poster_types_excluded.as_ref(),
                        query.poster_selected_excluded,
                    )
                })
            });
        }
    }

    if let Some(media_profiles) = query.media_profiles.as_ref() {
        books.retain(|row| {
            let profile = media_profile_for_media_type(&row.media_type);
            media_profiles
                .iter()
                .any(|value| value.eq_ignore_ascii_case(profile))
        });
    }

    if let Some(media_profiles_excluded) = query.media_profiles_excluded.as_ref() {
        books.retain(|row| {
            let profile = media_profile_for_media_type(&row.media_type);
            !media_profiles_excluded
                .iter()
                .any(|value| value.eq_ignore_ascii_case(profile))
        });
    }

    if let Some(deleted) = query.deleted {
        books.retain(|row| row.deleted == deleted);
    }

    if let Some(oneshot) = query.oneshot {
        books.retain(|row| row.oneshot == oneshot);
    }

    if let Some(number_sorts) = query.number_sorts.as_ref() {
        books.retain(|row| {
            row.metadata_number_sort
                .map(|number_sort| {
                    number_sorts
                        .iter()
                        .any(|value| (number_sort - *value).abs() <= f64::EPSILON)
                })
                .unwrap_or(false)
        });
    }

    if let Some(number_sorts_excluded) = query.number_sorts_excluded.as_ref() {
        books.retain(|row| {
            row.metadata_number_sort
                .map(|number_sort| {
                    !number_sorts_excluded
                        .iter()
                        .any(|value| (number_sort - *value).abs() <= f64::EPSILON)
                })
                .unwrap_or(false)
        });
    }

    if let Some(number_sort_gt) = query.number_sort_gt {
        books.retain(|row| {
            row.metadata_number_sort
                .map(|number_sort| number_sort > number_sort_gt)
                .unwrap_or(false)
        });
    }

    if let Some(number_sort_lt) = query.number_sort_lt {
        books.retain(|row| {
            row.metadata_number_sort
                .map(|number_sort| number_sort < number_sort_lt)
                .unwrap_or(false)
        });
    }

    if let Some(media_statuses) = query.media_statuses.as_ref() {
        books.retain(|row| {
            media_statuses
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&row.media_status))
        });
    }

    if let Some(media_statuses_excluded) = query.media_statuses_excluded.as_ref() {
        books.retain(|row| {
            !media_statuses_excluded
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&row.media_status))
        });
    }

    if let Some(read_statuses) = query.read_statuses.as_ref() {
        if context.user_id.is_none() {
            books.clear();
        } else {
            books.retain(|row| {
                read_statuses
                    .iter()
                    .any(|value| value.eq_ignore_ascii_case(&row.read_status))
            });
        }
    }

    if let Some(read_statuses_excluded) = query.read_statuses_excluded.as_ref() {
        if context.user_id.is_none() {
            books.clear();
        } else {
            books.retain(|row| {
                !read_statuses_excluded
                    .iter()
                    .any(|value| value.eq_ignore_ascii_case(&row.read_status))
            });
        }
    }

    if let Some(release_dates) = query.release_dates.as_ref() {
        books.retain(|row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return false;
            };
            release_dates.iter().any(|value| value == release_date)
        });
    }

    if let Some(release_dates_excluded) = query.release_dates_excluded.as_ref() {
        books.retain(|row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return true;
            };
            !release_dates_excluded
                .iter()
                .any(|value| value == release_date)
        });
    }

    if let Some(release_dates_null) = query.release_dates_null {
        books.retain(|row| row.metadata_release_date.is_none() == release_dates_null);
    }

    if let Some(release_date_gt) = query.release_date_gt.as_ref() {
        books.retain(|row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return false;
            };
            release_date > release_date_gt
        });
    }

    if let Some(release_date_lt) = query.release_date_lt.as_ref() {
        books.retain(|row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return false;
            };
            release_date < release_date_lt
        });
    }

    if let Some(release_date_in_last_days) = query.release_date_in_last_days
        && let Some(cutoff) =
            persisted_utc_date_minus_days(database_file, release_date_in_last_days).await?
    {
        books.retain(|row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return false;
            };
            release_date > &cutoff
        });
    }

    if let Some(release_date_not_in_last_days) = query.release_date_not_in_last_days
        && let Some(cutoff) =
            persisted_utc_date_minus_days(database_file, release_date_not_in_last_days).await?
    {
        books.retain(|row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return false;
            };
            release_date < &cutoff
        });
    }

    if let Some(release_date_begins_with) = query.release_date_begins_with.as_ref() {
        books.retain(|row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return false;
            };
            let normalized = release_date.to_ascii_lowercase();
            release_date_begins_with
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(release_date_ends_with) = query.release_date_ends_with.as_ref() {
        books.retain(|row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return false;
            };
            let normalized = release_date.to_ascii_lowercase();
            release_date_ends_with
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(release_date_begins_with_excluded) =
        query.release_date_begins_with_excluded.as_ref()
    {
        books.retain(|row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return true;
            };
            let normalized = release_date.to_ascii_lowercase();
            !release_date_begins_with_excluded
                .iter()
                .any(|value| normalized.starts_with(value))
        });
    }

    if let Some(release_date_ends_with_excluded) = query.release_date_ends_with_excluded.as_ref() {
        books.retain(|row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return true;
            };
            let normalized = release_date.to_ascii_lowercase();
            !release_date_ends_with_excluded
                .iter()
                .any(|value| normalized.ends_with(value))
        });
    }

    if let Some(release_date_contains_excluded) = query.release_date_contains_excluded.as_ref() {
        books.retain(|row| {
            let Some(release_date) = row.metadata_release_date.as_ref() else {
                return true;
            };
            let normalized = release_date.to_ascii_lowercase();
            !release_date_contains_excluded
                .iter()
                .any(|value| normalized.contains(value))
        });
    }

    if let Some(search) = query.search.as_ref() {
        let normalized = search.trim().to_ascii_lowercase();
        if !normalized.is_empty() {
            books.retain(|row| row.title.to_ascii_lowercase().contains(&normalized));
        }
    }

    books.sort_by(|left, right| {
        for sort_mode in &query.sort_modes {
            let ordering = match sort_mode {
                PersistedBooksSortMode::TitleAsc => left
                    .title
                    .to_ascii_lowercase()
                    .cmp(&right.title.to_ascii_lowercase()),
                PersistedBooksSortMode::CreatedDateDesc => right.created.cmp(&left.created),
                PersistedBooksSortMode::LastModifiedDateDesc => {
                    right.last_modified.cmp(&left.last_modified)
                }
                PersistedBooksSortMode::ReleaseDateDesc => {
                    right.metadata_release_date.cmp(&left.metadata_release_date)
                }
            };
            if ordering != std::cmp::Ordering::Equal {
                return ordering;
            }
        }
        left.id.cmp(&right.id)
    });

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
    user_id: Option<&str>,
) -> Result<Vec<PersistedBookSummary>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open books db: {error}"))?;

    let rows = if let Some(user_id) = user_id {
        sqlx::query(
            r#"SELECT b.ID,
                      b.SERIES_ID,
                      b.LIBRARY_ID,
                      b.URL,
                      b.CREATED_DATE,
                      b.LAST_MODIFIED_DATE,
                      CAST(b.FILE_LAST_MODIFIED AS TEXT) AS FILE_LAST_MODIFIED,
                      b.FILE_SIZE,
                      s.ONESHOT AS ONESHOT,
                      b.DELETED_DATE,
                      COALESCE(sm.TITLE, s.NAME) AS SERIES_TITLE,
                      COALESCE(bm.TITLE, b.NAME) AS TITLE,
                      bm.NUMBER_SORT AS NUMBER_SORT,
                      bm.RELEASE_DATE AS RELEASE_DATE,
                      COALESCE(m.STATUS, 'UNKNOWN') AS MEDIA_STATUS,
                      COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
                      COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT,
                      COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS LABELS,
                      COALESCE(GROUP_CONCAT(DISTINCT bmt.TAG), '') AS METADATA_TAGS,
                      COALESCE(
                        GROUP_CONCAT(
                          DISTINCT CASE
                            WHEN bma.ROLE IS NULL OR bma.ROLE = '' THEN bma.NAME
                            ELSE bma.NAME || '::' || bma.ROLE
                          END
                        ),
                        ''
                      ) AS METADATA_AUTHORS,
                      CASE
                        WHEN rp.BOOK_ID IS NULL THEN 'unread'
                        WHEN rp.COMPLETED = 1 THEN 'read'
                        ELSE 'in_progress'
                      END AS READ_STATUS
               FROM BOOK b
               JOIN SERIES s ON s.ID = b.SERIES_ID
               LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
               LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
               LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
               LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
               LEFT JOIN BOOK_METADATA_TAG bmt ON bmt.BOOK_ID = b.ID
               LEFT JOIN BOOK_METADATA_AUTHOR bma ON bma.BOOK_ID = b.ID
               LEFT JOIN READ_PROGRESS rp ON rp.BOOK_ID = b.ID
                                      AND rp.USER_ID = ?
               GROUP BY b.ID,
                        b.SERIES_ID,
                        b.LIBRARY_ID,
                        b.URL,
                        b.CREATED_DATE,
                        b.LAST_MODIFIED_DATE,
                        b.FILE_LAST_MODIFIED,
                        b.FILE_SIZE,
                        s.ONESHOT,
                        b.DELETED_DATE,
                        sm.TITLE,
                        s.NAME,
                        bm.TITLE,
                        b.NAME,
                        bm.NUMBER_SORT,
                        bm.RELEASE_DATE,
                        m.STATUS,
                        m.MEDIA_TYPE,
                        m.PAGE_COUNT,
                        rp.BOOK_ID,
                        rp.COMPLETED"#,
        )
        .bind(user_id)
        .fetch_all(&pool)
        .await
    } else {
        sqlx::query(
            r#"SELECT b.ID,
                      b.SERIES_ID,
                      b.LIBRARY_ID,
                      b.URL,
                      b.CREATED_DATE,
                      b.LAST_MODIFIED_DATE,
                      CAST(b.FILE_LAST_MODIFIED AS TEXT) AS FILE_LAST_MODIFIED,
                      b.FILE_SIZE,
                      s.ONESHOT AS ONESHOT,
                      b.DELETED_DATE,
                      COALESCE(sm.TITLE, s.NAME) AS SERIES_TITLE,
                      COALESCE(bm.TITLE, b.NAME) AS TITLE,
                      bm.NUMBER_SORT AS NUMBER_SORT,
                      bm.RELEASE_DATE AS RELEASE_DATE,
                      COALESCE(m.STATUS, 'UNKNOWN') AS MEDIA_STATUS,
                      COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
                      COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT,
                      COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS LABELS,
                      COALESCE(GROUP_CONCAT(DISTINCT bmt.TAG), '') AS METADATA_TAGS,
                      COALESCE(
                        GROUP_CONCAT(
                          DISTINCT CASE
                            WHEN bma.ROLE IS NULL OR bma.ROLE = '' THEN bma.NAME
                            ELSE bma.NAME || '::' || bma.ROLE
                          END
                        ),
                        ''
                      ) AS METADATA_AUTHORS,
                      'unread' AS READ_STATUS
               FROM BOOK b
               JOIN SERIES s ON s.ID = b.SERIES_ID
               LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
               LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
               LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
               LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
               LEFT JOIN BOOK_METADATA_TAG bmt ON bmt.BOOK_ID = b.ID
               LEFT JOIN BOOK_METADATA_AUTHOR bma ON bma.BOOK_ID = b.ID
               GROUP BY b.ID,
                        b.SERIES_ID,
                        b.LIBRARY_ID,
                        b.URL,
                        b.CREATED_DATE,
                        b.LAST_MODIFIED_DATE,
                        b.FILE_LAST_MODIFIED,
                        b.FILE_SIZE,
                        s.ONESHOT,
                        b.DELETED_DATE,
                        sm.TITLE,
                        s.NAME,
                        bm.TITLE,
                        b.NAME,
                        bm.NUMBER_SORT,
                        bm.RELEASE_DATE,
                        m.STATUS,
                        m.MEDIA_TYPE,
                        m.PAGE_COUNT"#,
        )
        .fetch_all(&pool)
        .await
    }
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
                read_status: row.get::<String, _>("READ_STATUS"),
                metadata_number_sort: row.get::<Option<f64>, _>("NUMBER_SORT"),
                metadata_release_date: row.get::<Option<String>, _>("RELEASE_DATE"),
                deleted: row.get::<Option<String>, _>("DELETED_DATE").is_some(),
                oneshot: row.get::<bool, _>("ONESHOT"),
                labels: parse_csv_values(&row.get::<String, _>("LABELS")),
                metadata_tags: parse_csv_values(&row.get::<String, _>("METADATA_TAGS")),
                metadata_authors: parse_csv_values(&row.get::<String, _>("METADATA_AUTHORS")),
            }
        })
        .collect();

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
    let mut oneshot_bootstrap_series_id = exact_oneshot_bootstrap_series_id(payload);

    if !strict_native_shape
        && oneshot_bootstrap_series_id.is_some()
        && !query_string.trim().is_empty()
    {
        return None;
    }

    if strict_native_shape {
        oneshot_bootstrap_series_id = None;
    }

    let mut filters = match if strict_native_shape {
        parse_native_books_filters_with_mode(
            payload.and_then(|value| value.get("condition")),
            OperatorValidationMode::Strict,
        )
    } else {
        parse_native_books_filters(payload.and_then(|value| value.get("condition")))
    } {
        Ok(filters) => filters,
        Err(error) => {
            if strict_native_shape {
                return Some(invalid_native_books_list_response(error));
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

    if strict_native_shape {
        if !native_books_filters_persisted_compatible(&filters) {
            return Some(invalid_native_books_list_response(
                DiscoveryError::InvalidRequest(
                    "unsupported native books filter combination".to_string(),
                ),
            ));
        }
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

    None
}

async fn native_owned_books_latest_response(
    headers: &HeaderMap,
    uri: &Uri,
    auth_state: &DiscoveryAuthState,
    database_file: &FsPath,
) -> Option<Response> {
    let sorts = query_values(uri.query().unwrap_or_default(), "sort");
    if !sorts.is_empty() {
        return None;
    }

    if !database_file.exists() {
        return None;
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

    match load_persisted_books_page(
        database_file,
        &context,
        PersistedBooksBrowseQuery {
            library_ids: None,
            series_ids: None,
            series_ids_excluded: None,
            read_list_ids: None,
            read_list_ids_excluded: None,
            titles: None,
            titles_excluded: None,
            titles_contains: None,
            titles_contains_excluded: None,
            titles_begins_with: None,
            titles_begins_with_excluded: None,
            titles_ends_with: None,
            titles_ends_with_excluded: None,
            deleted: None,
            oneshot: None,
            tags: None,
            tags_excluded: None,
            tags_null: None,
            media_profiles: None,
            media_profiles_excluded: None,
            authors: None,
            authors_excluded: None,
            poster_types: None,
            poster_types_excluded: None,
            poster_selected: None,
            poster_selected_excluded: None,
            media_statuses: None,
            media_statuses_excluded: None,
            read_statuses: None,
            read_statuses_excluded: None,
            release_dates: None,
            release_dates_excluded: None,
            release_dates_null: None,
            release_date_gt: None,
            release_date_lt: None,
            release_date_begins_with: None,
            release_date_ends_with: None,
            release_date_contains_excluded: None,
            release_date_begins_with_excluded: None,
            release_date_ends_with_excluded: None,
            release_date_in_last_days: None,
            release_date_not_in_last_days: None,
            number_sorts: None,
            number_sorts_excluded: None,
            number_sort_gt: None,
            number_sort_lt: None,
            search: None,
            page,
            size,
            unpaged,
            sort_modes: vec![PersistedBooksSortMode::LastModifiedDateDesc],
        },
    )
    .await
    {
        Ok(page) => {
            let mut response =
                Json(books_page_payload(page, context.is_admin, !unpaged)).into_response();
            mark_native(&mut response);
            Some(response)
        }
        Err(error) => Some(
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("native books latest failed: {error}") })),
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

    let parse_mode = if strict_native_shape {
        OperatorValidationMode::Strict
    } else {
        OperatorValidationMode::Lenient
    };
    let mut filters = match parse_native_series_filters_with_mode(
        payload.and_then(|value| value.get("condition")),
        parse_mode,
    ) {
        Ok(filters) => filters,
        Err(error) => {
            if strict_native_shape {
                return Some(invalid_native_series_list_response(error));
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

    if strict_native_shape {
        if !native_series_filters_persisted_compatible(&filters) {
            return Some(invalid_native_series_list_response(
                DiscoveryError::InvalidRequest(
                    "unsupported native series filter combination".to_string(),
                ),
            ));
        }
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

    None
}

fn parse_native_series_filters(
    condition: Option<&Value>,
) -> Result<NativeSeriesFilters, DiscoveryError> {
    parse_native_series_filters_with_mode(condition, OperatorValidationMode::Lenient)
}

fn parse_native_series_filters_with_mode(
    condition: Option<&Value>,
    mode: OperatorValidationMode,
) -> Result<NativeSeriesFilters, DiscoveryError> {
    let Some(condition) = condition else {
        return Ok(NativeSeriesFilters::default());
    };

    if condition.get("type").and_then(Value::as_str).is_none() {
        let normalized = normalize_webui_series_condition(condition)?;
        return parse_native_series_filters_with_mode(Some(&normalized), mode);
    }

    let Some(condition_type) = condition.get("type").and_then(Value::as_str) else {
        return Err(DiscoveryError::InvalidRequest(
            "series condition missing type".to_string(),
        ));
    };

    match condition_type {
        "LibraryId" => parse_library_id_filter(condition, mode),
        "CollectionId" => parse_collection_id_filter(condition, mode),
        "Title" => parse_series_title_filter(condition, mode),
        "TitleSort" => parse_series_title_sort_filter(condition, mode),
        "Deleted" => parse_deleted_filter(condition, mode),
        "OneShot" => parse_oneshot_filter(condition, mode),
        "ReadStatus" => parse_series_read_status_filter(condition, mode),
        "Genre" => parse_series_genre_filter(condition, mode),
        "Tag" => parse_series_tag_filter(condition, mode),
        "Language" => parse_series_language_filter(condition, mode),
        "Publisher" => parse_series_publisher_filter(condition, mode),
        "AgeRating" => parse_series_age_rating_filter(condition, mode),
        "ReleaseDate" => parse_series_release_date_filter(condition, mode),
        "SharingLabel" => parse_series_sharing_label_filter(condition, mode),
        "SeriesStatus" => parse_series_status_filter(condition, mode),
        "Complete" => parse_series_complete_filter(condition, mode),
        "Author" => parse_series_author_filter(condition, mode),
        "AllOfSeries" => parse_composite_filters(condition, true, mode),
        "AnyOfSeries" => parse_composite_filters(condition, false, mode),
        _unsupported if mode.is_strict() => Err(DiscoveryError::InvalidRequest(format!(
            "unsupported native series condition type: {condition_type}",
        ))),
        _unsupported => Ok(NativeSeriesFilters::default()),
    }
}

fn parse_native_books_filters(
    condition: Option<&Value>,
) -> Result<NativeBooksFilters, DiscoveryError> {
    parse_native_books_filters_with_mode(condition, OperatorValidationMode::Lenient)
}

fn parse_native_books_filters_with_mode(
    condition: Option<&Value>,
    mode: OperatorValidationMode,
) -> Result<NativeBooksFilters, DiscoveryError> {
    let Some(condition) = condition else {
        return Ok(NativeBooksFilters::default());
    };

    if condition.get("type").and_then(Value::as_str).is_none() {
        let normalized = normalize_webui_books_condition(condition)?;
        return parse_native_books_filters_with_mode(Some(&normalized), mode);
    }

    let Some(condition_type) = condition.get("type").and_then(Value::as_str) else {
        return Err(DiscoveryError::InvalidRequest(
            "books condition missing type".to_string(),
        ));
    };

    match condition_type {
        "LibraryId" => parse_books_library_id_filter(condition, mode),
        "SeriesId" => parse_books_series_id_filter(condition, mode),
        "ReadListId" => parse_books_read_list_id_filter(condition, mode),
        "Title" => parse_books_title_filter(condition, mode),
        "Deleted" => parse_books_deleted_filter(condition, mode),
        "OneShot" => parse_books_oneshot_filter(condition, mode),
        "Tag" => parse_books_tag_filter(condition, mode),
        "ReadStatus" => parse_books_read_status_filter(condition, mode),
        "MediaProfile" => parse_books_media_profile_filter(condition, mode),
        "MediaStatus" => parse_books_media_status_filter(condition, mode),
        "Author" => parse_books_author_filter(condition, mode),
        "Poster" => parse_books_poster_filter(condition, mode),
        "NumberSort" => parse_books_number_sort_filter(condition, mode),
        "ReleaseDate" => parse_books_release_date_filter(condition, mode),
        "AllOfBook" => parse_books_composite_filters(condition, true, mode),
        "AnyOfBook" => parse_books_composite_filters(condition, false, mode),
        _unsupported if mode.is_strict() => Err(DiscoveryError::InvalidRequest(format!(
            "unsupported native books condition type: {condition_type}",
        ))),
        _unsupported => Ok(NativeBooksFilters::default()),
    }
}

#[derive(Clone, Copy)]
enum OperatorValidationMode {
    Lenient,
    Strict,
}

impl OperatorValidationMode {
    fn is_strict(self) -> bool {
        matches!(self, Self::Strict)
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
            ("collectionId", "CollectionId"),
            ("title", "Title"),
            ("titleSort", "TitleSort"),
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
            ("readListId", "ReadListId"),
            ("title", "Title"),
            ("deleted", "Deleted"),
            ("oneShot", "OneShot"),
            ("tag", "Tag"),
            ("readStatus", "ReadStatus"),
            ("mediaProfile", "MediaProfile"),
            ("mediaStatus", "MediaStatus"),
            ("author", "Author"),
            ("numberSort", "NumberSort"),
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

    let fallback_type = if label == "books" {
        "AllOfBook"
    } else {
        "AllOfSeries"
    };

    Ok(json!({
        "type": fallback_type,
        "conditions": [],
    }))
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
    filters.collection_ids = None;
    filters.titles = None;
    filters.titles_excluded = None;
    filters.titles_contains = None;
    filters.titles_contains_excluded = None;
    filters.titles_begins_with = None;
    filters.titles_begins_with_excluded = None;
    filters.titles_ends_with = None;
    filters.titles_ends_with_excluded = None;
    filters.title_sorts = None;
    filters.title_sorts_excluded = None;
    filters.title_sorts_contains = None;
    filters.title_sorts_contains_excluded = None;
    filters.title_sorts_begins_with = None;
    filters.title_sorts_begins_with_excluded = None;
    filters.title_sorts_ends_with = None;
    filters.title_sorts_ends_with_excluded = None;
    filters.deleted = None;
    filters.oneshot = None;
    filters.read_statuses = None;
    filters.read_statuses_excluded = None;
    filters.genres = None;
    filters.genres_excluded = None;
    filters.genres_null = None;
    filters.tags = None;
    filters.tags_excluded = None;
    filters.tags_null = None;
    filters.languages = None;
    filters.publishers = None;
    filters.age_ratings = None;
    filters.release_dates = None;
    filters.release_dates_excluded = None;
    filters.release_dates_null = None;
    filters.release_date_gt = None;
    filters.release_date_lt = None;
    filters.release_date_begins_with = None;
    filters.release_date_ends_with = None;
    filters.release_date_contains_excluded = None;
    filters.release_date_begins_with_excluded = None;
    filters.release_date_ends_with_excluded = None;
    filters.release_date_in_last_days = None;
    filters.release_date_not_in_last_days = None;
    filters.sharing_labels = None;
    filters.sharing_labels_excluded = None;
    filters.sharing_labels_null = None;
    filters.series_statuses = None;
    filters.series_statuses_excluded = None;
    filters.complete = None;
    filters.authors = None;
    filters.authors_excluded = None;
}

fn coerce_legacy_books_filters_for_persisted(filters: &mut NativeBooksFilters) {
    filters.direct_browse_family = None;
    filters.series_ids = None;
    filters.series_ids_excluded = None;
    filters.read_list_ids = None;
    filters.read_list_ids_excluded = None;
    filters.titles = None;
    filters.titles_excluded = None;
    filters.titles_contains = None;
    filters.titles_contains_excluded = None;
    filters.titles_begins_with = None;
    filters.titles_begins_with_excluded = None;
    filters.titles_ends_with = None;
    filters.titles_ends_with_excluded = None;
    filters.deleted = None;
    filters.oneshot = None;
    filters.tags = None;
    filters.tags_excluded = None;
    filters.tags_null = None;
    filters.read_statuses = None;
    filters.media_profiles = None;
    filters.media_statuses = None;
    filters.media_statuses_excluded = None;
    filters.authors = None;
    filters.authors_excluded = None;
    filters.poster_types = None;
    filters.poster_types_excluded = None;
    filters.poster_selected = None;
    filters.poster_selected_excluded = None;
    filters.release_dates = None;
    filters.release_dates_excluded = None;
    filters.release_dates_null = None;
    filters.release_date_gt = None;
    filters.release_date_lt = None;
    filters.release_date_begins_with = None;
    filters.release_date_ends_with = None;
    filters.release_date_contains_excluded = None;
    filters.release_date_begins_with_excluded = None;
    filters.release_date_ends_with_excluded = None;
    filters.release_date_in_last_days = None;
    filters.release_date_not_in_last_days = None;
    filters.number_sorts = None;
    filters.number_sorts_excluded = None;
    filters.number_sort_gt = None;
    filters.number_sort_lt = None;
    filters.read_statuses_excluded = None;
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

fn parse_books_library_id_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeBooksFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(format!(
                "unsupported operator for LibraryId: {operator}",
            )));
        }
        return Ok(NativeBooksFilters::default());
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(NativeBooksFilters::default());
    };

    Ok(NativeBooksFilters {
        library_ids: Some(vec![value.to_string()]),
        ..NativeBooksFilters::default()
    })
}

fn parse_books_series_id_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeBooksFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" && operator != "isnot" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(format!(
                "unsupported operator for SeriesId: {operator}",
            )));
        }
        return Ok(NativeBooksFilters::default());
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(NativeBooksFilters::default());
    };

    if operator == "isnot" {
        return Ok(NativeBooksFilters {
            series_ids_excluded: Some(vec![value.to_string()]),
            ..NativeBooksFilters::default()
        });
    }

    Ok(NativeBooksFilters {
        direct_browse_family: Some(DirectBrowseBooksListFamily::BrowseBookSiblingsUnpaged),
        series_ids: Some(vec![value.to_string()]),
        ..NativeBooksFilters::default()
    })
}

fn parse_books_read_list_id_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeBooksFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" && operator != "isnot" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(format!(
                "unsupported operator for ReadListId: {operator}",
            )));
        }
        return Ok(NativeBooksFilters::default());
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(NativeBooksFilters::default());
    };

    if operator == "isnot" {
        return Ok(NativeBooksFilters {
            read_list_ids_excluded: Some(vec![value.to_string()]),
            ..NativeBooksFilters::default()
        });
    }

    Ok(NativeBooksFilters {
        read_list_ids: Some(vec![value.to_string()]),
        ..NativeBooksFilters::default()
    })
}

fn parse_books_title_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeBooksFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is"
        && operator != "isnot"
        && operator != "contains"
        && operator != "doesnotcontain"
        && operator != "beginswith"
        && operator != "doesnotbeginwith"
        && operator != "endswith"
        && operator != "doesnotendwith"
    {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(format!(
                "unsupported operator for Title: {operator}",
            )));
        }
        return Ok(NativeBooksFilters::default());
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(NativeBooksFilters::default());
    };
    let value = value.to_ascii_lowercase();

    Ok(match operator.as_str() {
        "is" => NativeBooksFilters {
            titles: Some(vec![value]),
            ..NativeBooksFilters::default()
        },
        "isnot" => NativeBooksFilters {
            titles_excluded: Some(vec![value]),
            ..NativeBooksFilters::default()
        },
        "contains" => NativeBooksFilters {
            titles_contains: Some(vec![value]),
            ..NativeBooksFilters::default()
        },
        "doesnotcontain" => NativeBooksFilters {
            titles_contains_excluded: Some(vec![value]),
            ..NativeBooksFilters::default()
        },
        "beginswith" => NativeBooksFilters {
            titles_begins_with: Some(vec![value]),
            ..NativeBooksFilters::default()
        },
        "doesnotbeginwith" => NativeBooksFilters {
            titles_begins_with_excluded: Some(vec![value]),
            ..NativeBooksFilters::default()
        },
        "endswith" => NativeBooksFilters {
            titles_ends_with: Some(vec![value]),
            ..NativeBooksFilters::default()
        },
        _ => NativeBooksFilters {
            titles_ends_with_excluded: Some(vec![value]),
            ..NativeBooksFilters::default()
        },
    })
}

fn parse_books_deleted_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeBooksFilters, DiscoveryError> {
    let Some(operator) = condition.get("operator").and_then(Value::as_str) else {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(
                "missing operator for Deleted".to_string(),
            ));
        }
        return Ok(NativeBooksFilters::default());
    };

    let deleted = match operator.to_ascii_lowercase().as_str() {
        "istrue" => true,
        "isfalse" => false,
        _ => {
            if mode.is_strict() {
                return Err(DiscoveryError::InvalidRequest(format!(
                    "unsupported operator for Deleted: {operator}",
                )));
            }
            return Ok(NativeBooksFilters::default());
        }
    };

    Ok(NativeBooksFilters {
        deleted: Some(deleted),
        ..NativeBooksFilters::default()
    })
}

fn parse_books_oneshot_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeBooksFilters, DiscoveryError> {
    let Some(operator) = condition.get("operator").and_then(Value::as_str) else {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(
                "missing operator for OneShot".to_string(),
            ));
        }
        return Ok(NativeBooksFilters::default());
    };

    let oneshot = match operator.to_ascii_lowercase().as_str() {
        "istrue" => true,
        "isfalse" => false,
        _ => {
            if mode.is_strict() {
                return Err(DiscoveryError::InvalidRequest(format!(
                    "unsupported operator for OneShot: {operator}",
                )));
            }
            return Ok(NativeBooksFilters::default());
        }
    };

    Ok(NativeBooksFilters {
        oneshot: Some(oneshot),
        ..NativeBooksFilters::default()
    })
}

fn parse_books_tag_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeBooksFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" && operator != "isnot" && operator != "isnull" && operator != "isnotnull" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(format!(
                "unsupported operator for Tag: {operator}",
            )));
        }
        return Ok(NativeBooksFilters::default());
    }

    if operator == "isnull" {
        return Ok(NativeBooksFilters {
            tags_null: Some(true),
            ..NativeBooksFilters::default()
        });
    }

    if operator == "isnotnull" {
        return Ok(NativeBooksFilters {
            tags_null: Some(false),
            ..NativeBooksFilters::default()
        });
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(NativeBooksFilters::default());
    };

    if operator == "isnot" {
        return Ok(NativeBooksFilters {
            tags_excluded: Some(vec![value.to_ascii_lowercase()]),
            ..NativeBooksFilters::default()
        });
    }

    Ok(NativeBooksFilters {
        tags: Some(vec![value.to_ascii_lowercase()]),
        ..NativeBooksFilters::default()
    })
}

fn parse_books_read_status_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeBooksFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" && operator != "isnot" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(format!(
                "unsupported operator for ReadStatus: {operator}",
            )));
        }
        return Ok(NativeBooksFilters::default());
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(NativeBooksFilters::default());
    };
    let normalized = value.to_ascii_lowercase();

    if operator == "isnot" {
        return Ok(NativeBooksFilters {
            read_statuses_excluded: Some(vec![normalized]),
            ..NativeBooksFilters::default()
        });
    }

    Ok(NativeBooksFilters {
        read_statuses: Some(vec![normalized]),
        ..NativeBooksFilters::default()
    })
}

fn parse_books_media_profile_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeBooksFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" && operator != "isnot" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(format!(
                "unsupported operator for MediaProfile: {operator}",
            )));
        }
        return Ok(NativeBooksFilters::default());
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(NativeBooksFilters::default());
    };
    let normalized = value.to_ascii_lowercase();
    if operator == "isnot" {
        return Ok(NativeBooksFilters {
            media_profiles_excluded: Some(vec![normalized]),
            ..NativeBooksFilters::default()
        });
    }

    Ok(NativeBooksFilters {
        media_profiles: Some(vec![normalized]),
        ..NativeBooksFilters::default()
    })
}

fn parse_books_media_status_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeBooksFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" && operator != "isnot" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(format!(
                "unsupported operator for MediaStatus: {operator}",
            )));
        }
        return Ok(NativeBooksFilters::default());
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(NativeBooksFilters::default());
    };

    if operator == "isnot" {
        return Ok(NativeBooksFilters {
            media_statuses_excluded: Some(vec![value.to_ascii_lowercase()]),
            ..NativeBooksFilters::default()
        });
    }

    Ok(NativeBooksFilters {
        media_statuses: Some(vec![value.to_ascii_lowercase()]),
        ..NativeBooksFilters::default()
    })
}

fn parse_books_author_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeBooksFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();

    if operator == "contains" {
        return parse_books_string_filter(condition, "Author", "contains_or_is", mode, |value| {
            NativeBooksFilters {
                authors: Some(vec![value]),
                ..NativeBooksFilters::default()
            }
        });
    }

    if operator != "is" && operator != "isnot" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(format!(
                "unsupported operator for Author: {operator}",
            )));
        }
        return parse_books_string_filter(condition, "Author", "contains_or_is", mode, |value| {
            NativeBooksFilters {
                authors: Some(vec![value]),
                ..NativeBooksFilters::default()
            }
        });
    }

    let Some(encoded) = parse_author_match_value(condition.get("value")) else {
        return Ok(NativeBooksFilters::default());
    };

    if operator == "isnot" {
        return Ok(NativeBooksFilters {
            authors_excluded: Some(vec![encoded]),
            ..NativeBooksFilters::default()
        });
    }

    Ok(NativeBooksFilters {
        authors: Some(vec![encoded]),
        ..NativeBooksFilters::default()
    })
}

fn parse_books_poster_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeBooksFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" && operator != "isnot" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(format!(
                "unsupported operator for Poster: {operator}",
            )));
        }
        return Ok(NativeBooksFilters::default());
    }

    let Some(value) = condition.get("value").and_then(Value::as_object) else {
        return Ok(NativeBooksFilters::default());
    };

    let poster_type = value
        .get("type")
        .and_then(Value::as_str)
        .map(|raw| raw.to_ascii_lowercase());
    let poster_selected = value.get("selected").and_then(Value::as_bool);

    if poster_type.is_none() && poster_selected.is_none() {
        return Ok(NativeBooksFilters::default());
    }

    if operator == "isnot" {
        return Ok(NativeBooksFilters {
            poster_types_excluded: poster_type.map(|value| vec![value]),
            poster_selected_excluded: poster_selected,
            ..NativeBooksFilters::default()
        });
    }

    Ok(NativeBooksFilters {
        poster_types: poster_type.map(|value| vec![value]),
        poster_selected,
        ..NativeBooksFilters::default()
    })
}

fn parse_books_number_sort_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeBooksFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is"
        && operator != "isnot"
        && operator != "greaterthan"
        && operator != "lessthan"
    {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(format!(
                "unsupported operator for NumberSort: {operator}",
            )));
        }
        return Ok(NativeBooksFilters::default());
    }

    let Some(value) = condition
        .get("value")
        .and_then(Value::as_f64)
        .or_else(|| {
            condition
                .get("value")
                .and_then(Value::as_i64)
                .map(|v| v as f64)
        })
        .or_else(|| {
            condition
                .get("value")
                .and_then(Value::as_u64)
                .map(|v| v as f64)
        })
    else {
        return Ok(NativeBooksFilters::default());
    };

    if operator == "is" {
        return Ok(NativeBooksFilters {
            number_sorts: Some(vec![value]),
            ..NativeBooksFilters::default()
        });
    }

    if operator == "isnot" {
        return Ok(NativeBooksFilters {
            number_sorts_excluded: Some(vec![value]),
            ..NativeBooksFilters::default()
        });
    }

    if operator == "greaterthan" {
        return Ok(NativeBooksFilters {
            number_sort_gt: Some(value),
            ..NativeBooksFilters::default()
        });
    }

    Ok(NativeBooksFilters {
        number_sort_lt: Some(value),
        ..NativeBooksFilters::default()
    })
}

fn parse_books_release_date_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeBooksFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is"
        && operator != "isnot"
        && operator != "isnull"
        && operator != "isnotnull"
        && operator != "greaterthan"
        && operator != "lessthan"
        && operator != "after"
        && operator != "before"
        && operator != "isinthelast"
        && operator != "isnotinthelast"
        && operator != "beginswith"
        && operator != "endswith"
        && operator != "doesnotcontain"
        && operator != "doesnotbeginwith"
        && operator != "doesnotendwith"
    {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(format!(
                "unsupported operator for ReleaseDate: {operator}",
            )));
        }
        return Ok(NativeBooksFilters::default());
    }

    if operator == "isnull" {
        return Ok(NativeBooksFilters {
            release_dates_null: Some(true),
            ..NativeBooksFilters::default()
        });
    }

    if operator == "isnotnull" {
        return Ok(NativeBooksFilters {
            release_dates_null: Some(false),
            ..NativeBooksFilters::default()
        });
    }

    if operator == "after" {
        let Some(date_time) = condition
            .get("dateTime")
            .and_then(Value::as_str)
            .and_then(normalize_release_date_date_time)
        else {
            return Ok(NativeBooksFilters::default());
        };

        return Ok(NativeBooksFilters {
            release_date_gt: Some(date_time),
            ..NativeBooksFilters::default()
        });
    }

    if operator == "before" {
        let Some(date_time) = condition
            .get("dateTime")
            .and_then(Value::as_str)
            .and_then(normalize_release_date_date_time)
        else {
            return Ok(NativeBooksFilters::default());
        };

        return Ok(NativeBooksFilters {
            release_date_lt: Some(date_time),
            ..NativeBooksFilters::default()
        });
    }

    if operator == "isinthelast" {
        let Some(days) = condition
            .get("duration")
            .and_then(Value::as_str)
            .and_then(parse_iso8601_duration_to_days)
        else {
            return Ok(NativeBooksFilters::default());
        };

        return Ok(NativeBooksFilters {
            release_date_in_last_days: Some(days),
            ..NativeBooksFilters::default()
        });
    }

    if operator == "isnotinthelast" {
        let Some(days) = condition
            .get("duration")
            .and_then(Value::as_str)
            .and_then(parse_iso8601_duration_to_days)
        else {
            return Ok(NativeBooksFilters::default());
        };

        return Ok(NativeBooksFilters {
            release_date_not_in_last_days: Some(days),
            ..NativeBooksFilters::default()
        });
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(NativeBooksFilters::default());
    };

    if operator == "greaterthan" {
        return Ok(NativeBooksFilters {
            release_date_gt: Some(value.to_ascii_lowercase()),
            ..NativeBooksFilters::default()
        });
    }

    if operator == "lessthan" {
        return Ok(NativeBooksFilters {
            release_date_lt: Some(value.to_ascii_lowercase()),
            ..NativeBooksFilters::default()
        });
    }

    if operator == "beginswith" {
        return Ok(NativeBooksFilters {
            release_date_begins_with: Some(vec![value.to_ascii_lowercase()]),
            ..NativeBooksFilters::default()
        });
    }

    if operator == "endswith" {
        return Ok(NativeBooksFilters {
            release_date_ends_with: Some(vec![value.to_ascii_lowercase()]),
            ..NativeBooksFilters::default()
        });
    }

    if operator == "doesnotcontain" {
        return Ok(NativeBooksFilters {
            release_date_contains_excluded: Some(vec![value.to_ascii_lowercase()]),
            ..NativeBooksFilters::default()
        });
    }

    if operator == "doesnotbeginwith" {
        return Ok(NativeBooksFilters {
            release_date_begins_with_excluded: Some(vec![value.to_ascii_lowercase()]),
            ..NativeBooksFilters::default()
        });
    }

    if operator == "doesnotendwith" {
        return Ok(NativeBooksFilters {
            release_date_ends_with_excluded: Some(vec![value.to_ascii_lowercase()]),
            ..NativeBooksFilters::default()
        });
    }

    if operator == "isnot" {
        return Ok(NativeBooksFilters {
            release_dates_excluded: Some(vec![value.to_ascii_lowercase()]),
            ..NativeBooksFilters::default()
        });
    }

    Ok(NativeBooksFilters {
        release_dates: Some(vec![value.to_ascii_lowercase()]),
        ..NativeBooksFilters::default()
    })
}

fn parse_series_string_filter<F>(
    condition: &Value,
    filter_name: &str,
    expected_operator: &str,
    mode: OperatorValidationMode,
    build: F,
) -> Result<NativeSeriesFilters, DiscoveryError>
where
    F: Fn(String) -> NativeSeriesFilters,
{
    ensure_series_operator(condition, filter_name, expected_operator, mode)?;
    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(NativeSeriesFilters::default());
    };

    Ok(build(value.to_ascii_lowercase()))
}

fn parse_books_string_filter<F>(
    condition: &Value,
    filter_name: &str,
    expected_operator: &str,
    mode: OperatorValidationMode,
    build: F,
) -> Result<NativeBooksFilters, DiscoveryError>
where
    F: Fn(String) -> NativeBooksFilters,
{
    ensure_books_operator(condition, filter_name, expected_operator, mode)?;
    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(NativeBooksFilters::default());
    };

    Ok(build(value.to_ascii_lowercase()))
}

fn parse_nullable_series_string_filter<FI, FE, FN>(
    condition: &Value,
    mode: OperatorValidationMode,
    filter_name: &str,
    build_include: FI,
    build_exclude: FE,
    build_null: FN,
) -> Result<NativeSeriesFilters, DiscoveryError>
where
    FI: Fn(String) -> NativeSeriesFilters,
    FE: Fn(String) -> NativeSeriesFilters,
    FN: Fn(bool) -> NativeSeriesFilters,
{
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();

    if operator != "is"
        && operator != "contains"
        && operator != "isnot"
        && operator != "isnull"
        && operator != "isnotnull"
    {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(format!(
                "unsupported operator for {filter_name}: {operator}",
            )));
        }
        return Ok(NativeSeriesFilters::default());
    }

    if operator == "isnull" {
        return Ok(build_null(true));
    }
    if operator == "isnotnull" {
        return Ok(build_null(false));
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(NativeSeriesFilters::default());
    };
    let normalized = value.to_ascii_lowercase();

    if operator == "isnot" {
        return Ok(build_exclude(normalized));
    }

    Ok(build_include(normalized))
}

fn parse_author_match_value(value: Option<&Value>) -> Option<String> {
    let value = value?;
    if let Some(raw) = value.as_str() {
        let normalized = raw.trim().to_ascii_lowercase();
        return (!normalized.is_empty()).then_some(normalized);
    }

    let object = value.as_object()?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| !v.is_empty());
    let role = object
        .get("role")
        .and_then(Value::as_str)
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| !v.is_empty());

    match (name, role) {
        (None, None) => None,
        (Some(name), None) => Some(name),
        (None, Some(role)) => Some(format!("::{role}")),
        (Some(name), Some(role)) => Some(format!("{name}::{role}")),
    }
}

fn normalize_release_date_date_time(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let candidate = if trimmed.len() >= 10 {
        &trimmed[..10]
    } else {
        trimmed
    };

    let bytes = candidate.as_bytes();
    if bytes.len() != 10
        || !bytes[0].is_ascii_digit()
        || !bytes[1].is_ascii_digit()
        || !bytes[2].is_ascii_digit()
        || !bytes[3].is_ascii_digit()
        || bytes[4] != b'-'
        || !bytes[5].is_ascii_digit()
        || !bytes[6].is_ascii_digit()
        || bytes[7] != b'-'
        || !bytes[8].is_ascii_digit()
        || !bytes[9].is_ascii_digit()
    {
        return None;
    }

    Some(candidate.to_string())
}

fn parse_iso8601_duration_to_days(raw: &str) -> Option<i64> {
    let mut text = raw.trim();
    if text.is_empty() {
        return None;
    }

    let mut sign = 1.0_f64;
    if let Some(stripped) = text.strip_prefix('-') {
        sign = -1.0;
        text = stripped;
    } else if let Some(stripped) = text.strip_prefix('+') {
        text = stripped;
    }

    let Some(stripped) = text.strip_prefix('P') else {
        return None;
    };

    let mut in_time = false;
    let mut number = String::new();
    let mut total_seconds = 0.0_f64;

    for ch in stripped.chars() {
        if ch == 'T' {
            in_time = true;
            continue;
        }

        if ch.is_ascii_digit() || ch == '.' {
            number.push(ch);
            continue;
        }

        if number.is_empty() {
            return None;
        }

        let parsed = number.parse::<f64>().ok()?;
        number.clear();

        match ch {
            'D' => {
                total_seconds += parsed * 86_400.0;
            }
            'H' if in_time => {
                total_seconds += parsed * 3_600.0;
            }
            'M' if in_time => {
                total_seconds += parsed * 60.0;
            }
            'S' if in_time => {
                total_seconds += parsed;
            }
            _ => return None,
        }
    }

    if !number.is_empty() {
        return None;
    }

    Some(((sign * total_seconds) / 86_400.0).trunc() as i64)
}

fn ensure_series_operator(
    condition: &Value,
    filter_name: &str,
    expected_operator: &str,
    mode: OperatorValidationMode,
) -> Result<(), DiscoveryError> {
    let Some(operator) = condition.get("operator").and_then(Value::as_str) else {
        return Ok(());
    };
    let op = operator.to_ascii_lowercase();
    let is_supported = if expected_operator == "contains_or_is" {
        op == "contains" || op == "is"
    } else {
        op == expected_operator
    };

    if is_supported || !mode.is_strict() {
        Ok(())
    } else {
        Err(DiscoveryError::InvalidRequest(format!(
            "unsupported operator for {filter_name}: {operator}",
        )))
    }
}

fn ensure_books_operator(
    condition: &Value,
    filter_name: &str,
    expected_operator: &str,
    mode: OperatorValidationMode,
) -> Result<(), DiscoveryError> {
    let Some(operator) = condition.get("operator").and_then(Value::as_str) else {
        return Ok(());
    };
    let op = operator.to_ascii_lowercase();
    let is_supported = if expected_operator == "contains_or_is" {
        op == "contains" || op == "is"
    } else {
        op == expected_operator
    };

    if is_supported || !mode.is_strict() {
        Ok(())
    } else {
        Err(DiscoveryError::InvalidRequest(format!(
            "unsupported operator for {filter_name}: {operator}",
        )))
    }
}

fn parse_books_composite_filters(
    condition: &Value,
    all_of: bool,
    mode: OperatorValidationMode,
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
    let mut series_excluded_groups: Vec<Vec<String>> = vec![];
    let mut read_list_groups: Vec<Vec<String>> = vec![];
    let mut read_list_excluded_groups: Vec<Vec<String>> = vec![];
    let mut title_groups: Vec<Vec<String>> = vec![];
    let mut title_excluded_groups: Vec<Vec<String>> = vec![];
    let mut title_contains_groups: Vec<Vec<String>> = vec![];
    let mut title_contains_excluded_groups: Vec<Vec<String>> = vec![];
    let mut title_begins_with_groups: Vec<Vec<String>> = vec![];
    let mut title_begins_with_excluded_groups: Vec<Vec<String>> = vec![];
    let mut title_ends_with_groups: Vec<Vec<String>> = vec![];
    let mut title_ends_with_excluded_groups: Vec<Vec<String>> = vec![];
    let mut tag_groups: Vec<Vec<String>> = vec![];
    let mut tag_excluded_groups: Vec<Vec<String>> = vec![];
    let mut read_status_groups: Vec<Vec<String>> = vec![];
    let mut read_status_excluded_groups: Vec<Vec<String>> = vec![];
    let mut media_profile_groups: Vec<Vec<String>> = vec![];
    let mut media_profile_excluded_groups: Vec<Vec<String>> = vec![];
    let mut media_status_groups: Vec<Vec<String>> = vec![];
    let mut media_status_excluded_groups: Vec<Vec<String>> = vec![];
    let mut author_groups: Vec<Vec<String>> = vec![];
    let mut author_excluded_groups: Vec<Vec<String>> = vec![];
    let mut poster_type_groups: Vec<Vec<String>> = vec![];
    let mut poster_type_excluded_groups: Vec<Vec<String>> = vec![];
    let mut release_date_groups: Vec<Vec<String>> = vec![];
    let mut release_date_excluded_groups: Vec<Vec<String>> = vec![];
    let mut release_date_gt_bounds: Vec<String> = vec![];
    let mut release_date_lt_bounds: Vec<String> = vec![];
    let mut release_date_begins_with_groups: Vec<Vec<String>> = vec![];
    let mut release_date_ends_with_groups: Vec<Vec<String>> = vec![];
    let mut release_date_contains_excluded_groups: Vec<Vec<String>> = vec![];
    let mut release_date_begins_with_excluded_groups: Vec<Vec<String>> = vec![];
    let mut release_date_ends_with_excluded_groups: Vec<Vec<String>> = vec![];
    let mut release_date_in_last_days_bounds: Vec<i64> = vec![];
    let mut release_date_not_in_last_days_bounds: Vec<i64> = vec![];
    let mut number_sort_groups: Vec<Vec<f64>> = vec![];
    let mut number_sort_excluded_groups: Vec<Vec<f64>> = vec![];
    let mut number_sort_gt_bounds: Vec<f64> = vec![];
    let mut number_sort_lt_bounds: Vec<f64> = vec![];

    for child in children {
        child_count += 1;
        let parsed = parse_native_books_filters_with_mode(Some(child), mode)?;
        let is_series_leaf = parsed.series_ids.is_some()
            && parsed.series_ids_excluded.is_none()
            && parsed.read_list_ids.is_none()
            && parsed.read_list_ids_excluded.is_none()
            && parsed.titles.is_none()
            && parsed.titles_excluded.is_none()
            && parsed.titles_contains.is_none()
            && parsed.titles_contains_excluded.is_none()
            && parsed.titles_begins_with.is_none()
            && parsed.titles_begins_with_excluded.is_none()
            && parsed.titles_ends_with.is_none()
            && parsed.titles_ends_with_excluded.is_none()
            && parsed.library_ids.is_none()
            && parsed.deleted.is_none()
            && parsed.oneshot.is_none()
            && parsed.tags.is_none()
            && parsed.tags_excluded.is_none()
            && parsed.tags_null.is_none()
            && parsed.read_statuses.is_none()
            && parsed.read_statuses_excluded.is_none()
            && parsed.media_profiles.is_none()
            && parsed.media_profiles_excluded.is_none()
            && parsed.media_statuses.is_none()
            && parsed.media_statuses_excluded.is_none()
            && parsed.authors.is_none()
            && parsed.authors_excluded.is_none()
            && parsed.poster_types.is_none()
            && parsed.poster_types_excluded.is_none()
            && parsed.poster_selected.is_none()
            && parsed.poster_selected_excluded.is_none()
            && parsed.release_dates.is_none()
            && parsed.release_dates_excluded.is_none()
            && parsed.release_dates_null.is_none()
            && parsed.release_date_gt.is_none()
            && parsed.release_date_lt.is_none()
            && parsed.release_date_begins_with.is_none()
            && parsed.release_date_ends_with.is_none()
            && parsed.release_date_contains_excluded.is_none()
            && parsed.release_date_begins_with_excluded.is_none()
            && parsed.release_date_ends_with_excluded.is_none()
            && parsed.release_date_in_last_days.is_none()
            && parsed.release_date_not_in_last_days.is_none()
            && parsed.number_sorts.is_none()
            && parsed.number_sorts_excluded.is_none()
            && parsed.number_sort_gt.is_none()
            && parsed.number_sort_lt.is_none();
        if is_series_leaf {
            series_leaf_count += 1;
        }

        if let Some(ids) = parsed.library_ids {
            library_groups.push(ids);
        }
        if let Some(ids) = parsed.series_ids {
            series_groups.push(ids);
        }
        if let Some(ids) = parsed.series_ids_excluded {
            series_excluded_groups.push(ids);
        }
        if let Some(ids) = parsed.read_list_ids {
            read_list_groups.push(ids);
        }
        if let Some(ids) = parsed.read_list_ids_excluded {
            read_list_excluded_groups.push(ids);
        }
        if let Some(titles) = parsed.titles {
            title_groups.push(titles);
        }
        if let Some(titles_excluded) = parsed.titles_excluded {
            title_excluded_groups.push(titles_excluded);
        }
        if let Some(titles_contains) = parsed.titles_contains {
            title_contains_groups.push(titles_contains);
        }
        if let Some(titles_contains_excluded) = parsed.titles_contains_excluded {
            title_contains_excluded_groups.push(titles_contains_excluded);
        }
        if let Some(titles_begins_with) = parsed.titles_begins_with {
            title_begins_with_groups.push(titles_begins_with);
        }
        if let Some(titles_begins_with_excluded) = parsed.titles_begins_with_excluded {
            title_begins_with_excluded_groups.push(titles_begins_with_excluded);
        }
        if let Some(titles_ends_with) = parsed.titles_ends_with {
            title_ends_with_groups.push(titles_ends_with);
        }
        if let Some(titles_ends_with_excluded) = parsed.titles_ends_with_excluded {
            title_ends_with_excluded_groups.push(titles_ends_with_excluded);
        }
        if let Some(tags) = parsed.tags {
            tag_groups.push(tags);
        }
        if let Some(tags_excluded) = parsed.tags_excluded {
            tag_excluded_groups.push(tags_excluded);
        }
        if let Some(read_statuses) = parsed.read_statuses {
            read_status_groups.push(read_statuses);
        }
        if let Some(read_statuses_excluded) = parsed.read_statuses_excluded {
            read_status_excluded_groups.push(read_statuses_excluded);
        }
        if let Some(media_profiles) = parsed.media_profiles {
            media_profile_groups.push(media_profiles);
        }
        if let Some(media_profiles_excluded) = parsed.media_profiles_excluded {
            media_profile_excluded_groups.push(media_profiles_excluded);
        }
        if let Some(media_statuses) = parsed.media_statuses {
            media_status_groups.push(media_statuses);
        }
        if let Some(media_statuses_excluded) = parsed.media_statuses_excluded {
            media_status_excluded_groups.push(media_statuses_excluded);
        }
        if let Some(authors) = parsed.authors {
            author_groups.push(authors);
        }
        if let Some(authors_excluded) = parsed.authors_excluded {
            author_excluded_groups.push(authors_excluded);
        }
        if let Some(poster_types) = parsed.poster_types {
            poster_type_groups.push(poster_types);
        }
        if let Some(poster_types_excluded) = parsed.poster_types_excluded {
            poster_type_excluded_groups.push(poster_types_excluded);
        }
        if let Some(release_dates) = parsed.release_dates {
            release_date_groups.push(release_dates);
        }
        if let Some(release_dates_excluded) = parsed.release_dates_excluded {
            release_date_excluded_groups.push(release_dates_excluded);
        }
        if let Some(release_date_gt) = parsed.release_date_gt {
            release_date_gt_bounds.push(release_date_gt);
        }
        if let Some(release_date_lt) = parsed.release_date_lt {
            release_date_lt_bounds.push(release_date_lt);
        }
        if let Some(release_date_begins_with) = parsed.release_date_begins_with {
            release_date_begins_with_groups.push(release_date_begins_with);
        }
        if let Some(release_date_ends_with) = parsed.release_date_ends_with {
            release_date_ends_with_groups.push(release_date_ends_with);
        }
        if let Some(release_date_contains_excluded) = parsed.release_date_contains_excluded {
            release_date_contains_excluded_groups.push(release_date_contains_excluded);
        }
        if let Some(release_date_begins_with_excluded) = parsed.release_date_begins_with_excluded {
            release_date_begins_with_excluded_groups.push(release_date_begins_with_excluded);
        }
        if let Some(release_date_ends_with_excluded) = parsed.release_date_ends_with_excluded {
            release_date_ends_with_excluded_groups.push(release_date_ends_with_excluded);
        }
        if let Some(release_date_in_last_days) = parsed.release_date_in_last_days {
            release_date_in_last_days_bounds.push(release_date_in_last_days);
        }
        if let Some(release_date_not_in_last_days) = parsed.release_date_not_in_last_days {
            release_date_not_in_last_days_bounds.push(release_date_not_in_last_days);
        }
        if let Some(number_sorts) = parsed.number_sorts {
            number_sort_groups.push(number_sorts);
        }
        if let Some(number_sorts_excluded) = parsed.number_sorts_excluded {
            number_sort_excluded_groups.push(number_sorts_excluded);
        }
        if let Some(number_sort_gt) = parsed.number_sort_gt {
            number_sort_gt_bounds.push(number_sort_gt);
        }
        if let Some(number_sort_lt) = parsed.number_sort_lt {
            number_sort_lt_bounds.push(number_sort_lt);
        }

        aggregate.deleted = merge_boolean_filter(aggregate.deleted, parsed.deleted)?;
        aggregate.oneshot = merge_boolean_filter(aggregate.oneshot, parsed.oneshot)?;
        aggregate.tags_null = merge_boolean_filter(aggregate.tags_null, parsed.tags_null)?;
        aggregate.poster_selected =
            merge_boolean_filter(aggregate.poster_selected, parsed.poster_selected)?;
        aggregate.poster_selected_excluded = merge_boolean_filter(
            aggregate.poster_selected_excluded,
            parsed.poster_selected_excluded,
        )?;
        aggregate.release_dates_null =
            merge_boolean_filter(aggregate.release_dates_null, parsed.release_dates_null)?;
    }

    aggregate.library_ids = merge_string_groups(library_groups, all_of);
    aggregate.series_ids = merge_string_groups(series_groups, all_of);
    aggregate.series_ids_excluded = merge_string_groups(series_excluded_groups, all_of);
    aggregate.read_list_ids = merge_string_groups(read_list_groups, all_of);
    aggregate.read_list_ids_excluded = merge_string_groups(read_list_excluded_groups, all_of);
    aggregate.titles = merge_string_groups(title_groups, all_of);
    aggregate.titles_excluded = merge_string_groups(title_excluded_groups, all_of);
    aggregate.titles_contains = merge_string_groups(title_contains_groups, all_of);
    aggregate.titles_contains_excluded =
        merge_string_groups(title_contains_excluded_groups, all_of);
    aggregate.titles_begins_with = merge_string_groups(title_begins_with_groups, all_of);
    aggregate.titles_begins_with_excluded =
        merge_string_groups(title_begins_with_excluded_groups, all_of);
    aggregate.titles_ends_with = merge_string_groups(title_ends_with_groups, all_of);
    aggregate.titles_ends_with_excluded =
        merge_string_groups(title_ends_with_excluded_groups, all_of);
    aggregate.tags = merge_string_groups(tag_groups, all_of);
    aggregate.tags_excluded = merge_string_groups(tag_excluded_groups, all_of);
    aggregate.read_statuses = merge_string_groups(read_status_groups, all_of);
    aggregate.read_statuses_excluded = merge_string_groups(read_status_excluded_groups, all_of);
    aggregate.media_profiles = merge_string_groups(media_profile_groups, all_of);
    aggregate.media_profiles_excluded = merge_string_groups(media_profile_excluded_groups, all_of);
    aggregate.media_statuses = merge_string_groups(media_status_groups, all_of);
    aggregate.media_statuses_excluded = merge_string_groups(media_status_excluded_groups, all_of);
    aggregate.authors = merge_string_groups(author_groups, all_of);
    aggregate.authors_excluded = merge_string_groups(author_excluded_groups, all_of);
    aggregate.poster_types = merge_string_groups(poster_type_groups, all_of);
    aggregate.poster_types_excluded = merge_string_groups(poster_type_excluded_groups, all_of);
    aggregate.release_dates = merge_string_groups(release_date_groups, all_of);
    aggregate.release_dates_excluded = merge_string_groups(release_date_excluded_groups, all_of);
    aggregate.release_date_gt = merge_release_date_lower_bound(release_date_gt_bounds, all_of);
    aggregate.release_date_lt = merge_release_date_upper_bound(release_date_lt_bounds, all_of);
    aggregate.release_date_begins_with =
        merge_string_groups(release_date_begins_with_groups, all_of);
    aggregate.release_date_ends_with = merge_string_groups(release_date_ends_with_groups, all_of);
    aggregate.release_date_contains_excluded =
        merge_string_groups(release_date_contains_excluded_groups, all_of);
    aggregate.release_date_begins_with_excluded =
        merge_string_groups(release_date_begins_with_excluded_groups, all_of);
    aggregate.release_date_ends_with_excluded =
        merge_string_groups(release_date_ends_with_excluded_groups, all_of);
    aggregate.release_date_in_last_days =
        merge_release_date_in_last_days_bound(release_date_in_last_days_bounds, all_of);
    aggregate.release_date_not_in_last_days =
        merge_release_date_not_in_last_days_bound(release_date_not_in_last_days_bounds, all_of);
    aggregate.number_sorts = merge_f64_groups(number_sort_groups, all_of);
    aggregate.number_sorts_excluded = merge_f64_groups(number_sort_excluded_groups, all_of);
    aggregate.number_sort_gt = merge_numeric_lower_bound_f64(number_sort_gt_bounds, all_of);
    aggregate.number_sort_lt = merge_numeric_upper_bound_f64(number_sort_lt_bounds, all_of);
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

fn merge_u16_lower_bound(bounds: Vec<u16>, all_of: bool) -> Option<u16> {
    if bounds.is_empty() {
        return None;
    }

    if all_of {
        bounds.into_iter().max()
    } else {
        bounds.into_iter().min()
    }
}

fn merge_u16_upper_bound(bounds: Vec<u16>, all_of: bool) -> Option<u16> {
    if bounds.is_empty() {
        return None;
    }

    if all_of {
        bounds.into_iter().min()
    } else {
        bounds.into_iter().max()
    }
}

fn merge_f64_groups(groups: Vec<Vec<f64>>, all_of: bool) -> Option<Vec<f64>> {
    if groups.is_empty() {
        return None;
    }

    if all_of {
        let mut intersection = groups[0].clone();
        for group in groups.iter().skip(1) {
            intersection.retain(|candidate| group.iter().any(|value| *value == *candidate));
        }
        Some(intersection)
    } else {
        let mut union = vec![];
        for group in groups {
            for candidate in group {
                if !union.iter().any(|value| *value == candidate) {
                    union.push(candidate);
                }
            }
        }
        Some(union)
    }
}

fn merge_release_date_lower_bound(bounds: Vec<String>, all_of: bool) -> Option<String> {
    if bounds.is_empty() {
        return None;
    }

    if all_of {
        bounds.into_iter().max()
    } else {
        bounds.into_iter().min()
    }
}

fn merge_release_date_upper_bound(bounds: Vec<String>, all_of: bool) -> Option<String> {
    if bounds.is_empty() {
        return None;
    }

    if all_of {
        bounds.into_iter().min()
    } else {
        bounds.into_iter().max()
    }
}

fn merge_release_date_in_last_days_bound(bounds: Vec<i64>, all_of: bool) -> Option<i64> {
    if bounds.is_empty() {
        return None;
    }

    if all_of {
        bounds.into_iter().min()
    } else {
        bounds.into_iter().max()
    }
}

fn merge_release_date_not_in_last_days_bound(bounds: Vec<i64>, all_of: bool) -> Option<i64> {
    if bounds.is_empty() {
        return None;
    }

    if all_of {
        bounds.into_iter().max()
    } else {
        bounds.into_iter().min()
    }
}

fn merge_numeric_lower_bound_f64(bounds: Vec<f64>, all_of: bool) -> Option<f64> {
    if bounds.is_empty() {
        return None;
    }

    if all_of {
        bounds
            .into_iter()
            .max_by(|left, right| left.total_cmp(right))
    } else {
        bounds
            .into_iter()
            .min_by(|left, right| left.total_cmp(right))
    }
}

fn merge_numeric_upper_bound_f64(bounds: Vec<f64>, all_of: bool) -> Option<f64> {
    if bounds.is_empty() {
        return None;
    }

    if all_of {
        bounds
            .into_iter()
            .min_by(|left, right| left.total_cmp(right))
    } else {
        bounds
            .into_iter()
            .max_by(|left, right| left.total_cmp(right))
    }
}

fn parse_library_id_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeSeriesFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(format!(
                "unsupported operator for LibraryId: {operator}",
            )));
        }
        return Ok(NativeSeriesFilters::default());
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(NativeSeriesFilters::default());
    };

    Ok(NativeSeriesFilters {
        library_ids: Some(vec![value.to_string()]),
        ..NativeSeriesFilters::default()
    })
}

fn parse_collection_id_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeSeriesFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(format!(
                "unsupported operator for CollectionId: {operator}",
            )));
        }
        return Ok(NativeSeriesFilters::default());
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(NativeSeriesFilters::default());
    };

    Ok(NativeSeriesFilters {
        collection_ids: Some(vec![value.to_string()]),
        ..NativeSeriesFilters::default()
    })
}

fn parse_series_title_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeSeriesFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is"
        && operator != "isnot"
        && operator != "contains"
        && operator != "doesnotcontain"
        && operator != "beginswith"
        && operator != "doesnotbeginwith"
        && operator != "endswith"
        && operator != "doesnotendwith"
    {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(format!(
                "unsupported operator for Title: {operator}",
            )));
        }
        return Ok(NativeSeriesFilters::default());
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(NativeSeriesFilters::default());
    };
    let value = value.to_ascii_lowercase();

    Ok(match operator.as_str() {
        "is" => NativeSeriesFilters {
            titles: Some(vec![value]),
            ..NativeSeriesFilters::default()
        },
        "isnot" => NativeSeriesFilters {
            titles_excluded: Some(vec![value]),
            ..NativeSeriesFilters::default()
        },
        "contains" => NativeSeriesFilters {
            titles_contains: Some(vec![value]),
            ..NativeSeriesFilters::default()
        },
        "doesnotcontain" => NativeSeriesFilters {
            titles_contains_excluded: Some(vec![value]),
            ..NativeSeriesFilters::default()
        },
        "beginswith" => NativeSeriesFilters {
            titles_begins_with: Some(vec![value]),
            ..NativeSeriesFilters::default()
        },
        "doesnotbeginwith" => NativeSeriesFilters {
            titles_begins_with_excluded: Some(vec![value]),
            ..NativeSeriesFilters::default()
        },
        "endswith" => NativeSeriesFilters {
            titles_ends_with: Some(vec![value]),
            ..NativeSeriesFilters::default()
        },
        _ => NativeSeriesFilters {
            titles_ends_with_excluded: Some(vec![value]),
            ..NativeSeriesFilters::default()
        },
    })
}

fn parse_series_title_sort_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeSeriesFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is"
        && operator != "isnot"
        && operator != "contains"
        && operator != "doesnotcontain"
        && operator != "beginswith"
        && operator != "doesnotbeginwith"
        && operator != "endswith"
        && operator != "doesnotendwith"
    {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(format!(
                "unsupported operator for TitleSort: {operator}",
            )));
        }
        return Ok(NativeSeriesFilters::default());
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(NativeSeriesFilters::default());
    };
    let value = value.to_ascii_lowercase();

    Ok(match operator.as_str() {
        "is" => NativeSeriesFilters {
            title_sorts: Some(vec![value]),
            ..NativeSeriesFilters::default()
        },
        "isnot" => NativeSeriesFilters {
            title_sorts_excluded: Some(vec![value]),
            ..NativeSeriesFilters::default()
        },
        "contains" => NativeSeriesFilters {
            title_sorts_contains: Some(vec![value]),
            ..NativeSeriesFilters::default()
        },
        "doesnotcontain" => NativeSeriesFilters {
            title_sorts_contains_excluded: Some(vec![value]),
            ..NativeSeriesFilters::default()
        },
        "beginswith" => NativeSeriesFilters {
            title_sorts_begins_with: Some(vec![value]),
            ..NativeSeriesFilters::default()
        },
        "doesnotbeginwith" => NativeSeriesFilters {
            title_sorts_begins_with_excluded: Some(vec![value]),
            ..NativeSeriesFilters::default()
        },
        "endswith" => NativeSeriesFilters {
            title_sorts_ends_with: Some(vec![value]),
            ..NativeSeriesFilters::default()
        },
        _ => NativeSeriesFilters {
            title_sorts_ends_with_excluded: Some(vec![value]),
            ..NativeSeriesFilters::default()
        },
    })
}

fn parse_deleted_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeSeriesFilters, DiscoveryError> {
    let Some(operator) = condition.get("operator").and_then(Value::as_str) else {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(
                "missing operator for Deleted".to_string(),
            ));
        }
        return Ok(NativeSeriesFilters::default());
    };

    let deleted = match operator.to_ascii_lowercase().as_str() {
        "istrue" => true,
        "isfalse" => false,
        _ => {
            if mode.is_strict() {
                return Err(DiscoveryError::InvalidRequest(format!(
                    "unsupported operator for Deleted: {operator}",
                )));
            }
            return Ok(NativeSeriesFilters::default());
        }
    };

    Ok(NativeSeriesFilters {
        deleted: Some(deleted),
        ..NativeSeriesFilters::default()
    })
}

fn parse_oneshot_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeSeriesFilters, DiscoveryError> {
    let Some(operator) = condition.get("operator").and_then(Value::as_str) else {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(
                "missing operator for OneShot".to_string(),
            ));
        }
        return Ok(NativeSeriesFilters::default());
    };

    let oneshot = match operator.to_ascii_lowercase().as_str() {
        "istrue" => true,
        "isfalse" => false,
        _ => {
            if mode.is_strict() {
                return Err(DiscoveryError::InvalidRequest(format!(
                    "unsupported operator for OneShot: {operator}",
                )));
            }
            return Ok(NativeSeriesFilters::default());
        }
    };

    Ok(NativeSeriesFilters {
        oneshot: Some(oneshot),
        ..NativeSeriesFilters::default()
    })
}

fn parse_series_read_status_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeSeriesFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" && operator != "isnot" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(format!(
                "unsupported operator for ReadStatus: {operator}",
            )));
        }
        return Ok(NativeSeriesFilters::default());
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(NativeSeriesFilters::default());
    };
    let normalized = value.to_ascii_lowercase();

    if operator == "isnot" {
        return Ok(NativeSeriesFilters {
            read_statuses_excluded: Some(vec![normalized]),
            ..NativeSeriesFilters::default()
        });
    }

    Ok(NativeSeriesFilters {
        read_statuses: Some(vec![normalized]),
        ..NativeSeriesFilters::default()
    })
}

fn parse_series_genre_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeSeriesFilters, DiscoveryError> {
    parse_nullable_series_string_filter(
        condition,
        mode,
        "Genre",
        |value| NativeSeriesFilters {
            genres: Some(vec![value]),
            ..NativeSeriesFilters::default()
        },
        |value| NativeSeriesFilters {
            genres_excluded: Some(vec![value]),
            ..NativeSeriesFilters::default()
        },
        |is_null| NativeSeriesFilters {
            genres_null: Some(is_null),
            ..NativeSeriesFilters::default()
        },
    )
}

fn parse_series_tag_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeSeriesFilters, DiscoveryError> {
    parse_nullable_series_string_filter(
        condition,
        mode,
        "Tag",
        |value| NativeSeriesFilters {
            tags: Some(vec![value]),
            ..NativeSeriesFilters::default()
        },
        |value| NativeSeriesFilters {
            tags_excluded: Some(vec![value]),
            ..NativeSeriesFilters::default()
        },
        |is_null| NativeSeriesFilters {
            tags_null: Some(is_null),
            ..NativeSeriesFilters::default()
        },
    )
}

fn parse_series_language_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeSeriesFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" && operator != "isnot" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(format!(
                "unsupported operator for Language: {operator}",
            )));
        }
        return Ok(NativeSeriesFilters::default());
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(NativeSeriesFilters::default());
    };
    let normalized = value.to_ascii_lowercase();

    if operator == "isnot" {
        return Ok(NativeSeriesFilters {
            languages_excluded: Some(vec![normalized]),
            ..NativeSeriesFilters::default()
        });
    }

    Ok(NativeSeriesFilters {
        languages: Some(vec![normalized]),
        ..NativeSeriesFilters::default()
    })
}

fn parse_series_publisher_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeSeriesFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" && operator != "isnot" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(format!(
                "unsupported operator for Publisher: {operator}",
            )));
        }
        return Ok(NativeSeriesFilters::default());
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(NativeSeriesFilters::default());
    };
    let normalized = value.to_ascii_lowercase();

    if operator == "isnot" {
        return Ok(NativeSeriesFilters {
            publishers_excluded: Some(vec![normalized]),
            ..NativeSeriesFilters::default()
        });
    }

    Ok(NativeSeriesFilters {
        publishers: Some(vec![normalized]),
        ..NativeSeriesFilters::default()
    })
}

fn parse_series_age_rating_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeSeriesFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is"
        && operator != "isnot"
        && operator != "isnull"
        && operator != "isnotnull"
        && operator != "greaterthan"
        && operator != "lessthan"
    {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(format!(
                "unsupported operator for AgeRating: {operator}",
            )));
        }
        return Ok(NativeSeriesFilters::default());
    }

    if operator == "isnull" {
        return Ok(NativeSeriesFilters {
            age_ratings_null: Some(true),
            ..NativeSeriesFilters::default()
        });
    }
    if operator == "isnotnull" {
        return Ok(NativeSeriesFilters {
            age_ratings_null: Some(false),
            ..NativeSeriesFilters::default()
        });
    }

    let Some(value) = condition.get("value") else {
        return Ok(NativeSeriesFilters::default());
    };

    let parsed = if let Some(number) = value.as_u64() {
        number as u16
    } else if let Some(raw) = value.as_str() {
        match raw.parse::<u16>() {
            Ok(value) => value,
            Err(_) => return Ok(NativeSeriesFilters::default()),
        }
    } else {
        return Ok(NativeSeriesFilters::default());
    };

    match operator.as_str() {
        "isnot" => Ok(NativeSeriesFilters {
            age_ratings_excluded: Some(vec![parsed]),
            ..NativeSeriesFilters::default()
        }),
        "greaterthan" => Ok(NativeSeriesFilters {
            age_rating_gt: Some(parsed),
            ..NativeSeriesFilters::default()
        }),
        "lessthan" => Ok(NativeSeriesFilters {
            age_rating_lt: Some(parsed),
            ..NativeSeriesFilters::default()
        }),
        _ => Ok(NativeSeriesFilters {
            age_ratings: Some(vec![parsed]),
            ..NativeSeriesFilters::default()
        }),
    }
}

fn parse_series_release_date_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeSeriesFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is"
        && operator != "isnot"
        && operator != "isnull"
        && operator != "isnotnull"
        && operator != "greaterthan"
        && operator != "lessthan"
        && operator != "after"
        && operator != "before"
        && operator != "isinthelast"
        && operator != "isnotinthelast"
        && operator != "beginswith"
        && operator != "endswith"
        && operator != "doesnotcontain"
        && operator != "doesnotbeginwith"
        && operator != "doesnotendwith"
    {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(format!(
                "unsupported operator for ReleaseDate: {operator}",
            )));
        }
        return Ok(NativeSeriesFilters::default());
    }

    if operator == "isnull" {
        return Ok(NativeSeriesFilters {
            release_dates_null: Some(true),
            ..NativeSeriesFilters::default()
        });
    }

    if operator == "isnotnull" {
        return Ok(NativeSeriesFilters {
            release_dates_null: Some(false),
            ..NativeSeriesFilters::default()
        });
    }

    if operator == "after" {
        let Some(date_time) = condition
            .get("dateTime")
            .and_then(Value::as_str)
            .and_then(normalize_release_date_date_time)
        else {
            return Ok(NativeSeriesFilters::default());
        };

        return Ok(NativeSeriesFilters {
            release_date_gt: Some(date_time),
            ..NativeSeriesFilters::default()
        });
    }

    if operator == "before" {
        let Some(date_time) = condition
            .get("dateTime")
            .and_then(Value::as_str)
            .and_then(normalize_release_date_date_time)
        else {
            return Ok(NativeSeriesFilters::default());
        };

        return Ok(NativeSeriesFilters {
            release_date_lt: Some(date_time),
            ..NativeSeriesFilters::default()
        });
    }

    if operator == "isinthelast" {
        let Some(days) = condition
            .get("duration")
            .and_then(Value::as_str)
            .and_then(parse_iso8601_duration_to_days)
        else {
            return Ok(NativeSeriesFilters::default());
        };

        return Ok(NativeSeriesFilters {
            release_date_in_last_days: Some(days),
            ..NativeSeriesFilters::default()
        });
    }

    if operator == "isnotinthelast" {
        let Some(days) = condition
            .get("duration")
            .and_then(Value::as_str)
            .and_then(parse_iso8601_duration_to_days)
        else {
            return Ok(NativeSeriesFilters::default());
        };

        return Ok(NativeSeriesFilters {
            release_date_not_in_last_days: Some(days),
            ..NativeSeriesFilters::default()
        });
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(NativeSeriesFilters::default());
    };

    if operator == "greaterthan" {
        return Ok(NativeSeriesFilters {
            release_date_gt: Some(value.to_ascii_lowercase()),
            ..NativeSeriesFilters::default()
        });
    }

    if operator == "lessthan" {
        return Ok(NativeSeriesFilters {
            release_date_lt: Some(value.to_ascii_lowercase()),
            ..NativeSeriesFilters::default()
        });
    }

    if operator == "beginswith" {
        return Ok(NativeSeriesFilters {
            release_date_begins_with: Some(vec![value.to_ascii_lowercase()]),
            ..NativeSeriesFilters::default()
        });
    }

    if operator == "endswith" {
        return Ok(NativeSeriesFilters {
            release_date_ends_with: Some(vec![value.to_ascii_lowercase()]),
            ..NativeSeriesFilters::default()
        });
    }

    if operator == "doesnotcontain" {
        return Ok(NativeSeriesFilters {
            release_date_contains_excluded: Some(vec![value.to_ascii_lowercase()]),
            ..NativeSeriesFilters::default()
        });
    }

    if operator == "doesnotbeginwith" {
        return Ok(NativeSeriesFilters {
            release_date_begins_with_excluded: Some(vec![value.to_ascii_lowercase()]),
            ..NativeSeriesFilters::default()
        });
    }

    if operator == "doesnotendwith" {
        return Ok(NativeSeriesFilters {
            release_date_ends_with_excluded: Some(vec![value.to_ascii_lowercase()]),
            ..NativeSeriesFilters::default()
        });
    }

    if operator == "isnot" {
        return Ok(NativeSeriesFilters {
            release_dates_excluded: Some(vec![value.to_ascii_lowercase()]),
            ..NativeSeriesFilters::default()
        });
    }

    Ok(NativeSeriesFilters {
        release_dates: Some(vec![value.to_ascii_lowercase()]),
        ..NativeSeriesFilters::default()
    })
}

fn parse_series_sharing_label_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeSeriesFilters, DiscoveryError> {
    parse_nullable_series_string_filter(
        condition,
        mode,
        "SharingLabel",
        |value| NativeSeriesFilters {
            sharing_labels: Some(vec![value]),
            ..NativeSeriesFilters::default()
        },
        |value| NativeSeriesFilters {
            sharing_labels_excluded: Some(vec![value]),
            ..NativeSeriesFilters::default()
        },
        |is_null| NativeSeriesFilters {
            sharing_labels_null: Some(is_null),
            ..NativeSeriesFilters::default()
        },
    )
}

fn parse_series_status_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeSeriesFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" && operator != "isnot" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(format!(
                "unsupported operator for SeriesStatus: {operator}",
            )));
        }
        return Ok(NativeSeriesFilters::default());
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(NativeSeriesFilters::default());
    };

    if operator == "isnot" {
        return Ok(NativeSeriesFilters {
            series_statuses_excluded: Some(vec![value.to_ascii_lowercase()]),
            ..NativeSeriesFilters::default()
        });
    }

    Ok(NativeSeriesFilters {
        series_statuses: Some(vec![value.to_ascii_lowercase()]),
        ..NativeSeriesFilters::default()
    })
}

fn parse_series_complete_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeSeriesFilters, DiscoveryError> {
    let Some(operator) = condition.get("operator").and_then(Value::as_str) else {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(
                "missing operator for Complete".to_string(),
            ));
        }
        return Ok(NativeSeriesFilters::default());
    };

    let complete = match operator.to_ascii_lowercase().as_str() {
        "istrue" => true,
        "isfalse" => false,
        _ => {
            if mode.is_strict() {
                return Err(DiscoveryError::InvalidRequest(format!(
                    "unsupported operator for Complete: {operator}",
                )));
            }
            return Ok(NativeSeriesFilters::default());
        }
    };

    Ok(NativeSeriesFilters {
        complete: Some(complete),
        ..NativeSeriesFilters::default()
    })
}

fn parse_series_author_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<NativeSeriesFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();

    if operator == "contains" {
        return parse_series_string_filter(condition, "Author", "contains_or_is", mode, |value| {
            NativeSeriesFilters {
                authors: Some(vec![value]),
                ..NativeSeriesFilters::default()
            }
        });
    }

    if operator != "is" && operator != "isnot" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidRequest(format!(
                "unsupported operator for Author: {operator}",
            )));
        }
        return parse_series_string_filter(condition, "Author", "contains_or_is", mode, |value| {
            NativeSeriesFilters {
                authors: Some(vec![value]),
                ..NativeSeriesFilters::default()
            }
        });
    }

    let Some(encoded) = parse_author_match_value(condition.get("value")) else {
        return Ok(NativeSeriesFilters::default());
    };

    if operator == "isnot" {
        return Ok(NativeSeriesFilters {
            authors_excluded: Some(vec![encoded]),
            ..NativeSeriesFilters::default()
        });
    }

    Ok(NativeSeriesFilters {
        authors: Some(vec![encoded]),
        ..NativeSeriesFilters::default()
    })
}

fn parse_composite_filters(
    condition: &Value,
    all_of: bool,
    mode: OperatorValidationMode,
) -> Result<NativeSeriesFilters, DiscoveryError> {
    let Some(children) = condition.get("conditions").and_then(Value::as_array) else {
        return Err(DiscoveryError::InvalidRequest(
            "series composite filter missing conditions".to_string(),
        ));
    };

    let mut aggregate = NativeSeriesFilters::default();
    let mut library_groups: Vec<Vec<String>> = vec![];
    let mut title_groups: Vec<Vec<String>> = vec![];
    let mut title_excluded_groups: Vec<Vec<String>> = vec![];
    let mut title_contains_groups: Vec<Vec<String>> = vec![];
    let mut title_contains_excluded_groups: Vec<Vec<String>> = vec![];
    let mut title_begins_with_groups: Vec<Vec<String>> = vec![];
    let mut title_begins_with_excluded_groups: Vec<Vec<String>> = vec![];
    let mut title_ends_with_groups: Vec<Vec<String>> = vec![];
    let mut title_ends_with_excluded_groups: Vec<Vec<String>> = vec![];
    let mut title_sort_groups: Vec<Vec<String>> = vec![];
    let mut title_sort_excluded_groups: Vec<Vec<String>> = vec![];
    let mut title_sort_contains_groups: Vec<Vec<String>> = vec![];
    let mut title_sort_contains_excluded_groups: Vec<Vec<String>> = vec![];
    let mut title_sort_begins_with_groups: Vec<Vec<String>> = vec![];
    let mut title_sort_begins_with_excluded_groups: Vec<Vec<String>> = vec![];
    let mut title_sort_ends_with_groups: Vec<Vec<String>> = vec![];
    let mut title_sort_ends_with_excluded_groups: Vec<Vec<String>> = vec![];
    let mut read_status_groups: Vec<Vec<String>> = vec![];
    let mut read_status_excluded_groups: Vec<Vec<String>> = vec![];
    let mut genre_groups: Vec<Vec<String>> = vec![];
    let mut genre_excluded_groups: Vec<Vec<String>> = vec![];
    let mut tag_groups: Vec<Vec<String>> = vec![];
    let mut tag_excluded_groups: Vec<Vec<String>> = vec![];
    let mut language_groups: Vec<Vec<String>> = vec![];
    let mut language_excluded_groups: Vec<Vec<String>> = vec![];
    let mut publisher_groups: Vec<Vec<String>> = vec![];
    let mut publisher_excluded_groups: Vec<Vec<String>> = vec![];
    let mut release_date_groups: Vec<Vec<String>> = vec![];
    let mut release_date_excluded_groups: Vec<Vec<String>> = vec![];
    let mut release_date_gt_bounds: Vec<String> = vec![];
    let mut release_date_lt_bounds: Vec<String> = vec![];
    let mut release_date_begins_with_groups: Vec<Vec<String>> = vec![];
    let mut release_date_ends_with_groups: Vec<Vec<String>> = vec![];
    let mut release_date_contains_excluded_groups: Vec<Vec<String>> = vec![];
    let mut release_date_begins_with_excluded_groups: Vec<Vec<String>> = vec![];
    let mut release_date_ends_with_excluded_groups: Vec<Vec<String>> = vec![];
    let mut release_date_in_last_days_bounds: Vec<i64> = vec![];
    let mut release_date_not_in_last_days_bounds: Vec<i64> = vec![];
    let mut sharing_label_groups: Vec<Vec<String>> = vec![];
    let mut sharing_label_excluded_groups: Vec<Vec<String>> = vec![];
    let mut series_status_groups: Vec<Vec<String>> = vec![];
    let mut series_status_excluded_groups: Vec<Vec<String>> = vec![];
    let mut author_groups: Vec<Vec<String>> = vec![];
    let mut author_excluded_groups: Vec<Vec<String>> = vec![];
    let mut age_rating_groups: Vec<Vec<u16>> = vec![];
    let mut age_rating_excluded_groups: Vec<Vec<u16>> = vec![];
    let mut age_rating_gt_bounds: Vec<u16> = vec![];
    let mut age_rating_lt_bounds: Vec<u16> = vec![];

    for child in children {
        let parsed = parse_native_series_filters_with_mode(Some(child), mode)?;
        if let Some(ids) = parsed.library_ids {
            library_groups.push(ids);
        }
        if let Some(titles) = parsed.titles {
            title_groups.push(titles);
        }
        if let Some(titles_excluded) = parsed.titles_excluded {
            title_excluded_groups.push(titles_excluded);
        }
        if let Some(titles_contains) = parsed.titles_contains {
            title_contains_groups.push(titles_contains);
        }
        if let Some(titles_contains_excluded) = parsed.titles_contains_excluded {
            title_contains_excluded_groups.push(titles_contains_excluded);
        }
        if let Some(titles_begins_with) = parsed.titles_begins_with {
            title_begins_with_groups.push(titles_begins_with);
        }
        if let Some(titles_begins_with_excluded) = parsed.titles_begins_with_excluded {
            title_begins_with_excluded_groups.push(titles_begins_with_excluded);
        }
        if let Some(titles_ends_with) = parsed.titles_ends_with {
            title_ends_with_groups.push(titles_ends_with);
        }
        if let Some(titles_ends_with_excluded) = parsed.titles_ends_with_excluded {
            title_ends_with_excluded_groups.push(titles_ends_with_excluded);
        }
        if let Some(title_sorts) = parsed.title_sorts {
            title_sort_groups.push(title_sorts);
        }
        if let Some(title_sorts_excluded) = parsed.title_sorts_excluded {
            title_sort_excluded_groups.push(title_sorts_excluded);
        }
        if let Some(title_sorts_contains) = parsed.title_sorts_contains {
            title_sort_contains_groups.push(title_sorts_contains);
        }
        if let Some(title_sorts_contains_excluded) = parsed.title_sorts_contains_excluded {
            title_sort_contains_excluded_groups.push(title_sorts_contains_excluded);
        }
        if let Some(title_sorts_begins_with) = parsed.title_sorts_begins_with {
            title_sort_begins_with_groups.push(title_sorts_begins_with);
        }
        if let Some(title_sorts_begins_with_excluded) = parsed.title_sorts_begins_with_excluded {
            title_sort_begins_with_excluded_groups.push(title_sorts_begins_with_excluded);
        }
        if let Some(title_sorts_ends_with) = parsed.title_sorts_ends_with {
            title_sort_ends_with_groups.push(title_sorts_ends_with);
        }
        if let Some(title_sorts_ends_with_excluded) = parsed.title_sorts_ends_with_excluded {
            title_sort_ends_with_excluded_groups.push(title_sorts_ends_with_excluded);
        }
        if let Some(read_statuses) = parsed.read_statuses {
            read_status_groups.push(read_statuses);
        }
        if let Some(read_statuses_excluded) = parsed.read_statuses_excluded {
            read_status_excluded_groups.push(read_statuses_excluded);
        }
        if let Some(genres) = parsed.genres {
            genre_groups.push(genres);
        }
        if let Some(genres_excluded) = parsed.genres_excluded {
            genre_excluded_groups.push(genres_excluded);
        }
        if let Some(tags) = parsed.tags {
            tag_groups.push(tags);
        }
        if let Some(tags_excluded) = parsed.tags_excluded {
            tag_excluded_groups.push(tags_excluded);
        }
        if let Some(languages) = parsed.languages {
            language_groups.push(languages);
        }
        if let Some(languages_excluded) = parsed.languages_excluded {
            language_excluded_groups.push(languages_excluded);
        }
        if let Some(publishers) = parsed.publishers {
            publisher_groups.push(publishers);
        }
        if let Some(publishers_excluded) = parsed.publishers_excluded {
            publisher_excluded_groups.push(publishers_excluded);
        }
        if let Some(age_ratings) = parsed.age_ratings {
            age_rating_groups.push(age_ratings);
        }
        if let Some(age_ratings_excluded) = parsed.age_ratings_excluded {
            age_rating_excluded_groups.push(age_ratings_excluded);
        }
        if let Some(age_rating_gt) = parsed.age_rating_gt {
            age_rating_gt_bounds.push(age_rating_gt);
        }
        if let Some(age_rating_lt) = parsed.age_rating_lt {
            age_rating_lt_bounds.push(age_rating_lt);
        }
        if let Some(release_dates) = parsed.release_dates {
            release_date_groups.push(release_dates);
        }
        if let Some(release_dates_excluded) = parsed.release_dates_excluded {
            release_date_excluded_groups.push(release_dates_excluded);
        }
        if let Some(release_date_gt) = parsed.release_date_gt {
            release_date_gt_bounds.push(release_date_gt);
        }
        if let Some(release_date_lt) = parsed.release_date_lt {
            release_date_lt_bounds.push(release_date_lt);
        }
        if let Some(release_date_begins_with) = parsed.release_date_begins_with {
            release_date_begins_with_groups.push(release_date_begins_with);
        }
        if let Some(release_date_ends_with) = parsed.release_date_ends_with {
            release_date_ends_with_groups.push(release_date_ends_with);
        }
        if let Some(release_date_contains_excluded) = parsed.release_date_contains_excluded {
            release_date_contains_excluded_groups.push(release_date_contains_excluded);
        }
        if let Some(release_date_begins_with_excluded) = parsed.release_date_begins_with_excluded {
            release_date_begins_with_excluded_groups.push(release_date_begins_with_excluded);
        }
        if let Some(release_date_ends_with_excluded) = parsed.release_date_ends_with_excluded {
            release_date_ends_with_excluded_groups.push(release_date_ends_with_excluded);
        }
        if let Some(release_date_in_last_days) = parsed.release_date_in_last_days {
            release_date_in_last_days_bounds.push(release_date_in_last_days);
        }
        if let Some(release_date_not_in_last_days) = parsed.release_date_not_in_last_days {
            release_date_not_in_last_days_bounds.push(release_date_not_in_last_days);
        }
        if let Some(sharing_labels) = parsed.sharing_labels {
            sharing_label_groups.push(sharing_labels);
        }
        if let Some(sharing_labels_excluded) = parsed.sharing_labels_excluded {
            sharing_label_excluded_groups.push(sharing_labels_excluded);
        }
        if let Some(series_statuses) = parsed.series_statuses {
            series_status_groups.push(series_statuses);
        }
        if let Some(series_statuses_excluded) = parsed.series_statuses_excluded {
            series_status_excluded_groups.push(series_statuses_excluded);
        }
        if let Some(authors) = parsed.authors {
            author_groups.push(authors);
        }
        if let Some(authors_excluded) = parsed.authors_excluded {
            author_excluded_groups.push(authors_excluded);
        }

        aggregate.deleted = merge_boolean_filter(aggregate.deleted, parsed.deleted)?;
        aggregate.oneshot = merge_boolean_filter(aggregate.oneshot, parsed.oneshot)?;
        aggregate.genres_null = merge_boolean_filter(aggregate.genres_null, parsed.genres_null)?;
        aggregate.tags_null = merge_boolean_filter(aggregate.tags_null, parsed.tags_null)?;
        aggregate.age_ratings_null =
            merge_boolean_filter(aggregate.age_ratings_null, parsed.age_ratings_null)?;
        aggregate.sharing_labels_null =
            merge_boolean_filter(aggregate.sharing_labels_null, parsed.sharing_labels_null)?;
        aggregate.release_dates_null =
            merge_boolean_filter(aggregate.release_dates_null, parsed.release_dates_null)?;
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
    aggregate.read_statuses_excluded = merge_string_groups(read_status_excluded_groups, all_of);
    aggregate.titles = merge_string_groups(title_groups, all_of);
    aggregate.titles_excluded = merge_string_groups(title_excluded_groups, all_of);
    aggregate.titles_contains = merge_string_groups(title_contains_groups, all_of);
    aggregate.titles_contains_excluded =
        merge_string_groups(title_contains_excluded_groups, all_of);
    aggregate.titles_begins_with = merge_string_groups(title_begins_with_groups, all_of);
    aggregate.titles_begins_with_excluded =
        merge_string_groups(title_begins_with_excluded_groups, all_of);
    aggregate.titles_ends_with = merge_string_groups(title_ends_with_groups, all_of);
    aggregate.titles_ends_with_excluded =
        merge_string_groups(title_ends_with_excluded_groups, all_of);
    aggregate.title_sorts = merge_string_groups(title_sort_groups, all_of);
    aggregate.title_sorts_excluded = merge_string_groups(title_sort_excluded_groups, all_of);
    aggregate.title_sorts_contains = merge_string_groups(title_sort_contains_groups, all_of);
    aggregate.title_sorts_contains_excluded =
        merge_string_groups(title_sort_contains_excluded_groups, all_of);
    aggregate.title_sorts_begins_with = merge_string_groups(title_sort_begins_with_groups, all_of);
    aggregate.title_sorts_begins_with_excluded =
        merge_string_groups(title_sort_begins_with_excluded_groups, all_of);
    aggregate.title_sorts_ends_with = merge_string_groups(title_sort_ends_with_groups, all_of);
    aggregate.title_sorts_ends_with_excluded =
        merge_string_groups(title_sort_ends_with_excluded_groups, all_of);
    aggregate.genres = merge_string_groups(genre_groups, all_of);
    aggregate.genres_excluded = merge_string_groups(genre_excluded_groups, all_of);
    aggregate.tags = merge_string_groups(tag_groups, all_of);
    aggregate.tags_excluded = merge_string_groups(tag_excluded_groups, all_of);
    aggregate.languages = merge_string_groups(language_groups, all_of);
    aggregate.languages_excluded = merge_string_groups(language_excluded_groups, all_of);
    aggregate.publishers = merge_string_groups(publisher_groups, all_of);
    aggregate.publishers_excluded = merge_string_groups(publisher_excluded_groups, all_of);
    aggregate.age_ratings = merge_u16_groups(age_rating_groups, all_of);
    aggregate.age_ratings_excluded = merge_u16_groups(age_rating_excluded_groups, all_of);
    aggregate.age_rating_gt = merge_u16_lower_bound(age_rating_gt_bounds, all_of);
    aggregate.age_rating_lt = merge_u16_upper_bound(age_rating_lt_bounds, all_of);
    aggregate.release_dates = merge_string_groups(release_date_groups, all_of);
    aggregate.release_dates_excluded = merge_string_groups(release_date_excluded_groups, all_of);
    aggregate.release_date_gt = merge_release_date_lower_bound(release_date_gt_bounds, all_of);
    aggregate.release_date_lt = merge_release_date_upper_bound(release_date_lt_bounds, all_of);
    aggregate.release_date_begins_with =
        merge_string_groups(release_date_begins_with_groups, all_of);
    aggregate.release_date_ends_with = merge_string_groups(release_date_ends_with_groups, all_of);
    aggregate.release_date_contains_excluded =
        merge_string_groups(release_date_contains_excluded_groups, all_of);
    aggregate.release_date_begins_with_excluded =
        merge_string_groups(release_date_begins_with_excluded_groups, all_of);
    aggregate.release_date_ends_with_excluded =
        merge_string_groups(release_date_ends_with_excluded_groups, all_of);
    aggregate.release_date_in_last_days =
        merge_release_date_in_last_days_bound(release_date_in_last_days_bounds, all_of);
    aggregate.release_date_not_in_last_days =
        merge_release_date_not_in_last_days_bound(release_date_not_in_last_days_bounds, all_of);
    aggregate.sharing_labels = merge_string_groups(sharing_label_groups, all_of);
    aggregate.sharing_labels_excluded = merge_string_groups(sharing_label_excluded_groups, all_of);
    aggregate.series_statuses = merge_string_groups(series_status_groups, all_of);
    aggregate.series_statuses_excluded = merge_string_groups(series_status_excluded_groups, all_of);
    aggregate.authors = merge_string_groups(author_groups, all_of);
    aggregate.authors_excluded = merge_string_groups(author_excluded_groups, all_of);

    Ok(aggregate)
}

fn merge_boolean_filter(
    left: Option<bool>,
    right: Option<bool>,
) -> Result<Option<bool>, DiscoveryError> {
    match (left, right) {
        (Some(a), Some(b)) if a == b => Ok(Some(a)),
        (Some(_), Some(_)) => Ok(None),
        (Some(value), None) | (None, Some(value)) => Ok(Some(value)),
        (None, None) => Ok(None),
    }
}

fn series_page_payload(page: PageEnvelope<PersistedSeriesSummary>) -> Value {
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

fn series_payload(series: &PersistedSeriesSummary) -> Value {
    let metadata = json!({
        "status": series.status.as_str(),
        "statusLock": false,
        "title": series.title.as_str(),
        "titleLock": false,
        "titleSort": series.title_sort.as_str(),
        "titleSortLock": false,
        "summary": series.summary.as_str(),
        "summaryLock": false,
        "readingDirection": series.reading_direction.as_str(),
        "readingDirectionLock": false,
        "publisher": series.publisher.as_str(),
        "publisherLock": false,
        "ageRating": series.age_rating,
        "ageRatingLock": false,
        "language": series.language.as_str(),
        "languageLock": false,
        "genres": series.genres.clone(),
        "genresLock": false,
        "tags": series.tags.clone(),
        "tagsLock": false,
        "totalBookCount": null,
        "totalBookCountLock": false,
        "sharingLabels": series.labels.clone(),
        "sharingLabelsLock": false,
        "links": [],
        "linksLock": false,
        "alternateTitles": series.alternate_titles.clone(),
        "alternateTitlesLock": false,
        "created": series.metadata_created.as_str(),
        "lastModified": series.metadata_last_modified.as_str()
    });

    let books_metadata = json!({
        "authors": series.books_metadata_authors.clone(),
        "tags": series.books_metadata_tags.clone(),
        "releaseDate": series.books_metadata_release_date.clone(),
        "summary": series.books_metadata_summary.as_str(),
        "summaryNumber": series.books_metadata_summary_number.as_str(),
        "created": series.books_metadata_created.as_str(),
        "lastModified": series.books_metadata_last_modified.as_str()
    });

    json!({
        "id": series.id.as_str(),
        "libraryId": series.library_id.as_str(),
        "name": series.title.as_str(),
        "url": format!("series/{}", series.id),
        "created": series.created.as_str(),
        "lastModified": series.last_modified.as_str(),
        "fileLastModified": series.file_last_modified.as_str(),
        "booksCount": series.books_count,
        "booksReadCount": series.books_read_count,
        "booksUnreadCount": series.books_unread_count,
        "booksInProgressCount": series.books_in_progress_count,
        "metadata": metadata,
        "booksMetadata": books_metadata,
        "deleted": series.deleted,
        "oneshot": series.oneshot
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_native_series_filter_type_gracefully_falls_back_to_default() {
        let condition = json!({
            "type": "UnknownSeriesFilter",
            "operator": "is",
            "value": "x",
        });

        let parsed = parse_native_series_filters(Some(&condition))
            .expect("unknown native series filter should not error");

        assert!(parsed.library_ids.is_none());
        assert!(parsed.read_statuses.is_none());
        assert!(parsed.tags.is_none());
    }

    #[test]
    fn unknown_native_books_filter_type_gracefully_falls_back_to_default() {
        let condition = json!({
            "type": "UnknownBooksFilter",
            "operator": "is",
            "value": "x",
        });

        let parsed = parse_native_books_filters(Some(&condition))
            .expect("unknown native books filter should not error");

        assert!(parsed.library_ids.is_none());
        assert!(parsed.series_ids.is_none());
        assert!(parsed.read_statuses.is_none());
    }

    #[test]
    fn webui_series_filter_shape_is_normalized_into_native_filter() {
        let condition = json!({
            "libraryId": {
                "operator": "is",
                "value": "lib-a",
            }
        });

        let parsed = parse_native_series_filters(Some(&condition))
            .expect("legacy webui series condition should be normalized");

        assert_eq!(parsed.library_ids, Some(vec!["lib-a".to_string()]));
    }

    #[test]
    fn webui_series_collection_filter_shape_is_normalized_into_native_filter() {
        let condition = json!({
            "collectionId": {
                "operator": "is",
                "value": "collection-1",
            }
        });

        let parsed = parse_native_series_filters(Some(&condition))
            .expect("legacy webui collection condition should be normalized");

        assert_eq!(
            parsed.collection_ids,
            Some(vec!["collection-1".to_string()])
        );
    }

    #[test]
    fn webui_series_title_sort_filter_shape_is_normalized_into_native_filter() {
        let condition = json!({
            "titleSort": {
                "operator": "contains",
                "value": "series",
            }
        });

        let parsed = parse_native_series_filters(Some(&condition))
            .expect("legacy webui titleSort condition should be normalized");

        assert_eq!(
            parsed.title_sorts_contains,
            Some(vec!["series".to_string()]),
        );
    }

    #[test]
    fn webui_books_filter_shape_is_normalized_into_native_filter() {
        let condition = json!({
            "seriesId": {
                "operator": "is",
                "value": "series-1",
            }
        });

        let parsed = parse_native_books_filters(Some(&condition))
            .expect("legacy webui books condition should be normalized");

        assert_eq!(
            parsed.direct_browse_family,
            Some(DirectBrowseBooksListFamily::BrowseBookSiblingsUnpaged)
        );
        assert_eq!(parsed.series_ids, Some(vec!["series-1".to_string()]));
    }

    #[test]
    fn webui_books_number_sort_shape_is_normalized_into_native_filter() {
        let condition = json!({
            "numberSort": {
                "operator": "greaterThan",
                "value": 5.0,
            }
        });

        let parsed = parse_native_books_filters(Some(&condition))
            .expect("legacy webui books number-sort condition should be normalized");

        assert_eq!(parsed.number_sort_gt, Some(5.0));
    }

    #[test]
    fn webui_books_read_list_shape_is_normalized_into_native_filter() {
        let condition = json!({
            "readListId": {
                "operator": "is",
                "value": "readlist-1",
            }
        });

        let parsed = parse_native_books_filters(Some(&condition))
            .expect("legacy webui books read-list condition should be normalized");

        assert_eq!(parsed.read_list_ids, Some(vec!["readlist-1".to_string()]),);
    }

    #[test]
    fn webui_books_title_shape_is_normalized_into_native_filter() {
        let condition = json!({
            "title": {
                "operator": "beginsWith",
                "value": "book",
            }
        });

        let parsed = parse_native_books_filters(Some(&condition))
            .expect("legacy webui books title condition should be normalized");

        assert_eq!(parsed.titles_begins_with, Some(vec!["book".to_string()]),);
    }

    #[test]
    fn persisted_books_sort_modes_accept_supported_and_fall_back_for_unknown_values() {
        assert_eq!(
            parse_persisted_books_sort_modes(&[]),
            vec![PersistedBooksSortMode::TitleAsc],
        );
        assert_eq!(
            parse_persisted_books_sort_modes(&["metadata.releaseDate,desc".to_string()]),
            vec![PersistedBooksSortMode::ReleaseDateDesc],
        );
        assert_eq!(
            parse_persisted_books_sort_modes(&["unsupported.sort,asc".to_string()]),
            vec![PersistedBooksSortMode::TitleAsc],
        );
    }

    #[test]
    fn persisted_series_sort_modes_accept_supported_and_fall_back_for_unknown_values() {
        assert_eq!(
            parse_persisted_series_sort_modes(&[]),
            vec![PersistedSeriesSortMode::TitleAsc],
        );
        assert_eq!(
            parse_persisted_series_sort_modes(&["lastModifiedDate,desc".to_string()]),
            vec![PersistedSeriesSortMode::Latest],
        );
        assert_eq!(
            parse_persisted_series_sort_modes(&["unsupported.sort,asc".to_string()]),
            vec![PersistedSeriesSortMode::TitleAsc],
        );
    }

    #[test]
    fn books_all_of_single_series_leaf_sets_browse_series_paged_family() {
        let condition = json!({
            "type": "AllOfBook",
            "conditions": [
                {
                    "type": "SeriesId",
                    "operator": "is",
                    "value": "series-42"
                }
            ]
        });

        let parsed = parse_native_books_filters(Some(&condition))
            .expect("all-of single series filter should parse");

        assert_eq!(
            parsed.direct_browse_family,
            Some(DirectBrowseBooksListFamily::BrowseSeriesPaged)
        );
        assert_eq!(parsed.series_ids, Some(vec!["series-42".to_string()]));
    }

    #[test]
    fn series_filter_matrix_parses_expected_leaf_shapes() {
        let library = parse_native_series_filters(Some(&json!({
            "type": "LibraryId",
            "operator": "is",
            "value": "library-A",
        })))
        .expect("library id filter should parse");
        assert_eq!(library.library_ids, Some(vec!["library-A".to_string()]));

        let deleted = parse_native_series_filters(Some(&json!({
            "type": "Deleted",
            "operator": "isTrue",
        })))
        .expect("deleted filter should parse");
        assert_eq!(deleted.deleted, Some(true));

        let genre = parse_native_series_filters(Some(&json!({
            "type": "Genre",
            "operator": "contains",
            "value": "SciFi",
        })))
        .expect("genre filter should parse");
        assert_eq!(genre.genres, Some(vec!["scifi".to_string()]));

        let age = parse_native_series_filters(Some(&json!({
            "type": "AgeRating",
            "operator": "is",
            "value": "16",
        })))
        .expect("age rating filter should parse");
        assert_eq!(age.age_ratings, Some(vec![16]));
    }

    #[test]
    fn books_filter_matrix_parses_expected_leaf_shapes() {
        let library = parse_native_books_filters(Some(&json!({
            "type": "LibraryId",
            "operator": "is",
            "value": "library-A",
        })))
        .expect("books library id filter should parse");
        assert_eq!(library.library_ids, Some(vec!["library-A".to_string()]));

        let series = parse_native_books_filters(Some(&json!({
            "type": "SeriesId",
            "operator": "is",
            "value": "series-1",
        })))
        .expect("books series id filter should parse");
        assert_eq!(
            series.direct_browse_family,
            Some(DirectBrowseBooksListFamily::BrowseBookSiblingsUnpaged)
        );
        assert_eq!(series.series_ids, Some(vec!["series-1".to_string()]));

        let series_excluded = parse_native_books_filters(Some(&json!({
            "type": "SeriesId",
            "operator": "isNot",
            "value": "series-2",
        })))
        .expect("books series id isNot filter should parse");
        assert_eq!(
            series_excluded.series_ids_excluded,
            Some(vec!["series-2".to_string()]),
        );

        let tag = parse_native_books_filters(Some(&json!({
            "type": "Tag",
            "operator": "is",
            "value": "Favorite",
        })))
        .expect("books tag filter should parse");
        assert_eq!(tag.tags, Some(vec!["favorite".to_string()]));

        let deleted = parse_native_books_filters(Some(&json!({
            "type": "Deleted",
            "operator": "isFalse",
        })))
        .expect("books deleted filter should parse");
        assert_eq!(deleted.deleted, Some(false));
    }

    #[test]
    fn strict_mode_parses_books_poster_author_and_tag_nullable_filters() {
        let poster = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "Poster",
                "operator": "is",
                "value": {
                    "type": "USER_UPLOADED",
                    "selected": true,
                }
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept Poster is object payload");
        assert_eq!(poster.poster_types, Some(vec!["user_uploaded".to_string()]));
        assert_eq!(poster.poster_selected, Some(true));

        let poster_excluded = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "Poster",
                "operator": "isNot",
                "value": {
                    "type": "GENERATED",
                    "selected": false,
                }
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept Poster isNot object payload");
        assert_eq!(
            poster_excluded.poster_types_excluded,
            Some(vec!["generated".to_string()]),
        );
        assert_eq!(poster_excluded.poster_selected_excluded, Some(false));

        let books_author_is = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "Author",
                "operator": "is",
                "value": {
                    "name": "Jane Writer",
                    "role": "writer",
                }
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept Author is AuthorMatch payload");
        assert_eq!(
            books_author_is.authors,
            Some(vec!["jane writer::writer".to_string()]),
        );

        let books_author_is_not_role_only = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "Author",
                "operator": "isNot",
                "value": {
                    "role": "writer",
                }
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept Author isNot role-only payload");
        assert_eq!(
            books_author_is_not_role_only.authors_excluded,
            Some(vec!["::writer".to_string()]),
        );

        let books_tag_is_not = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "Tag",
                "operator": "isNot",
                "value": "favorite-tag",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept Tag isNot operator");
        assert_eq!(
            books_tag_is_not.tags_excluded,
            Some(vec!["favorite-tag".to_string()]),
        );

        let books_tag_is_null = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "Tag",
                "operator": "isNull",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept Tag isNull operator");
        assert_eq!(books_tag_is_null.tags_null, Some(true));

        let books_tag_is_not_null = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "Tag",
                "operator": "isNotNull",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept Tag isNotNull operator");
        assert_eq!(books_tag_is_not_null.tags_null, Some(false));
    }

    #[test]
    fn strict_mode_parses_series_author_and_nullable_metadata_filters() {
        let series_author_is = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "Author",
                "operator": "is",
                "value": {
                    "name": "John Doe",
                    "role": "writer",
                }
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept Author is AuthorMatch payload");
        assert_eq!(
            series_author_is.authors,
            Some(vec!["john doe::writer".to_string()]),
        );

        let series_author_is_not_role_only = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "Author",
                "operator": "isNot",
                "value": {
                    "role": "writer",
                }
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept Author isNot role-only payload");
        assert_eq!(
            series_author_is_not_role_only.authors_excluded,
            Some(vec!["::writer".to_string()]),
        );

        let series_tag_is_not = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "Tag",
                "operator": "isNot",
                "value": "favorite",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept Tag isNot operator");
        assert_eq!(
            series_tag_is_not.tags_excluded,
            Some(vec!["favorite".to_string()]),
        );

        let series_tag_is_null = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "Tag",
                "operator": "isNull",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept Tag isNull operator");
        assert_eq!(series_tag_is_null.tags_null, Some(true));

        let series_genre_is_not = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "Genre",
                "operator": "isNot",
                "value": "scifi",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept Genre isNot operator");
        assert_eq!(
            series_genre_is_not.genres_excluded,
            Some(vec!["scifi".to_string()]),
        );

        let series_genre_is_not_null = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "Genre",
                "operator": "isNotNull",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept Genre isNotNull operator");
        assert_eq!(series_genre_is_not_null.genres_null, Some(false));

        let series_sharing_label_is_not = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "SharingLabel",
                "operator": "isNot",
                "value": "family",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept SharingLabel isNot operator");
        assert_eq!(
            series_sharing_label_is_not.sharing_labels_excluded,
            Some(vec!["family".to_string()]),
        );

        let series_sharing_label_is_null = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "SharingLabel",
                "operator": "isNull",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept SharingLabel isNull operator");
        assert_eq!(series_sharing_label_is_null.sharing_labels_null, Some(true));
    }

    #[test]
    fn series_and_books_string_filters_are_operator_lenient_in_compat_mode() {
        let series_title = parse_native_series_filters(Some(&json!({
            "type": "Title",
            "operator": "unsupported-op",
            "value": "Series",
        })))
        .expect("series title filter should parse in lenient mode");
        assert_eq!(series_title.titles_contains, None);

        let books_author = parse_native_books_filters(Some(&json!({
            "type": "Author",
            "operator": "unsupported-op",
            "value": "John Doe",
        })))
        .expect("books author filter should parse in lenient mode");
        assert_eq!(books_author.authors, Some(vec!["john doe".to_string()]));
    }

    #[test]
    fn strict_mode_rejects_unsupported_operators_and_unknown_condition_types() {
        let series_error = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "Language",
                "operator": "unsupported-op",
                "value": "EN",
            })),
            OperatorValidationMode::Strict,
        )
        .expect_err("strict series parse should reject unsupported operator");
        assert!(matches!(
            series_error,
            DiscoveryError::InvalidRequest(message) if message.contains("Language")
        ));

        let books_error = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "UnknownBooksFilter",
                "operator": "is",
                "value": "x",
            })),
            OperatorValidationMode::Strict,
        )
        .expect_err("strict books parse should reject unknown filter type");
        assert!(matches!(
            books_error,
            DiscoveryError::InvalidRequest(message)
                if message.contains("unsupported native books condition type")
        ));
    }

    #[test]
    fn strict_mode_still_accepts_supported_operator_shapes() {
        let series = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "Language",
                "operator": "is",
                "value": "EN",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept supported operator");
        assert_eq!(series.languages, Some(vec!["en".to_string()]));

        let collection = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "CollectionId",
                "operator": "is",
                "value": "collection-1",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept CollectionId is operator");
        assert_eq!(
            collection.collection_ids,
            Some(vec!["collection-1".to_string()]),
        );

        let series_title_contains = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "Title",
                "operator": "contains",
                "value": "series",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept Title contains operator");
        assert_eq!(
            series_title_contains.titles_contains,
            Some(vec!["series".to_string()]),
        );

        let series_title_sort_begins_with = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "TitleSort",
                "operator": "beginsWith",
                "value": "series",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept TitleSort beginsWith operator");
        assert_eq!(
            series_title_sort_begins_with.title_sorts_begins_with,
            Some(vec!["series".to_string()]),
        );

        let series_status_excluded = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "SeriesStatus",
                "operator": "isNot",
                "value": "ONGOING",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept SeriesStatus isNot operator");
        assert_eq!(
            series_status_excluded.series_statuses_excluded,
            Some(vec!["ongoing".to_string()]),
        );

        let series_release_date_excluded = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "isNot",
                "value": "2024-01-15",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept ReleaseDate isNot operator");
        assert_eq!(
            series_release_date_excluded.release_dates_excluded,
            Some(vec!["2024-01-15".to_string()]),
        );

        let series_release_date_null = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "isNull",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept ReleaseDate isNull operator");
        assert_eq!(series_release_date_null.release_dates_null, Some(true));

        let series_release_date_gt = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "greaterThan",
                "value": "2024-01-15",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept ReleaseDate greaterThan operator");
        assert_eq!(
            series_release_date_gt.release_date_gt,
            Some("2024-01-15".to_string()),
        );

        let series_release_date_lt = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "lessThan",
                "value": "2024-12-31",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept ReleaseDate lessThan operator");
        assert_eq!(
            series_release_date_lt.release_date_lt,
            Some("2024-12-31".to_string()),
        );

        let series_release_date_after = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "after",
                "dateTime": "2024-01-15T10:30:00Z",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept ReleaseDate after operator");
        assert_eq!(
            series_release_date_after.release_date_gt,
            Some("2024-01-15".to_string()),
        );

        let series_release_date_before = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "before",
                "dateTime": "2024-12-31T23:59:59+00:00",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept ReleaseDate before operator");
        assert_eq!(
            series_release_date_before.release_date_lt,
            Some("2024-12-31".to_string()),
        );

        let series_release_date_is_in_the_last = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "isInTheLast",
                "duration": "P10D",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept ReleaseDate isInTheLast operator");
        assert_eq!(
            series_release_date_is_in_the_last.release_date_in_last_days,
            Some(10)
        );

        let series_release_date_is_not_in_the_last = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "isNotInTheLast",
                "duration": "P1D",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept ReleaseDate isNotInTheLast operator");
        assert_eq!(
            series_release_date_is_not_in_the_last.release_date_not_in_last_days,
            Some(1),
        );

        let series_release_date_begins_with = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "beginsWith",
                "value": "2024-01",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept ReleaseDate beginsWith operator");
        assert_eq!(
            series_release_date_begins_with.release_date_begins_with,
            Some(vec!["2024-01".to_string()]),
        );

        let series_release_date_ends_with = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "endsWith",
                "value": "-15",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept ReleaseDate endsWith operator");
        assert_eq!(
            series_release_date_ends_with.release_date_ends_with,
            Some(vec!["-15".to_string()]),
        );

        let series_release_date_does_not_contain = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "doesNotContain",
                "value": "2025",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept ReleaseDate doesNotContain operator");
        assert_eq!(
            series_release_date_does_not_contain.release_date_contains_excluded,
            Some(vec!["2025".to_string()]),
        );

        let series_release_date_does_not_begin_with = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "doesNotBeginWith",
                "value": "2025",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept ReleaseDate doesNotBeginWith operator");
        assert_eq!(
            series_release_date_does_not_begin_with.release_date_begins_with_excluded,
            Some(vec!["2025".to_string()]),
        );

        let series_release_date_does_not_end_with = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "doesNotEndWith",
                "value": "-99",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept ReleaseDate doesNotEndWith operator");
        assert_eq!(
            series_release_date_does_not_end_with.release_date_ends_with_excluded,
            Some(vec!["-99".to_string()]),
        );

        let series_release_date_excluded = parse_native_series_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "isNot",
                "value": "2024-01-15",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict series parse should accept ReleaseDate isNot operator");
        assert_eq!(
            series_release_date_excluded.release_dates_excluded,
            Some(vec!["2024-01-15".to_string()]),
        );

        let books = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "MediaStatus",
                "operator": "is",
                "value": "READY",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept supported operator");
        assert_eq!(books.media_statuses, Some(vec!["ready".to_string()]));

        let books_series_is_not = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "SeriesId",
                "operator": "isNot",
                "value": "series-1",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept SeriesId isNot operator");
        assert_eq!(
            books_series_is_not.series_ids_excluded,
            Some(vec!["series-1".to_string()]),
        );

        let books_read_status_excluded = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "ReadStatus",
                "operator": "isNot",
                "value": "READ",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept ReadStatus isNot operator");
        assert_eq!(
            books_read_status_excluded.read_statuses_excluded,
            Some(vec!["read".to_string()]),
        );

        let books_media_status_excluded = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "MediaStatus",
                "operator": "isNot",
                "value": "READY",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept MediaStatus isNot operator");
        assert_eq!(
            books_media_status_excluded.media_statuses_excluded,
            Some(vec!["ready".to_string()]),
        );

        let books_release_date_excluded = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "isNot",
                "value": "2024-01-15",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept ReleaseDate isNot operator");
        assert_eq!(
            books_release_date_excluded.release_dates_excluded,
            Some(vec!["2024-01-15".to_string()]),
        );

        let books_release_date_not_null = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "isNotNull",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept ReleaseDate isNotNull operator");
        assert_eq!(books_release_date_not_null.release_dates_null, Some(false));

        let books_release_date_gt = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "greaterThan",
                "value": "2024-01-15",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept ReleaseDate greaterThan operator");
        assert_eq!(
            books_release_date_gt.release_date_gt,
            Some("2024-01-15".to_string()),
        );

        let books_release_date_lt = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "lessThan",
                "value": "2024-12-31",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept ReleaseDate lessThan operator");
        assert_eq!(
            books_release_date_lt.release_date_lt,
            Some("2024-12-31".to_string()),
        );

        let books_release_date_after = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "after",
                "dateTime": "2024-01-15T10:30:00Z",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept ReleaseDate after operator");
        assert_eq!(
            books_release_date_after.release_date_gt,
            Some("2024-01-15".to_string()),
        );

        let books_release_date_before = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "before",
                "dateTime": "2024-12-31T23:59:59+00:00",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept ReleaseDate before operator");
        assert_eq!(
            books_release_date_before.release_date_lt,
            Some("2024-12-31".to_string()),
        );

        let books_release_date_is_in_the_last = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "isInTheLast",
                "duration": "P10D",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept ReleaseDate isInTheLast operator");
        assert_eq!(
            books_release_date_is_in_the_last.release_date_in_last_days,
            Some(10)
        );

        let books_release_date_is_not_in_the_last = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "isNotInTheLast",
                "duration": "P1D",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept ReleaseDate isNotInTheLast operator");
        assert_eq!(
            books_release_date_is_not_in_the_last.release_date_not_in_last_days,
            Some(1),
        );

        let books_number_sort_is = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "NumberSort",
                "operator": "is",
                "value": 10.5,
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept NumberSort is operator");
        assert_eq!(books_number_sort_is.number_sorts, Some(vec![10.5]));

        let books_number_sort_is_not = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "NumberSort",
                "operator": "isNot",
                "value": 1.0,
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept NumberSort isNot operator");
        assert_eq!(
            books_number_sort_is_not.number_sorts_excluded,
            Some(vec![1.0])
        );

        let books_number_sort_gt = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "NumberSort",
                "operator": "greaterThan",
                "value": 5.0,
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept NumberSort greaterThan operator");
        assert_eq!(books_number_sort_gt.number_sort_gt, Some(5.0));

        let books_number_sort_lt = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "NumberSort",
                "operator": "lessThan",
                "value": 11.0,
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept NumberSort lessThan operator");
        assert_eq!(books_number_sort_lt.number_sort_lt, Some(11.0));

        let books_read_list_is = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "ReadListId",
                "operator": "is",
                "value": "readlist-1",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept ReadListId is operator");
        assert_eq!(
            books_read_list_is.read_list_ids,
            Some(vec!["readlist-1".to_string()]),
        );

        let books_read_list_is_not = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "ReadListId",
                "operator": "isNot",
                "value": "readlist-1",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept ReadListId isNot operator");
        assert_eq!(
            books_read_list_is_not.read_list_ids_excluded,
            Some(vec!["readlist-1".to_string()]),
        );

        let books_tag_is = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "Tag",
                "operator": "is",
                "value": "favorite-tag",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept Tag is operator");
        assert_eq!(books_tag_is.tags, Some(vec!["favorite-tag".to_string()]));

        let books_author_contains = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "Author",
                "operator": "contains",
                "value": "jane",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept Author contains operator");
        assert_eq!(
            books_author_contains.authors,
            Some(vec!["jane".to_string()]),
        );

        let books_media_profile_is = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "MediaProfile",
                "operator": "is",
                "value": "epub",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept MediaProfile is operator");
        assert_eq!(
            books_media_profile_is.media_profiles,
            Some(vec!["epub".to_string()]),
        );

        let books_title_contains = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "Title",
                "operator": "contains",
                "value": "book",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept Title contains operator");
        assert_eq!(
            books_title_contains.titles_contains,
            Some(vec!["book".to_string()]),
        );

        let books_title_does_not_end_with = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "Title",
                "operator": "doesNotEndWith",
                "value": "-x",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept Title doesNotEndWith operator");
        assert_eq!(
            books_title_does_not_end_with.titles_ends_with_excluded,
            Some(vec!["-x".to_string()]),
        );

        let books_release_date_begins_with = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "beginsWith",
                "value": "2024-01",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept ReleaseDate beginsWith operator");
        assert_eq!(
            books_release_date_begins_with.release_date_begins_with,
            Some(vec!["2024-01".to_string()]),
        );

        let books_release_date_ends_with = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "endsWith",
                "value": "-15",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept ReleaseDate endsWith operator");
        assert_eq!(
            books_release_date_ends_with.release_date_ends_with,
            Some(vec!["-15".to_string()]),
        );

        let books_release_date_does_not_contain = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "doesNotContain",
                "value": "2025",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept ReleaseDate doesNotContain operator");
        assert_eq!(
            books_release_date_does_not_contain.release_date_contains_excluded,
            Some(vec!["2025".to_string()]),
        );

        let books_release_date_does_not_begin_with = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "doesNotBeginWith",
                "value": "2025",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept ReleaseDate doesNotBeginWith operator");
        assert_eq!(
            books_release_date_does_not_begin_with.release_date_begins_with_excluded,
            Some(vec!["2025".to_string()]),
        );

        let books_release_date_does_not_end_with = parse_native_books_filters_with_mode(
            Some(&json!({
                "type": "ReleaseDate",
                "operator": "doesNotEndWith",
                "value": "-99",
            })),
            OperatorValidationMode::Strict,
        )
        .expect("strict books parse should accept ReleaseDate doesNotEndWith operator");
        assert_eq!(
            books_release_date_does_not_end_with.release_date_ends_with_excluded,
            Some(vec!["-99".to_string()]),
        );
    }

    #[test]
    fn series_composite_filters_merge_with_intersection_union_and_boolean_conflicts() {
        let all_of = parse_native_series_filters(Some(&json!({
            "type": "AllOfSeries",
            "conditions": [
                {"type": "Language", "operator": "is", "value": "EN"},
                {"type": "Language", "operator": "is", "value": "FR"},
                {"type": "Deleted", "operator": "isTrue"},
                {"type": "Deleted", "operator": "isFalse"}
            ]
        })))
        .expect("all-of series composite should parse");
        assert_eq!(all_of.languages, Some(vec![]));
        assert_eq!(all_of.deleted, None);

        let any_of = parse_native_series_filters(Some(&json!({
            "type": "AnyOfSeries",
            "conditions": [
                {"type": "Tag", "operator": "contains", "value": "Action"},
                {"type": "Tag", "operator": "contains", "value": "Drama"},
                {"type": "Tag", "operator": "contains", "value": "Action"}
            ]
        })))
        .expect("any-of series composite should parse");
        assert_eq!(
            any_of.tags,
            Some(vec!["action".to_string(), "drama".to_string()])
        );
    }

    #[test]
    fn books_composite_filters_merge_and_only_single_leaf_all_of_sets_browse_paged() {
        let all_of_multi_leaf = parse_native_books_filters(Some(&json!({
            "type": "AllOfBook",
            "conditions": [
                {"type": "SeriesId", "operator": "is", "value": "series-1"},
                {"type": "SeriesId", "operator": "is", "value": "series-1"}
            ]
        })))
        .expect("all-of books composite should parse");
        assert_eq!(
            all_of_multi_leaf.series_ids,
            Some(vec!["series-1".to_string()])
        );
        assert_eq!(all_of_multi_leaf.direct_browse_family, None);

        let any_of = parse_native_books_filters(Some(&json!({
            "type": "AnyOfBook",
            "conditions": [
                {"type": "SeriesId", "operator": "is", "value": "series-1"},
                {"type": "SeriesId", "operator": "is", "value": "series-2"}
            ]
        })))
        .expect("any-of books composite should parse");
        assert_eq!(
            any_of.series_ids,
            Some(vec!["series-1".to_string(), "series-2".to_string()])
        );
        assert_eq!(any_of.direct_browse_family, None);
    }

    #[test]
    fn webui_unknown_leaf_normalizes_to_empty_compat_composite() {
        let series = parse_native_series_filters(Some(&json!({
            "unexpected": {
                "operator": "is",
                "value": "x"
            }
        })))
        .expect("unknown webui series leaf should degrade to empty composite");
        assert!(series.library_ids.is_none());
        assert!(series.deleted.is_none());
        assert!(series.oneshot.is_none());
        assert!(series.read_statuses.is_none());
        assert!(series.genres.is_none());
        assert!(series.tags.is_none());

        let books = parse_native_books_filters(Some(&json!({
            "unexpected": {
                "operator": "is",
                "value": "x"
            }
        })))
        .expect("unknown webui books leaf should degrade to empty composite");
        assert!(books.direct_browse_family.is_none());
        assert!(books.library_ids.is_none());
        assert!(books.series_ids.is_none());
        assert!(books.deleted.is_none());
        assert!(books.oneshot.is_none());
        assert!(books.tags.is_none());
    }

    #[test]
    fn persisted_sort_modes_preserve_supported_sort_sequence() {
        assert_eq!(
            parse_persisted_books_sort_modes(&[
                "unsupported.sort,asc".to_string(),
                "createdDate,desc".to_string(),
            ]),
            vec![PersistedBooksSortMode::CreatedDateDesc],
        );

        assert_eq!(
            parse_persisted_books_sort_modes(&[
                "createdDate,desc".to_string(),
                "lastModifiedDate,desc".to_string(),
                "metadata.releaseDate,desc".to_string(),
            ]),
            vec![
                PersistedBooksSortMode::CreatedDateDesc,
                PersistedBooksSortMode::LastModifiedDateDesc,
                PersistedBooksSortMode::ReleaseDateDesc,
            ],
        );

        assert_eq!(
            parse_persisted_series_sort_modes(&[
                "unsupported.sort,asc".to_string(),
                "lastModifiedDate,desc".to_string(),
            ]),
            vec![PersistedSeriesSortMode::Latest],
        );

        assert_eq!(
            parse_persisted_series_sort_modes(&[
                "metadata.titleSort,asc".to_string(),
                "lastModifiedDate,desc".to_string(),
            ]),
            vec![
                PersistedSeriesSortMode::TitleAsc,
                PersistedSeriesSortMode::Latest
            ],
        );
    }

    #[test]
    fn native_books_persisted_compatibility_allows_supported_scalar_filters() {
        let media_status_filters = NativeBooksFilters {
            media_statuses: Some(vec!["ready".to_string()]),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &media_status_filters
        ));

        let media_status_excluded_filters = NativeBooksFilters {
            media_statuses_excluded: Some(vec!["ready".to_string()]),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &media_status_excluded_filters
        ));

        let read_status_filters = NativeBooksFilters {
            read_statuses: Some(vec!["unread".to_string()]),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &read_status_filters
        ));

        let read_status_excluded_filters = NativeBooksFilters {
            read_statuses_excluded: Some(vec!["read".to_string()]),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &read_status_excluded_filters
        ));

        let series_id_filters = NativeBooksFilters {
            direct_browse_family: Some(DirectBrowseBooksListFamily::BrowseBookSiblingsUnpaged),
            series_ids: Some(vec!["series-1".to_string()]),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &series_id_filters
        ));

        let release_date_filters = NativeBooksFilters {
            release_dates: Some(vec!["2024-01-15".to_string()]),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &release_date_filters
        ));

        let release_date_excluded_filters = NativeBooksFilters {
            release_dates_excluded: Some(vec!["2024-01-15".to_string()]),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &release_date_excluded_filters
        ));

        let release_date_null_filters = NativeBooksFilters {
            release_dates_null: Some(true),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &release_date_null_filters
        ));

        let release_date_gt_filters = NativeBooksFilters {
            release_date_gt: Some("2024-01-01".to_string()),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &release_date_gt_filters
        ));

        let release_date_lt_filters = NativeBooksFilters {
            release_date_lt: Some("2024-12-31".to_string()),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &release_date_lt_filters
        ));

        let release_date_begins_with_filters = NativeBooksFilters {
            release_date_begins_with: Some(vec!["2024-01".to_string()]),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &release_date_begins_with_filters
        ));

        let release_date_ends_with_filters = NativeBooksFilters {
            release_date_ends_with: Some(vec!["-15".to_string()]),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &release_date_ends_with_filters
        ));

        let release_date_contains_excluded_filters = NativeBooksFilters {
            release_date_contains_excluded: Some(vec!["2025".to_string()]),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &release_date_contains_excluded_filters
        ));

        let release_date_begins_with_excluded_filters = NativeBooksFilters {
            release_date_begins_with_excluded: Some(vec!["2025".to_string()]),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &release_date_begins_with_excluded_filters
        ));

        let release_date_ends_with_excluded_filters = NativeBooksFilters {
            release_date_ends_with_excluded: Some(vec!["-99".to_string()]),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &release_date_ends_with_excluded_filters
        ));

        let release_date_is_in_last_filters = NativeBooksFilters {
            release_date_in_last_days: Some(7),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &release_date_is_in_last_filters
        ));

        let release_date_is_not_in_last_filters = NativeBooksFilters {
            release_date_not_in_last_days: Some(30),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &release_date_is_not_in_last_filters
        ));

        let deleted_filters = NativeBooksFilters {
            deleted: Some(false),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(&deleted_filters));

        let title_contains_filters = NativeBooksFilters {
            titles_contains: Some(vec!["book".to_string()]),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &title_contains_filters
        ));

        let title_excluded_filters = NativeBooksFilters {
            titles_excluded: Some(vec!["book x".to_string()]),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &title_excluded_filters
        ));

        let oneshot_filters = NativeBooksFilters {
            oneshot: Some(true),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(&oneshot_filters));

        let number_sort_filters = NativeBooksFilters {
            number_sorts: Some(vec![10.5]),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &number_sort_filters
        ));

        let number_sort_excluded_filters = NativeBooksFilters {
            number_sorts_excluded: Some(vec![1.0]),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &number_sort_excluded_filters
        ));

        let number_sort_gt_filters = NativeBooksFilters {
            number_sort_gt: Some(5.0),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &number_sort_gt_filters
        ));

        let number_sort_lt_filters = NativeBooksFilters {
            number_sort_lt: Some(11.0),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &number_sort_lt_filters
        ));

        let read_list_filters = NativeBooksFilters {
            read_list_ids: Some(vec!["readlist-1".to_string()]),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &read_list_filters
        ));

        let read_list_excluded_filters = NativeBooksFilters {
            read_list_ids_excluded: Some(vec!["readlist-1".to_string()]),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &read_list_excluded_filters
        ));

        let media_profile_filters = NativeBooksFilters {
            media_profiles: Some(vec!["epub".to_string()]),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(
            &media_profile_filters
        ));

        let tag_filters = NativeBooksFilters {
            tags: Some(vec!["favorite-tag".to_string()]),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(&tag_filters));

        let author_filters = NativeBooksFilters {
            authors: Some(vec!["jane".to_string()]),
            ..NativeBooksFilters::default()
        };
        assert!(native_books_filters_persisted_compatible(&author_filters));
    }

    #[test]
    fn native_series_persisted_compatibility_allows_deleted_and_oneshot_filters() {
        let deleted_filters = NativeSeriesFilters {
            deleted: Some(false),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(&deleted_filters));

        let oneshot_filters = NativeSeriesFilters {
            oneshot: Some(true),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(&oneshot_filters));

        let collection_filters = NativeSeriesFilters {
            collection_ids: Some(vec!["collection-1".to_string()]),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &collection_filters
        ));

        let title_contains_filters = NativeSeriesFilters {
            titles_contains: Some(vec!["series".to_string()]),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &title_contains_filters
        ));

        let title_sort_begins_with_filters = NativeSeriesFilters {
            title_sorts_begins_with: Some(vec!["series".to_string()]),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &title_sort_begins_with_filters
        ));

        let genre_filters = NativeSeriesFilters {
            genres: Some(vec!["scifi".to_string()]),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(&genre_filters));

        let tag_filters = NativeSeriesFilters {
            tags: Some(vec!["favorite".to_string()]),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(&tag_filters));

        let language_filters = NativeSeriesFilters {
            languages: Some(vec!["en".to_string()]),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &language_filters
        ));

        let publisher_filters = NativeSeriesFilters {
            publishers: Some(vec!["pubhouse".to_string()]),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &publisher_filters
        ));

        let age_rating_filters = NativeSeriesFilters {
            age_ratings: Some(vec![16]),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &age_rating_filters
        ));

        let sharing_label_filters = NativeSeriesFilters {
            sharing_labels: Some(vec!["family".to_string()]),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &sharing_label_filters
        ));

        let author_filters = NativeSeriesFilters {
            authors: Some(vec!["john".to_string()]),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(&author_filters));

        let series_status_filters = NativeSeriesFilters {
            series_statuses: Some(vec!["ongoing".to_string()]),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &series_status_filters
        ));

        let series_status_excluded_filters = NativeSeriesFilters {
            series_statuses_excluded: Some(vec!["ongoing".to_string()]),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &series_status_excluded_filters
        ));

        let release_date_filters = NativeSeriesFilters {
            release_dates: Some(vec!["2024-01-15".to_string()]),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &release_date_filters
        ));

        let release_date_excluded_filters = NativeSeriesFilters {
            release_dates_excluded: Some(vec!["2024-01-15".to_string()]),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &release_date_excluded_filters
        ));

        let release_date_null_filters = NativeSeriesFilters {
            release_dates_null: Some(true),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &release_date_null_filters
        ));

        let release_date_gt_filters = NativeSeriesFilters {
            release_date_gt: Some("2024-01-01".to_string()),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &release_date_gt_filters
        ));

        let release_date_lt_filters = NativeSeriesFilters {
            release_date_lt: Some("2024-12-31".to_string()),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &release_date_lt_filters
        ));

        let release_date_begins_with_filters = NativeSeriesFilters {
            release_date_begins_with: Some(vec!["2024-01".to_string()]),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &release_date_begins_with_filters
        ));

        let release_date_ends_with_filters = NativeSeriesFilters {
            release_date_ends_with: Some(vec!["-15".to_string()]),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &release_date_ends_with_filters
        ));

        let release_date_contains_excluded_filters = NativeSeriesFilters {
            release_date_contains_excluded: Some(vec!["2025".to_string()]),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &release_date_contains_excluded_filters
        ));

        let release_date_begins_with_excluded_filters = NativeSeriesFilters {
            release_date_begins_with_excluded: Some(vec!["2025".to_string()]),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &release_date_begins_with_excluded_filters
        ));

        let release_date_ends_with_excluded_filters = NativeSeriesFilters {
            release_date_ends_with_excluded: Some(vec!["-99".to_string()]),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &release_date_ends_with_excluded_filters
        ));

        let release_date_is_in_last_filters = NativeSeriesFilters {
            release_date_in_last_days: Some(7),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &release_date_is_in_last_filters
        ));

        let release_date_is_not_in_last_filters = NativeSeriesFilters {
            release_date_not_in_last_days: Some(30),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &release_date_is_not_in_last_filters
        ));

        let release_date_filters = NativeSeriesFilters {
            release_dates: Some(vec!["2024-01-15".to_string()]),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &release_date_filters
        ));

        let release_date_excluded_filters = NativeSeriesFilters {
            release_dates_excluded: Some(vec!["2024-01-15".to_string()]),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &release_date_excluded_filters
        ));

        let supported_read_status = NativeSeriesFilters {
            read_statuses: Some(vec!["unread".to_string()]),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &supported_read_status
        ));

        let supported_read_status_excluded = NativeSeriesFilters {
            read_statuses_excluded: Some(vec!["read".to_string()]),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &supported_read_status_excluded
        ));

        let supported_complete = NativeSeriesFilters {
            complete: Some(true),
            ..NativeSeriesFilters::default()
        };
        assert!(native_series_filters_persisted_compatible(
            &supported_complete
        ));
    }

    #[test]
    fn release_date_datetime_normalization_extracts_iso_date() {
        assert_eq!(
            normalize_release_date_date_time("2024-01-15T10:30:00Z"),
            Some("2024-01-15".to_string()),
        );
        assert_eq!(
            normalize_release_date_date_time("2024-12-31"),
            Some("2024-12-31".to_string()),
        );
        assert!(normalize_release_date_date_time("2024/01/15").is_none());
        assert!(normalize_release_date_date_time("").is_none());
    }

    #[test]
    fn iso8601_duration_parses_to_truncated_days() {
        assert_eq!(parse_iso8601_duration_to_days("P1D"), Some(1));
        assert_eq!(parse_iso8601_duration_to_days("PT36H"), Some(1));
        assert_eq!(parse_iso8601_duration_to_days("P1DT12H"), Some(1));
        assert_eq!(parse_iso8601_duration_to_days("PT12H"), Some(0));
        assert_eq!(parse_iso8601_duration_to_days("-PT12H"), Some(0));
        assert!(parse_iso8601_duration_to_days("P1M").is_none());
        assert!(parse_iso8601_duration_to_days("foo").is_none());
    }
}
